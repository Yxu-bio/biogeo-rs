# 项目状态与路线

## 总目标

项目目标不是分别重写 DEC、DIVA 和 BayArea，而是实现一套可配置的
BioGeoBEARS-like 历史生物地理框架：统一状态空间、沿枝 Q、节点 split table、
pruning、posterior、参数优化和后续 BSM；模型名称只是 `ModelConfig` preset。

交付形态是可独立运行、后续由新版 RASP 以子进程调用的命令行计算引擎。Rust 负责所有
影响统计结果的数值能力；GUI、绘图、祖先范围可视化和报告呈现由新版 RASP 负责，不参考
旧版 RASP，也不计作 Rust CLI 缺口。职责和进程协议见
[`rasp-cli-integration.md`](rasp-cli-integration.md)。

BioGeoBEARS 是框架语义 golden。LAGRANGE-ng 保持为独立的 LAGRANGE-ng 语义与
性能参考，不反向决定 BioGeoBEARS-like preset。

## 当前完成位置

### 统一计算核心

- 已完成范围状态空间、`max_range_size`、null range 和稳定状态顺序。
- 已完成精确 `0/1` 与显式 opt-in 的 BioGeoBEARS `0/1/?` 不确定范围观测；`1` 为必含、
  `0` 为必不含、`?` 为无约束，兼容状态直接生成未归一化 tip likelihood。固定推断、后验、
  专用/通用优化、分析结果重放和生物地理随机历史共用同一观测结果。
- 已完成 `d/e` anagenesis Q、稀疏表示和 uniformization 分支传播。
- 已完成 BioGeoBEARS `a` 的 singleton-to-singleton range-switching Q；方向性倍率与
  `d/j` 共用，并在生物地理随机历史中保留为一次原子 `a` 事件。
- 已完成非分时期 `b` 枝长指数；likelihood、posterior 和随机历史传播统一使用
  `branch_length^b`。分时期输入按 BioGeoBEARS 的 `non-stratified only` 限制要求 `b=1`。
- 已完成方向性 area-to-area dispersal multiplier；扩张率按当前范围内所有来源区域
  对目标区域的倍率求和。
- 已完成固定距离指数 `x`、环境距离指数 `n`、两类距离矩阵与手工 dispersal
  matrix 的 `manual^w` 逐元素组合，以及 area-specific extirpation 有效倍率；固定推断和固定
  修饰下的 `d/e` 优化共用同一 Q。
- 已区分严格正的原始 `AreaSizeVector` 与非负的最终
  `ExtirpationMultiplierVector`，按 BioGeoBEARS 语义计算 `area_size^u`。
- 已完成一次释放一个指数的 `d/e/x`、`d/e/n`、`d/e/u` 有界多起点优化；指数
  边界、收敛起点数和最终边界位置均进入结果诊断。其他修饰参数可保持固定。
- 已完成联合 `d/e/x/n/u` 有界优化与 `dec-xnu-optimize` CLI；`d/e` 在 log 空间、
  `x/n/u` 在各自显式边界内搜索，固定推断与联合优化仍调用同一
  `LikelihoodEngine`，没有为对照 fixture 增加特殊公式。
- 已完成通用二维 profile 执行器与 `dec-xn-profile`、`dec-xu-profile`、
  `dec-nu-profile`：每个 `x/n/u` 网格点只优化 nuisance 参数 `d/e`，并报告近似
  95% likelihood-ratio 支持区、网格边界、有限/失败点数、收敛点数和似然加权相关
  系数。单个网格点无法得到有限 lnL 时会被标记，不再中止整个 profile。
- 已完成 branch/time context 与 time-stratified piecewise-Q；fixed likelihood、
  downpass/uppass posterior 和优化共用同一分段传播实现。
- 已完成按时期 manual dispersal、原始地理距离、环境距离和 area size 输入；固定
  `x/n/u`、单指数优化、联合 `d/e/x/n/u` 优化及二维 profile 都从同一原始 schedule
  重建各时期 Q。
- 已完成按时期 `areas_allowed` 与 `areas_adjacency`：每期从 master state space
  生成允许状态 mask，同一 mask 同时约束 Q、节点 split table、root prior 和时期边界
  投影；被禁状态的 likelihood 质量归零且不重新分配。
- 已完成二叉树 pruning、flat/equal root prior 和数值缩放。
- 已完成 BioGeoBEARS 超短枝直接祖先语义：显式阈值、严格 `<` 判定、恒等节点 likelihood/
  uppass、节点 posterior、split 省略和生物地理随机历史状态复制共用 `NodeEvent`；阈值与
  命中边进入 CLI 诊断、分析结果、重放指纹和 BSM 元数据。
- 已完成统一 `LikelihoodEngine` 的固定 lnL、node-state posterior 和 split
  posterior。
- 已完成完整 BSM 计算主线：`HistorySkeletonSampler` 先直接消费拟合时的 branch
  propagator、按节点 split table、root prior 和 pruning conditional likelihood，联合
  抽取根状态、节点分裂和分支起止状态；随后以端点条件 uniformization bridge 抽取每条
  分支内部的 range expansion/local extirpation 事件、时间和状态链。均质 Q、按时期
  piecewise-Q、时期边界状态与 state mask 共用拟合时的真实过程，并已贯通固定模型 API
  和 `--bsm-samples` CLI。
- 已完成本机确定性有界并行 BSM：`indexed-chacha12-v1` 以 master seed + sample index
  派生独立随机流，共享只读 pruning/Q/cladogenesis 与线程安全转移行缓存；CLI 支持
  `--bsm-threads auto|N` 和 `--bsm-max-in-flight N`，并按 sample index 顺序写表。
- 已完成超长/高事件率分支的自适应 uniformization：默认传播和条件 bridge 在
  `lambda*T > 64` 时自动数值细分，按端点条件分布抽取子段边界并合并路径；数值子段
  不改变时期、Q 索引或事件语义，65,536 个子段上限会显式报错。
- 已完成每条随机历史的沿枝事件硬预算：`StochasticMapLimits` 和
  `--bsm-max-events-per-sample` 在所有分支/时期间累计，超限时返回准确 sample index；流式
  元数据记录 `completed_samples`，预算或采样失败的样本不会进入 writer。
- 已完成流式 BSM 的崩溃一致检查点：八张表全部刷新并同步后才发布提交标记；
  `--bsm-resume` 按运行指纹验证任务、截断未提交残尾并从绝对 sample index 续采。
- 已完成固定区间分片 BSM writer：`--bsm-shard-samples N` 生成不可变完整分片、单一活动
  分片和可重建 manifest；分片内检查点续跑、完成后目录发布以及跨线程确定性均已验证。
- 已完成 BSM 协作式运行控制：核心 API 共享取消令牌和可选截止时间，CLI 支持 `Ctrl+C` 与
  `--bsm-time-limit-seconds`。受控停止会提交完整有序前缀、记录 `cancelled/time_limit`，并可
  从该检查点恢复；取消和恢复均保持按 sample index 派生的确定性结果。
- 已完成进程内交互暂停：核心 `StochasticMapPauseToken` 在既有数值安全点等待并由条件变量
  恢复；CLI `--bsm-interactive` 接受 pause/resume/status/cancel。暂停不改变 RNG 或指纹，
  暂停中的 Ctrl+C/截止时间仍能终止，Windows release 真实进程与分片恢复已验证。
- 已完成 `y/s/v/j` 与 `mx01y/s/v/j` 参数化 scenario generator。
- 已完成 `biogeo-parameter-table-v1`、六种 preset 模板和通用 `model-evaluate` /
  `model-optimize` CLI。固定、自由、联动、边界与优化坐标可由文件声明；静态和分时期
  `x/n/u/w` 在每个候选点重建修饰，`a/b` 也可任意固定、释放或联动。无输入指数、
  无效自由参数和未实现参数语义均显式拒绝。
- 已完成 RASP-facing 通用任务控制：`ExecutionCancellationToken` 贯通 `model-optimize`、
  `model-batch`、`dataset-batch` 和既有 BSM；`--progress-format tsv` 输出版本化逐行事件。
  优化在完整似然评估之间安全取消，批处理取消后停止启动新任务，并以 v2 attempt 区分
  `complete/failed/cancelled/not_started`。
- 已完成 `biogeo-analysis-result-v2` 与 `model-bsm`：固定评估和优化结果以原子、非覆盖目录
  保存原始/冻结参数表、自包含输入包、lnL 位值和收敛诊断；重放时用稳定的
  `biogeo-model-identity-v1` 校验完整静态/时期模型并重新计算 lnL，随后复用既有 BSM
  sampler、确定性并行、检查点和分片 writer。exact range、ambiguous range、
  detection + 两时期修饰、1/4 worker 八表一致性及固定 DEC 八表等价均有回归。
- 已完成首版 `model-batch`：一个共享数据配置可由版本化 manifest 顺序运行任意多张参数表，
  每个模型原子发布为标准分析结果；`--resume` 逐字节校验任务身份并只补缺失模型。稳定比较表
  报告 `k/lnL/AIC/AICc/delta/weight/rank`，只纳入真正收敛的优化结果。模型失败不再阻断
  后续模型，每次调用都有不可变 attempt 汇总。
- 已完成 `biogeo-model-averaged-ancestral-ranges-v2`：对每个入选模型严格重放节点顶端
  posterior 和 cladogenetic split scenario，按 AIC/AICc 权重在线累加；节点、状态、情景、
  权重和概率使用规范化机器表。小样本
  BioGeoBEARS 外部 golden 最大概率差约 `3.50e-6`，Psychotria 六模型同时覆盖两套准则、
  36 个归一化节点组和逐字节恢复。
- 已完成 `dataset-batch`：每个数据集行引用独立模型 manifest 和完整版本化分析配置，支持
  不同 Newick、显式命名多树 NEXUS、观测模式、状态空间、修饰输入和优化设置。比较严格留在
  各数据集内部；跨数据集只汇总完成/失败并支持身份校验恢复。
- 已完成 BioGeoBEARS 的 founder-event pairwise modifier：同一有效 dispersal matrix
  同时进入沿枝 Q 与节点 `j` 权重；静态和时间分层矩阵均按祖先来源到 founder
  daughter 目标的有向均值计算，节点按距今年龄选择时期矩阵。

### 已验证 preset

- DEC：固定 lnL、node posterior、split posterior、`d/e` 优化均有
  BioGeoBEARS golden。
- DEC+J：固定 lnL、两类 posterior、`d/e/j` 优化均有 golden；新增静态非对称和
  三时期非对称 fixture，覆盖方向、零权重 scenario、逐节点时期选择以及固定 modifier
  下的联合优化。三组 BioGeoBEARS 优化均要求 convergence code 0。
- 非默认 `mx01*`：复杂 daughter-size、平衡 vicariance 和多区域 founder
  daughter 已有固定、split、优化 golden。
- DIVALIKE：已复用统一引擎完成固定 lnL、node posterior、split posterior 和
  `d/e` 优化 golden；四区域 `2+2` vicariance 已锁定。
- DIVALIKE+J：正式 preset 使用 BioGeoBEARS linked-weight 语义
  `y=v=(2-j)/2, s=0`，保留 `mx01v=0.5`，并使用 `j<2` 的优化边界。4-area
  与 5-area 全状态复杂夹具已覆盖固定 lnL、两类 posterior、split weight 和
  `d/e/j` 优化；最优 lnL 差为 `9.3e-8`。
- BAYAREALIKE：已复用统一引擎完成固定 lnL、node posterior、split posterior 和
  `d/e` 优化 golden；节点仅允许左右子代 exact-copy 祖先范围。复杂夹具覆盖
  5 areas、8 tips、`max_range_size=5` 的完整 32-state 空间。
- BAYAREALIKE+J：正式 preset 使用 `y=1-j, s=v=0`，保留 `mx01y=0.9999`，
  并使用 `j<1` 的优化边界。相同 4-area/5-area 复杂夹具已完成固定、posterior、
  split 和 `d/e/j` 对齐；最优 lnL 差为 `6.7e-9`。
- 静态与时间分层 dispersal multipliers：方向性、零连通率、跨 epoch 分支、
  node/split posterior 和 `d/e` 优化均已有 BioGeoBEARS golden。
- 按时期原始修饰：三时期全时变合成数据与官方 Psychotria M4b 五时期数据已覆盖固定
  lnL、node/split posterior 和 `d/e` 优化。官方重复时期还锁定了分段与静态模型的
  数学等价性，并保留 BioGeoBEARS stratified 数值路径的独立审计值。
- `distance^x`、`envdistance^n`、手工矩阵组合和 area-specific extirpation：
  固定 lnL、node/split posterior 和固定修饰下的 `d/e` 优化均已有 BioGeoBEARS
  golden。
- 自由 `x/n/u`：三个 x/n 案例与两个内部 `u`、一个真实下界 `u` 已有
  BioGeoBEARS golden；u 案例最优 lnL 差为 `1.4e-8` 到 `4.4e-7`。完全相同的
  原始面积会使 `e` 与 `u` 结构性不可辨识，CLI 会在优化前拒绝。
- 四区域全修饰 pair-profile 的峰值和三个边缘点已用 BioGeoBEARS 固定 `x/n/u`
  后优化 `d/e` 对照，lnL 绝对差为 `9.9e-8` 到 `3.4e-6`。三张完整 Rust 截面也
  已成为确定性回归。
- 联合 `d/e/x/n/u` 已用 BioGeoBEARS 官方示例数据扩展验证：Psychotria M4 用于
  暴露小数据的 ridge；正式案例采用官方 197-tip Conifer 树和地理距离，并由
  BioGeoBEARS 自身模拟 tip ranges。生成参数、环境距离和面积协变量全部冻结且可审计。
- Conifer 联合案例在 Rust 与 BioGeoBEARS 两组最优参数处交叉固定重算 lnL，绝对差
  分别为 `3.6e-7` 和 `6.0e-6`；这锁定了五个参数进入同一 Q/likelihood 的语义。
- 区域状态约束：BioGeoBEARS 官方 BSM 3-taxon `areas_allowed` 案例与合成 adjacency
  案例均已覆盖固定 lnL、node posterior 和 `d/e` 优化。固定 lnL 差分别为
  `1.15e-8` 与 `1.20e-8`；官方 areas-allowed split posterior 最大差为 `8.5e-10`。
- `a/b/w`：BioGeoBEARS 官方 Psychotria M4 的 19-tip、4-area 全范围状态空间上已冻结
  9 个单参数 profile 点和 1 个联合点；Rust 与 BioGeoBEARS 最大绝对 lnL 差为
  `3.33e-7`，联合点差为 `1.22e-7`。通用优化器同时释放三者的路径已有 CLI 回归。
- detection：官方 Psychotria 的固定 profile、2432 个 tip-state 相对似然、单独/联合
  `mf/dp/fdp` 优化、跨模块固定/后验，以及静态和五时期 `x/j/y/v/mf` 联合优化均已冻结。
  五时期 Rust 在 BGB 坐标处的固定 lnL 最大差为 `7.90e-6`，分时期与静态等价目标在
  严格参考点相差 `2.55e-12`。
- 不确定范围：官方 Psychotria M4 衍生案例已冻结 304 个 tip-state likelihood、固定 lnL、
  288 个内部节点 posterior 和 `d/e` 优化；固定与优化 lnL 绝对差分别为 `1.34e-7` 和
  `3.25e-7`。全未知、纯 absence-only 和混合约束另由 BioGeoBEARS 1.1.3 底层函数逐格
  golden 锁定；标准 BGB 文件工作流更早的输入拒绝作为包装层限制记录，不复制到 Rust 核心。
- 完整 BSM：BioGeoBEARS 官方 `BSM_3taxa/M3areas_allowed` 案例已在 ML 参数下冻结
  5000 条 BioGeoBEARS 生物地理随机历史，并与 5000 条独立 Rust 随机历史比较事件数、`d/e`、
  `y/s/v/j`、时期事件占比、状态占据时间和“时期 × 状态”占据时间。39 项分布门禁
  全部通过；最大均值偏差 `2.43` 个 Monte Carlo 标准误，最大经验 CDF 差 `0.0368`
  （门限 `0.04`）。另两个 Rust seed 各 5000 条随机历史的复核也全部通过。
- Ponerinae 真实分层 BSM pilot 已覆盖 1534 tips、7 areas、120 master states 和 7 份时期
  状态 mask。Rust 100 条历史的所有正占据状态均满足对应 mask；BioGeoBEARS 单条历史在
  22 个困难分支触发 manual fallback，产生 24 次禁用状态转移和 `11.0676` 时间单位的禁用
  状态占据。该行为作为 BGB 风险诊断记录，不复制进 Rust，也不充当分布 golden。
- 模型比较：AIC、AICc 和两类 Akaike weight 已直接调用 BioGeoBEARS 1.1.3 官方函数生成
  golden。官方 Psychotria M4 六模型 release 批量实跑全部收敛；相同任务 `--resume` 仅校验
  既有结果并在约 `0.11` 秒内重建逐字节相同的比较表。

### 外部验证与性能

- 项目内独立 R library 可运行 BioGeoBEARS，不依赖用户其他 R 项目环境。
- LAGRANGE-ng 本地二进制、官方示例和冻结 reference 已单独验证。
- `8 areas / 128 tips / max_range_size=5` 固定 likelihood 中，Rust 对
  BioGeoBEARS 热会话本轮按均值约快 138-187 倍、按中位数约快 162-182 倍，
  lnL 相对差约 `1e-9`。
- `8 areas / 32 tips / max_range_size=5` 的完整 DEC `d/e` 优化约快 281 倍，
  最优 lnL 绝对差约 `5.9e-6`。该倍率同时包含优化器策略差异。
- 官方 Ponerinae 1534-tip、7-area、120-state、7-stratum DEC 中，固定参数 lnL 差
  `7.73e-6`，d/e 独立优化的 lnL 差 `1.84e-5`。BioGeoBEARS 完整优化用时
  1197.49 s，Rust 5 次复测中位数 6.24 s，保守端到端比约 192x。
- 同一 Ponerinae 配置的生物地理随机历史中，BioGeoBEARS 单条完整进程为 219.48 s，
  其中 setup 145.94 s、采样 68.35 s；Rust 单条完整进程为 0.574 s，约快 382x。
  Rust 100 条含四个分片和 549 MiB 八表写出共 10.848 s，按每条吞吐与 BGB 采样阶段相比
  约快 630x。1/10 worker 的相同 10 条 Rust 历史八表逐字节一致。
- `12 areas / 1000 tips / max_range_size=5` 的 1586 状态固定 DEC release CLI
  五次平均 `0.6395` 秒；统一运行入口实测 working-set 高水位约 `68.27 MiB`，
  平均逻辑核使用量约 `0.96`。
- 197-tip Conifer 联合 `d/e/x/n/u` 单起点优化中，Rust release 约 2 秒；
  BioGeoBEARS `optimx/bobyqa` 约 410 秒。Rust 找到的 lnL 高 `0.00606`，而
  BioGeoBEARS 结果虽返回 convergence 0，但 `KKT1=FALSE`，所以该结果用于端到端
  性能和邻域比较，likelihood 语义仍以双方参数处的交叉固定重算为准。
- 同一 Conifer 案例已改由版本化参数表再次运行；通用优化入口得到
  `lnL=-358.614345952854990`、288 次迭代和 515 次评估，lnL 与五个参数均和专用
  `dec-xnu-optimize` 冻结结果逐位一致。
- 官方 BSM 5000 条随机历史对照中，BioGeoBEARS `runBSM` 随机历史采样阶段为 `550.62` 秒；
  本轮当前 release 单目录重跑中，1 worker 中位数 `1.268` 秒、16 worker `0.979` 秒，
  观测比值约 `434x` 与 `562x`。更早的单目录/5 分片交替热运行用于估计分片约 25% 的
  本机开销。两种目录格式都通过同一批 5000 对 5000 的 39 项分布门禁。
- 并行扩展基准中，官方三物种轻负载 10,000 条本轮由 1 worker 中位数 `2.086` 秒降至
  16 worker 的 `1.501` 秒（`1.39x`）；197-tip、41-state 复杂负载 100 条由 `2.188`
  秒降至 `1.236` 秒（`1.77x`）。两组 1/2/4/8/16 worker、每档三次的八表数据指纹
  均完全一致；串行格式化和磁盘写出已是主要扩展瓶颈。

## 进度评估

按“DEC 可用主线”衡量，当前约在 **99.2%**：固定推断、posterior、单参数与联合优化、
复杂节点语义、完整 BSM、官方数据分布对齐、性能基准和参数可辨识性诊断都已具备；
Windows 公开科研发布候选包、可移植结果和机器 schema 已具备；项目许可证及来源审计已完成，
主要缺少服务器资源监测和少量外围统计。

按“完整 BioGeoBEARS-like 框架”衡量，当前约在 **99.4%**。DEC、DEC+J、
DIVALIKE、DIVALIKE+J、BAYAREALIKE 和 BAYAREALIKE+J 六个 preset 已作为统一
引擎配置成立，方向性倍率和基础时间分层
已经贯通，固定地理/环境/面积指数、自由单指数优化和按区域 extirpation 也已对齐；
联合 `d/e/x/n/u` 优化、二维 profile、按时期变化的原始修饰和范围状态约束也已可用。
pairwise modifier 对 founder-event 的静态、分层和优化语义也已对齐。完整 BSM 的节点、
分支端点、时期边界和沿枝事件已经可联合抽样，逐条随机历史汇总与 BioGeoBEARS 分布级外部
对照也已完成；逐条消费 API、版本化流式目录、单样本事件预算和运行元数据也已落地。
八表同步检查点、崩溃残尾截断、绝对 sample index 续采和运行指纹校验也已落地。
通用参数表的固定/优化结果现在也可经版本化结果目录严格重放到同一 BSM 引擎。
任务级总事件预算、完整历史窗口内存预算、固定区间分片输出和交互暂停已经落地。严格
Newick 标签/注释/枝长契约、单树及显式命名多树 NEXUS/TRANSLATE、规范 Newick 转换、普通
已定年古老末端和超短枝直接祖先也已覆盖；精确范围与显式不确定范围的只读输入诊断也已
落地。单数据配置的模型比较、模型平均祖先范围和多数据集/多树分层批调度均已可用。年龄区间、
stem/crown/both 和类群约束下的随机化石放置、跨模型 split scenario 平均、参数表达式嵌套
关系与 likelihood-ratio test 也已进入稳定机器结果。Windows 科研试用包总检查已经通过；
新版 RASP 的 v0.1 进程、schema、生命周期和项目迁移边界也已冻结并由安装版参考宿主验收。
剩余明显缺口主要是跨进程并发调度和 Linux/调度器资源探测。项目已采用
`GPL-3.0-or-later`，来源兼容性审计已完成；代码签名和专用 CI 不作为发布前提。Windows 单任务的 working-set 高水位与 CPU 时间已经进入
稳定输出；图形与报告由新版 RASP 消费
这些数值结果实现。
这个百分比不是代码行数，而是按可验证功能里程碑估算。

## 当前主要缺口

1. BSM 已有单条回调消费 API、确定性有界并行、锁定的 `biogeo-bsm-tsv-v1` 兼容 writer，
   以及 full/compact/summary 三种 v2 流式目录；
   兼容批量/标准输出模式仍会保留全部随机历史。流式目录已支持崩溃一致检查点和断点续跑；
   协作式取消与耗时上限、任务总事件预算、完整历史窗口内存预算和固定区间分片 writer
   和进程内交互暂停已经实现。窗口预算具有可审计的保守上界，但不等价于整个进程 RSS
   硬限制。
2. CLI 已有 `biogeo-analysis-result-v2`、`biogeo-input-bundle-v1`、v1 只读兼容与非覆盖迁移工具；
   树、观测、静态修饰和时期二级依赖都以相对路径自包含。统一请求现已记录单次执行耗时和结果
   字节数，计划阶段提供状态、Q、split 与数值载荷参考；Windows `analysis-run` 已用系统 API
   记录进程 working-set 高水位和命令 CPU 时间，Linux/cgroup 对应实现仍后置。规范 TSV、
   BioGeoBEARS/LAGRANGE `.data`、常见 RASP CSV 以及 BioGeoBEARS 分时期 block matrix 已有
   严格导入；其他外围数据格式仍按实际案例扩展。首版
   `model-batch` 已覆盖同一数据配置上的多模型拟合、恢复、失败 attempt 和信息准则比较；
   AIC/AICc 模型平均祖先范围也作为批量 v2 完成结果发布；`dataset-batch` 已分层覆盖每组
   独立数据、树和修饰输入。两层目前都顺序执行，跨进程并发调度尚未实现。主入口已支持
   `biogeo-cli-error-v1` 机器错误记录和稳定退出码。`biogeo-cli-progress-v1` 已报告单模型、
   两级 batch 和多起点优化进度；同一取消令牌会停止当前通用优化和后续 batch 任务。旧专用
   优化命令仍仅用于验证兼容，不输出该进度协议。32 个公开格式号已固定当前新版 RASP 接口；
   Windows MSVC v3 发布包、SHA-256 payload 校验、可选的 Authenticode/CI 构建记录、非覆盖安装和
   安装后科学冒烟均已完成。
   发布包现包含全部可移植 `examples/`、release status、locked 构建信息、引擎源码清单、
   第三方 notices 和实际许可证文本；六 preset、五时期分析和时间预算停止/恢复示例会由安装后
   的 exe 直接运行，而不依赖仓库内的开发路径。当前包标记为公开科研发布候选版，采用
   `GPL-3.0-or-later`，可以公开分发、构建和安装。新版 RASP 参考宿主已组合验证只读
   中文源输入、删除源目录、跨项目移动结果、严格重放和移动后重新采样；宿主不重复实现科学计算。
3. 普通已定年古老 tip 已通过官方化石案例的 fixed、optimization、posterior 和 BSM；
   超短枝直接祖先也已通过官方 `add_hook()` 派生树的 fixed、optimization、两类 posterior
   和 BSM 结构门禁。独立 `fossil-place` 现已实现年龄区间、stem/crown/both、类群约束、
   side branch/direct-ancestor hook、顺序依赖化石和确定性多树结果。更复杂的 cladogenesis
   modifier 组合和大数据 posterior/BSM 资源上限仍需继续扩展。
4. `mf/dp/fdp` detection likelihood、输入契约、动态优化及五时期跨模块联合优化已经实现
   并通过官方 Psychotria golden。`mx01r` 已完成源码与运行时审计，确认 BioGeoBEARS
   1.1.3 不消费它；当前作为固定 0.5 的兼容空操作，不开放虚假的优化维度。

## 推荐推进顺序

1. 已完成 `d/e/x/n/u` 联合点估计、官方来源的大数据验证和 pair-profile 诊断；
   profile 用于解释具体数据的 ridge，不作为删减模型功能的门槛。
2. 已完成按时期 area size/距离输入，并确保 fixed、posterior 和优化共用分段语义。
3. 已完成区域可用性/邻接约束；固定、posterior 与优化共享同一时期 mask。
4. 已完成 pairwise modifier 对 founder-event `j` 的节点权重语义，并用静态与分层
   BioGeoBEARS golden 锁定 fixed、posterior、split 和 `d/e/j` 优化。
5. 已完成 DIVALIKE+J 与 BAYAREALIKE+J 正式 preset、CLI、公共 API、各自 BGB
   参数边界及复杂 fixed/posterior/split/optimization golden；六模型矩阵闭合。
6. 已完成统一 branch propagator/split 语义上的条件历史骨架抽样，并用精确 posterior、
   时间分层 Q 和范围状态约束验证经验分布。
7. 已完成给定分支起止状态的 conditional CTMC bridge、沿枝事件与时间、分时期边界
   抽样以及完整 BSM API/CLI；解析解、事件计数、时期占比和状态约束回归已覆盖。
8. 已完成 BioGeoBEARS 官方 BSM 案例的 5000 条随机历史分布级汇总、冻结 golden 和独立门禁；
   不要求相同 seed 的逐条路径相等。
9. 已完成逐条消费 API、v1 八表兼容目录、v2 full/compact/summary、完整/未完成状态和
   防覆盖语义；v2 增加不可变 ID 字典、稀疏占据、`a` 计数、困难分支诊断和时期状态约束
   写出前审计。官方 5000 条随机历史的 39 项分布门禁已直接通过 summary v2。
10. 已完成本机确定性有界并行、按索引随机流、顺序 writer、资源参数和跨线程逐字节门禁。
11. 已完成超长/高事件率 segment 自动细分；解析分布、高 Poisson 均值、超过 10,000
    个虚拟跳和“不泄漏数值子段”回归均已覆盖。
12. 已完成单样本沿枝事件硬预算、跨分支累计、并行 sample index 错误和失败目录
    `completed_samples` 记录。
13. 已完成八表 `flush + sync_data` 后发布不可变检查点、失败回滚、崩溃残尾截断、绝对
    sample index 续采和运行指纹校验；恢复后八表与一次性完整运行逐字节一致。
14. 已完成共享取消令牌、可选截止时间、CLI `Ctrl+C` 和耗时上限；取消/超时会提交完整前缀，
    元数据区分停止原因，恢复结果与一次性运行逐字节一致。
15. 已完成任务总事件预算；超限样本不会写出，累计事件数进入 v2 检查点，v1 检查点可迁移，
    提高上限续跑后与无限制基线逐字节一致。
16. 已完成完整历史窗口内存预算；根据树结构、时期 segment 和单样本事件上限计算保守字节
    上界，自动收缩 worker/在途数。
17. 已完成固定区间分片 writer；完整分片不可变、活动分片按 v2 检查点续跑、manifest 可从
    目录事实重建，且分片拼接与单目录八表逐字节一致。
18. 已完成核心 pause token 和 `--bsm-interactive` 标准输入控制；暂停/恢复保持随机历史不变，
    status 报告有序消费前缀，取消仍走耐久 checkpoint。
19. 已建立 BioGeoBEARS 1.1.3 的 23 行参数兼容矩阵和通用参数依赖图；固定、自由、联动、
    边界、未知引用、循环依赖与安全算术解析已经进入 core，六个正式 preset 从参数表解析后
    与原有 `ModelConfig::preset_*` 结构相等。
20. 已完成动态维度参数表优化器；`Linear/Log/Logit` 坐标、显式多起点、联动值越界拒绝、
    完整解析参数与最终模型返回均已进入 core。DEC、DEC+J 与原有专用优化器交叉一致，
    自定义 `y/v` 自由和 `s=y/2` 联动组合通过固定重算。
21. 已完成独立释放 `y/s/v/mx01/mx01y/mx01s/mx01v/mx01j` 的 BioGeoBEARS
    优化与固定剖面对照。复杂 5-area/8-tip fixture 共冻结 240 个 profile 点；Rust 已复刻
    `rexpokit::maxent` 的 `1e-7` 迭代停止和三位舍入语义。BGB convergence=0 仍可能落在
    较差 MaxEnt 台阶，因此 golden 同时记录 optimizer、profile_grid、来源与 gap；Rust
    平滑权重严格对齐点估计，MaxEnt 参数按最佳似然与固定点交叉重算验收。
22. 已完成版本化 23 行参数文件、六种 preset 模板、通用固定评估和动态维度优化 CLI；
    静态/分时期 `x/n/u` 与旧专用命令逐位交叉一致，官方 197-tip Conifer 五维优化也
    复现既有 Rust golden。
23. 已完成 `a/b/w` 非默认语义和通用优化入口；官方 Psychotria M4 10 点 profile、
    `a` 的精确 Q、`b` 的传播/随机历史时间尺度、静态与分时期 `manual^w` 均有门禁。
24. 已完成 `mf/dp/fdp` detection 末端观测似然、计数输入校验和动态优化；官方
    Psychotria 8 点 profile、2432 个 tip-state 相对似然值与四组优化 golden 均已通过。
25. 已完成 detection 跨模块固定门禁：静态 `x/n/u`、非默认 `y/s/v/j + mx01*`、全栈
    静态修饰和官方五时期输入；`x/j/y/v/mf` 五维联合优化也通过同点重算与多起点搜索。
    组合祖先范围与 split posterior 也已通过；重复五时期用静态等价 BGB 参考，同时锁定
    Rust 分时期/静态等价性。时期联合优化现也采用相同三起点完成。
26. 已完成版本化通用结果接入生物地理随机历史：结果目录原子防覆盖、内部/外部指纹、
    稳定模型身份、状态与 lnL 重放、非收敛诊断和同一 BSM 执行层均已落地。
27. 已完成 `mx01r` 语义审计：源码只有参数定义/序列化对象，非分时期与分时期 root prior
    均未接入它；复杂静态和官方 Psychotria 五时期三点扰动的所有提取量严格零差。Rust
    保留 23 行兼容表并拒绝非默认值或释放优化。
28. 已完成受约束 detection 全栈门禁：五时期状态数 `16/8/4/2/2`，固定 lnL、288 个
    fixnode 节点状态、408 个校正 split、`d/e/x/n/u` 五维优化和 20,000 条生物地理随机
    历史分布均已对齐。直接 BGB stratified uppass 的 `0.2937` 级差异保留为缺陷审计，
    不通过放宽容差复制。
29. 已修复稀有端点 CTMC bridge 的浮点尾部停滞；当累计 Poisson 质量舍入到 1 附近时，
    改用递推尾部上界。至少 17 次跃迁、约 `3.5e-8` 端点概率已有回归。
30. 已完成树输入边界第一阶段：单引号/转义标签、UTF-8、方括号注释、内部标签、缺失枝长
    默认拒绝和 root edge 明确拒绝均已进入 core。官方 `M3areas_allowed_wFossilBranch`
    年龄 `0.09` 末端已通过 fixed、`d/e` optimization、node/split posterior 和 20,000 条
    生物地理随机历史门禁；BGB 局部 COO 状态索引按范围位集合映回主状态空间。
31. 已完成显式直接祖先契约：BioGeoBEARS 默认阈值、严格边界、identity node、后验、
    优化、随机历史和结果重放均已接通；官方派生双树 golden 不依赖 fixture 特判。
32. 已完成单树 NEXUS 输入：自动识别 `#NEXUS`、解析 `TREES/TRANSLATE`，多树和 `UTREE`
    明确拒绝；APE 从官方三类群化石树导出的 NEXUS 与原 Newick 的 112 行固定/后验输出
    逐字节一致。
33. 已完成 `validate-inputs`：正式 Newick/NEXUS 与范围解析路径共同检查树范围对应、二叉性、
    枝长/年龄摘要、古老末端和直接祖先；重复区域名、重复/缺失 tip 在解析层提前拒绝。
34. 已完成显式多树选择和格式转换：`--tree-name` 贯通校验、分析、优化、结果重放与 BSM
    指纹；默认仍拒绝多树。`convert-tree` 只输出规范 Newick，不补枝长或改变拓扑。APE
    官方派生多树 fixture 与原 Newick/单树 NEXUS 的 112 行模型语义输出逐字节一致。
35. 已完成 BioGeoBEARS `0/1/?` 不确定范围观测：严格显式入口、任意 tip likelihood、
    官方 Psychotria 衍生 fixed/posterior/optimization golden、底层边界语义、结果重放和
    跨线程生物地理随机历史均已覆盖，且没有 fixture 特判。
36. 已完成首版 manifest 批量优化和稳定模型比较：逐模型标准结果、原子初始化、失败后
    校验续跑、收敛资格、AIC/AICc/delta/weight/rank 及官方 Psychotria 六模型实跑均已覆盖。
37. 已明确“Rust CLI 计算引擎 + 新版 RASP 展示层”的职责边界，并增加
    `--error-format tsv`、`biogeo-cli-error-v1` 稳定错误分类和既有退出码的机器记录；协议不
    假定旧版 RASP 的项目或 GUI 结构。
38. 已完成分层多数据集/多树任务：`model-batch` 首错后继续并写逐模型不可变 attempt；
    `dataset-batch` 以独立版本化配置复用整个模型批量入口，按数据集隔离比较、汇总失败并
    恢复缺失任务。不同树的 lnL 不会进入同一组 Akaike weight。
39. 已完成模型平均祖先范围机器结果首版：批量结果升级为 v2，比较表当时升级为 v2；AICc
    必须覆盖完整 AIC 候选集，非收敛模型排除。输出分离模型权重、节点、状态与概率；
    BioGeoBEARS 两模型 golden 和官方 Psychotria 六模型恢复门禁均通过。
40. 已完成新版 RASP 所需的机器进度和通用取消：版本化逐行事件不污染 stdout，通用优化按
    似然评估安全点停止；两级 batch 不再把取消当普通失败继续运行，并持久化 v2 attempt。
41. 已完成可移植输入包和格式迁移：v2 默认自包含，v1/v2 加载分支严格，时期二级依赖
    结构化重写并保留原文；检查和迁移均有版本化 TSV 输出。迁移在临时目录完成双向科学重放后
    才原子发布，跨目录重定位和删除原输入后的 BSM 已有回归。
42. 已完成 Windows 发布安装和结果 schema 契约：首批九项格式进入注册表；真实 CLI 优化、目录、
    inspect、迁移、机器错误与进度逐项验收。发布脚本产生版本化目录/ZIP/SHA-256，安装器校验
    payload 后原子非覆盖发布且不修改 PATH；安装后 exe 再完成真实优化和结果重放。
43. 已完成随机化石放置：对照 BioGeoBEARS 1.1.3 的候选枝与抽样函数，实现年龄区间、
    stem/crown/both、MRCA 类群约束、普通侧枝和直接祖先 hook；每个 replicate 独立确定性 seed，
    不覆盖结果目录记录每次连接，后续似然仍只消费固定 Newick。
44. 已完成跨模型 split scenario 平均：结果升级为
    `biogeo-model-averaged-ancestral-ranges-v2`，按节点、祖先、有序左右子范围和事件类型取并集，
    缺失情景先置零再按 AIC/AICc 权重累加，直接祖先不伪装为分裂。
45. 已完成参数约束驱动的嵌套关系和似然比检验：`biogeo-model-comparison-v3` 对全部有向模型对
    做 23 参数符号表达式嵌入；三组 `+J` 正确识别为 `j=0` 边界嵌套，跨家族不嵌套。LRT 明确
    区分不可用、似然次序错误、普通卡方参考与单边界 1 自由度 half-chi-square 风险。
46. 三项新格式已进入 schema registry，真实 CLI 进程会生成随机化石目录、comparison v3 和
    model-average v2 后逐字段校验。随后进入了统一分析请求和运行前资源规划阶段；
    Linux/调度器探测仍后置。
47. 已完成 `biogeo-analysis-request-v1` 统一单模型任务：严格 key/value 请求把树、观测、参数表、
    状态空间、修饰和优化配置解析回现有通用执行路径；`analysis-template` 生成非覆盖骨架，
    `analysis-plan` 报告状态/Q/split 规模、自由参数顺序和不冒充 RSS 的资源参考量，
    `analysis-run` 生成原有 `biogeo-analysis-result-v2` 并记录耗时、结果字节数和收敛状态。
    中文与空格路径、原始请求进入可移植输入包以及四项新 schema 已有真实进程门禁。
48. `analysis-run` 已升级为 `biogeo-analysis-run-v2`：Windows 通过系统 API 输出进程生命周期
    working-set 高水位、命令作用域用户/内核 CPU 时间、平均逻辑核使用量和实际单模型 worker
    数；非 Windows 明确降级为 `NA`。v1 schema 保留，发布安装门禁会验证 v2 遥测。
49. 已在当前 release 与隔离 BioGeoBEARS 1.1.3 上重跑 DEC 性能：219 状态固定 likelihood
    按负载约快 52-187 倍，32-tip `d/e` 完整优化约快 281 倍；1000-tip、1586 状态固定
    likelihood 平均约 0.64 秒。全部对照继续以 lnL 对齐为前提，完整口径见
    `docs/performance-benchmark.md`。
50. 已重跑 BSM 性能：官方 5000 条案例中 Rust 单 worker/16 worker 相对 BioGeoBEARS
    分别约快 434/562 倍；官方轻负载和 197-tip 复杂负载的线程扩展分别约 1.39/1.77 倍。
    全部线程档位保持八张表指纹一致，当前主要扩展瓶颈定位到串行 writer 与磁盘同步。

所有新增功能继续遵循同一顺序：先锁定内部事件表和固定参数结果，再做优化与性能；
不通过 fixture 名称、区域数或参考工具名称加入核心特判。

51. 已完成第一轮超大型树审计和输入热点修复：范围表与 detection 表的 tip 匹配由逐行线性扫描
    改为一次哈希索引，100,000-tip `validate-inputs` 从 22.61 秒降至 1.224 秒；平衡
    100,000-tip、93-state 固定 DEC 端到端约 2.14 秒，working-set 高水位约 412.23 MiB。
    `analysis-plan` 现按节点数和核心数值载荷将该任务标为 moderate 风险。该结论仅覆盖固定
    likelihood；超深递归树、大状态空间 posterior 和超大树 BSM 输出仍是下一阶段的明确风险。
52. 已完成百万末端固定似然实测：平衡 1,000,000-tip、5-area、完整 32-state DEC 在当前
    Windows PC 的 release CLI 中用时 10.761 秒，峰值 working set 为 1.733 GiB，CPU 时间
    10.641 秒且 lnL 正常返回。资源预检将其标为 high；该结果不外推到祖先后验、split posterior
    或百万末端生物地理随机历史。
53. 已完成固定 100-tip、`max_range_size=5` 的区域数扩展实测：5/10/20 区域分别产生
    32/638/21,700 个状态，固定 DEC 中位时间为 0.0076/0.0247/1.6418 秒，进程高水位约为
    5.18/8.24/112.16 MiB。10 到 20 区域的状态数约增长 34 倍而时间约增长 67 倍，确认
    Q 传播和分裂情景在高状态数时带来超出基础 `nodes × states` 存储的计算成本。
54. 同一 100-tip 案例已扩展到 30 区域：`max_range_size=5` 产生 174,437 个状态、167 万个
    Q 非零转移和 334 万个 split scenarios。三次固定 DEC 均成功，时间为
    13.18/17.62/21.29 秒，峰值 working set 中位数约 863.50 MiB，lnL 一致为
    `-703.014726901135987`。连续运行变慢显示当前 PC 已受可用物理内存和换页影响。
55. 已完成 BioGeoBEARS/RASP 输入互操作第一阶段：分析入口按内容直接识别 LAGRANGE `.data`
    和常见 `ID,Name,Area...` CSV，`convert-ranges` 可输出规范 TSV；
    `convert-biogeobears-strata` 严格读取时间边界、逐时期 dispersal/adjacency block 和尾部
    `END`，原子生成相对路径 schedule。现已加入逐时期 `allowed_ranges`，明确区分
    BioGeoBEARS 内置 all-pairs adjacency 与论文脚本的 edge-covered 状态生成规则。
    官方 Ponerinae 1534-tip、7-area、`max_range_size=5` 输入复现七时期状态数
    `36,36,27,20,24,20,38`；CSV 通过显式 taxon/area map 与原始 `.data` 得到逐位相同的
    Rust 固定 lnL。在相同状态、时期和参数上，BioGeoBEARS 固定 lnL 差 `7.73e-6`；
    d/e 独立优化后 lnL 差 `1.84e-5`、参数差约 `1e-5`。BioGeoBEARS
    `optimx/bobyqa` 完整进程用时 1197.49 s；Rust 直接配对用时 3.83 s，长时满载后 5 次
    复测中位数为 6.24 s，因此当前 PC 保守端到端比约 192x，直接配对观测为 312x。
56. 已完成 BSM v2 通用输出契约：full/compact/summary 的单目录与分片共六个格式号
    进入 schema registry，真实 CLI 进程逐项校验根目录、引用表、八张数据表、manifest 和 shard。
    Ponerinae 1534-tip/7-area 的同 100 条历史中，summary v2 与 legacy v1 保留的所有汇总值
    逐样本一致，时间从 10.848 s 降到 3.065 s，输出从 575,694,792 bytes 降到
    705,641 bytes。这是通用级别与 schema 实现，核心代码未按 fixture 名称、tip 数或区域数分支。
57. 已完成通用 `bsm-inspect`：快速模式验证八种 v1/v2 布局的元数据、checkpoint、表长、
    引用字典和分片连续性；深度模式以常量级行缓冲验证事件分类、时期比例、状态占据、ID、
    分支段连续性和逐事件状态链。检查结果使用 `biogeo-bsm-inspection-v1`，六种 v2 真实目录
    进入进程级 schema 门禁；1534-tip Ponerinae 的 summary 与 575 MB legacy 结果均已通过。
58. 已完成 `analysis-workflow` 单任务编排：统一请求依次经过计划、分析结果、随机历史生成和
    快速/深度检查，默认 compact，固定两个权威子目录且不另造似然实现。首次运行严格非覆盖；
    `--resume` 复用完整分析结果、逐字节核对封存请求并恢复 BSM 检查点。分析完成后删除原始树、
    范围和参数文件的恢复回归、请求变更拒绝、真实进程 schema 以及中文空格路径均已通过。
59. 已完成 Ponerinae 真实规模统一工作流验收：1534 tips、7 areas、120 states 和七时期状态约束
    进入 portable d/e 优化请求，126 次评估得到既有 golden lnL。2500 事件预算确定性提交
    2 条历史和 2047 个沿枝事件后停止；请求侧树、范围、参数和时期目录不可用时，工作流从
    可移植分析结果恢复到 10 条和 10352 个事件。最终深度检查为 0 违规，恢复目录与同 seed
    一次性基线的 35 个文件逐字节一致；验证脚本和所有机器输出均保留，不向核心加入案例特判。
60. 已完成 `--version` 与 `engine-info` 能力发现首版：`biogeo-engine-capabilities-v1` 公开版本、
    构建平台、进程可见并行度、六 preset、推荐/兼容命令、输入与随机历史格式和关键能力标志。
    32 个 `public_formats` 与 schema registry 由真实进程和 Windows 安装后 exe 双向集合比较；
    新版 RASP 可以先完成稳定握手，不再解析帮助文本或按版本号猜测功能。
61. 已完成 v0.1 命令帮助和兼容政策冻结：`engine-info` 宣告的 22 个推荐命令与 16 个兼容命令
    全部具有命令专属 `--help`，按解析器语义列出必需/可选参数、默认行为、结果格式和退出码；
    x/n/u 优化、二维 profile 和统一工作流不会再显示实际被拒绝的参数。新增
    `biogeo-compatibility-policy-v1`，规定同格式号 schema 不可静默增删字段、未知格式和字段拒绝、
    显式迁移及至少一个完整次版本的弃用窗口。真实进程逐命令帮助、未知 request v999/未知字段、
    schema 契约和 Windows 安装后 exe 均进入发布门禁。
62. 已完成 `model-workflow` 多模型高级编排：版本化请求统一复用 model-batch、AIC/AICc
    比较、祖先结果模型平均、显式单模型 BSM 和深度检查。自动选择要求唯一 rank 1，显式 ID 不会
    被信息准则暗中改写；取消、时间预算停止和执行资源调整恢复均有真实进程回归。
63. 已完成新版 RASP 无 GUI 接入参考宿主和稳定状态机：严格协商能力与 schema registry，分流
    机器进度、机器错误和诊断行，按退出码与 artifact 联合判断拒绝、取消、预算停止、失败和完成，
    并在导入前验证工作流身份、选中分析结果及 BSM 深度检查。中文空格路径、stdin 取消 4096 条
    BSM 后改线程恢复以及安装版 release exe 重跑均已通过。
64. 已完成公开 CLI 示例收口：六个 preset 各有自包含可移植分析请求，官方 Psychotria M4b
    五时期示例同时覆盖 manual dispersal、距离和面积修饰，恢复示例以退出码 124 和
    `bsm_time_limit` 演示拟合结果复用及确定性续采。统一门禁要求六模型全部收敛、逐模型重放
    通过、15 个分层依赖完整，并确认恢复前后的拟合 metadata 哈希不变。
65. 已完成多模型真实数据验收：官方 Psychotria 19-tip/4-area/16-state 和由完整来源确定性派生的
    Ponerinae 32-tip/7-area/120-state 子集，均运行六个正式 preset。两项任务先以 0 秒预算停止，
    再从同一目录恢复 4 条生物地理随机历史；12 个模型结果恢复前后均通过科学重放，两个顶层
    结果逐字段满足已注册 schema，最终深度检查均为 0 违规。
66. A4/B4 验收期间修复了两个通用工程风险：分析结果的浮点元数据改为 Rust 最短往返表示并继续
    保留 IEEE-754 位值，避免极小有限 lnL 被十进制定点格式破坏；Windows 原子目录发布仅对
    `PermissionDenied/WouldBlock` 做有界退避重试。六个并发进程连续 5 轮共 30 次发布全部成功，
    非 Windows 和其他 I/O 错误仍立即返回。
67. 已完成 D1 六 preset 修饰组合矩阵：同一 4-area/6-tip fixture 对 DEC、DEC+J、DIVALIKE、
    DIVALIKE+J、BAYAREALIKE 和 BAYAREALIKE+J 分别运行静态与两时期的 manual、地理距离、
    环境距离、area size 和事件专属 `mx01*`，共 12 项固定似然、祖先/分裂后验、可移植结果和
    重放全部通过。另冻结 12 项缺失输入、重复来源和非法配置拒绝规则；分时期 `b!=1` 与缺失
    manual 输入的诊断已改为准确说明实际约束。该矩阵同时进入总科学门禁和 Windows 安装版门禁。
68. 已修复 Windows R 4.5 继承 Linux `C.UTF-8` 环境后误判中文绝对路径不存在的问题：验证层只在
    Windows 调用 R 时临时移除不兼容 locale，并在退出后恢复环境，不改变 Rust 引擎或 fixture。
    官方化石末端和受约束 detection 两组各 20,000 条生物地理随机历史分布对照重新通过；后者
    同时覆盖 288 个节点状态概率和 408 个 split 概率。
69. 已完成 E1 Windows v0.1 科研试用包总检查：一条命令依次执行 locked workspace 测试、
    `clippy -D warnings`、完整框架语义、locked MSVC release 构建、v3 打包安装、公开示例、真实
    多模型工作流和新版 RASP 参考宿主。首次总耗时 285.24 秒并保存不可变检查记录。现在未签名包可以直接
    构建和安装，并仍会校验文件清单和实际启动 EXE。来源和依赖许可已完成工程审计，现只等待所有者选定项目许可方案。
70. 发布收口检查发现 Windows PowerShell `Compress-Archive` 会把 ZIP 条目写成反斜杠路径；虽然
    Windows 自带解压可接受，但其他库可能把它解释成文件名字符。打包器现按排序后的文件列表用
    `ZipArchive` 写标准 `/` 路径，门禁拒绝反斜杠、重复条目、越界顶层目录和缺少 exe/metadata
    的 ZIP。修复后的完整总门禁用时 266.22 秒再次通过，证明不是只做了局部打包冒烟。
71. 已完成 C3/C4 新版 RASP 接入收口：现有参考宿主新增一个组合项目迁移场景，在中文和空格
    目录读取只读树、范围、参数和请求，完成工作流后删除全部源输入，把结果移动到另一项目目录，
    再导入相对 artifact、重算 lnL 并新生成 3 条生物地理随机历史。源码宿主 5/5 和安装版 EXE
    5/5 均通过。接入说明同时冻结 v0.1 的进程、schema、终态、恢复、项目移动和责任边界，不约束
    新版 RASP 的语言、GUI、数据库或内部任务类。
72. C3/C4 包刷新时实际遇到一次 Windows 暂存包目录发布的 `Access denied`；原失败清理正确移除
    ZIP、暂存目录和半成品。构建器现只对 Windows access denied、sharing violation 和 lock
    violation 使用 `5/10/20/40/80/160 ms` 有界重试，其他移动错误仍立即失败；随后同一持久包
    构建成功。
73. 已完成 D2 特殊树输入收口：缺失枝长默认拒绝并支持显式统一填充，BOM、混合大小写、嵌套
    NEXUS 注释、`TRANSLATE`、带空格/转义标签和显式树名均进入真实进程测试；多分叉和 `UTREE`
    返回稳定机器错误。官方化石树的 Newick、单树 NEXUS 和多树 NEXUS 共 112 行模型结果一致，
    可移植结果在删除源输入后仍能重放。
74. 已完成 D3 六 preset 生物地理随机历史分布门禁：每个 preset 抽取 20,000 条完整历史，逐节点
    检查状态、split scenario 和 `y/s/v/j` 经验频率与精确后验，不受支持的事件类型严格为零；
    同时复跑官方 BioGeoBEARS 5000 条对 Rust 5000 条的 39 项全路径分布检查并全部通过。
75. 已完成 D4 大状态空间资源门禁：状态分配前用精确组合数估计规模，可选 `max_states` 只控制
    执行资源而不进入科学模型身份。真实 100-tip 输入的 20/30 区域、`max_range_size=5` 分别以
    21,700/174,437 个状态成功计划；30 区域、`max_range_size=15` 的 614,429,672 个状态在
    100 万上限处稳定提前拒绝，未尝试巨额分配。
76. D2-D4 接入全部检查后，Windows v0.1 科研试用包总检查首次一次通过：410 项 Rust 测试、
    Clippy 零警告、完整 BioGeoBEARS 语义与随机历史分布、发布打包安装、公开示例、真实工作流和
    新版 RASP 参考宿主总耗时 284.562 秒。不可变证据为
    `validation/benchmark-runs/v0.1-release-candidate-20260824T041348Z-37312.tsv`。
77. 已完成 E3 GitHub 科研软件打包：Windows v3 包记录文件清单、SHA-256、构建环境、源码清单、
    版本文档和第三方许可证。未签名包可以直接构建和安装，不再要求额外“内部候选”确认参数。
    Authenticode 和 CI 仅保留为可选信息，不影响 GitHub 公开资格；项目许可证随后确定为 `GPL-3.0-or-later`。
78. E4 已完成确定性磁盘写满故障回归：分别在数据行写入、表同步和检查点临时文件写入中注入
    `StorageFull`，均清理临时检查点并回滚到上一个已提交检查点，恢复后八张生物地理随机历史表
    与一次性基线逐字节一致。Windows PC 稳定性脚本会循环六 preset 优化、4096 条 compact
    随机历史和深度检查，并核对每轮科学指纹、峰值 working set 和磁盘空间。正式运行持续
    7,210.72 秒，共完成 367 轮、1,503,232 条随机历史和 17,664,484,003 bytes 累计逻辑写入；
    两个科学指纹零漂移，最高 working set 41,398,272 bytes，367 份 stderr 全为空。受测 EXE、
    首轮完整结果和运行上下文已封存在
    `validation/benchmark-runs/windows-pc-stability-2h-20260824T084700Z/`。
79. E3/E4 收口后的 Windows v0.1 总检查再次一次通过：411 项 Rust 测试、Clippy、完整
    BioGeoBEARS 对照/分布语义、v3 打包来源与安装规则、低空间预检、公开示例、真实工作流
    和新版 RASP 宿主总耗时 288.82 秒。最终 release EXE 哈希为
    `049b23e8bc29469652739b45cca71f3da76171c7843e79ef320190a6ca16a7be`；它另跑 10 轮、40,960 条
    随机历史并复现两小时证据的两个科学指纹。总门禁证据为
    `validation/benchmark-runs/v0.1-release-candidate-e3e4-20260824.tsv`。
80. 已把 Windows 发布政策调整为 GitHub 科研软件模式：未签名包不再与“不可公开”绑定，构建和安装不再
    要求额外内部确认参数；代码签名和 CI 仅作为可选增强。实际未签名打包、解压、安装、文件损坏拒绝、
    公开示例、真实数据工作流、大状态空间、PC 冒烟和新版 RASP 参考宿主均通过。项目采用
    `GPL-3.0-or-later` 后，公开标志已更新为 `true`。
81. 已完成 GitHub 科研软件首页和来源记录：根 `README.md` 现在说明软件定位、已验证平台、六模型能力、
    单模型/多模型/生物地理随机历史命令和文档入口。首页的 DEC 示例从 50 次优化迭代提高到 200 次，
    现以 123 次似然计算收敛；结果重放 lnL 逐位一致，100 条 compact 分片随机历史深度检查 25 个文件、
    1,734 行且 0 违规。这条首页流程已加入发布自动检查。BioGeoBEARS `1.1.3` 官方测试数据的
    `GPL-2.0-or-later` 来源、上游修订号和许可全文已入库。
82. 项目所有者决定整个仓库使用 `GPL-3.0-or-later`。完整 GPL v3 文本已加入根目录，Cargo
    工作区与两个 crate 使用同一 SPDX 标识；Windows 包和安装目录包含项目许可证，发布状态更新为
    `public_research_release_candidate/public_distribution_allowed=true`。411 项 Rust 测试、Clippy、
    首页公开示例以及未签名 Windows 包的构建、解压、安装和安装后科学检查均通过。
