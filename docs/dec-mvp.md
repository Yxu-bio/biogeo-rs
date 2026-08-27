# DEC MVP

本项目从 DEC 开始，但核心已经按可配置的 BioGeoBEARS-like 似然框架组织。
DEC 和 DEC+J 是 `ModelConfig` preset，而不是两套独立 pruning 算法。

## 范围

第一个里程碑支持：

- Newick 树输入；当前支持最小解析器和 CLI 文件读取。
- tip range 的 presence/absence 数据。
- DEC 沿分支变化模型：`d` 表示范围扩张，`e` 表示局部灭绝。
- 稀疏 Q 矩阵表示。
- 带数值缩放的树 pruning likelihood。
- DEC cladogenesis 下的内部节点范围 posterior 输出。
- DEC cladogenesis 下的内部节点分裂情景 posterior 输出。
- BioGeoBEARS 默认 DEC+J linked-weight 语义和 `d/e/j` 优化。
- 使用小型手工例子和经典工具结果做 golden tests。

当前仍未完整支持：

- trait-dependent dispersal。
- GUI、绘图和报告由新版 RASP 负责，不属于 Rust CLI 的实现范围。内部 CTMC 路径、超长/
  高事件率 segment 自动细分、确定性有界并行、逐条消费 API、版本化流式写出、崩溃一致
  检查点续跑、协作式取消、耗时上限、单样本/任务总事件预算、完整历史窗口内存预算、逐条
  随机历史汇总、固定区间分片输出、标准输入交互暂停/恢复和 BioGeoBEARS 官方案例的 5000 条
  随机历史分布级 golden 已实现。
- 已实现 `mf/dp/fdp` imperfect-detection observation model；尚无 R 或 Python 直接绑定，
  新版 RASP 优先通过稳定 CLI 子进程协议接入。

## 架构

核心模型拆成几个明确部分：

- `StateSpace`：稳定的 range-state 顺序，底层用 bitset 表示。
- `ModelConfig`：一份配置同时包含沿枝参数和节点分裂配置，并生成 Q 与
  cladogenesis scenario table。
- `DecAnageneticModel`：当前沿分支转移配置，负责由 `d/e` 构造 Q 矩阵。
- `CladogenesisConfig`：`y/s/v/j` 权重和 `mx01y/s/v/j` daughter-size
  最大熵约束。
- `LikelihoodEngine`：所有 preset 共用的 pruning、祖先范围 posterior 和
  split scenario posterior 入口。
- `Optimizer`：在固定参数 likelihood 之上拟合 `d/e`，当前 MVP 使用 log-rate
  空间的 Nelder-Mead；DEC+J 再增加有界 `j` 坐标。

兼容入口 `run_fixed_dec()`、`run_fixed_dec_j()` 仍保留，但内部都委托给同一个
`LikelihoodEngine`。优化循环会复用已经转换好的 tip likelihood，不在每次参数
评估时重复解析 tip ranges。

## 状态顺序

range states 先按范围大小排序，再按区域索引的字典序排序。如果包含
null range，则 null range 固定为 state 0。

两个区域且不包含 null range 时：

| Index | Bits | Range |
| --- | --- | --- |
| 0 | `0b01` | A |
| 1 | `0b10` | B |
| 2 | `0b11` | A+B |

## DEC Q 规则

对每个非空 source range：

- 如果目标 range 仍被允许，新增一个当前缺失区域，转移率为
  `d * source_range_size`。当前 BioGeoBEARS golden 使用这种解释：
  当前 range 中任一已占区域都可以作为扩散来源。
- 如果目标 range 仍被允许，失去一个当前存在区域，转移率为 `e`。
- 对角线元素等于该行所有流出率之和的负数。

在这个 MVP 中，如果包含 null range，则暂时把它视为 absorbing state。
单区域状态可以通过局部灭绝转移到 null range，但 null range 不会重新扩散。

## 沿分支传播

第一版使用 uniformization 计算：

```text
v_bottom = exp(Q * branch_length) * v_top
```

这里的 Q 是行生成矩阵，非对角线表示 `source -> target` 的转移率。
`v_top` 是子节点或 tip 侧的 conditional likelihood 向量，传播结果
`v_bottom` 是该分支底部、也就是祖先侧状态对应的数据 likelihood。

选择 uniformization 的原因：

- 直接支持稀疏 Q 矩阵和矩阵向量乘。
- 不需要先构造每条分支完整的 `P = exp(Q*t)`。
- 对小模型足够稳定，也能自然扩展到较大状态空间。

后续如果 benchmark 显示需要，可以再加入 dense matrix exponential、Krylov
`expmv` 或 BLAS 后端作为可选 fast path。

## Pruning likelihood

第一版实现固定 Q 矩阵的 postorder pruning：

```text
tip likelihoods
  -> 沿每条分支传播到祖先侧
  -> 子节点 likelihood 逐状态相乘
  -> 节点缩放，累计 log scale
  -> root prior 加权求和
```

固定 Q pruning 保留为通用 Mk/CTMC 路径。DEC 路径使用单独的
cladogenesis pruning：每个二叉内部节点先把左右子树 likelihood 沿分支
传播到节点处，再用 sparse split scenario table 合并成祖先状态 likelihood。

root prior 先支持三种形式：

- `Equal`：归一化均匀先验，每个状态权重为 `1 / num_states`。
- `Flat`：非归一化平坦权重，每个状态权重为 `1`，用于锁定当前
  BioGeoBEARS golden 的常数项约定。
- `Given`：用户指定权重，不自动归一化。

## DEC cladogenesis

第一版实现 DEC 风格的 sparse split scenario table。每条 scenario 记录：

```text
ancestor_state -> (left_state, right_state, weight)
```

默认 DEC preset 使用 `mx01y=mx01s=mx01v=mx01j=0.0001`，因此表现为：

- null range 没有分裂情景。
- 单区域祖先 `A` 只允许 `(A, A)`。
- 多区域祖先允许 subset sympatry：
  - `(ancestor, singleton)`
  - `(singleton, ancestor)`
- 多区域祖先允许 ordered vicariance：
  - 对每个 singleton 子集 `S`，加入 `(S, ancestor - S)` 和
    `(ancestor - S, S)`。
  - 这匹配 BioGeoBEARS 默认 DEC 的 `mx01v=0.0001` 语义：vicariance 中较小
    daughter range 的大小为 1；因此四区域祖先范围不包含 `2+2` 平衡分裂。
- 同一祖先状态下的原始 scenario weight 最后统一归一化。
- DEC+J preset 会加入 singleton founder-event scenario，并采用 BioGeoBEARS 默认的 linked
  weight 解释：`y=s=v=(3-j)/3`，`founder_event=j`。

非默认 `mx01*` 不再使用 `Singleton/Any` 两档开关。框架先为每个祖先范围大小
生成 BioGeoBEARS 风格的最大熵 daughter-size 概率，再分别乘到 `y/s/v/j`
scenario。这样 `mx01v=0.5` 会自然加入平衡 vicariance，较大的 `mx01y` 会加入
widespread range-copying，较大的 `mx01j` 也允许完全位于祖先范围之外的多区域
founder daughter。

`DecCladogeneticModel` 只是 `CladogenesisConfig::preset_dec()` 的兼容包装；正式
配置入口是 `ModelConfig::with_range_size_config()`。

带 cladogenesis 的 pruning 目前要求内部节点为二叉节点。多分叉树需要在
Newick 解析或预处理阶段先处理成二叉树。

## 固定参数 DEC 入口

`run_fixed_dec()` 是当前的端到端固定参数入口，数据流为：

```text
Tree + StateSpace + TipRange + d/e + root prior
  -> tip ranges 转 one-hot likelihoods
  -> DEC Q(d,e)
  -> DEC cladogenesis table
  -> cladogenesis pruning
  -> PruningResult
```

当前内置手工 golden case：

- 两个区域：A、B。
- 状态空间：A、B、A+B。
- 两 tip 零分支树。
- tip 0 为 A，tip 1 为 B。
- root prior 使用 `Flat`。
- 唯一可行祖先状态为 A+B；A+B 下 `(A, B)` 的分裂情景权重为 `1/6`。
- 因此期望 `lnL = ln(1/6)`。

如果 root prior 使用 `Equal`，还会乘以根状态均匀权重 `1/3`，因此期望
`lnL = ln(1/18)`。

## DEC d/e 优化入口

`optimize_dec_de()` 在 `run_fixed_dec()` 的固定参数 likelihood 之上做二维优化：

```text
Tree + StateSpace + TipRange + initial d/e + bounds + root prior
  -> 在 ln(d), ln(e) 空间构造 Nelder-Mead simplex
  -> 每个候选点调用 run_fixed_dec()
  -> 返回最佳 d/e、lnL、迭代次数、评估次数和收敛标记
```

优化在 log-rate 空间进行，因此候选 `d/e` 始终为正数。默认边界为
`1e-12 <= d,e <= 10`，这主要用于验证和小型 fixture；后续如果引入更大数据集，
需要再根据树高和状态空间规模调整默认边界与优化策略。

默认只从用户给定的初始 `d/e` 起点运行一次优化。为了降低局部最优或边界行为的
风险，也可以设置 `multi_start_points_per_axis > 1`，在 log-rate 空间加入
`n x n` 个规则网格起点，并从所有起点的结果中选择最高 likelihood。

## 祖先范围概率

当前已经支持 DEC cladogenesis 路径下的内部节点范围概率输出。这里输出的是完整
downpass posterior，而不是单纯把 postorder uppass 的 conditional likelihood
归一化。

计算流程为：

```text
postorder pruning
  -> 保存每个节点的子树 conditional likelihood
  -> 从 root prior 开始做 downpass
  -> 父节点外部 likelihood + 兄弟分支 likelihood + cladogenesis scenario
  -> 沿分支用 exp(Q*t)^T 传到子节点
  -> outside likelihood * subtree likelihood
  -> 每个节点归一化为范围状态概率
```

这些概率的状态含义和 pruning 一致：内部节点表示该节点分裂前的祖先范围状态；
分裂后左右子分支继承范围属于后续 cladogenesis scenario / branch-start state，
目前没有单独输出。

## 分裂情景概率

在 ancestral range posterior 的同一套 downpass 基础上，当前还可以输出每个内部
节点的 cladogenesis scenario posterior：

```text
ancestor_range -> left_branch_start_range + right_branch_start_range
```

每个 scenario 的未归一化权重为：

```text
outside(node, ancestor)
  * scenario_weight
  * left_branch_likelihood(left_start)
  * right_branch_likelihood(right_start)
```

然后在同一个内部节点内归一化为概率。这个输出解释的是节点分裂事件本身；它和
ancestral range posterior 的关系是：同一 ancestor range 下所有 split scenario
概率相加，等于该节点祖先范围 posterior 中对应 ancestor range 的概率。

## 输入格式雏形

当前已经支持从字符串解析最小 Newick 和 tip range 表。

Newick 支持范围：

- UTF-8 tip label 和内部节点标签。
- 单引号标签；两个连续单引号转义为一个字面单引号。
- 平衡方括号注释，包括嵌套注释；注释不进入模型语义。
- 分支长度，例如 `(A:0,B:0);`。
- 嵌套树，例如 `((A:1,B:1):0.5,C:1.5);`。
- 非根枝缺少分支长度时默认报错；核心 API 可显式选择统一填充值。

Newick 当前限制：

- tip label 必须唯一。
- 包含空格或 Newick 分隔符的标签必须使用单引号。
- root edge 明确拒绝，不会被 likelihood 静默忽略。
- 暂不直接读取 NEXUS；多分叉不会静默二叉化。

tip range 表使用空白分隔，第一行为 header：

```text
tip AreaA AreaB
A   1     0
B   0     1
```

规则：

- 第一列必须叫 `tip`。
- 后续列名是区域名，列顺序决定 bitset 区域索引。
- 每个 tip 行只能使用 `0` 或 `1`。
- tip 名称必须存在于 Newick 的 tip labels 中。
- range 表解析后生成 `TipRange`，再交给 `run_fixed_dec()`。

## CLI 使用方式

当前 CLI 提供固定参数 DEC 子命令：

```text
biogeo-cli dec --tree tree.nwk --ranges ranges.tsv --d 0.1 --e 0.2
```

仓库内置的最小手工例子可以这样运行：

```text
cargo run -p biogeo-cli -- dec --tree examples/two_tip/tree.nwk --ranges examples/two_tip/ranges.tsv --d 0.1 --e 0.2
```

也提供 `d/e` 优化子命令：

```text
biogeo-cli dec-optimize --tree tree.nwk --ranges ranges.tsv --include-null-range
```

如果需要多起点优化：

```text
biogeo-cli dec-optimize --tree tree.nwk --ranges ranges.tsv --include-null-range --multi-start-points 3
```

固定参数和优化命令都可以追加内部节点范围概率表：

```text
biogeo-cli dec --tree tree.nwk --ranges ranges.tsv --d 0.1 --e 0.2 --ancestral-probs
biogeo-cli dec-optimize --tree tree.nwk --ranges ranges.tsv --ancestral-probs
```

也可以追加内部节点分裂情景概率表：

```text
biogeo-cli dec --tree tree.nwk --ranges ranges.tsv --d 0.1 --e 0.2 --split-probs
biogeo-cli dec-optimize --tree tree.nwk --ranges ranges.tsv --split-probs
```

可选参数：

- `--max-range-size <n>`：最大 range size；默认等于区域数。
- `--include-null-range`：把 null range 加入状态空间；当前 null range 仍按
  absorbing state 处理。
- `--root-prior <flat|equal>`：根先验；默认是 `flat`。
- `--ancestral-probs`：在 summary 后追加内部节点范围概率 TSV 表。
- `--split-probs`：在 summary 后追加内部节点 cladogenesis split scenario 概率表。
- `--traceback-samples <n>`：固定模型中追加 n 条联合条件历史骨架；默认 0。
- `--bsm-samples <n>`：固定模型中追加 n 条完整 BSM；默认 0。完整 BSM 已包含历史骨架，
  因此不能与 `--traceback-samples` 同时使用。
- `--bsm-output-dir <dir>`：把 BSM 按单条随机历史流式写入新的 `biogeo-bsm-tsv-v1`
  目录；已有目录拒绝覆盖。省略时保留兼容的标准输出模式。
- `--bsm-output-level <legacy|full|compact|summary>`：目录输出表示；默认 legacy。新版 RASP
  建议 compact，仅做分布统计时使用 summary。
- `--bsm-threads <auto|n>`：完整随机历史采样 worker 数；默认读取当前进程可见并行度，
  并由样本数封顶，不固定为 16。
- `--bsm-max-in-flight <n>`：有界并行窗口；必须不小于实际 worker 数，默认是
  `min(sample_count, 2 * workers)`。
- `--bsm-max-events-per-sample <n>`：每条随机历史实际沿枝 `d/e` 事件硬上限；默认无限制，
  预算跨所有分支和时期累计，超限时保留准确 sample index。
- `--bsm-max-events-total <n>`：按 sample index 排序的任务前缀事件总上限；超限样本不写出，
  已完成前缀会提交并可提高上限后续跑。
- `--bsm-memory-budget-mb <n>`：流式模式的完整历史窗口预算；需要单样本事件上限，并会自动
  降低实际 worker/在途数。它不包含 bridge 临时数组、共享缓存和 writer 缓冲，不是 RSS
  硬上限。
- `--bsm-checkpoint-samples <n>`：流式目录每次提交的样本数；默认是
  `min(sample_count, max(1024, max_in_flight))`，单条历史昂贵时可调小。
- `--bsm-shard-samples <n>`：把绝对 sample index 划成固定大小区间，每个完整区间发布为
  不可变八表目录，并生成可重建 manifest；最后一个区间可短于 n。
- `--bsm-resume`：从已有流式目录的最后一个有效检查点续跑；恢复前按检查点记录的八表
  字节长度截断未提交残尾，并校验树、范围、模型、seed、样本总数和事件上限的运行指纹。
- `--bsm-interactive`：从标准输入读取 `pause/resume/status/cancel` 行命令；暂停在进程内保留
  有界窗口，取消仍提交完整前缀并可由 `--bsm-resume` 接管。
- `--seed <u64>`：条件历史或完整 BSM 抽样随机种子；默认 1。
- `--mx01 <x>`：联动设置 `mx01y/s/v/j`，默认 `0.0001`。
- `--mx01y/s/v/j <x>`：覆盖某一事件的 daughter-size 约束。

固定参数、DEC 优化和 DEC+J 优化都接受这些选项。优化命令当前仍只拟合
`d/e[/j]`，`mx01*` 在一次优化中保持固定。
core 的通用参数表优化器已经可以独立释放 `y/s/v/mx01*`；用户可配置的通用 CLI
仍属于后续版本化参数配置工作，不用专用 DEC 命令假装已经覆盖。

输出先采用简单的 tab 分隔 key/value 格式：

```text
model	DEC
lnL	-1.791759469228055
states	3
areas	2
tips	2
max_range_size	2
include_null_range	false
root_prior	flat
d	0.1
e	0.2
```

这个输出格式便于验证脚本直接提取 `lnL`。BioGeoBEARS 输出作为
BioGeoBEARS-like 框架语义 golden；LAGRANGE-ng 只作为独立 LAGRANGE-ng
语义和性能参考，不要求两者 lnL 相同。

如果启用 `--ancestral-probs`，会在 summary 后追加：

```text
ancestral_state_probabilities
node	label	kind	clade	state_index	range_bits	range	probability
2	node_2	root	A+B	2	3	AreaA+AreaB	1.000000000000000
```

Newick 内部节点有标签时 CLI 会保留并输出；无标签时使用 `node_<id>`。
`clade` 是该节点下所有 tip label 排序后的拼接，主要用于跨工具对照。

如果启用 `--split-probs`，会在 summary 后追加：

```text
split_scenario_probabilities
node	label	kind	clade	left_clade	right_clade	ancestor_state_index	ancestor_range_bits	ancestor_range	left_state_index	left_range_bits	left_range	right_state_index	right_range_bits	right_range	scenario_weight	probability
2	node_2	root	A+B	A	B	2	3	AreaA+AreaB	0	1	AreaA	1	2	AreaB	0.166666666666667	1.000000000000000
```

## 验证

每次优化都必须保持这些检查通过：

- Q 矩阵每行之和在浮点误差范围内等于 0。
- 非对角线转移率不能为负。
- 状态顺序稳定。
- 2 区域和 3 区域的小模型要和手工检查矩阵一致。
- 两状态 CTMC 的分支传播要和解析解一致。
- 两 tip 小树的 pruning likelihood 要和解析解一致。
- DEC split scenario table 每个祖先状态的权重和要等于 1。
- 两 tip 小树的 DEC cladogenesis pruning 要和手工枚举结果一致。
- `run_fixed_dec()` 的两区域零分支 golden case 要保持 `ln(1/6)` 和
  `ln(1/18)` 两个 root prior 约定下的结果。
- 从 Newick 字符串和 tip range 表字符串解析出的输入，要能复现同一个
  `ln(1/6)` golden case。
- `--ancestral-probs` 输出的内部节点范围概率要能和 BioGeoBEARS 的
  `ML_marginal_prob_each_state_at_branch_top_AT_node` 按 `clade + range_bits`
  对齐。
- `--split-probs` 输出的同一节点内所有 split scenario 概率和要等于 1；在零分支
  两 tip 手工案例中，唯一可行的 `A+B -> A + B` 分裂概率要等于 1。
- `--split-probs` 还要能和 BioGeoBEARS 内部
  `calc_uppass_scenario_probs_new2()` 重建的 split-event posterior 按
  `clade + left_clade + right_clade + ancestor/left/right range_bits` 对齐。
- `--traceback-samples` 的每条历史必须满足 split daughter、分支起点、分支终点和
  子节点状态严格相接；大量重复抽样的节点与 split 频率必须回归精确 posterior。
- 时间分层 Q 与范围状态约束启用时，历史抽样必须继续复用同一时期传播器、split table
  和 state mask，不能退回均一模型。
- `--bsm-samples` 的实际事件必须在时间上有序、逐状态相接，并与分支/segment 端点一致；
  两状态解析例的条件事件时间和条件事件数、piecewise-Q 的时期事件占比必须落在 Monte
  Carlo 误差阈值内。
- `validation/golden/` 只保存 BioGeoBEARS-like 语义基准；LAGRANGE-ng 的冻结
  输出放在 `validation/reference/`，不参与 Rust 的 BioGeoBEARS golden 判定。
