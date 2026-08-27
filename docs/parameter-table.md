# 版本化参数表与通用模型命令

## 目标

`biogeo-parameter-table-v1` 把 BioGeoBEARS 风格的参数声明从专用命令行选项中
分离出来。DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE 和
BAYAREALIKE+J 不再要求各写一套优化入口；它们只是 23 行参数表的不同初始配置。

参数表负责声明：

- 参数是固定、自由还是由其他参数联动得到；
- 固定值或优化初值；
- 上下界；
- `linear`、`log` 或 `logit` 优化坐标；
- 联动表达式。

同一张表由固定似然和优化路径共同读取。优化器每次评估候选点时都会重新解析联动值、
构造沿枝 Q、构造节点 split table，并按候选 `x/n/u` 重建静态或分时期修饰；参数表
不是只用于展示结果的元数据。

## 生成 preset

```powershell
cargo run -q -p biogeo-cli -- parameter-template --preset dec
cargo run -q -p biogeo-cli -- parameter-template --preset dec+j
cargo run -q -p biogeo-cli -- parameter-template --preset divalike
cargo run -q -p biogeo-cli -- parameter-template --preset divalike+j
cargo run -q -p biogeo-cli -- parameter-template --preset bayarealike
cargo run -q -p biogeo-cli -- parameter-template --preset bayarealike+j
```

六张冻结示例位于 `examples/parameter_tables/`。生成器输出与这些文件逐字比较，格式或
preset 默认值发生变化时必须显式升级版本或更新验证证据。

## 文件格式

第一条有效行必须是版本，第二条有效行必须是固定表头：

```text
biogeo-parameter-table-v1
name    mode    value   min     max     transform   expression
d       free    0.01    1e-12   5       log
e       fixed   0.02    1e-12   5       log
ysv     derived         1e-5    3       logit       3-j
```

实际文件使用制表符，不是任意空白。空行和以 `#` 开头的整行注释会被忽略。

- `fixed`：`value` 必填，`expression` 必须为空。
- `free`：`value` 是初值且必须位于边界内，`expression` 必须为空。
- `derived`：`value` 必须为空，`expression` 必填。
- `linear`：直接在模型参数坐标中搜索。
- `log`：适合严格正的速率。
- `logit`：把有限开区间映射到无界优化坐标，初值不能正好落在边界上。

表达式只允许有限数字、参数名、括号以及 `+ - * /`。不执行函数调用、R 代码、shell
或文件内容；未知引用、循环依赖、除零、非有限值和联动值越界都会报错。

当前通用命令要求恰好包含 BioGeoBEARS 的 23 个参数名。缺行和额外名称都会报错，避免
拼写错误被当成新参数。

## 固定评估

`model-evaluate` 要求参数表中没有自由参数：

```powershell
cargo run --release -q -p biogeo-cli -- model-evaluate `
  --tree examples/two_tip/tree.nwk `
  --ranges examples/two_tip/ranges.tsv `
  --parameters <fixed-parameters.tsv> `
  --max-range-size 2
```

可追加 `--ancestral-probs` 和 `--split-probs`。输出包含版本、输入路径、lnL、状态空间、
每个参数的声明模式、解析后数值、表达式和边界状态。

## 通用优化

`model-optimize` 要求至少一个自由参数：

```powershell
cargo run --release -q -p biogeo-cli -- model-optimize `
  --tree <tree.nwk> `
  --ranges <ranges.tsv> `
  --parameters <parameters.tsv> `
  --initial-step 0.5 `
  --tolerance 1e-8 `
  --max-iterations 500
```

所有真正到达似然目标的自由组合都使用同一个动态维度优化器。例如可以固定 `d/e`、
释放 `y/v` 并设 `s=y/2`，也可以同时释放 `d/e/x/n/u`。若自由参数没有通过联动图影响
任何 Q、split weight 或 daughter-size 约束，命令会拒绝运行，不会报告虚假的最优值。

复杂似然面可重复提供显式多起点。每个向量使用参数表中稳定的自由参数顺序，并在模型
坐标而非优化器内部坐标中填写：

```powershell
--additional-start -2.4,0.001,0.99,0.001,0.01 `
--additional-start 2.4,2.8,0.01,0.99,0.9
```

每个起点都会经过维数、边界、参数变换和联动表达式校验；输出记录总起点数和收敛起点数。
接口采用显式向量而不是高维笛卡尔网格，避免自由参数增多时搜索成本指数膨胀。

命令行前置 `--progress-format tsv` 后，每个起点初始化、迭代和完成都会输出版本化机器事件；
`Ctrl+C` 在完整似然评估之间协作式取消并返回退出码 130，不发布半成品分析结果。字段与
批处理传播语义见 [`progress-and-cancellation.md`](progress-and-cancellation.md)。

## x、n、u 与时期输入

当参数表中的 `x`、`n` 或 `u` 不是固定 0 时，必须提供对应原始输入：

```text
x -> --distance-matrix 或 --dispersal-strata 的 distance_matrix 列
n -> --environment-distance-matrix 或 --dispersal-strata 的 environment_distance_matrix 列
u -> --area-sizes 或 --dispersal-strata 的 area_sizes 列
```

还可提供 `--dispersal-multipliers`、`--extirpation-multipliers` 和范围状态约束时期表。
同一种原始修饰不能同时来自静态文件和时期表；原始面积与已变换的 extirpation multiplier
也互斥。分时期输入在每个候选参数点重新生成各时期 Q。

官方 197-tip Conifer 五维回归可直接运行：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-parameter-table-framework.ps1
```

该检查释放 `d/e/x/n/u`，通用命令与原专用优化器的 lnL、五个参数、288 次迭代和
515 次评估全部一致；BioGeoBEARS 冻结结果仍作为独立外部参考。

## 六 preset 修饰组合门禁

统一参数模型不能只在 DEC 上接受修饰。固定组合矩阵让六个正式 preset 分别运行一次静态任务
和一次两时期任务；两类任务都启用 manual dispersal、地理距离 `x`、环境距离 `n`、area size
`u` 和非默认事件专属 `mx01y/s/v/j`。每项任务必须完成似然、祖先/分裂后验、可移植结果发布
和科学重放：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-preset-modifier-matrix.ps1
```

同一门禁还逐项拒绝缺少 `x/n/u/w` 原始输入、静态与时期重复来源、area size 与已变换
extirpation 同时输入、分时期 `b!=1`、非默认 `mx01r` 和没有任何有效节点事件的配置。拒绝结果
必须使用 `biogeo-cli-error-v1`、退出码 2 和稳定 `configuration_error`，不能静默忽略参数。

## 当前语义边界

已进入统一似然模型的参数：

- `d/e/a`，其中 `a` 仅生成 singleton range 之间的瞬时 range-switching；
- `b`，即非分时期树上的 `branch_length^b`；
- 有原始输入时的 `x/n/u/w`，其中 `w` 只作用于 manual dispersal multiplier；
- `y/s/v/j`；
- `mx01y/mx01s/mx01v/mx01j`；
- 可作为联动中间量的 `ysv/ys/mx01`；
- 启用 detection 输入时的 `mf/dp/fdp`，候选参数每次变化都会重建末端观测似然。

`w` 为非默认值或自由参数时必须提供 `--dispersal-multipliers`，或在
`--dispersal-strata` 的 `matrix` 列提供每期手工倍率。没有这类输入时，`w` 必须固定为 1。
BioGeoBEARS 将 `b` 标记为 `non-stratified only`，因此分时期模型仍要求 `b=1`；静态模型
可固定、释放或联动 `b`。

`mx01r` 保留在 23 行兼容表中。对隔离的 BioGeoBEARS 1.1.3 做全源码追踪后，只有参数
定义和序列化示例对象包含该名称；非分时期/分时期入口均把 root prior 设为 `NULL`，Q、
C 表和 MaxEnt 路径也不读取它。复杂静态案例与官方 Psychotria 五时期案例在
`0.0001/0.5/0.9999` 三点的 lnL、根后验、split probability 和时期 cladogenesis 权重
逐元素最大绝对差均为 0。因此当前兼容语义要求：

```text
mx01r=0.5
```

释放它只会产生平坦、不可识别的优化维度，所以非默认固定值、自由值或联动值均被拒绝。
完整证据和版本升级规则见 [`mx01r-audit.md`](mx01r-audit.md)。这项结论限定于冻结的
BioGeoBEARS 1.1.3；若上游版本以后真正消费该参数，必须重新建立 root 语义 golden。

`a/b/w` 的外部语义门禁使用 BioGeoBEARS 官方 Psychotria M4 树和范围数据，覆盖 9 个
单参数点与 1 个联合点：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-abw-profile.ps1
```

10 点最大绝对 lnL 差为 `3.33e-7`，默认门限为 `5e-7`。R golden 由
`validation/biogeobears-abw-profile-golden.R` 独立生成。

`mf/dp/fdp` 已支持固定、自由、联动及与其他参数联合配置。它们只在显式提供 detection
观测模式时生效：

```powershell
cargo run --release -q -p biogeo-cli -- model-evaluate `
  --tree validation/fixtures/biogeobears_official/psychotria_detection/tree.nwk `
  --use-detection-model `
  --detections validation/fixtures/biogeobears_official/psychotria_detection/detections.tsv `
  --controls validation/fixtures/biogeobears_official/psychotria_detection/controls.tsv `
  --parameters <fixed-parameters.tsv> `
  --max-range-size 4
```

`--ranges` 与 detection 模式互斥；仅传计数文件但不显式启用该模式也会被拒绝。官方
Psychotria 固定 profile、全部 tip-state 相对似然、四组 observation 参数优化，以及
静态/五时期 `x/j/y/v/mf` 跨模块联合优化对照见
[`detection-model.md`](detection-model.md)。

## 版本化结果与生物地理随机历史

通用参数命令已支持把固定评估或优化点写入自包含、非覆盖的 `biogeo-analysis-result-v2` 目录：

```powershell
--analysis-result-dir <fit-result>
```

随后使用同一统一模型和 BSM 执行器重放：

```powershell
cargo run --release -q -p biogeo-cli -- model-bsm `
  --analysis-result <fit-result> `
  --bsm-samples 1000 `
  --bsm-output-dir <bsm-result> `
  --bsm-threads auto `
  --seed 1
```

重放前会校验外部输入、冻结参数表、稳定模型身份、状态空间和重新计算的 lnL；exact range、
detection 以及静态/分时期修饰使用同一路径。目录布局、非收敛诊断、原子发布和当前可移植性
边界见 [`analysis-result.md`](analysis-result.md)。
