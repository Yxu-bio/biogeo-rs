# BioGeoBEARS-like 统一框架架构

## 目标

项目实现的是一套可配置的历史生物地理似然框架，不是把 DEC、DEC+J、
DIVALIKE、DIVALIKE+J、BAYAREALIKE 和 BAYAREALIKE+J 分别写成互不相干的算法。

模型名称只负责提供 preset：

```text
ModelConfig::preset_dec(d, e)
ModelConfig::preset_dec_j(d, e, j)
ModelConfig::preset_divalike(d, e)
ModelConfig::preset_divalike_j(d, e, j)
ModelConfig::preset_bayarealike(d, e)
ModelConfig::preset_bayarealike_j(d, e, j)
```

所有 preset 使用同一个状态空间、沿枝传播、节点合并、树 pruning、posterior
和优化基础设施。

框架发布为命令行计算引擎，后续由新版 RASP 通过版本化进程协议调用。Rust 不重复实现
GUI、绘图或报告层，也不根据旧版 RASP 的内部结构设计核心接口。

## 当前数据流

```text
StateSpace + observation inputs
              |
              v
       ObservationModel
 exact / ambiguity / detection
              |
              v
       tip likelihoods ----------------+
                                      |
ModelConfig                           |
  |                                   |
  +-> anagenesis -> Q / branch         |
  |                  segments          |
  +-> cladogenesis -> split table      |
                                      v
                            LikelihoodEngine
                              |       |       |
                              |       |       +-> split posterior
                              |       +----------> node-state posterior
                              +------------------> pruning lnL
```

`LikelihoodEngine` 不知道当前 preset 叫 DEC 还是 DEC+J。它只消费 branch
propagator、split table 和 tip likelihood。均质模型的 branch propagator 包含一个
Q；时间分层模型则让每条枝包含多个按年龄排列的 Q segment。这样新增模型时，应
主要增加配置和 scenario 生成规则，而不是复制 pruning 实现。

末端观测同样不属于 preset。精确范围、不确定范围和 detection 分别把各自输入转换为统一的
`TipLikelihood`；引擎只消费每个 tip 在状态空间上的非负 likelihood 向量。因而加入 `?`
不需要修改 Q、split generator 或 pruning，也不会出现为了某个 fixture 单独调整 lnL 的分支。

## 当前类型边界

- `StateSpace`：区域组合、`max_range_size`、null range 和稳定状态顺序。
- `TipRangeConstraint` / `ParsedTipRanges`：保存 `0/1/?` 的必含、必不含和未知约束，并在
  给定 `StateSpace` 后生成统一 `TipLikelihood`。严格精确解析和 ambiguity opt-in 分离。
- `TipLikelihood`：末端观测与似然引擎之间的公共边界；精确范围、不确定范围和 detection
  都必须在进入 pruning 前转换成这个类型。
- `DecAnageneticModel`：`d/e/a` 沿枝过程、非分时期 `branch_length^b`、方向性
  dispersal modifier、按区域 extirpation 倍率和 time-stratified Q 构建。`a` 只连接
  两个 singleton range；`b!=1` 与分时期过程按 BioGeoBEARS 限制互斥。
- `DispersalMultiplierMatrix`：有方向的 area-to-area 非负矩阵；支持经过检查的
  元素幂和逐元素组合。
- `AreaSizeVector`：严格正的原始区域面积；按候选 `u` 生成最终灭绝倍率。
- `ExtirpationMultiplierVector`：按区域排列的非负有效灭绝倍率。
- `BinaryAreaMatrix`：带区域顺序的二值矩阵，用于解析每期 areas-allowed 或
  adjacency 输入；它与连续 dispersal multiplier 类型分离，避免把约束误当倍率。
- `RangeStateConstraint` / `StateMask`：把二值区域矩阵转换成 master state space 上的
  允许状态 mask，并负责边界投影。
- `TimeStratifiedAnagenesis`：按距今年龄由年轻到年老排列的时期列表；每个时期可包含
  manual dispersal、原始地理/环境距离变换后的有效矩阵和 area-specific
  extirpation 倍率，也可包含范围状态约束。`TimeStratifiedDispersal` 保留为旧两列输入
  的兼容类型。
- branch propagator：均质 Q 或每条分支的 piecewise-Q segments；pruning 与
  posterior 共用同一传播接口。
- `CladogenesisConfig`：`y/s/v/j` 事件权重与 `mx01y/s/v/j` daughter-size
  最大熵约束；构建 split table 时可接收与 Q 相同的有效 dispersal matrix。
- `ModelConfig`：把沿枝与节点过程组成一个可执行模型。
- `ParameterSpec` / `ParameterTable`：以固定、自由、联动三种模式声明 BioGeoBEARS
  23 行参数，并保存边界与 `Linear/Log/Logit` 优化坐标；表达式依赖图负责检测未知引用、
  循环和不会到达似然目标的自由参数。
- `LoadedParameterModelContext`：CLI 已加载的静态或分时期原始修饰上下文。它在每个候选
  参数点把 `ResolvedParameters` 转成同一个 `ModelConfig`，并按候选 `x/n/u/w` 重建有效
  dispersal/extirpation 修饰。
- `LikelihoodEngine`：固定参数 likelihood、祖先范围 posterior 和节点分裂
  posterior 的统一执行器。
- `HistorySkeletonSampler`：由根向叶做条件随机回溯，直接复用同一 branch propagator、
  split table、root prior 和 pruning 结果，联合抽取节点状态、daughter state 与分支
  终止状态；`StochasticMapSampler` 是同一实现的公共别名，并继续按真实 Q segment
  抽取端点条件 CTMC bridge。两者都不拥有另一套模型语义。
- `BiogeographicStochasticMap`：包含联合历史骨架、逐分支/逐时期状态链以及已去除虚拟
  自转移后的 `d/e/a` 事件与时间；`a` 被保留为一次原子 range-switching，而不是拆成
  一次 extirpation 加一次 expansion。
- DEC/DEC+J 便捷函数：只构造 preset，再委托给统一引擎。
- DIVALIKE preset：复用同一 `d/e` Q 和引擎，只把节点过程配置为
  `y=1, s=0, v=1, j=0`，并固定 `mx01v=0.5`。
- DIVALIKE+J preset：在同一配置上按 BioGeoBEARS 链接
  `y=v=(2-j)/2, s=0`，保留 `mx01v=0.5`，优化上界为 `j=1.99999`。
- BAYAREALIKE preset：复用同一 `d/e` Q 和引擎，节点只保留 exact range-copying，
  即 `y=1, s=0, v=0, j=0` 与 `mx01y=0.9999`。
- BAYAREALIKE+J preset：按 `y=1-j, s=v=0` 加入 founder event，保留
  `mx01y=0.9999`，优化上界为 `j=0.99999`。
- 优化器：`optimize_de_with_model` 通过 model factory 改变 preset 参数并反复调用
  同一个引擎；DEC、DIVALIKE 与 BAYAREALIKE wrapper 不拥有另一套 likelihood。
- 通用 +J 优化器：`optimize_decj_dej_with_model` 只更新 `d/e/j`，由 model factory
  在每次目标函数计算时重建带固定 modifier 的完整 `ModelConfig`；固定与优化路径
  不使用两套 C 表。DEC+J、DIVALIKE+J 与 BAYAREALIKE+J wrapper 只选择不同 preset
  和 BioGeoBEARS 参数边界。
- 指数优化器：`optimize_de_exponent_with_model` 在 log 空间搜索 `d/e`，同时在显式
  有界的线性坐标中搜索一个指数。model factory 每次重建同一 `ModelConfig`，因此
  `dec-x-optimize`、`dec-n-optimize` 和 `dec-u-optimize` 不拥有单独的 likelihood
  实现。
- 联合修饰优化器：`optimize_de_xnu_with_model` 在同一目标函数中搜索
  `log(d)/log(e)/x/n/u`；`DecXnuOptimizationConfig` 保存五个初值、边界、步长和
  多起点策略，`dec-xnu-optimize` 只负责解析输入并构造通用 model factory。
- 动态维度参数优化器：`optimize_parameter_table` 从参数表自动发现自由维度，解析所有
  联动参数，再调用 model factory 和同一个 `LikelihoodEngine`。`model-evaluate`、
  `model-optimize` 与六种 `parameter-template` 构成首个版本化通用用户入口；文件契约和
  尚未开放参数的门禁见 `docs/parameter-table.md`。

## 沿枝修饰与时间分层

对于从当前范围向新区域 `b` 的扩张，方向性矩阵使用 BioGeoBEARS 的求和语义：

```text
effective[a,b] = manual[a,b]^w * distance[a,b]^x * envdistance[a,b]^n
rate(range -> range+b) = d * sum(effective[a,b] for a in range)
rate({a} -> {b}) = a_parameter * effective[a,b]
extirpation_multiplier[a] = area_size[a]^u
rate(range -> range-a) = e * extirpation_multiplier[a]
founder_pairwise(A,D) = mean(effective[a,d] for a in A, d in D)
raw_founder_weight(A -> A+D) = j * maxent01j(A,D) * founder_pairwise(A,D)
```

全 1 矩阵自然退化为原来的 `d * range_size`。矩阵允许非对称值和 0，因此可以表达
单向扩散和禁止连通。地理距离与环境距离分别按 BioGeoBEARS 的 `distance^x` 和
`envdistance^n` 逐元素变换，再和手工倍率逐元素相乘。区域灭绝向量是已经变换后的
有效倍率。`--extirpation-multipliers` 直接接收这个最终向量；`--area-sizes` 则接收
严格正的原始面积，并由固定或候选 `u` 计算 `area_size^u`。手工倍率由固定或候选 `w`
计算 `manual^w`，同时修饰 `d`、`j` 和 `a`。这些输入边界避免把
已经变换的倍率再次取幂。当前 CLI 可固定 `x/n/u` 后优化 `d/e`，可通过三个
`dec-*-optimize` 命令一次释放一个指数，也可通过 `dec-xnu-optimize` 同时释放
`d/e/x/n/u`。这些入口既接受一套静态地理距离、环境距离和原始面积，也接受在每个
时期分别提供三类原始输入的 extended strata。候选 `x/n/u` 每次都会从各时期原始值
重建有效 dispersal/extirpation 修饰，因此固定似然、单指数优化、五维联合优化和
二维 profile 使用同一套分段 Q 语义。

当 `j>0` 时，同一有效 pairwise matrix 还进入 founder-event 的节点权重。`A` 是
祖先范围，`D` 是完全位于祖先范围之外的 founder daughter；矩阵方向固定为祖先区域
来源 `a` 到新 daughter 区域目标 `d`，多区域 daughter 使用全部 `A x D` 元素的算术
均值。该因子先乘到 j scenario 的未归一化权重，再与同一祖先行的 y/s/v/j scenarios
共同归一化。全 1 矩阵严格退化为无 modifier 的旧行为；0 可以删除特定方向的 founder
scenario。area-specific extirpation 不进入 C 表，只修改 Q。

当所有原始面积完全相同时，`area_size^u` 对各区域给出同一个倍数，该倍数可被 `e`
吸收，因此 `e/u` 结构性不可辨识。`dec-u-optimize` 在进入似然优化前拒绝这种输入。
面积有差异仍不保证数据提供足够信息，所以正式 golden 同时检查多起点、内部解和
`e` 保持内部时的 u 边界解，不把 `e=0` 导致的任意 u 当作有效边界证据。

自由指数优化默认使用边界内初值和多起点 Nelder-Mead，并输出 `converged_starts`、
`starts` 与 `exponent_bound`。BioGeoBEARS 对照同时覆盖内部最优和边界最优；边界
案例使用预先定义的 profile 网格，因为其 L-BFGS-B 曾在投影梯度仍很大时返回
convergence code 0。profile 只用于建立外部 golden，不改变 Rust 核心公式。

联合指数点估计前使用通用 `DecPairProfileConfig`。它接受两个任意命名、严格递增的
网格轴；在每个网格点固定这两个参数，并用同一棵树、状态空间和
`LikelihoodEngine` 优化 `d/e`。结果保留完整网格、每点收敛状态、最优点是否处于
所选网格边缘，以及 `delta_lnL <= 2.995732` 的近似二维 95% likelihood-ratio
支持区。这个阈值依赖大样本近似，边界或小树上只能视为诊断，不能当成严格置信区间。

`likelihood_weighted_correlation` 用 `exp(-delta_lnL)` 加权网格点，描述高似然区域
是否呈线性斜向脊线。它接近 `+1/-1` 只是 ridge 线索，接近 0 也不能证明参数可辨识；
必须同时检查支持区跨度和网格边界。四区域 6-tip 全修饰 fixture 中，`x` 与 `u`
支持区分别覆盖全部扫描值，这只说明该数据不适合解释五维点估计，不限制联合优化器
本身。profile 与点估计是并列工具：前者诊断具体数据的信息量，后者完整实现模型
参数空间。profile 中个别网格点若无法得到有限 lnL，会被标为失败点并从最优值、
支持区和加权相关计算中排除，不会让整张截面丢失。

原始面积除以其几何均值只用于改善外部验证的数值尺度。对自由 `u`，共同缩放面积会
被 `e` 的重参数化吸收，因此不会改变 lnL、`x/n/u` 或相对区域效应；Rust 核心不要求
这种归一化，也不会根据 fixture 名称自动执行。

距离矩阵、指数、组合结果和区域倍率都要求有限且非负。距离矩阵对角线不参与区域
扩张，允许使用 BioGeoBEARS 文件常见的 0，并在有效矩阵中规范化为 1；非对角的
0 距离配负指数会产生无穷倍率，因此在构建 Q 前明确报错。时间分层文件给出逐渐
增大的 `oldest_age`；extended 格式的每行可分别引用 manual、distance、
environment distance 和 area size 文件，路径相对 schedule 文件解析。树节点年龄按
最长 root-to-tip 深度作为现在计算，非现生 tip 因较短路径得到正年龄。

一条枝跨越时间边界时会被切成多个 segment。向根计算 conditional likelihood 时，
从年轻段到年老段依次作用；计算 outside likelihood 的转置传播时顺序反转。这两条
路径由同一个 branch propagator 实现，避免 fixed likelihood 与 posterior 使用不同
时间语义。

### 范围状态约束与时期边界

七列 schedule 在原五列后追加 `areas_allowed` 与 `areas_adjacency`；八列格式再追加
`allowed_ranges`。未提供的时期仍用 `-` 或 `none`。三者都改变该时期允许存在的范围状态，
不只是把某些 dispersal rate 设为 0：

- `areas_allowed` 精确复现 BioGeoBEARS 的既有规则：null range 总是允许；非空范围
  取编号最小的首个区域，并要求该行指向范围内每个区域的值均为 1。
- `areas_adjacency` 要求范围内所有有序区域对在邻接子矩阵中均为 1；singleton 只检查
  自身对角项。
- `allowed_ranges` 是版本化的显式状态集合，支持 BioGeoBEARS 脚本直接赋值
  `lists_of_states_lists_0based` 的用法。它与矩阵约束同时出现时取交集，不会改变另外两种
  约束的既有定义。
- 全局仍保留稳定 master state indices；每期 Q 跳过禁用来源和目标状态，每期 C 表
  删除含禁用 ancestor/daughter 的场景，并在每个允许祖先行内重新归一化。
- 枝跨时期边界时，进入下一时期前把该时期禁用状态的 conditional/outside likelihood
  置 0，不向其他状态重新分配。节点按距今年龄选择对应 C 表，equal root prior 只在
  根时期允许的状态上均分。

因此固定 lnL、node posterior、split posterior 和参数优化消费的是同一套 Q/C/mask，
没有独立的“约束版 pruning”。

pairwise dispersal modifier 已完整支持 `j>0`。静态模型由 `ModelConfig` 同时用一套
有效矩阵构建 Q 与 C；时间分层模型则为每个时期构建对应 C 表，内部节点按距今年龄
选择时期。固定 likelihood、node posterior、split posterior 与 `d/e/j` 优化都消费
同一个过程对象，不存在只修饰 Q 的 DEC+J 快捷路径。

## 外部语义契约

两种外部程序的角色必须分开：

| 参考 | 角色 | 是否决定 Rust 语义通过 |
| --- | --- | --- |
| BioGeoBEARS | BioGeoBEARS-like 框架语义 golden | 是 |
| LAGRANGE-ng | 独立 LAGRANGE-ng 语义、兼容性和性能参考 | 否 |

BioGeoBEARS golden 覆盖固定参数 lnL、祖先范围 posterior、split scenario
posterior 和参数优化结果。LAGRANGE-ng 有自己的冻结参考和运行输出；其 split
scenario 数量或权重与 BioGeoBEARS 不同时，应保留为显式语义差异。

BioGeoBEARS 当前 stratified uppass 源码把环境距离固定取为第一个时期，而不是当前
时期；官方 Psychotria M4b 的重复时期输入在高 `d` 优化时也会让 stratified 目标值与
数学等价的静态目标值相差约 `0.0016`。验证清单因此同时保留直接 stratified 审计值
和严格静态等价参考，既不复制参考实现缺陷，也不删除差异证据。

detection 五维联合优化进一步要求 R/Rust 都使用同一组显式三起点。BioGeoBEARS
stratified 的优化目标从 `optim_result$value` 读取，不能被运行后可能受 uppass 影响的
`total_loglikelihood` 字段替换。官方重复五时期案例中，BGB 直接/静态最佳目标相差
`1.84e-6`；Rust 在静态严格参考点的两条路径相差 `2.55e-12`。

adjacency fixture 还暴露出 BioGeoBEARS stratified split 输出的内部不一致：node-state
posterior 将禁用的 A+C、B+C、A+B+C 置 0，但同一结果对象的 split 表仍给这些祖先
非零质量。固定 lnL 与 node posterior 继续作为 golden；该 adjacency split 表标记为
不可信，不驱动 Rust 改写。Rust 单测强制检查每个节点按 ancestor 汇总的 split 概率
必须等于该节点的 state posterior。

## Daughter-size 最大熵语义

`CladogenesisRangeSizeConfig` 保存四个事件特异约束：

- `mx01y`：range-copying，仅取 daughter size 等于祖先范围大小的概率。
- `mx01s`：subset sympatry，按真子集大小取概率。
- `mx01v`：vicariance，按两个 daughter 中较小者的范围大小取概率。
- `mx01j`：founder event，按祖先范围之外的新 daughter 范围大小取概率。

每个原始 scenario 权重都是“事件权重乘 daughter-size 概率”，最后在同一祖先
状态内统一归一化。概率分布采用 BioGeoBEARS 的离散最大熵解释，并与其 R 实现
一样保留三位小数。默认四个值都由 `mx01=0.0001` 联动，因此复现原来的
singleton daughter 行为；非默认值可以放开 widespread range-copying、平衡
vicariance 和多区域 founder daughter。

Rust 复刻 BioGeoBEARS 实际调用的 `rexpokit::maxent()` 一维 improved iterative
scaling 语义：均匀先验、`(daughter_size_count + 1) * mx01*` 目标均值、最大概率变化
不超过 `1e-7` 时停止，最后按 R 的规则保留三位小数。该停止条件在理论边界上可能留下
约 `0.001` 的相邻尺寸概率；由于它会进入 C 表并改变 lnL，这里把它视为需要对齐的
BioGeoBEARS 计算语义，而不是替换成数学上的退化点质量。十分位及其
`±0.00025/±0.0005` 舍入边界压力点已由固定剖面 golden 锁定。

语义来源需要与发布许可分开记录：本地验证使用的 `rexpokit 0.26.6.15`
`DESCRIPTION` 声明 `GPL (>= 2)`，其 `itscale5` 注释又说明例程来自 FD。当前 Rust
实现没有链接该二进制，但正式选择项目许可证和发布源码前仍需完成第三方来源与兼容性
审计，不能仅凭“重写成 Rust”推断许可证义务已经消失。

## 不允许的对齐方式

- 根据 fixture 名称、tip 数量或区域数量改变核心公式。
- 在 pruning 中判断“当前参考工具是谁”。
- 为追平最后几位小数加入没有模型含义的常数。
- 把 LAGRANGE-ng 的 split 规则悄悄混入 BioGeoBEARS DEC preset。

如果未来需要支持 LAGRANGE-ng 兼容模式，应增加明确命名的 preset 或 profile，
并用独立 reference 验证，而不是改写 BioGeoBEARS preset。

## 条件历史抽样边界

条件历史抽样不能把每个节点的 marginal posterior 独立采样。当前实现依次抽取根状态、
给定祖先状态的 split scenario，以及给定 daughter state 的分支终止状态，使一条历史中：

```text
split.left/right == child branch start state
branch end state == child node state
```

分支转移行按 `(edge, start_state)` 延迟计算并缓存；均一 Q 和 piecewise-Q 仍通过同一
`BranchPropagator::propagate_transpose` 接口。分层 split table 按节点年龄选择，状态
mask 同时约束根、节点和时期边界。详细公式、API 和验证见 `docs/bsm-phase1.md`。

完整 BSM 在 `HistorySkeleton` 给出的分支起止状态之上，直接复用
`OwnedBranchPropagator` 内的实际 Q segments。均质 segment 使用端点条件
uniformization bridge；跨时期分支先按前向转移与后缀 likelihood 抽取边界状态，再在
每个 segment 内抽 bridge。实现不会根据端点差异事后拼接任意路径。详细公式、API、
时间方向与验证见 `docs/bsm-ctmc-bridge.md`。

## 后续实现顺序

1. 已完成：保持 DEC/DEC+J 的 BioGeoBEARS golden 全量通过。
2. 已完成：把 `mx01/mx01v/mx01s/mx01y/mx01j` 扩展成完整的 split-size
   weight 配置，并加入非默认复杂 golden。
3. 已完成：用同一 scenario generator 增加 DIVALIKE preset，并对齐固定参数
   likelihood 与 split posterior golden，包括四区域 `2+2` vicariance。
4. 已完成：补齐 DIVALIKE node-state posterior golden，并把 `d/e` 优化器提升为
   接受 model factory 的通用入口；DIVALIKE 三组优化与 BioGeoBEARS
   `optimx/bobyqa` 收敛结果对齐。
5. 已完成：增加 BAYAREALIKE preset，并以 2-area、4-area 和 5-area 全状态夹具
   对齐固定 lnL、node-state posterior、split posterior 与 `d/e` 优化 golden。
6. 已完成：加入方向性 dispersal multipliers、branch/time context 和
   time-stratified piecewise-Q；固定、posterior 与 `d/e` 优化均有 BioGeoBEARS golden。
7. 已完成：加入固定距离指数 `x`、环境距离指数 `n`、手工矩阵逐元素组合和
   area-specific extirpation 有效倍率；固定、posterior 与固定修饰下的 `d/e`
   优化均有 golden。
8. 已完成：增加一次释放一个 `x` 或 `n` 的有界多起点优化，覆盖内部解、边界解、
   固定伴随指数和全修饰组合，并用 BioGeoBEARS 自由优化/profile golden 对齐。
9. 已完成：增加原始 `AreaSizeVector`、固定 u 与自由 u；两个内部解和一个真实下界
   解通过 BioGeoBEARS 对照，完全相同的面积在优化前作为不可辨识输入拒绝。
10. 已完成：增加 `x-n`、`x-u`、`n-u` profile/ridge 诊断，以及联合
    `d/e/x/n/u` 优化。五维参数进入 Q 和 likelihood 的语义已在 BioGeoBEARS 官方
    197-tip Conifer 数据基础上通过双方参数处的交叉固定重算锁定。
11. 已完成：增加按时期 manual、原始地理/环境距离和 area size；固定似然、两类
    posterior、`d/e`、单指数、联合 `d/e/x/n/u` 优化与二维 profile 共用同一 schedule。
12. 已完成：实现按时期 areas-allowed 与 adjacency；Q、C、root prior、边界投影、
    fixed/posterior/optimization 已用官方与合成 BioGeoBEARS fixture 锁定。
13. 已完成：实现 pairwise modifier 对 founder-event `j` 的节点修饰，并以静态、
    时间分层和优化 golden 锁定。
14. 已完成：补齐 DIVALIKE+J 与 BAYAREALIKE+J；六个正式 preset 共用同一引擎，
    复杂 4-area/5-area fixture 对齐 fixed、两类 posterior、split weight 和优化。
15. 已完成：BSM 第一阶段条件历史骨架直接消费拟合时使用的 branch propagator、
    split table 和 pruning likelihood；固定 seed API/CLI、分层 Q、状态约束和经验
    posterior 回归已覆盖。
16. 已完成：实现条件 uniformization CTMC bridge，抽取分支内部 range expansion/
    local extirpation 事件及时间；piecewise-Q 边界状态、state mask、公共 API、固定 seed
    CLI 和解析/统计回归均已覆盖。
17. 已完成：在 BioGeoBEARS 官方 BSM 案例上分别抽取 5000 条生物地理随机历史，冻结事件数、
    事件类型、时期占比、状态占据时间和“时期 × 状态”占据时间，并以均值 Monte Carlo
    标准误和经验 CDF 差做 39 项分布门禁；不要求相同 seed 的逐路径对照。
18. 已完成：增加逐条随机历史消费 API、锁定的 `biogeo-bsm-tsv-v1` 兼容 writer，以及
    full/compact/summary 三种 v2；v2 包含不可变 ID 字典、稀疏占据、`a` 计数和困难分支诊断。
    官方 5000 条随机历史分布门禁已直接读取 summary v2，并保持 39 项全通过。
19. 已完成：增加 `indexed-chacha12-v1` 按样本索引随机流、线程安全 prepared sampler、
    固定大小 worker pool、有界窗口和顺序 writer；CLI 自动/显式 worker 配置与
    1/2/4/8/16 worker 八表逐字节门禁已覆盖。
20. 已完成：增加超长/高事件率 segment 的自适应 uniformization；传播按半群性质细分，
    bridge 按端点条件中间状态递归分解，数值子段不会改变时期或事件语义。
21. 已完成：增加每条随机历史沿枝事件硬预算；预算跨分支和时期累计，自适应 bridge
    增量检查；预算或采样失败不会把失败样本交给 writer，元数据记录完整写入调用的有序
    前缀。
22. 已完成：增加八表崩溃一致提交协议；八表全部刷新并同步后才发布包含各表字节长度和
    运行指纹的不可变检查点。失败时回滚，恢复时截断残尾并按绝对 sample index 续采；
    不同 worker 数恢复后仍与一次性完整运行逐字节一致。
23. 已完成：增加共享取消令牌和可选截止时间；核心采样在样本、分支和时期边界协作停止，
    CLI 接入 `Ctrl+C` 与 `--bsm-time-limit-seconds`。流式停止提交完整前缀并记录原因，恢复后
    仍与一次性运行逐字节一致。
24. 已完成：增加按有序样本前缀累计的任务总事件预算；超限时提交准确前缀和累计事件数，
    v1 检查点可从事件计数表迁移，调整总上限续跑仍与一次性结果逐字一致。
25. 已完成：增加完整历史窗口内存预算；以拓扑、时期 segment 和单样本事件上限计算保守
    字节上界，自动收缩 worker/在途数并记录审计元数据。该预算不冒充进程 RSS 硬上限。
26. 已完成：增加固定区间分片 writer；完整分片目录不可变，活动分片保留 v2 检查点，
    manifest 可重建。分片拼接、分片内受控停止续跑、发布崩溃窗口和损坏拒绝均已覆盖。
27. 已完成：增加核心 pause token 和 `--bsm-interactive`；pause/resume 在既有协作安全点等待/
    唤醒，status 使用原子有序前缀进度，cancel 复用耐久提交。暂停确定性、EOF、取消、deadline
    以及 Windows release 分片任务均已覆盖。
28. 已完成：参数依赖图进入动态维度优化执行层；每个参数显式声明 `Linear/Log/Logit`
    坐标，优化器按稳定顺序消费任意 `Free` 维度并解析 `Derived` 链。DEC、DEC+J 专用路径
    交叉一致，自定义 `y/v` 自由与 `s=y/2` 联动组合通过固定重算。
29. 已完成：为独立释放的 `y/s/v/mx01/mx01y/mx01s/mx01v/mx01j` 建立
    BioGeoBEARS 优化与固定剖面 golden。5-area、8-tip、`max_range_size=5` 案例覆盖
    240 个剖面点；MaxEnt 参数同时覆盖三位概率舍入边界，并记录 BGB 优化器结果、
    profile 筛查结果和两者差值。Rust 通用优化器对平滑权重点估计严格对齐，对台阶参数
    要求不低于筛查后的 BGB 最佳似然，且所有 BGB 固定点均可交叉重算。
30. 已完成：将距离、环境、面积和时期输入整理为通用参数模型构建上下文，并提供版本化
    参数配置与 CLI。
31. 已完成：实现 `a/b/w` 的 Q、枝长和手工倍率指数语义；固定、自由和联合组合进入通用
    优化器，并以官方 Psychotria M4 profile 对齐 BioGeoBEARS。
32. 已完成：实现 `mf/dp/fdp` detection/observation 末端似然、严格计数输入契约和
    动态参数优化；官方 Psychotria 固定 profile、tip-state 相对似然和优化均已对齐。
33. 已完成：建立 detection 与距离、环境、面积、手工倍率、枝长缩放、节点事件和
    `mx01*` 联合启用的固定 golden，覆盖静态与官方五时期输入；另以显式多起点完成
    `x/j/y/v/mf` 五维联合优化对照。
34. 已完成：组合祖先范围和 split posterior 对照；对 BioGeoBEARS 已知不可靠的重复时期
    uppass 保留直接审计值，并以数学等价静态结果严格验收，同时要求 Rust 两条路径一致。
35. 已完成：在官方重复五时期输入下同时释放 `x/j/y/v/mf`；R/Rust 对称三起点、双方
    最优坐标固定重算和 Rust stratified/static-equivalent 等价性均已进入门禁。
36. 已完成：增加首版 `biogeo-analysis-result-v1`、原子非覆盖发布、输入/内部文件指纹和
    `biogeo-model-identity-v1`；`model-bsm` 严格重放 lnL 后复用同一 sampler/writer。
    exact ranges、detection、时期修饰、跨线程确定性及固定 DEC 八表等价均有回归。
37. 已完成：审计 `mx01r` 的所有源码出现点，并在复杂静态与官方五时期案例上扰动
    `0.0001/0.5/0.9999`；lnL、root/split posterior 和时期 cladogenesis 权重严格零差。
    该行作为 BioGeoBEARS 1.1.3 的固定兼容空操作保留，不加入无意义的优化维度。
38. 已完成：建立时期、逐期状态约束、founder event、非默认 `mx01*` 和 detection 同时
    启用的全栈组合门禁；fixnode 条件似然绕开 BGB stratified uppass 缺陷，固定、后验、
    split、`d/e/x/n/u` 优化和 20,000 条生物地理随机历史经验分布均已验收。
39. 已完成：修复稀有条件端点下 Poisson 累计质量舍入导致的 bridge 假不收敛；普通路径
    保持原停止判据，机器精度区使用递推尾部上界，并加入有向 18 状态链回归。
40. 已完成：Newick 单引号/转义标签、UTF-8、方括号注释、内部标签和缺失枝长严格策略；
    root edge 明确拒绝。官方年龄 `0.09` 化石末端已通过固定、优化、两类 posterior 和
    20,000 条生物地理随机历史门禁。
41. 已完成：BioGeoBEARS 超短枝直接祖先语义；显式阈值、严格 `<` 边界、恒等节点
    likelihood/uppass、posterior、split 省略、随机历史和结果重放已由官方派生双树 golden
    与内部结构回归锁定。
42. 已完成：单树 NEXUS/TRANSLATE 输入层；APE 从官方化石树导出的 NEXUS 与原 Newick
    固定似然及两类 posterior 标准输出逐字节一致，多树和 UTREE 不静默选择。
43. 已完成：`validate-inputs` 复用正式树、范围和直接祖先解析路径，输出版本化树/范围摘要；
    重复区域、重复/缺失 tip 和非二叉树在拟合前明确拒绝。
44. 已完成：显式命名多树 NEXUS 选择与规范 Newick 转换。默认解析仍拒绝多树；选择名称
    贯通校验、全部分析/优化、结果重放和 BSM 指纹。官方 APE 多树 fixture 与原 Newick、
    单树 NEXUS 的模型语义输出逐字节一致，转换不补枝长、不二叉化也不改拓扑。
45. 已完成：BioGeoBEARS `0/1/?` 不确定范围观测。官方 Psychotria 衍生案例锁定 304 个
    tip likelihood、固定 lnL、288 个节点 posterior 和 `d/e` 优化；底层函数的全未知、
    absence-only、混合约束逐格对齐，分析结果重放和跨线程随机历史也有回归。
46. 已完成：首版 `model-batch` 在一个共享数据配置上运行多张参数表，逐模型复用
    `model-optimize + biogeo-analysis-result-v2`；原子初始化、严格身份恢复、收敛资格和
    AIC/AICc/Akaike weight 稳定表已有官方函数 golden 与 Psychotria 六模型实跑。
47. 已完成：明确 Rust CLI 与新版 RASP 的职责边界；主入口支持版本化机器错误、稳定分类和
    退出码，同时保留人工错误模式。该协议只定义子进程边界，不依赖旧版 RASP。
48. 已完成：批量层按科学可比性分层。`model-batch` 在同一数据集内首错后继续并写不可变
    attempt；`dataset-batch` 每行引用独立模型表和完整分析配置，支持不同 Newick/命名多树
    NEXUS、观测和修饰输入，恢复时逐字节校验身份。数据集之间不共同归一化模型权重。
49. 已完成：模型平均祖先范围机器结果。每个入选分析结果严格重放，同树同状态 posterior
    按 AIC/AICc 权重在线累加；AICc 不对不完整候选子集重新归一化。v2 现已增加跨模型
    cladogenetic split scenario 并集、事件类型和缺失为零语义。
50. 已完成：抽取通用 `ExecutionCancellationToken`，为动态维度参数优化增加评估间安全点和
    迭代回调；CLI 以 `biogeo-cli-progress-v1` 报告单模型及两级 batch 层级，取消后的 v2
    attempt 明确记录 `cancelled/not_started`，且不发布半成品分析结果。
51. 已完成：分析结果升级为自包含 `biogeo-analysis-result-v2`，内嵌
    `biogeo-input-bundle-v1`。时期表二级依赖结构化收集并重写相对路径，原文作为 provenance
    保留；加载器拒绝路径越界和清单不一致。v1 只读兼容、检查命令和经双重科学重放后原子发布的
    非覆盖迁移命令均已接通。
52. 已完成：建立首批九项 `biogeo-schema-registry-v1` 机器契约；真实 CLI 进程生成 v2 结果和
    输入包，完成 inspect、v1→v2 双重重放迁移、错误及进度输出后逐字段验收。Windows MSVC
    release 可构建为非覆盖 ZIP/目录，payload 由 SHA-256 清单校验，指定目录安装采用暂存后
    原子发布且不修改 PATH；安装后 exe 的真实优化与结果重放进入发布门禁。
53. 已完成：独立随机化石树生成器支持年龄区间、stem/crown/both、MRCA 类群约束、side
    branch/direct-ancestor hook 和确定性多 replicate；随机过程不进入固定树似然。
54. 已完成：模型比较升级为 v3，以符号参数表达式判断嵌套关系并生成带边界风险的 LRT；
    模型平均升级为 v2，覆盖 split scenario。三项新格式进入 registry，真实 CLI 产物逐字段验收。
55. 已完成：增加逐时期显式允许范围集合及八列 schedule，模型指纹、输入包、fixed、通用
    evaluate/optimize 和 `analysis-plan` 共用同一状态掩码。BioGeoBEARS block converter 明确区分
    内置 all-pairs adjacency 与脚本自定义 edge-covered 规则；官方 Ponerinae 1534-tip、7-area、
    `max_range_size=5` 输入复现七时期状态数，CSV 显式 taxon/area 映射与 `.data` 固定 lnL 逐位一致。
    同配置 BioGeoBEARS 固定 lnL 差低于 `8e-6`；d/e 独立优化的 lnL 差低于 `2e-5`，当前 PC 端到端
    优化时间直接配对为 3.83 s 对 1197.49 s，Rust 5 次复测中位数 6.24 s 对应保守约 192x。
56. 下一步：Linux/调度器资源探测仍后置；公开发布前另需确定许可证、第三方许可说明和
    可引用信息。

每一步都先锁定固定参数内部量，再加入优化；性能优化必须保持同一套 semantic
golden 通过。
