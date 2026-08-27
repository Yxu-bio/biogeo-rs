# DEC 性能基准

## 测试环境

本页记录 2026-07-23 在当前 Windows PC 上对 release 构建的实测结果：

- 当前进程可用并行度：16；
- Rust：1.95.0，`x86_64-pc-windows-msvc`；
- R：4.5.0；
- BioGeoBEARS：1.1.3，来自项目隔离的 `validation/r-lib/R-4.5`；
- 固定 likelihood 和参数优化均使用相同树、范围、状态空间、null range、根先验和
  cladogenesis 参数，并以 lnL 对齐作为 benchmark 通过条件。

这不是跨机器排名。防病毒扫描、CPU 电源策略和其他进程会影响短任务，因此同时报告均值和
中位数。

## 固定 DEC likelihood

`benchmark-dec-stress.ps1` 对 Rust 的每次测量都启动一个新 CLI 进程，包含输入解析和模型构建；
BioGeoBEARS 则在同一已加载、已预热的 R 会话内重复计时，不重复计算 R 和 package 启动成本。
因此该口径偏向 BioGeoBEARS。

| 案例 | Rust release | BioGeoBEARS 热会话 | 加速 | lnL 绝对差 |
|---|---:|---:|---:|---:|
| 8 区域，32 tips，`max=5`，219 状态，`mx01=0.0001` | 均值 0.027884 s；中位数 0.027624 s | 均值/中位数 1.450000 s | 均值 52.00x；中位数 52.49x | 7.30e-7 |
| 8 区域，128 tips，`max=5`，219 状态，`mx01=0.0001` | 均值 0.028369 s；中位数 0.029225 s | 均值 5.303333 s；中位数 5.320000 s | 均值 186.94x；中位数 182.04x | 4.31e-6 |
| 同一输入，复杂拆分权重 `mx01=0.5` | 均值 0.044049 s；中位数 0.037142 s | 均值 6.080000 s；中位数 6.030000 s | 均值 138.03x；中位数 162.35x | 1.91e-6 |

128-tip 默认组 Rust 7 次结果为 0.026282–0.029773 s。复杂权重组有两个明显较慢的系统噪声
样本，所以其中位数比均值更能表示典型运行时间。三组 lnL 相对差均低于 `3.4e-9`。

## d/e 参数优化

8 区域、32 tips、`max_range_size=5`、219 状态的完整优化结果：

| 工具 | 优化器 | 时间 | 评估 | 最优 lnL |
|---|---|---:|---:|---:|
| Rust | log-rate Nelder-Mead | 均值 1.158489 s；中位数 1.104484 s | 222 | -172.0898676590 |
| BioGeoBEARS | `optim/L-BFGS-B` | 325.530000 s | 23 objective + 23 gradient | -172.0898617296 |

端到端优化加速为均值 **281.00x**、中位数 **294.73x**。两边都报告收敛，最优 lnL
绝对差为 `5.93e-6`，`d/e` 绝对差分别为 `3.19e-5` 和 `1.40e-4`。

不同优化器的“评估次数”不可直接相除：BioGeoBEARS 数值梯度内部还有未进入
`counts["function"]` 的 likelihood 调用。因此 281x 表示用户实际等待时间差，固定 likelihood
的 138–187x 更接近计算核心和数据结构的综合差异。

## 1000-tip 压力

12 区域、1000 tips、`max_range_size=5`、1586 状态的固定 DEC：

- Rust release CLI 五次均值 `0.639512 s`；
- 统一 `analysis-run` 端到端 `0.668341 s`；
- Windows 实测进程 working-set 高水位 `71,589,888` 字节，约 68.27 MiB；
- 命令 CPU 时间 `0.640625 s`，平均逻辑核使用量 `0.9585`；
- lnL 为 `-10410.5131201341`。

该结果显示当前单模型 likelihood 主线接近单核执行。BioGeoBEARS 同规模运行此前 30 分钟仍未
返回一次 timing，因此本页不写未经完成测量的精确加速倍数。类似数据 2–3 小时的用户经验只能
说明可能的量级，不能代替可复现 benchmark。

## 生物地理随机历史（BSM）

BSM 使用 BioGeoBEARS 官方 `examples/BSM_3taxa/M3areas_allowed` 案例和同一组 ML 参数。
两端各 5000 条随机历史不要求相同 seed 产生相同路径，而是比较事件总数、事件类型、时期占比、
状态占据时间及“时期 × 状态”占据时间等 39 项分布；当前全部通过。

BioGeoBEARS 1.1.3 `runBSM` 冻结实测为 `550.62 s`。2026-07-23 当前 Rust release：

| Rust BSM worker | 5000 条中位数 | 相对 BioGeoBEARS |
|---:|---:|---:|
| 1 | 1.268341 s | 434.13x |
| 2 | 1.024887 s | 537.25x |
| 4 | 0.947639 s | 581.05x |
| 8 | 0.990876 s | 555.69x |
| 16 | 0.979016 s | 562.42x |

Rust 时间包含固定 likelihood、端点条件随机历史采样以及八张版本化 TSV 表写出。BioGeoBEARS
时间来自项目隔离环境中的采样批次。即使只使用 1 个 Rust worker，观测加速仍约 434x，
因此主要提升来自算法、数据结构和减少 R 对象开销，而不是简单使用更多 CPU。

并行扩展受串行格式化、磁盘同步和短任务固定成本限制：

- 官方三类群 10,000 条：1 worker `2.0858 s`，16 worker `1.5008 s`，约 1.39x；
- 197-tip、41 状态复杂负载 100 条：1 worker `2.1876 s`，16 worker `1.2364 s`，约 1.77x。

每一档 1/2/4/8/16 worker、每档三次运行的八张数据表指纹完全一致，说明并行调度不改变
sample-index RNG 结果。这里没有宣称 16 线程能获得线性加速；当前下一性能瓶颈主要是 writer，
更大服务器任务应优先使用固定分片、批量同步和有界在途窗口。

## Ponerinae 官方分层 DEC

2026-08-10 使用 1534-tip short-name MCC 树、7 区域 LAGRANGE `.data`、论文的 7 个
时间边界和 adjacency block。论文脚本不是把 adjacency 交给 BioGeoBEARS 的 all-pairs
检查，而是用它生成 `lists_of_states_lists_0based`：多区域范围中的每个区域至少与
范围内另一区域相邻。Rust `edge-covered` 导入生成完全相同的 7 份显式状态表，
`max_range_size=5`、包含 null range，逐时期状态数为
`36,36,27,20,24,20,38`；全局 master state space 为 120。基础 DEC 对照不使用
dispersal multiplier，它属于另行的 `+W` 模型实验。

固定 `d=e=0.01`、`mx01*=0.0001`：

| 工具 | 时间口径 | 时间 | lnL |
|---|---|---:|---:|
| Rust release | 3 次独立 CLI 完整进程均值 | 0.063207 s | -3279.174634278399 |
| BioGeoBEARS 1.1.3 | 已加载 R 会话内的 engine 评估 | 26.130000 s | -3279.174626549246 |
| BioGeoBEARS 1.1.3 | Rscript 完整进程 | 116.457417 s | 同上 |

lnL 绝对差为 `7.73e-6`。BioGeoBEARS engine/Rust 完整进程比为约 **413x**，
两个完整进程比为约 **1842x**。后者包含 R 包加载和分层树准备，不应解读为
纯 likelihood kernel 加速。

从相同 `d=e=0.01` 起点优化 d/e，参数边界为 `1e-12..10`：

| 工具 | 优化器 | 完整时间 | 报告评估数 | d | e | 最优 lnL |
|---|---|---:|---:|---:|---:|---:|
| Rust | log-rate Nelder-Mead | 3.833808 s | 126 | 0.027730762 | 0.021323517 | -3049.873438616853 |
| BioGeoBEARS | `optimx/bobyqa` | 1197.487132 s | 67 | 0.027721661 | 0.021311827 | -3049.873457051032 |

两边均收敛，最优 lnL 差 `1.84e-5`，d/e 差分别为 `9.10e-6` 和 `1.17e-5`。
直接配对的这次端到端时间比为 **312x**。但在 BGB 长时间满载后另行连续运行 5 次
Rust，时间为 `5.80–6.74 s`，中位数 `6.24 s`，对应更保守的 **192x**（范围
178–206x）。因此本机该真实优化负载应报告为约 **190x 以上**，312x 保留为直接配对观测，
不作为唯一典型值。

按 5 次 Rust 均值和各自报告评估数粗略归一化，BGB engine 每次评估约 16.83 s，
Rust 完整进程每次约 0.0503 s，比值约 **334x**。不同优化器的评估并非严格等价，
所以端到端比才是用户等待时间对照，归一化比只是评估成本的辅助指标。

使用 `optim/L-BFGS-B` 的首次 BGB 长跑在约 31.7 分钟后因数值差分探测到非有限 fn 失败。
基准因此改用官方分层模板的 `on_NaN_error=-1e50`、`optimx/bobyqa` 和 speedup 配置。
这是 BioGeoBEARS 优化器稳健性处理，没有改动有效参数点的 likelihood；Rust 最优点另行固定
交叉评估，两端 lnL 差 `4.62e-5`。

### Ponerinae 生物地理随机历史 pilot

使用上面的 BioGeoBEARS 最优参数 `d=0.0277216607`、`e=0.0213118267`，并保持同一棵树、
120 个 master states、7 个时期状态约束和 DEC split 配置：

| 工具 | 工作量 | 时间 | 备注 |
|---|---:|---:|---|
| BioGeoBEARS 1.1.3 | 1 条 | setup 145.94 s；采样 68.35 s；完整进程 219.48 s | 1 worker |
| Rust release | 1 条 | 完整进程 0.574 s | 1 worker，含八表写出 |
| Rust release | 10 条 | 1 worker 1.718 s；10 workers 1.043 s | 两次八表逐字节相同 |
| Rust release legacy v1 | 100 条 | 10.848 s | 16 workers，4 个 25 条分片，575,694,792 bytes |
| Rust release summary v2 | 100 条 | 3.065 s | 同配置、seed 和分片，705,641 bytes |

按单条冷启动完整进程计算，Rust 约快 **382x**。按 BioGeoBEARS 已完成 setup 后的采样时间
与 Rust 100 条完整任务的平均时间比较，观测吞吐差约 **630x**；该口径仍偏向
BioGeoBEARS，因为 Rust 时间还包含一次 setup 和 549.0 MiB 的 TSV 写出。

Rust 100 条历史的沿枝事件均值为 `1034.44`（979--1086），每条均有 1533 个真实节点
分裂事件。BioGeoBEARS pilot 的 1001 个沿枝事件以及 `d/e/y/s/v` 分项均落在 Rust 100 条
样本的观测范围内，7 个时期事件数也全部落在范围内。但该 BioGeoBEARS 历史有 22 条分支
在 40000 次尝试后进入 manual fallback，产生 24 次禁用状态转移，并在时期禁止的状态中
累计占据 `11.0676` 时间单位。Rust 对 100 条历史的 84000 个“样本 × 时期 × 状态”汇总行
检查为 0 个禁用状态正占据。

因此 Ponerinae pilot 用于真实负载性能和风险诊断，不作为分布 golden，也不要求 Rust 复制
BioGeoBEARS 的违规 fallback。科学分布对齐仍由上面的官方无-fallback 5000 对 5000 案例
负责。summary v2 与 legacy v1 逐样本的事件分项、时期事件、非零状态占据和非零时期状态
占据完全相同，且 100 条历史的禁止状态诊断均为 0。实测 summary v2 比 legacy v1 约快
`3.54x`，磁盘字节减少 `99.88%`；它不保留可重建路径，需要具体历史时应使用 compact v2。

2026-08-21 又以同一 Ponerinae 数据完成 `analysis-workflow` 中断恢复验收。d/e 优化加事件预算
中断的首次工作流为 `4.533 s`；恢复剩余 8 条 compact 分片历史并深度检查为 `1.382 s`；从
既有分析结果一次性生成相同 10 条的基线为 `0.867 s`。恢复结果与一次性基线共 35 个文件
逐字节一致，最终工作流两个权威子目录共 54 个文件、4,667,202 bytes。这里的时间用于工作流
工程验收，不与 BioGeoBEARS 采样时间重新计算加速比。

## 复现

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-stress.ps1 `
  -Areas 8 -Tips 32 -MaxRangeSize 5 -Mx01 0.0001 `
  -RustRepeats 7 -BioGeoBEARSRepeats 3

powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-stress.ps1 `
  -Areas 8 -Tips 128 -MaxRangeSize 5 -Mx01 0.0001 `
  -RustRepeats 7 -BioGeoBEARSRepeats 3

powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-stress.ps1 `
  -Areas 8 -Tips 128 -MaxRangeSize 5 -Mx01 0.5 `
  -RustRepeats 7 -BioGeoBEARSRepeats 3

powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-optimization.ps1 `
  -Tree validation/benchmark-runs/dec-stress-8a-32t-m5-mx0p0001/tree.nwk `
  -Ranges validation/benchmark-runs/dec-stress-8a-32t-m5-mx0p0001/ranges.tsv `
  -MaxRangeSize 5 -Mx01 0.0001 -RustRepeats 3 -BioGeoBEARSRepeats 1

powershell -ExecutionPolicy Bypass -File validation/benchmark-ponerinae-dec.ps1 `
  -DatasetDir E:\RASP\examples\phase1_reference_data\Dore_2025_Ponerinae `
  -Mode evaluate -RustRepeats 3 -BioGeoBEARSRepeats 1

powershell -ExecutionPolicy Bypass -File validation/benchmark-ponerinae-dec.ps1 `
  -DatasetDir E:\RASP\examples\phase1_reference_data\Dore_2025_Ponerinae `
  -Mode optimize -RustRepeats 1 -BioGeoBEARSRepeats 1

powershell -ExecutionPolicy Bypass -File validation/benchmark-bsm-parallel.ps1 `
  -Workload official-3taxon -SampleCount 5000

powershell -ExecutionPolicy Bypass -File validation/benchmark-bsm-parallel.ps1 `
  -Workload conifer-197tip
```

原始 timing 和摘要写入 `validation/benchmark-runs/`。这些运行产物被忽略，不充当科学
golden；语义正确性仍由 BioGeoBEARS fixture 和外部集成测试负责。

## 超大型树支持

超大型分析的主要负载不是单独由 tip 数决定，而近似由 `节点数 × 状态数` 决定。状态数又随
区域数和 `max_range_size` 组合增长。因此，1000-tip、1586-state 的分析可能比
10000-tip、93-state 的分析更难。

当前 PC 上的 release 固定 DEC 实测如下。10,000-tip 和 100,000-tip 案例均为平衡二叉树、
8 区域、`max_range_size=3`、包含 null range，共 93 个状态；它们关闭祖先概率和 split
概率，只测端到端固定似然。

| 案例 | 节点数 | 状态数 | 端到端时间 | working-set 高水位 |
|---|---:|---:|---:|---:|
| 1000 tips，12 区域，`max=5` | 1999 | 1586 | 0.668 s | 68.27 MiB |
| 10000 tips，8 区域，`max=3` | 19999 | 93 | 0.421 s | 46.2 MiB |
| 100000 tips，8 区域，`max=3` | 199999 | 93 | 2.139 s | 412.23 MiB |
| 1000000 tips，5 区域，`max=5` | 1999999 | 32 | 10.761 s | 1.733 GiB |

100,000-tip 输入曾因每行范围记录都线性扫描全部 tip 而退化为 `O(T^2)`。将树标签预建为
哈希索引后，`validate-inputs` 从 22.61 s 降至 1.224 s，固定 DEC 从约 22.74 s 降至
约 2.14--2.24 s，lnL 保持 `-756827.5277048699`。当前 `analysis-plan` 将该案例标为
`moderate` 风险，报告 148,799,256 字节核心数值载荷参考值，并明确提示该数字尚未包含
分配器、树结构和临时工作区。

1,000,000-tip 案例为平衡树、5 区域、`max_range_size=5`、包含 null range 的完整 32-state
空间。`analysis-plan` 报告 511,999,744 字节 pruning 数值载荷并标为 `high` 风险。真实
release 固定 DEC 在 10.761 s 内完成，峰值 working set 为 1.733 GiB，CPU 时间为
10.641 s，`lnL=-6007632.078196587041020`。测试时关闭祖先概率、split 概率和 BSM。

这里不能写 BioGeoBEARS 的 100,000-tip 精确加速倍数，因为尚未完成同规格 R 端运行。
现有 1000-tip、1586-state BioGeoBEARS 任务在 30 分钟内未返回一次 timing，也不能替代
同规格对照。Rust 在较小、已对齐案例上的显著优势和较低对象开销说明扩展性更好，但
100,000-tip 的跨工具倍数仍属于未测量项。

当前边界：

- 固定 likelihood 已验证平衡 100,000-tip、93-state 案例；这不等于任意状态空间均可承载。
- 祖先后验会额外保存 outside 和 branch likelihood，内存明显高于固定 likelihood。
- 生物地理随机历史还包含按边和状态的转移行缓存，每个样本输出量至少随节点数线性增长。
  v1 八表仍重复写 clade 文本；新 `compact` v2 已改用 node/edge/state ID 和一次性字典，
  `summary` v2 可完全省略逐路径明细。超大树的完整路径数量本身仍至少随节点数线性增长。
- Newick 解析和少数树遍历仍使用递归。平衡超大树深度较浅，但极端梳状深树仍有调用栈风险。
- 区域集合当前使用 `u64`，输入硬上限为 64 个区域；实际通常会更早受组合状态数限制。

下一轮超大规模工作的优先级是：迭代式 Newick 解析和后序遍历、likelihood-only
释放子节点工作向量的低内存模式，以及在真实大树上继续量化 compact/summary v2 的
writer 吞吐与最终目录规模。

2026-08-11 的官方三末端、5000 条随机历史单轮同机对照中，legacy v1 为 `1.513 s / 21.44 MB`，
summary v2 为 `0.836 s / 3.17 MB`，写出体积减少约 `85.2%`；summary v2 仍通过全部 39 项
BioGeoBEARS 分布门禁。该单轮结果说明格式开销，不替代多轮线程扩展中位数。

## 100-tip 区域数扩展

为单独观察状态空间增长，固定同一棵 100-tip 平衡树、`max_range_size=5`、包含 null range、
单区域 tip observations 和 DEC `d=e=0.01`，仅改变区域数。release CLI 每档独立启动 7 次，
表中为端到端固定 likelihood 中位数；内存来自同配置 `analysis-run` 的 Windows 进程高水位。

| 区域数 | 状态数 | Q 非零转移 | split scenarios | 中位时间 | 时间范围 | 峰值内存 |
|---:|---:|---:|---:|---:|---:|---:|
| 5 | 32 | 155 | 285 | 0.0076 s | 0.0070--0.0212 s | 5.18 MiB |
| 10 | 638 | 5110 | 10120 | 0.0247 s | 0.0228--0.0285 s | 8.24 MiB |
| 20 | 21700 | 201420 | 402440 | 1.6418 s | 1.5781--1.6971 s | 112.16 MiB |
| 30 | 174437 | 1670430 | 3339960 | 17.6232 s | 13.1826--21.2855 s | 863.50 MiB |

10 到 20 区域时，状态数约增长 34 倍，时间约增长 67 倍，内存约增长 13.6 倍。该结果说明
高区域数的成本不只是保存 `nodes × states` likelihood；Q 传播和 cladogenetic scenario
处理也会逐渐成为主导。三档 lnL 均正常返回，但不同区域数表示不同模型状态空间，lnL 不用于
跨档科学比较。

30 区域相对 20 区域的状态数约增长 8.04 倍，中位时间约增长 10.73 倍，中位峰值内存约增长
7.70 倍。三次运行均成功并得到 `lnL=-703.014726901135987`，但连续运行时间逐次增加，表明
当前 PC 的可用物理内存和换页开始影响结果；因此该档报告三次范围，不把最快一次作为典型值。
其 dense-Q 参考大小约 227 GiB，但引擎实际使用 167 万个非零转移的稀疏表示。

资源门禁现在先做无分配的组合状态数预估。用户可在版本化请求或通用模型命令中设置
`max_states` / `--max-states`；省略时不施加内置上限。2026-08-24 的 release 门禁复用上述
100-tip 数据：20 区域和 30 区域分别在上限恰为 21,700 与 174,437 时完成 `analysis-plan`，
耗时约 `0.176 s` 和 `0.563 s`。同一 30 区域数据把 `max_range_size` 改为 15 后，预估
614,429,672 个状态，并在 1,000,000 上限处于状态分配和 Q/分裂构造前返回
`code=resource_limit`。可重复命令为：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-large-state-space-resources.ps1
```
