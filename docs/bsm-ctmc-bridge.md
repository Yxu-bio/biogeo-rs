# BSM 第二阶段：条件 CTMC bridge

## 完成范围

本文将 biogeographic stochastic map 译为“生物地理随机历史”，将 biogeographic
stochastic mapping 译为“生物地理随机历史采样”。当前完整 BSM 由两层组成：

1. 条件历史骨架：联合抽取根状态、cladogenesis split、各分支起始状态和终止状态。
2. 条件沿枝路径：在每条分支的已知起止状态之间，抽取实际发生的 range expansion、
   local extirpation 事件及其时间。

第二层复用拟合时的同一 `OwnedBranchPropagator` 和 Q，不根据端点差异构造捷径，也没有
按 preset、区域数或 fixture 名称设置特殊路径。

## 均质分支算法

给定生成矩阵 `Q`、分支长度 `T`、起始状态 `a` 和终止状态 `b`，取：

```text
lambda = max_i(-Q[i,i])
R = I + Q / lambda
```

`R` 是包含虚拟自转移的离散转移矩阵。条件于端点的虚拟跳数 `N` 具有未归一化质量：

```text
Pr(N=n, X(T)=b | X(0)=a)
  = exp(-lambda*T) * (lambda*T)^n / n! * (R^n)[a,b]
```

实现依次完成：

1. 累加上述质量，直到 Poisson 尾部相对于端点概率小于容差；
2. 从条件分布中抽取虚拟跳数 `N`；
3. 通过终点 one-hot 的 `R` 后向幂，逐步抽取条件状态链；
4. 抽取 `N` 个 `Uniform(0,T)` 时间并排序；
5. 删除状态未变化的虚拟自转移，只保留真实 Q 事件。

这直接抽取端点条件路径，不需要反复模拟整条分支直到碰巧命中终点。BioGeoBEARS 当前
R 实现的 `stochastic_map_branch()` 采用前向等待时间模拟和终点拒绝，并在困难分支上有
`maxtries`/manual history 路径；Rust 不复刻这些重试和兜底细节，只要求目标条件分布
一致。

## 长分支与高事件率的自动细分

单段 uniformization 从 `exp(-lambda*T)` 开始递推；`lambda*T` 很大时该值会先下溢，
而且一条路径的虚拟跳数可能超过单段默认上限。默认传播与 BSM bridge 因此把
`lambda*T > 64` 的同一 Q 时间区间自动拆成至多 65,536 个数值子段。这个上限是显式
失败边界，不会退化成无界循环。

似然传播使用 CTMC 半群性质，依次应用各子段的 `exp(Q*dt)`。条件 bridge 在拆分点 `t`
按下式抽取中间状态 `z`：

```text
Pr(X(t)=z | X(0)=a, X(T)=b)
  proportional to P(t)[a,z] * P(T-t)[z,b]
```

随后递归抽取左右子 bridge。该分解直接来自 Markov 性质，保持整段端点条件路径分布，
不是为了匹配 fixture 而拼接事件。合并时只平移并连接事件时间、累加虚拟跳数；整段的
`endpoint_probability` 仍为 `P(T)[a,b]`。因此一个生物学时期 segment 在输出中仍是
一个 segment，数值子段不会产生新的 `q_index`、时期或事件类型。

`sample_uniformized_bridge_with_options()` 保留原来的单段诊断语义；默认入口和
`sample_uniformized_bridge_adaptive_with_options()` 使用自适应语义。低于阈值时默认
入口直接走原单段代码，固定 seed 的已有结果逐位不变。

稀有条件端点还需要处理浮点尾部：当端点概率较低时，所需相对误差可能小于
`1 - accumulated_poisson` 能表示的最小间隔。累计质量进入 `8 * f64::EPSILON` 后，bridge
改用当前 Poisson 权重和后续单调递减比率计算剩余尾部的几何上界。该判据仍严格上界未累积
端点质量，不是提高 10,000 次循环上限；18 状态有向链、至少 17 次跃迁和约 `3.5e-8`
端点概率的回归覆盖了原来的假不收敛路径。

## 跨时期分支

`segments_by_edge` 为 likelihood 传播按年轻到年老保存。历史抽样先反转为父节点到
子节点的时间顺序，也就是年老到年轻。对于跨越多个时期的分支：

1. 从已知分支终点向前计算每个 segment 边界的后缀 likelihood；
2. 已知当前边界状态 `s` 时，按
   `P_segment[s,z] * suffix_likelihood[z]` 抽取下一边界状态 `z`；
3. 在每个 segment 的两个已知边界状态之间独立抽取条件 bridge；
4. 把 segment 内时间平移为从父节点起算的 `time_from_parent`。

如果时期带 `StateMask`，边界后缀、前向候选质量和 segment 内 Q 都使用同一 mask。零概率
端点、边界状态不合法、segment 总长度不等于分支长度都会返回显式错误。

## 事件语义

DEC-like Q 的每次真实转移只改变一个区域位：

- 增加一个位：`RangeExpansion { area }`，CLI 标记为参数 `d`；
- 删除一个位：`LocalExtirpation { area }`，CLI 标记为参数 `e`。

这里的 local extirpation 是范围内失去一个区域，不等于整条谱系灭绝。事件记录同时保存
分支、segment、Q 索引、父节点起算时间、前后状态和变化区域。

## 公共 API

固定 seed 的批量入口：

```rust
let maps = engine.sample_stochastic_maps_seeded(
    &model,
    &pruning,
    1000,
    20260716,
)?;
```

大样本应使用逐条消费入口。回调只借用当前随机历史；回调返回后该历史立即释放，不会
形成随 `sample_count` 线性增长的 `Vec`：

```rust
engine.try_for_each_stochastic_map_seeded(
    &model,
    &pruning,
    1000,
    20260716,
    |sample_index, history| write_history(sample_index, history),
)?;
```

本机有界并行入口使用按样本索引派生的独立随机流，并始终按样本索引调用 consumer：

```rust
engine.try_for_each_stochastic_map_parallel_seeded(
    &model,
    &pruning,
    1000,
    20260716,
    StochasticMapParallelOptions::new(8, 16).with_limits(
        StochasticMapLimits::new(Some(100_000)),
    ),
    |sample_index, history| write_history(sample_index, history),
)?;
```

并行入口还可接受共享取消令牌和可选截止时间：

```rust
let cancellation = StochasticMapCancellationToken::new();
let pause = StochasticMapPauseToken::new();
let deadline = Instant::now().checked_add(Duration::from_secs(60));
let control = StochasticMapExecutionControl::new(cancellation.clone(), deadline)
    .with_pause_token(pause.clone());

let options = StochasticMapParallelOptions::new(8, 16)
    .with_execution_control(control);
```

其他线程调用 `cancellation.cancel()` 后，所有 worker 会看到同一个原子标志。取消和截止时间
采用协作式检查：窗口开始、每个样本、历史骨架前后、每条分支以及每个生物学时期 segment
前后都会检查。它不会异步破坏正在运行的数值代码，因此最坏响应时间是当前单个 segment 的
条件 bridge 完成时间，而不是整个在途窗口或整批任务。

其他线程调用 `pause.pause()` 后，worker 会在同一批安全点进入等待；`pause.resume()` 用条件
变量唤醒所有等待者。暂停不会销毁在途随机历史、改变 sample index 或重建 RNG，恢复后的
随机历史与未暂停运行逐字相同。暂停期间仍按绝对墙钟检查 deadline；`Ctrl+C`/取消令牌最多
在 50 ms 轮询间隔后唤醒暂停中的 worker，不会因暂停而死锁。

`StochasticMapLimits` 默认不设上限，因此不会改变既有随机历史。设置
`max_anagenetic_events_per_map` 后，预算在一条历史的所有分支和时期之间累计；自适应
bridge 每完成一个数值叶子子段就检查一次，避免先构造完整的超大事件向量再失败。恰好
不超过上限的样本保持相同 indexed RNG 路径和逐字结果。该上限只计实际沿枝 `d/e`
事件；由树拓扑确定数量的 cladogenesis 事件不计入。

当前固定协议为 `indexed-chacha12-v1`：master seed 经固定 SplitMix64 域分离扩展为
ChaCha12 key，sample index 作为 ChaCha stream id。因此相同版本、输入、master seed 和
sample index 的结果不依赖 worker 数、窗口大小或线程完成顺序。协议名称会写入输出元数据；
以后若必须改变随机流派生规则，需要显式升级协议名，不能静默改变既有结果。

需要控制随机数源时：

```rust
let mut sampler = engine.prepare_stochastic_map_sampler(&model, &pruning)?;
let map = sampler.sample_map(&mut rng)?;
let maps = sampler.sample_maps(1000, &mut rng)?;
```

`StochasticMapSampler` 是 `HistorySkeletonSampler` 的非破坏性公共别名。一个
`BiogeographicStochasticMap` 包含 `skeleton` 和逐分支 `BranchHistory`；后者再包含逐
时期 `BranchSegmentHistory` 与 `AnageneticEventSample`。

单条随机历史可以直接生成守恒检查后的统计摘要：

```rust
let summary = map.summarize(&states)?;
```

`BsmSampleSummary` 包含 `d/e` 计数、按范围几何分类的 `y/s/v/j` 计数、按 Q/时期的
事件计数、各状态占据时间和“时期 × 状态”占据时间。该层只读取随机历史内容，不依赖
fixture 名称或参考实现。

## CLI

固定模型命令支持：

```text
biogeo-cli dec \
  --tree tree.nwk \
  --ranges ranges.tsv \
  --d 0.1 \
  --e 0.2 \
  --bsm-samples 100 \
  --bsm-output-dir bsm-run \
  --bsm-threads auto \
  --bsm-max-in-flight 16 \
  --bsm-checkpoint-samples 16 \
  --bsm-shard-samples 1000 \
  --bsm-max-events-per-sample 100000 \
  --bsm-max-events-total 10000000 \
  --bsm-memory-budget-mb 1024 \
  --bsm-time-limit-seconds 3600 \
  --seed 42
```

`--bsm-threads` 接受 `auto` 或正整数；`auto` 使用当前进程可见的并行度，并由样本数封顶，
没有固定 16 线程上限。`--bsm-max-in-flight` 必须不小于实际 worker 数；省略时默认为
`min(sample_count, 2 * workers)`。当前执行器按固定窗口并行采样、按 sample index 顺序消费，
因此内存中的完整随机历史数有明确上界，writer 也不会因线程调度改变表格顺序。
`--bsm-max-events-per-sample` 接受包括 0 在内的非负整数，限制每条随机历史的实际沿枝
事件数；省略时为 `unlimited`。超限错误包含 sample index、上限和已确认至少需要的事件数。
`--bsm-max-events-total` 同样接受非负整数，但约束按 sample index 排序、已经完整写出的任务
前缀。某条历史会使累计事件数超限时，该历史不会交给 writer，前缀会立即提交，元数据状态
写为 `event_limit`，进程退出码为 3。检查点同时记录累计事件数；旧版 v1 检查点在恢复时会从
已提交的 `sample_event_counts.tsv` 重建该值，因此可提高总上限后继续运行。

`--bsm-memory-budget-mb` 是按 1 MiB = 1024² bytes 计算的“完整历史窗口预算”，只用于
`--bsm-output-dir` 流式模式，并要求同时设置 `--bsm-max-events-per-sample`。执行器根据树节点、
分支、时期 segment 和单样本事件上限，保守计算一条完整历史的逻辑保留字节上界，然后使用：

```text
预算可容纳样本数 = floor(memory_budget_bytes / bytes_per_sample_upper_bound)
实际 max_in_flight = min(配置值, 样本数, 预算可容纳样本数)
实际 workers = min(原 worker 数, 实际 max_in_flight)
```

预算连一条历史的上界都容纳不了时，会在创建输出目录前拒绝运行。该限制覆盖等待有序写出的
完整历史对象及其事件向量，但**不是进程 RSS 硬上限**：CTMC bridge/传播的 worker 临时数组、
共享 transition cache、writer 格式化缓冲、allocator 元数据均不计入。元数据会记录预算范围、
排除项、单样本上界和整个窗口上界，便于审计实际规划。
`--bsm-checkpoint-samples` 控制流式目录每次提交的样本数，省略时为
`min(sample_count, max(1024, max_in_flight))`。它只影响提交频率和故障后最多重算的样本数，
不影响随机结果；单条历史特别昂贵时可显式调小。
`--bsm-shard-samples` 接受正整数，并要求同时设置 `--bsm-output-dir`。省略时继续使用原有
单目录八表格式；设置后按绝对 sample index 划分固定区间，每个区间单独保存完整八表和
检查点。分片大小写入运行指纹，恢复时不能更改；最后一个分片可以短于指定大小。
`--bsm-time-limit-seconds` 接受有限的非负秒数，允许小数，省略时不设耗时上限。计时在固定
似然和可选 posterior 完成后、进入 BSM 输出与采样前开始。达到上限后返回准确的下一个
sample index；CLI 不把它当作参数错误，也不附加整页 usage。

`--bsm-interactive` 显式启用跨平台的标准输入控制。运行期间输入一行命令并回车：

- `pause`（或 `p`）：请求进程内暂停；
- `resume`（或 `r`）：恢复 worker；
- `status`（或 `s`）：报告运行/暂停请求状态和已被有序 consumer 接收的样本前缀；
- `cancel`（或 `q`/`quit`）：走现有受控取消路径。

控制消息只写入标准错误，不污染 TSV 或标准输出协议。`pause` 本身不发布 checkpoint，也不
退出进程；内存中仍只保留既定的有界在途窗口。`status` 的完成数可以新于最近一次耐久
checkpoint，受控取消后才会提交该完整前缀。标准输入若在暂停后关闭，CLI 会自动恢复，避免
留下永久等待的进程。交互开关是执行策略，不进入运行指纹，因此恢复任务时可以开启或关闭。

CLI 用 `Ctrl+C` 设置同一取消令牌；取消退出码为 130，耗时上限退出码为 124。暂停状态下
`Ctrl+C` 仍优先取消。对于流式目录，
两种受控停止都会先提交已经完整写入的有序样本前缀，即使尚未达到正常检查点间隔，然后把
`metadata.tsv` 状态分别写为 `cancelled` 或 `time_limit`。随后可用 `--bsm-resume` 恢复。
耗时上限、worker 数、最大在途数、内存预算、任务总事件上限和检查点间隔是执行策略，不改变
随机历史语义，因此恢复时允许调整；树、范围、模型、seed、样本总数和单样本事件上限仍由
运行指纹约束。无输出目录的
兼容模式没有持久检查点，取消后不能从内存结果恢复。

不提供 `--bsm-output-dir` 时，兼容输出段 `biogeographic_stochastic_maps` 包含：

- `bsm_node_states`：每个样本的节点状态；
- `bsm_cladogenetic_splits`：节点分裂及 daughter states；
- `bsm_branch_segments`：分支分段、时期 Q、端点概率和虚拟跳数；
- `bsm_sample_event_counts`：每条随机历史的 `d/e`、`y/s/v/j` 事件计数和总枝长；
- `bsm_sample_period_event_counts`：每条随机历史按 Q/时期汇总的沿枝事件数与占比；
- `bsm_sample_state_occupancy`：每条随机历史各范围状态的占据时间与占比；
- `bsm_sample_period_state_occupancy`：每条随机历史的“时期 × 状态”占据时间；
- `bsm_anagenetic_events`：实际 `d/e/a` 事件、区域、时间和前后状态。核心将 singleton
  range 之间的 `a` 转移记录为一次原子 `range_switching`。

`biogeo-bsm-tsv-v1` 的样本汇总列保持不变。通用参数表结果可先写为
`biogeo-analysis-result-v2`，再由 `model-bsm --analysis-result <dir>` 经过输入、模型身份和
lnL 重放校验后进入同一 sampler/writer；详见 [`analysis-result.md`](analysis-result.md)。
独立 `a` 计数和困难分支诊断已发布在 BSM v2，不会静默改变 v1 表头。

目录输出可用 `--bsm-output-level legacy|full|compact|summary` 选择表示。默认
`legacy` 保持 v1 兼容；`full` v2 保存完整详细行，`compact` v2 用整数 ID 和根目录字典
保存可重建路径，`summary` v2 只保存事件与占据分布。compact/summary 的占据表为稀疏表，
缺失组合表示 0。完整契约见 [`bsm-output-formats.md`](bsm-output-formats.md)。

提供 `--bsm-output-dir` 时，CLI 创建一个不存在的新目录，逐条写出同样的八张表：

- `node_states.tsv`、`cladogenetic_splits.tsv`、`branch_segments.tsv`；
- `sample_event_counts.tsv`、`sample_period_event_counts.tsv`；
- `sample_state_occupancy.tsv`、`sample_period_state_occupancy.tsv`；
- `anagenetic_events.tsv`。

`metadata.tsv` 记录所选格式号、输出级别、是否含路径、稀疏占据规则、完成状态、已提交样本数、运行指纹、模型、
lnL、seed、样本数、RNG 协议、进程可见并行度、实际 worker、最大在途数、检查点间隔、
单样本/任务总事件上限、累计事件数、内存规划、耗时上限、状态空间和主要模型参数。
`checkpoints/` 中的不可变检查点才是恢复时的提交依据；每个 v2 检查点记录已完成样本数、
累计沿枝事件数、运行指纹以及八张表各自的精确字节长度。

一次提交严格按以下顺序进行：先将八张表全部 `flush` 并同步到文件系统，再原子发布检查点。
捕获到采样或写出错误时，writer 丢弃未提交缓冲并把所有表截回最近检查点；进程突然终止时，
磁盘上可能存在检查点之后的残尾，但恢复会在继续采样前统一截断。因而八张表对外暴露的是
同一个崩溃一致的有序样本前缀，而不是分别猜测完成行数。

恢复时用与原运行相同的树、范围、模型、seed、样本总数和事件上限，并增加
`--bsm-resume`。线程数和最大在途数可以改变，因为随机流由绝对 sample index 派生；
影响结果的输入发生变化时，运行指纹检查会在修改表文件之前拒绝恢复。普通模式仍拒绝已有
目标目录，只有显式恢复模式会打开它。所有样本提交后 `metadata.tsv` 才标记为 `complete`。
标准输出保留格式、seed、样本数、RNG 协议、执行参数、恢复标志和目录指针。运行指纹用于
防止误用不同配置续跑，不是密码学文件校验；当前协议不检测相同长度的外部篡改或存储位腐坏，
也尚未提供同一输出目录的跨进程 writer 锁，因此一个目录同一时刻只能由一个进程写入。

显式设置 `--bsm-shard-samples` 时，根目录格式号改为
`biogeo-bsm-sharded-tsv-v1`，布局为：

```text
bsm-run/
  metadata.tsv
  manifest.tsv
  shards/
    shard-00000000000000000000-00000000000000001000/
      <八张 TSV + checkpoints/>
  in-progress/
    shard-00000000000000001000-00000000000000002000/
      <八张 TSV + checkpoints/>
```

`shards/` 中的完整分片是不可变事实来源：发布前必须存在覆盖整个固定区间的检查点，八表
实际长度必须与检查点精确相等；任何额外尾部也会被诊断为损坏，不会静默截断。
`in-progress/` 最多有一个活动分片，其八表可按最近检查点截断并续跑。分片完成后先关闭
writer，再用目录重命名发布到 `shards/`。如果进程在重命名后、更新 manifest 前终止，恢复
会扫描完整分片并重建 manifest；如果在完整检查点后、目录重命名前终止，恢复会直接完成
发布。`manifest.tsv` 因而是便于下游顺序读取的派生索引，不是唯一恢复依据。
Windows 上杀毒/索引过滤器可能让目录重命名短暂返回 code 5；发布只在目标目录仍不存在的
`PermissionDenied` 上做有限退避重试（最多约 0.8 秒），永久权限错误和其他 I/O 错误仍会
显式失败，完整活动分片可在下次恢复时再次发布。
根 `metadata.tsv` 若在状态更新时被截断，恢复会用已同步 manifest 的格式号和运行指纹校验
任务，再从分片事实重写 metadata；反过来 manifest 缺失时也可由有效 metadata 和分片目录
重建。有效 metadata 优先；只有它不可读取或不可解析时才使用 manifest。被选中的身份来源
若含有不匹配的运行指纹，会在触碰分片表之前拒绝恢复。

manifest 记录每个固定区间、样本数、分片及累计沿枝事件数、相对目录和八表字节长度。
各分片内部以及跨分片仍严格按绝对 sample index 排序，同一 seed 的数据行与单目录 writer
逐字节相同；下游可以按 manifest 拼接读取，无需生成一个可能达到数百 GB 的合并文件。
当前实现一次只维护一个活动分片，分片内部继续使用同一有界 worker pool 并行采样。

`--bsm-samples` 与 `--traceback-samples` 互斥，因为完整随机历史已经包含同一联合历史骨架。
相同程序版本、输入和 seed 的 Rust 输出可复现，但不承诺与 R 使用相同 seed 时逐路径
相同。

## 已完成验证

1. 零长度和零速率分支只接受相同端点。
2. 单向两状态 bridge 的事件时间均值与截断指数分布解析解一致。
3. 对称两状态、不同端点 bridge 的跳数均值与条件奇数 Poisson 解析解一致。
4. 强制数值细分后的对称两状态 bridge 仍符合相同的条件奇数 Poisson 跳数分布。
5. `lambda*T=1000` 的 bridge 和 `lambda*T=20000` 的完整 DEC 分支均可抽样；后者虚拟
   跳数超过 10,000，但输出仍只有一个生物学 segment，事件时间、端点和类型连续。
6. 两时期单向模型中，事件落入老时期的频率与分段危险率解析概率一致。
7. 完整随机历史逐节点、split、分支、segment、事件时间和状态链严格相接。
8. 时间分层状态约束下，大量样本的年轻时期边界和事件状态都满足 mask。
9. CLI 一区域案例稳定输出 `AreaA -> null` 的 local extirpation，并由固定 seed 逐字复现。
10. BioGeoBEARS 官方 `BSM_3taxa/M3areas_allowed` 案例使用其 ML 参数
   `d=5.98044354276819, e=1.31300515961732`，以不同 seed 分别生成 5000 条
   BioGeoBEARS 和 Rust 生物地理随机历史。事件总数、`d/e`、`y/s/v/j`、两时期事件数与占比、8 个
   状态及“时期 × 状态”占据时间共 39 项分布检查全部通过；最大均值偏差为 `2.43`
   个 Monte Carlo 标准误，最大经验 CDF 差为 `0.0368`，门限为 `0.04`。
11. 同一 BioGeoBEARS golden 再用两个独立 Rust seed 各抽 5000 条随机历史，39 项门禁仍
   全部通过；三个 seed 的最大均值偏差均不超过 `2.43`，最大 CDF 差均不超过 `0.038`。
12. 5000 条 BioGeoBEARS 随机历史没有触发 manual history，观测最大分支重试数为 2862，
    低于 `maxtries_per_branch=40000`，所以 golden 没有混入其均匀时间兜底路径。
13. 固定输入、seed 和 16 条样本在 1/2/4/8/16 worker 下的八张 TSV 数据表逐字节相同；
    RNG 已另用固定输出向量锁定，consumer 失败也会保留准确 sample index。
14. 每条历史事件上限跨分支累计；恰好等于上限时与无限制样本逐字相同，超限时并行错误
    保留准确 sample index。预算失败目录保持 `incomplete`、记录 `completed_samples`，且
    不写入超限样本的表行。
15. 已覆盖检查点后部分表已刷盘、部分表仍在缓冲的回滚；另模拟三张表含崩溃残尾后，
    以不同 worker 数恢复，最终八张表与一次性完整运行逐字节相同。模型变化会在触碰表文件
    前因运行指纹不一致而被拒绝。
16. 核心并行器在 consumer 完成三个样本后取消时，保留与基线逐字相同的前三条历史，并以
    `sample_index=3` 返回停止原因；到期截止时间在第 0 条样本前稳定停止。
17. CLI 即时取消会发布 0 样本的 `cancelled` 检查点，恢复后的八表与一次性运行逐字相同；
    0 秒上限会发布 `time_limit` 状态，改用新上限恢复后完整结束。
18. Windows release 可执行文件的定向 `CTRL_BREAK_EVENT` 冒烟测试通过同一 `ctrlc` handler
    以退出码 130 停止，并使元数据样本数与受控停止时发布的部分前缀检查点严格相等。当前
    无交互控制台测试环境不能安全定向模拟键盘 `CTRL_C_EVENT`，因此该项仍由 handler 接线、
    token 回归和人工终端操作共同覆盖。
19. 任务总事件上限在 1/4/16 worker 下停止于同一个 sample index，并保留同一个有序前缀；
    流式目录提交准确累计事件数，提高上限恢复后八表与无限制基线逐字节一致。v1 检查点缺失的
    累计事件数可从已提交事件计数表迁移。
20. 核心以内存预算把 8 worker/8 在途样本收缩为可容纳的 3/3，并保持历史逐条一致；CLI
    1 MiB 预算端到端测试确认实际 worker/窗口降低、规划上界不超过预算，且八张 TSV 与无预算
    基线逐字节一致。
21. 10 条随机历史按 `4/4/2` 固定区间分片后，按 manifest 拼接的八表与单目录输出逐字节
    相同；在第二个分片 sample 6 受控停止后改用不同 worker 数恢复，最终目录和 manifest
    与一次性分片运行一致。另覆盖活动目录尚未产生首个 checkpoint 时的安全重建、完整
    checkpoint 尚未重命名的恢复窗口、根 metadata 截断自愈、manifest 缺失重建、分片大小
    指纹不匹配的提前拒绝，以及已发布表存在额外尾部时不改写原文件的损坏诊断。
22. 核心在采样开始前暂停 4-worker/8-in-flight 任务，100 ms 内没有返回；恢复后的 16 条
    随机历史与未暂停基线逐字相同。暂停状态下取消和已到期 deadline 均立即返回对应原因。
    CLI 命令处理覆盖 pause/status/resume/cancel、未知命令和暂停后输入 EOF 自动恢复。Windows
    release 真实进程在分片任务中稳定暂停于 0，恢复后推进到 2866，再次暂停后取消，以退出码
    130 发布 `cancelled` 前缀；随后关闭交互、改为 1 worker 的 0 秒恢复停在同一 sample index。
    EOF 自动恢复实测还暴露并保留了一次 Windows 分片目录重命名瞬时 code 5；完整 checkpoint
    下次恢复可正常发布，当前 writer 已对这一特定瞬时错误加入有限重试。修复后的最新 release
    从暂停状态关闭 stdin 后自动恢复，运行到 1.5 秒 deadline，以退出码 124 在 sample 12000
    提交 `time_limit` 前缀，没有再次出现发布错误。
23. Ponerinae 1534-tip、7-area、120-state、7-stratum 真实输入已抽取 100 条 Rust 历史；
    12528 个正的“时期 × 状态”占据行全部满足对应 mask，总枝长逐条守恒。BioGeoBEARS
    单条参考在 22 个分支达到 40000 次尝试后启用 manual fallback，造成 24 次禁用状态转移
    和 `11.0676` 时间单位的禁用状态占据；该 fallback 被诊断输出明确标记，不作为 Rust
    应复制的模型语义。
24. DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE、BAYAREALIKE+J 在共同的 4-tip、
    3-area 数据上各抽取 20,000 条完整历史，共 120,000 条。逐节点状态、逐分裂场景和逐节点
    `y/s/v/j` 频率均通过相对各自精确后验的六标准误门禁；不属于 preset 的事件保持严格为零，
    每条历史的事件分类和状态占据总时间同时通过一致性检查。该门禁与六 preset 已有的
    BioGeoBEARS 精确 posterior golden 组成两段证据链，不要求两个随机数实现逐条路径相同。

分布门禁入口：

```powershell
# 使用冻结的 BioGeoBEARS 5000 条随机历史 golden，重新抽取独立 Rust 样本并比较
powershell -ExecutionPolicy Bypass -File validation/checks/check-bsm-distributions.ps1

# 六 preset 各 20000 条完整历史，对照各自精确节点和分裂后验
cargo test -p biogeo-core --test six_preset_bsm_distribution --release -- --nocapture

# 用 1000 条/分片运行同一 39 项官方分布门禁
powershell -ExecutionPolicy Bypass -File validation/checks/check-bsm-distributions.ps1 `
  -RustShardSamples 1000

# 显式重建耗时的 BioGeoBEARS golden
powershell -ExecutionPolicy Bypass -File validation/checks/check-bsm-distributions.ps1 -RefreshBioGeoBEARS

# 本机 1/2/4/8/16 worker 扩展曲线与跨线程八表指纹检查
powershell -ExecutionPolicy Bypass -File validation/benchmarks/benchmark-bsm-parallel.ps1
powershell -ExecutionPolicy Bypass -File validation/benchmarks/benchmark-bsm-parallel.ps1 -Workload conifer-197tip
```

门禁同时约束均值的 Monte Carlo 标准误和经验 CDF 最大差；时期占比另用事件总数加权的
聚合比例检查。两端随机算法不同，因此设计上不比较相同 seed 的逐条路径。流式目录模式
再次运行同一 5000 对 5000 门禁，39 项仍全部通过。确定性有界并行与输出标签缓存完成后，
本机 16 worker、最大在途 32 的 Rust 固定似然、5000 条采样和八表写出在加入耐久检查点前
为 `0.398` 秒；加入检查点后的历史五次热运行中位数为 `0.688` 秒。协作式取消检查接入后，
同配置五次热运行中位数为 `0.756` 秒，范围 `0.647-0.835` 秒，相对冻结的 BioGeoBEARS
`550.62` 秒约 `728x`。显式每 32 条同步一次会增至约 `4.596` 秒，因此默认值在
故障重算量和八文件同步成本之间取平衡。扩展基准中，三物种轻负载
10,000 条从单 worker 中位数 `1.180` 秒降至 16 worker 的 `0.855` 秒（`1.38x`）；
197-tip、41-state 的复杂输出负载 100 条从 `1.383` 秒降至 `0.482` 秒（`2.87x`）。两组
各 15 次运行的八张数据表指纹分别完全一致。轻负载和约 104 MB/100 条的复杂输出都已明显
受串行格式化与磁盘写入限制，因此不能把 16 worker 理解为应有 16 倍端到端加速。

加入任务总事件计数、完整历史窗口预算和精确 Vec 容量后，2026-07-16 再次对同一 5000 条
负载做 5 次热运行，16 worker 中位数为 `0.909` 秒，范围 `0.827-0.962` 秒，约为冻结
BioGeoBEARS 的 `606x`。临时关闭容量归一化的受控 A/B 中位数为 `0.903` 秒，仅快约 0.7%，
说明与早期 `0.756` 秒记录的差异主要不能归因于内存预算实现；当前负载仍以串行格式化、磁盘
和本机调度波动为主。两组运行的八表指纹均为
`B7FB2F5AA8EBE175C7F681C78C9AD213E49852CAD5BB6936BF585AAABE79310F`。

固定区间 writer 完成后，同一 release、seed 和 5000 条官方负载做 6 轮单目录/分片交替热
运行：单目录中位数 `0.756` 秒（范围 `0.676-1.170`），`--bsm-shard-samples 1000` 的
5 分片中位数 `0.946` 秒（范围 `0.898-1.207`），本机端到端开销约 25%。该成本来自重复
创建、同步和发布八表目录，换取固定文件规模和分片级恢复；按 `0.946` 秒计算仍约为冻结
BioGeoBEARS `550.62` 秒的 `582x`。单目录和 5 分片各自运行官方 5000 对 5000 分布门禁，
39 项均全部通过。

2026-07-23 对当前 release 再次独立重跑：官方 5000 条单目录在 1/2/4/8/16 worker 下
中位数为 `1.268/1.025/0.948/0.991/0.979` 秒，分别约为 BioGeoBEARS 的
`434x/537x/581x/556x/562x`。10,000 条轻负载的 1→16 worker 扩展为 `1.39x`；
197-tip 复杂负载 100 条为 `1.77x`。本轮所有线程档位仍生成完全相同的八表指纹。不同轮次
绝对耗时受 Windows 文件缓存、防病毒扫描和后台负载影响，因此对外应给出测量日期、重复次数
和中位数，不把历史最快值当作稳定下界。

## 尚需验证和工程化

- 兼容批量 API 和无 `--bsm-output-dir` 的标准输出模式仍会保留全部随机历史；大样本应使用
  已实现的逐条消费 API 或版本化目录 writer。当前本机执行器已有确定性有界并行和
  检查点续跑、固定区间分片 writer、协作式取消、耗时上限、任务总事件预算和完整历史窗口
  预算，并已实现标准输入驱动的进程内暂停/恢复；尚未实现 Linux/调度器资源探测。
- 默认传播和 bridge 已自动细分超长/高速率区间，并对数值子段数设置 65,536 的显式上限；
  每条随机历史和任务前缀的沿枝事件硬上限均已贯通 API/CLI。完整历史窗口预算已实现，
  但它有意不宣称限制 worker 数值临时量或进程 RSS；后续若需要 RSS 级约束，应由进程/容器/
  作业调度器提供外层硬限制。
- `biogeo-bsm-tsv-v1` 继续锁定；v2 已提供 full/compact/summary、不可变 ID 字典、困难分支
  诊断、状态约束写出前审计，并把输出级别纳入恢复指纹。
