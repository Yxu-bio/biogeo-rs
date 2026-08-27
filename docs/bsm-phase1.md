# BSM 第一阶段：条件历史骨架

## 本阶段范围

本阶段实现的是完整生物地理随机历史采样（biogeographic stochastic mapping，BSM）之前的
条件随机回溯层。
它从已经完成 pruning 的模型中联合抽取：

- 根节点的祖先范围；
- 每个内部节点的 cladogenesis split scenario；
- 每条分支在父节点一侧的起始范围；
- 每条分支在子节点一侧的终止范围。

这些量共同组成 `HistorySkeleton`。它已经是一条内部一致的离散地理历史骨架，但尚未
抽取分支内部发生的扩散、局部灭绝事件及其时间，所以当前接口和 CLI 都不把它标为
完整 BSM。

## 为什么不能独立抽节点 posterior

如果分别从每个节点的 marginal posterior 独立抽样，父节点 split 给出的 daughter
范围可能与子分支的起始范围不一致，分支终止范围也可能与子节点状态不一致。这样的
结果虽然每个节点单独看都符合 posterior，却不能组成一条可能发生的联合历史。

当前实现从根向叶进行 stochastic traceback：

1. 根状态 `a` 的抽样质量为：

   ```text
   root_prior[a] * subtree_likelihood[root][a]
   ```

2. 已知内部节点祖先状态 `a` 后，分裂情景 `(a -> l, r)` 的抽样质量为：

   ```text
   C[a,l,r]
     * branch_likelihood[left_edge][l]
     * branch_likelihood[right_edge][r]
   ```

3. 已知分支起始状态 `s` 后，分支终止状态 `t` 的抽样质量为：

   ```text
   P_edge[s,t] * subtree_likelihood[child][t]
   ```

4. 抽到的 `t` 作为子节点状态继续向下回溯。

pruning 中每个子树的缩放常数与候选状态无关，所以在同一次条件抽样归一化时会抵消，
直接使用 `scaled_likelihoods` 不改变抽样概率。

## 与统一模型框架的关系

`HistorySkeletonSampler` 不重建 DEC、DIVALIKE 或 BAYAREALIKE 规则。它直接消费
固定似然时使用的：

- `OwnedBranchPropagator`：均一 Q 或按时期分段的 piecewise-Q；
- `OwnedCladogeneticProcess`：按节点年龄选择的 split table；
- `PruningResult`：各节点缩放后的 subtree conditional likelihood；
- `RootPrior`：包括时期状态约束投影后的根先验。

因此六个 preset、方向性 dispersal、founder-event 修饰、时间分层 Q 和范围状态约束
共用同一抽样路径，没有按模型名或 fixture 加入特判。

## 公共 API

核心入口是：

```rust
let mut sampler = engine.prepare_history_skeleton_sampler(&model, &pruning)?;
let history = sampler.sample(&mut rng)?;
let histories = sampler.sample_many(1000, &mut rng)?;
```

需要直接按种子复现时，可以使用：

```rust
let histories = engine.sample_history_skeletons_seeded(
    &model,
    &pruning,
    1000,
    20260715,
)?;
```

sampler 按 `(edge_index, start_state)` 缓存实际访问过的转移矩阵行。这样重复抽样不会
反复做相同的 uniformization，也不会预先构造所有分支的完整稠密转移矩阵。

`BranchEndpointSample` 中的术语固定为：

- `start_state`：父节点分裂之后、进入分支时的 daughter range；
- `end_state`：沿分支传播之后、到达子节点时的 range。

这避免依赖不同软件对 branch top / bottom 的相反绘图约定。

## CLI

固定模型命令支持：

```text
biogeo-cli dec \
  --tree tree.nwk \
  --ranges ranges.tsv \
  --d 0.1 \
  --e 0.2 \
  --traceback-samples 100 \
  --seed 42
```

输出段名为 `conditional_history_skeletons`，包含三张 TSV 表：

- `traceback_node_states`；
- `traceback_splits`；
- `traceback_branch_endpoints`。

默认 `traceback_samples=0`，因此不改变原有固定模型输出。当前随机数实现固定依赖版本
并由 `Cargo.lock` 锁定；相同程序版本、输入和 seed 会得到逐条相同结果。

## 已完成验证

1. 权重抽样器拒绝 NaN、负权重、零总质量和越界随机数。
2. 两区域零长度分支 DEC 手工例中，根、split、分支起止状态全部为唯一可行历史。
3. 三叶、三区域 DEC+J 非平凡案例抽取 20,000 条历史；节点状态和 split scenario
   经验频率与精确 posterior 的绝对差小于 `0.02`。
4. 同一验证同时检查每条历史的父 split、分支起点、分支终点和子节点状态严格相接。
5. 启用两时期方向性 dispersal、area-specific extirpation 和年轻时期 range-state
   constraint 后，再抽取 20,000 条历史；经验 posterior 仍满足相同误差阈值，且所有
   年轻时期节点状态和 split 都满足状态 mask。
6. 相同 seed 的库 API 与 CLI 输出逐条可复现。

这里的 Monte Carlo 误差测试不是 BioGeoBEARS golden 的替代。固定似然、精确 node
posterior 和 split posterior 仍由现有 BioGeoBEARS golden 锁定；抽样测试验证新的
随机回溯是否忠实消费这些已锁定概率。

## 第二阶段现状

上述缺口现已由完整 BSM 第二阶段补齐。当前实现会在每条分支上，条件于已经抽到的
`start_state` 和 `end_state`，抽取一条连续时间马尔可夫链路径，包括：

- 分支内部事件次数；
- 每次事件是 range expansion 还是 local extirpation；
- 每次事件前后的范围状态；
- 事件在分支上的发生时间；
- piecewise-Q 分支跨时期边界时，各时期内的条件 bridge；
- 状态约束边界上的合法投影与零概率路径诊断。

实现采用 uniformization 条件 CTMC bridge，没有新增另一套 Q 或事件规则。解析量、
事件计数和分时期路径的内部统计回归已经完成；算法、API、CLI 和剩余 BioGeoBEARS
分布级外部对照见 `docs/bsm-ctmc-bridge.md`。
