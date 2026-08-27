# 生物地理框架验证

这里放的是框架语义回归和外部参考案例。两种外部工具承担不同角色：

- `validation/golden/biogeobears-*.tsv` 是 BioGeoBEARS-like 框架的语义 golden。
  Rust 与它不一致时，验证失败，必须解释或修正。
- `validation/reference/lagrange-ng-*.tsv` 是独立 LAGRANGE-ng 语义参考。
  它只和 LAGRANGE-ng 自身的新输出比较，不参与 Rust 的 BioGeoBEARS-like
  golden 判定。

原则是不为了追某个软件的输出去加入隐藏特判。如果两套工具的状态空间、根先验、
节点分裂情景或权重不同，差异必须留在明确的 preset、fixture 和报告里。

完整 BioGeoBEARS-like 语义门禁：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-framework-semantics.ps1
```

需要同时审计本机 LAGRANGE-ng 时显式加入：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-framework-semantics.ps1 -IncludeLagrangeReference -LagrangeScratchRoot C:\tmp
```

即使 LAGRANGE-ng 参考检查未启用或失败，它也不会被解释成 BioGeoBEARS golden
的数值差异。

## Rust 侧固定检查

运行：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-rust-dec-fixtures.ps1
```

脚本会读取 `validation/dec_fixtures.tsv`，逐个调用：

```text
cargo run -q -p biogeo-cli -- dec ...
```

然后把 CLI 输出的 `lnL` 与 manifest 中的 `expected_rust_lnL` 比较。

## BioGeoBEARS golden 输出

为避免污染用户全局 R 环境，本项目使用项目内 R library：

```text
validation/r-lib/
```

安装依赖：

```powershell
Rscript validation/setup-local-r-biogeobears.R
```

安装脚本会显式把 `.libPaths()` 设置为：

```text
validation/r-lib/R-<major>.<minor>
<R_HOME>/library
```

不会向 `C:/Users/.../AppData/Local/R/win-library/...` 写包。

运行：

```powershell
Rscript validation/biogeobears-dec-golden.R
```

这个脚本不会自动安装 R 包。它要求项目内 library 已经安装：

- `ape`
- `rexpokit`
- `cladoRcpp`
- `BioGeoBEARS`

如果依赖缺失，脚本会直接报错并说明缺少什么。依赖齐全后，它会把本项目的
range TSV 临时转换成 BioGeoBEARS / LAGRANGE 使用的 PHYLIP geography 格式，
再用固定 `d/e` 和 `j=0` 计算 BioGeoBEARS 的 DEC log-likelihood。

当前固定输出写入：

```text
validation/golden/biogeobears-dec.tsv
```

生成 golden 后，运行 Rust 与 BioGeoBEARS 的逐项对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1
```

这个脚本会读取 `validation/dec_fixtures.tsv` 中 `biogeobears_ready=true` 的案例，
重新调用 Rust CLI，并用 `external_tolerance` 与
`validation/golden/biogeobears-dec.tsv` 中的 BioGeoBEARS log-likelihood 比较。

当前非零分支 fixture 与 Rust CLI 的差异约为 `2e-8` 到 `1.1e-7`，属于
矩阵指数实现差异量级；核心模型不为此做特殊兼容。`max_range_size=3`
的复杂案例也会检查 DEC 扩散项是否按当前 BioGeoBEARS golden 语义随源范围大小缩放。

## BioGeoBEARS optimization golden 输出

固定参数 likelihood 对齐后，可以运行 BioGeoBEARS 的 `d/e` 优化对照：

```powershell
Rscript validation/biogeobears-dec-optim-golden.R
```

输出写入：

```text
validation/golden/biogeobears-dec-optim.tsv
```

再运行 Rust 与 BioGeoBEARS 的优化结果对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1
```

优化对照主要比较最优 `lnL`，同时打印 `d/e` 差异。小树或边界最优点下，参数值
可能比 likelihood 更不稳定，因此暂时不把 `d/e` 的逐位一致作为失败条件。
比较脚本要求 BioGeoBEARS golden 的 convergence code 为 0，并要求 Rust CLI 报告
`converged=true`。当前优化 fixture 与 Rust CLI 的最优 `lnL` 差异约为 `2e-8`
到 `1.1e-6`。

默认优化对照使用单起点。需要检查多起点优化路径时，可以传：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -MultiStartPoints 3
```

## 节点分裂参数独立优化与 profile golden

`validation/cladogenesis_parameter_optimization_fixtures.tsv` 在同一个复杂
5-area、8-tip、`max_range_size=5`、包含 null range 的案例上分别释放：

```text
y / s / v / mx01 / mx01y / mx01s / mx01v / mx01j
```

其余节点权重、`d/e` 和 founder-event 权重保持固定，因此每行只检查一个自由维度，
不会把多个参数的 ridge 误当成单参数实现错误。重生成隔离 R 环境中的 golden：

```powershell
Rscript validation/biogeobears-cladogenesis-parameter-optim-golden.R
```

输出包括：

```text
validation/golden/biogeobears-cladogenesis-parameter-optim.tsv
validation/golden/biogeobears-cladogenesis-parameter-profile.tsv
```

一键重生成并检查，或只检查现有 golden：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-biogeobears-cladogenesis-parameter-optimization.ps1 -RefreshBioGeoBEARS
powershell -ExecutionPolicy Bypass -File validation/check-biogeobears-cladogenesis-parameter-optimization.ps1
```

固定剖面共有 240 点。`y/s/v` 使用普通多起点；`mx01*` 除十分位外，还在每个点附近
加入 `±0.00025/±0.0005`，专门覆盖 BioGeoBEARS 把 `rexpokit::maxent` 结果保留三位
小数形成的台阶边界。Rust 使用相同的 improved iterative scaling、`1e-7` 最大概率变化
停止条件和三位舍入，所有固定点逐项比较 lnL。

不能只凭 BioGeoBEARS 的 `convergence=0` 认定全局最佳。当前 `mx01` 案例中，固定 profile
比多起点 L-BFGS-B 结果高约 `2.08e-5`；optimization golden 因此同时记录
`optimizer_*`、`profile_best_*`、`candidate_source` 和 `optimizer_gap`。平滑的 `y/s/v`
要求 Rust 点估计和 lnL 都对齐；台阶状 `mx01*` 不要求在同一平台上返回相同参数小数，
但 Rust 最佳 lnL 不得低于筛查后的 BGB 候选，并且 BGB 候选点必须能在 Rust 中固定重算。

## BioGeoBEARS ancestral probabilities golden 输出

内部节点范围概率需要 BioGeoBEARS 返回完整结果对象，因此不能使用只返回
log-likelihood 的 `skip_optim` 快速路径。运行：

```powershell
Rscript validation/biogeobears-dec-ancestral-golden.R
```

输出写入：

```text
validation/golden/biogeobears-dec-ancestral.tsv
```

再运行 Rust 与 BioGeoBEARS 的祖先范围概率对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1
```

这个对照使用 `case_id + clade + range_bits` 作为稳定键。`clade` 是节点下所有
tip label 排序后的拼接，避免直接比较 BioGeoBEARS/ape 和 Rust 各自不同的内部
node id。当前 fixture 的最大概率差异约为 `2.4e-8`。

## BioGeoBEARS split scenario probabilities golden 输出

Rust CLI 还支持输出内部节点的 cladogenesis split scenario posterior：

```powershell
cargo run -q -p biogeo-cli -- dec --tree <tree> --ranges <ranges> --d <d> --e <e> --split-probs
```

当前这部分已经有 Rust 单元测试覆盖，包括：

- 每个内部节点下的 split scenario 概率按节点归一化。
- 两 tip 零分支手工案例中，`A+B -> A + B` 的分裂概率为 `1`。

BioGeoBEARS 的最终 result 对象没有直接暴露 split-event probability 表。这里用
BioGeoBEARS 内部的 `get_Qmat_COOmat_from_res()`、`calc_uppass_scenario_probs_new2()`
和完整结果对象里的 uppass/downpass likelihood 表重建同一含义的 posterior。生成：

```powershell
Rscript validation/biogeobears-dec-split-golden.R
```

输出写入：

```text
validation/golden/biogeobears-dec-split.tsv
```

再运行 Rust 与 BioGeoBEARS 的 split posterior 对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1
```

这个对照使用
`case_id + clade + left_clade + right_clade + ancestor/left/right range_bits`
作为稳定键，同时检查 split scenario row count、是否有 Rust-only extra row、
split scenario weight 和 posterior probability。当前 fixture 的最大概率差异约为
`4.7e-9`，scenario weight 完全一致。

分层模型不能对所有节点固定使用 `timeperiod_i=1`。生成器以最大 root-to-tip 深度
作为现在计算节点年龄，再为每个节点读取对应时期的 COO table；较短的 tip 路径可
表示非现生 tip，不要求树必须超度量。

## DEC+J 与 founder-event modifier golden

`validation/decj_fixtures.tsv` 包含基础、静态非对称和三时期非对称三例。BioGeoBEARS
的同一有效 pairwise matrix 同时进入 Q 与 founder-event C：祖先范围为来源、范围外
daughter 为目标，多区域组合取有向元素的算术均值，再与 y/s/v/j scenarios 统一归一化。

生成和检查 fixed、node posterior、split posterior 与 `d/e/j` 优化：

```powershell
Rscript validation/biogeobears-decj-golden.R
Rscript validation/biogeobears-dec-ancestral-golden.R validation/decj_fixtures.tsv validation/golden/biogeobears-decj-ancestral.tsv
Rscript validation/biogeobears-dec-split-golden.R validation/decj_fixtures.tsv validation/golden/biogeobears-decj-split.tsv
Rscript validation/biogeobears-decj-optim-golden.R
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-decj.ps1
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/decj_fixtures.tsv -Golden validation/golden/biogeobears-decj-ancestral.tsv
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/decj_fixtures.tsv -Golden validation/golden/biogeobears-decj-split.tsv -WeightTolerance 1e-8
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-decj-optim.ps1
```

三例 fixed lnL 最大差为 `4.81e-8`，node posterior 最大差为 `1.25e-8`，split
probability 最大差为 `1.18e-8`，split weight 最大差为 `8.11e-9`，优化 lnL 最大差为
`3.51e-8`。优化 golden 记录 BioGeoBEARS convergence code，三例均为 0。

## 非默认 mx01 最大熵 golden

`validation/maxent_fixtures.tsv` 专门验证非默认 daughter-size 语义：

- 四区域 `mx01y=mx01s=mx01v=mx01j=0.5`，覆盖 widespread range-copying、
  任意真子集和 `2+2` 平衡 vicariance。
- 五区域事件特异参数，覆盖四套独立 size weights 和多区域 founder daughter。

重新生成固定 likelihood 与 split golden：

```powershell
Rscript validation/biogeobears-decj-golden.R validation/maxent_fixtures.tsv validation/golden/biogeobears-maxent.tsv
Rscript validation/biogeobears-dec-split-golden.R validation/maxent_fixtures.tsv validation/golden/biogeobears-maxent-split.tsv
Rscript validation/biogeobears-dec-optim-golden.R validation/maxent_fixtures.tsv validation/golden/biogeobears-maxent-optim.tsv
```

逐项检查 Rust：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-rust-decj-fixtures.ps1 -Manifest validation/maxent_fixtures.tsv
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-decj.ps1 -Manifest validation/maxent_fixtures.tsv -Golden validation/golden/biogeobears-maxent.tsv
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/maxent_fixtures.tsv -Golden validation/golden/biogeobears-maxent-split.tsv -WeightTolerance 1e-7
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/maxent_fixtures.tsv -Golden validation/golden/biogeobears-maxent-optim.tsv
```

当前两例的固定 lnL 差分别约为 `1.1e-8` 和 `3.8e-8`；495 与 3395 条 split
scenario 全部同键，最大 scenario weight 差约为 `3.1e-9`。
四区域自定义 `mx01*` 的 `d/e` 优化最优 lnL 差约为 `2.3e-8`。

## DIVALIKE golden

DIVALIKE 继续使用 DEC 的 `d/e` 沿枝 Q，但节点过程固定为
`y=1, s=0, v=1, j=0`，并使用 `mx01v=0.5` 允许 widespread vicariance。
fixture 清单为 `validation/divalike_fixtures.tsv`。

重新生成固定 likelihood、两类 posterior 和优化 golden：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/divalike_fixtures.tsv validation/golden/biogeobears-divalike.tsv DIVALIKE
Rscript validation/biogeobears-dec-split-golden.R validation/divalike_fixtures.tsv validation/golden/biogeobears-divalike-split.tsv DIVALIKE
Rscript validation/biogeobears-dec-ancestral-golden.R validation/divalike_fixtures.tsv validation/golden/biogeobears-divalike-ancestral.tsv DIVALIKE
Rscript validation/biogeobears-dec-optim-golden.R validation/divalike_fixtures.tsv validation/golden/biogeobears-divalike-optim.tsv DIVALIKE optimx
```

执行 Rust 对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/divalike_fixtures.tsv -Golden validation/golden/biogeobears-divalike.tsv -Command divalike
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/divalike_fixtures.tsv -Golden validation/golden/biogeobears-divalike-split.tsv -Command divalike -WeightTolerance 1e-8
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/divalike_fixtures.tsv -Golden validation/golden/biogeobears-divalike-ancestral.tsv -Command divalike
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/divalike_fixtures.tsv -Golden validation/golden/biogeobears-divalike-optim.tsv -Command divalike-optimize
```

当前三组 BioGeoBEARS-ready fixture 的固定 lnL 最大差为 `6.32e-8`；split
scenario 分别为 4、162 和 595 条，全部同键，最大 posterior 差为 `3.94e-9`，
scenario weight 零差。四区域祖先的 `AreaA+AreaB | AreaC+AreaD` 平衡分裂
已显式出现，归一化权重为 `1/14`。

node-state posterior 的最大差为 `1.68e-8`。优化 golden 使用 BioGeoBEARS
`optimx/bobyqa`，三组 `convcode` 均为 0；Rust 与 BioGeoBEARS 的最优 lnL 最大差
为 `8.16e-8`。`optim/L-BFGS-B` 在复杂五区域案例会以 convergence code 52
异常停止，因此不作为该案例的优化 golden。

## BAYAREALIKE golden

BAYAREALIKE 继续使用 DEC 的 `d/e` 沿枝 Q，节点过程固定为
`y=1, s=0, v=0, j=0` 与 `mx01y=0.9999`。因此每个非空祖先范围只有一个
exact range-copying scenario：左右子代范围都等于祖先范围。fixture 清单为
`validation/bayarealike_fixtures.tsv`。

重新生成固定 likelihood、两类 posterior 和优化 golden：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/bayarealike_fixtures.tsv validation/golden/biogeobears-bayarealike.tsv BAYAREALIKE
Rscript validation/biogeobears-dec-split-golden.R validation/bayarealike_fixtures.tsv validation/golden/biogeobears-bayarealike-split.tsv BAYAREALIKE
Rscript validation/biogeobears-dec-ancestral-golden.R validation/bayarealike_fixtures.tsv validation/golden/biogeobears-bayarealike-ancestral.tsv BAYAREALIKE
Rscript validation/biogeobears-dec-optim-golden.R validation/bayarealike_fixtures.tsv validation/golden/biogeobears-bayarealike-optim.tsv BAYAREALIKE optimx
```

执行 Rust 对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-rust-dec-fixtures.ps1 -Manifest validation/bayarealike_fixtures.tsv -Command bayarealike
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/bayarealike_fixtures.tsv -Golden validation/golden/biogeobears-bayarealike.tsv -Command bayarealike
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/bayarealike_fixtures.tsv -Golden validation/golden/biogeobears-bayarealike-split.tsv -Command bayarealike
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/bayarealike_fixtures.tsv -Golden validation/golden/biogeobears-bayarealike-ancestral.tsv -Command bayarealike
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/bayarealike_fixtures.tsv -Golden validation/golden/biogeobears-bayarealike-optim.tsv -Command bayarealike-optimize
```

三组 fixture 覆盖 2-area 小树、4-area 全状态树，以及
`5 areas / 8 tips / max_range_size=5 / 32 states` 的复杂案例。固定 lnL 最大差为
`2.37e-7`，split scenario weight 零差，split posterior 最大差为 `1.31e-7`，
node-state posterior 最大差为 `7.53e-9`。BioGeoBEARS `optimx/bobyqa` 的三组
`convcode` 均为 0，Rust 三组均报告收敛，最优 lnL 最大差为 `8.78e-7`。

## DIVALIKE+J 与 BAYAREALIKE+J golden

两个 +J preset 不是在已有模型上直接加一个独立 `j` 权重。它们使用
BioGeoBEARS 参数表的链接规则：

```text
DIVALIKE+J:    y = v = (2-j)/2, s = 0, mx01v = 0.5, j < 2
BAYAREALIKE+J: y = 1-j, s = v = 0, mx01y = 0.9999, j < 1
```

fixture 分别位于 `validation/divalikej_fixtures.tsv` 和
`validation/bayarealikej_fixtures.tsv`。两组都包含 4-area 完整状态空间优化案例，
以及 `5 areas / 8 tips / max_range_size=5 / 32 states` 的固定似然和 posterior 案例。
golden 可按以下模式重新生成；将 `DIVALIKE/divalikej` 替换为
`BAYAREALIKE/bayarealikej` 即生成另一组：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/divalikej_fixtures.tsv validation/golden/biogeobears-divalikej.tsv DIVALIKE
Rscript validation/biogeobears-dec-split-golden.R validation/divalikej_fixtures.tsv validation/golden/biogeobears-divalikej-split.tsv DIVALIKE
Rscript validation/biogeobears-dec-ancestral-golden.R validation/divalikej_fixtures.tsv validation/golden/biogeobears-divalikej-ancestral.tsv DIVALIKE
Rscript validation/biogeobears-decj-optim-golden.R validation/divalikej_fixtures.tsv validation/golden/biogeobears-divalikej-optim.tsv DIVALIKE
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-decj-optim.ps1 -Manifest validation/divalikej_fixtures.tsv -Golden validation/golden/biogeobears-divalikej-optim.tsv -Command divalikej-optimize -MultiStartPoints 2
```

四个固定 lnL 对照的绝对差为 `1.1e-8` 到 `6.1e-8`；node posterior 最大差
`1.9e-8`，split posterior 最大差 `1.1e-8`，split weight 最大差 `4.9e-9`。
DIVALIKE+J 与 BAYAREALIKE+J 的最优 lnL 差分别为 `9.3e-8` 和 `6.7e-9`。

## 方向性 dispersal multiplier golden

`validation/dispersal_fixtures.tsv` 验证静态 area-to-area 倍率矩阵。矩阵格式要求
范围表中的区域顺序同时出现在列和行：

```text
from    AreaA  AreaB  AreaC
AreaA   1      0.25   0
AreaB   2      1      0.5
AreaC   0.1    3      1
```

CLI 使用 `--dispersal-multipliers <matrix.tsv>`。扩张到目标区域的速率为 `d` 乘以
当前范围内各来源区域到该目标的 multiplier 之和，因此矩阵可以非对称，0 会禁止
对应方向的扩张。

生成与检查四类 golden：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/dispersal_fixtures.tsv validation/golden/biogeobears-dispersal.tsv DEC
Rscript validation/biogeobears-dec-split-golden.R validation/dispersal_fixtures.tsv validation/golden/biogeobears-dispersal-split.tsv DEC
Rscript validation/biogeobears-dec-ancestral-golden.R validation/dispersal_fixtures.tsv validation/golden/biogeobears-dispersal-ancestral.tsv DEC
Rscript validation/biogeobears-dec-optim-golden.R validation/dispersal_fixtures.tsv validation/golden/biogeobears-dispersal-optim.tsv DEC optim
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/dispersal_fixtures.tsv -Golden validation/golden/biogeobears-dispersal.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/dispersal_fixtures.tsv -Golden validation/golden/biogeobears-dispersal-split.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/dispersal_fixtures.tsv -Golden validation/golden/biogeobears-dispersal-ancestral.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/dispersal_fixtures.tsv -Golden validation/golden/biogeobears-dispersal-optim.tsv -Command dec-optimize
```

两组 fixture 的固定 lnL 最大差为 `1.02e-7`，split weight 零差，split posterior
最大差为 `5.89e-9`，node-state posterior 最大差为 `1.29e-8`，最优 lnL 最大差为
`1.08e-7`。复杂例中 `optimx/bobyqa` 虽返回 code 0，但 `kkt1=FALSE` 且停在较差
点；golden 使用收敛到更高 lnL 的 BioGeoBEARS `optim/L-BFGS-B`，没有放宽阈值。

## Distance、environment 与 extirpation modifier golden

`validation/anagenesis_modifier_fixtures.tsv` 验证 BioGeoBEARS 的以下 Q 语义：

```text
effective[a,b] = manual[a,b] * distance[a,b]^x * envdistance[a,b]^n
range -> range+b: d * sum(effective[a,b] for a in range)
extirpation_multiplier[a] = area_size[a]^u
range -> range-a: e * extirpation_multiplier[a]
```

距离矩阵使用与 dispersal multiplier 相同的严格带名格式。CLI 要求矩阵和指数成对
出现：

```powershell
--distance-matrix validation/fixtures/anagenesis_modifiers/three_area_distances.tsv `
--distance-exponent -1.2
```

环境距离使用同一矩阵格式，并要求以下参数成对出现：

```powershell
--environment-distance-matrix validation/fixtures/anagenesis_modifiers/three_area_environment.tsv `
--environment-distance-exponent -0.8
```

区域灭绝倍率文件按范围表区域顺序书写：

```text
area   multiplier
AreaA  0.5
AreaB  1
AreaC  2
```

CLI 使用 `--extirpation-multipliers <file.tsv>`。这里输入的是已经变换后的有效倍率，
对应 BioGeoBEARS 的 `area_of_areas^u` 最终结果。原始面积使用单独格式：

```text
area   size
AreaA  0.5
AreaB  1
AreaC  2
```

固定 u 时同时传入 `--area-sizes` 与 `--area-exponent`；这组输入与
`--extirpation-multipliers` 互斥。固定指数模式只估计 `d/e`，自由指数模式则通过
`dec-x-optimize`、`dec-n-optimize` 或 `dec-u-optimize` 一次估计一个指数。两类
距离矩阵的对角线不参与扩张，允许为 0；非对角 0 距离配负指数会产生无穷倍率，
Rust 在构建 Q 前明确拒绝。

生成与检查 golden：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/anagenesis_modifier_fixtures.tsv validation/golden/biogeobears-anagenesis-modifiers.tsv DEC
Rscript validation/biogeobears-dec-split-golden.R validation/anagenesis_modifier_fixtures.tsv validation/golden/biogeobears-anagenesis-modifiers-split.tsv DEC
Rscript validation/biogeobears-dec-ancestral-golden.R validation/anagenesis_modifier_fixtures.tsv validation/golden/biogeobears-anagenesis-modifiers-ancestral.tsv DEC
Rscript validation/biogeobears-dec-optim-golden.R validation/anagenesis_modifier_fixtures.tsv validation/golden/biogeobears-anagenesis-modifiers-optim.tsv DEC optimx
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/anagenesis_modifier_fixtures.tsv -Golden validation/golden/biogeobears-anagenesis-modifiers.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/anagenesis_modifier_fixtures.tsv -Golden validation/golden/biogeobears-anagenesis-modifiers-split.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/anagenesis_modifier_fixtures.tsv -Golden validation/golden/biogeobears-anagenesis-modifiers-ancestral.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/anagenesis_modifier_fixtures.tsv -Golden validation/golden/biogeobears-anagenesis-modifiers-optim.tsv -Command dec-optimize
```

三组 fixture 分别覆盖 `distance^x + extirpation`、纯 `envdistance^n`，以及
`manual * distance^x * envdistance^n + extirpation` 全组合。固定 lnL 最大差为
`6.71e-8`，split weight 零差，split posterior 最大差为 `1.85e-8`，node-state
posterior 最大差为 `1.87e-8`，最优 lnL 最大差为 `2.56e-7`。

优化 golden 使用 `optimx/bobyqa`，三组 `convcode` 都为 0。环境-only 案例的 `e`
位于下界，`kkt1=FALSE`；全组合案例 `bobyqa` 也报告 `kkt1=FALSE`，但独立
`optim/L-BFGS-B` 与 Rust 均到达同一 lnL 邻域。该全组合的 L-BFGS-B 最终返回
code 52，因此不作为正式 gate；这里保留诊断，不放宽 lnL 阈值。

pairwise matrix 在 BioGeoBEARS 中还会修饰 founder-event `j`。Rust 使用同一有效
矩阵计算祖先范围到 founder daughter 的有向 pairwise 均值，并在祖先行内与 y/s/v/j
场景共同归一化；静态和分层输入均可与 `j>0` 组合。区域 extirpation 向量仍只作用于 Q。

## 自由 x/n/u 指数优化 golden

`validation/exponent_optimization_fixtures.tsv` 覆盖六个风险不同的案例：内部最优
`x`、下界最优 `n`、全修饰组合下的内部 `n`，以及两个内部 `u` 和一个真实下界
`u`。CLI 入口为：

```powershell
cargo run -q -p biogeo-cli -- dec-x-optimize --tree <tree> --ranges <ranges> --distance-matrix <matrix.tsv>
cargo run -q -p biogeo-cli -- dec-n-optimize --tree <tree> --ranges <ranges> --environment-distance-matrix <matrix.tsv>
cargo run -q -p biogeo-cli -- dec-u-optimize --tree <tree> --ranges <ranges> --area-sizes <sizes.tsv>
```

被优化的指数不能同时通过对应的 `--distance-exponent`、
`--environment-distance-exponent` 或 `--area-exponent` 固定；初值、边界和步长分别由
`--init-exponent`、`--min-exponent`、`--max-exponent` 和
`--initial-exponent-step` 控制。这三个命令一次只释放一个指数；五维联合入口见下文。
三个命令也接受 extended `--dispersal-strata`：对应的原始距离或 area size 必须在
每个时期存在，候选指数会逐时期重建 Q。`--multi-start-points 2` 在三维参数空间产生
8 个网格角点加主初值，共 9 个起点；输出会报告收敛起点数和指数位于内部或边界。

生成和检查 BioGeoBEARS golden：

```powershell
Rscript validation/biogeobears-dec-exponent-optim-golden.R
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-exponent-optim.ps1
```

BioGeoBEARS 的 L-BFGS-B 在简单 `n` 案例曾以 code 0 停在 `n=-6.04`，但投影梯度
仍约为 `0.285`，且 lnL 低于边界解。正式 golden 因此用预先定义的 11 点 n profile，
在每个固定 n 上由 `optimx/bobyqa` 优化 `d/e`；`n=-10` 明确优于 `n=-8` 及其余
网格点。比较脚本既比较双方最优 lnL，也让 Rust 在 BioGeoBEARS 参数处固定重算
lnL，避免优化器差异掩盖 likelihood 语义差异。

x/n 三例的最优 lnL 差为 `7.3e-8` 到 `2.1e-7`。u 三例的最优 lnL 差为
`1.4e-8` 到 `4.4e-7`，固定重算差不超过 `4.7e-7`；两个内部 u 的差分别约
`1.2e-4` 和 `2.5e-5`，边界案例双方均为 `u=-10`，且该案例的 `e` 保持内部。

所有原始面积完全相同时，`area_size^u` 的共同倍数可被 `e` 吸收，u 结构性不可
辨识；CLI 会在优化前拒绝。面积有差异但最优 `e` 近 0 的案例也不作为 u 边界
golden，因为此时 u 对 likelihood 几乎没有作用。

## x/n/u 二维 profile 与 ridge 诊断

作为联合点估计的补充诊断，三个命令在二维网格上固定两个指数，并在每个网格点重新
优化 `d/e`：

```powershell
cargo run --release -q -p biogeo-cli -- dec-xn-profile <共同输入> --area-exponent <u> --x-min -2.5 --x-max 2.5 --x-points 11 --n-min -4 --n-max 2 --n-points 7
cargo run --release -q -p biogeo-cli -- dec-xu-profile <共同输入> --environment-distance-exponent <n> --x-min -2.5 --x-max 2.5 --x-points 11 --u-min -4 --u-max 4 --u-points 9
cargo run --release -q -p biogeo-cli -- dec-nu-profile <共同输入> --distance-exponent <x> --n-min -4 --n-max 2 --n-points 7 --u-min -4 --u-max 4 --u-points 9
```

共同输入包含 tree、range，以及一套静态地理距离矩阵、环境距离矩阵和原始面积，或
一个在每个时期包含这些原始输入的 extended schedule；可再乘手工 dispersal
matrix。第三个指数必须显式固定，不能静默默认为 0。默认
`support_delta=2.995732` 是二维参数 likelihood-ratio 的近似 95% 阈值，小树和参数
边界下仅用于诊断。输出还包含完整点表、网格边界、支持区跨度、每点有限/失败状态、
收敛状态和似然加权相关系数；相关系数只描述线性 ridge 方向，不自动判定可辨识性。
某个网格点无法得到有限 lnL 时会被记录为失败点，并从峰值和支持区计算中排除。

复杂 fixture 使用 4 areas、6 tips、16 states、手工方向矩阵、`distance^x`、
`envdistance^n` 和 `area_size^u`。三张截面均在 `x=1.5, n=-2, u=0` 得到同一离散
最优 lnL `-13.546545518162791`，且全部 239 个 `d/e` 子优化收敛。但是 `x` 的支持区
覆盖 11/11 个网格值，`u` 覆盖 9/9；这说明该 fixture 的五维点估计不适合做生物学
解释，但不构成禁用或删减联合优化功能的理由。

回归与 BioGeoBEARS 代表点对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-dec-pair-profiles.ps1
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/pair_profile_semantic_fixtures.tsv -Golden validation/golden/biogeobears-pair-profile-semantic-optim.tsv -Command dec-optimize -MultiStartPoints 2
```

BioGeoBEARS 对照固定 `x/n/u`，只优化 `d/e`，覆盖截面峰值和三个边缘点；四个 lnL
绝对差为 `9.9e-8` 到 `3.4e-6`。profile 执行器没有 fixture 分支或对齐常数。

## 联合 d/e/x/n/u 优化与官方数据验证

`dec-xnu-optimize` 同时释放五个参数，要求提供静态地理距离、环境距离和原始面积，
或提供每时期包含三者的 extended schedule：

```powershell
cargo run --release -q -p biogeo-cli -- dec-xnu-optimize --tree <tree> --ranges <ranges> --distance-matrix <distance.tsv> --environment-distance-matrix <environment.tsv> --area-sizes <sizes.tsv> --max-range-size 3
cargo run --release -q -p biogeo-cli -- dec-xnu-optimize --tree <tree> --ranges <ranges> --dispersal-strata <raw-strata.tsv> --max-range-size 3
```

`d/e` 在 log 空间搜索，`x/n/u` 使用各自的显式线性边界。对应参数由
`--init-{x,n,u}`、`--min-{x,n,u}`、`--max-{x,n,u}` 和
`--initial-{x,n,u}-step` 控制。`--multi-start-points 2` 会使用主初值加五维空间的
32 个角点，共 33 个起点。联合入口和固定推断调用同一个 `LikelihoodEngine`，没有
按案例名称、tip 数量或参考程序设置分支。

官方数据通过项目隔离的 BioGeoBEARS 包导入：

```powershell
Rscript validation/import-biogeobears-official-fixtures.R
Rscript validation/simulate-biogeobears-conifer-xnu.R
```

`validation/fixtures/biogeobears_official/psychotria_m4` 来自官方 Psychotria M4 示例，
用于保留 19-tip 小数据 ridge 诊断。正式联合案例使用官方 Conifer DEC+x 示例的
197-tip 树、tip 名称和地理距离矩阵；由于 BioGeoBEARS 安装包没有可直接配套的环境
距离与面积联合示例，验证脚本明确生成独立、固定的环境距离和几何均值为 1 的面积
协变量，再调用 BioGeoBEARS 自身的 Q/C 构造和 `simulate_biogeog_history` 生成 tip
ranges。官方输入与派生输入的边界、随机种子和生成参数均记录在
`validation/fixtures/biogeobears_official/README.md`，没有把派生协变量标成官方数据。

固定语义和联合优化分别检查：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/xnu_fixed_fixtures.tsv -Golden validation/golden/biogeobears-dec-xnu-fixed.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-xnu-optim.ps1
```

在 Conifer 案例的生成参数处，Rust 与 BioGeoBEARS 固定 lnL 差为 `1.36e-6`。在 Rust
最优点和 BioGeoBEARS 最优点交叉固定重算时，差分别为 `3.59e-7` 与 `5.96e-6`，因此
五个参数进入 Q 与 pruning likelihood 的语义已独立于优化器锁定。Rust 从生成参数
单起点约 2 秒收敛到 lnL `-358.61434595`；BioGeoBEARS `optimx/bobyqa` 约 410 秒到
`-358.62040375`。后者虽返回 convergence 0，但 `KKT1=FALSE`、`KKT2=TRUE`，所以
Rust 高 `0.00606` 的 lnL 不能简单解释为两个模型不一致；正式语义 gate 依赖交叉固定
重算，优化结果用于回归、邻域和端到端性能比较。耗时较长的 BioGeoBEARS 优化结果已
冻结，日常 gate 只重跑 Rust；需要更新 golden 时才运行
`validation/biogeobears-dec-xnu-optim-golden.R`。

## 版本化参数表框架

`biogeo-parameter-table-v1` 将 23 行 BioGeoBEARS 参数的固定、自由、联动、边界和
优化坐标保存为严格 TSV。六种 preset 的生成结果冻结在
`examples/parameter_tables/`；通用 `model-evaluate` 和 `model-optimize` 在每个候选
参数点重新构造 Q、节点 split table 及静态/分时期 `x/n/u` 修饰。

独立门禁同时检查六张模板逐字一致，以及官方 197-tip Conifer 数据上的五维优化：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-parameter-table-framework.ps1
```

Conifer 配置位于 `validation/parameter_tables/conifer_197tip_xnu.tsv`。通用入口得到
`lnL=-358.614345952854990`、288 次迭代和 515 次评估；lnL 与 `d/e/x/n/u` 均和原
`dec-xnu-optimize` Rust golden 逐位一致。脚本还检查该 lnL 不低于 BioGeoBEARS
冻结优化结果的容差下界。参数格式、表达式安全边界和当前尚未开放的兼容参数见
`docs/parameter-table.md`。

## a/b/w 固定 profile golden

`a`（singleton range-switching）、`b`（非分时期枝长指数）和 `w`（manual multiplier
指数）使用 BioGeoBEARS 官方 Psychotria M4 的 19-tip 树与 4-area tip ranges 对照。
手工倍率矩阵由官方距离矩阵按 `exp(-distance)` 确定性派生，规则与数值均已冻结。

重新生成 BioGeoBEARS golden：

```powershell
Rscript validation/biogeobears-abw-profile-golden.R
```

日常 Rust 门禁：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-abw-profile.ps1
```

`validation/abw_profile_fixtures.tsv` 包含 baseline、各参数多个固定点和一个联合
`a/b/w` 点。10 点最大绝对 lnL 差为 `3.33e-7`，默认失败门限为 `5e-7`。R 脚本真实调用
`check_BioGeoBEARS_run()` 和 `bears_optim_run(skip_optim=TRUE)`，golden 不来自 Rust 输出。

## Time-stratified dispersal golden

CLI 的 `--dispersal-strata <strata.tsv>` 接受由年轻到年老、严格递增的边界：

```text
oldest_age  matrix
0.2         young.tsv
0.6         middle.tsv
1.0         old.tsv
```

matrix 路径相对 strata 文件解析。最老边界必须覆盖树根年龄；每条枝按距今年龄切成
piecewise-Q segments。静态矩阵与 strata 选项互斥。`j>0` 时，每个时期的同一有效
矩阵还构建该时期的 founder-event C 权重，内部节点按距今年龄选择对应 C 表。

生成与检查：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/time_stratified_fixtures.tsv validation/golden/biogeobears-time-stratified.tsv DEC
Rscript validation/biogeobears-dec-split-golden.R validation/time_stratified_fixtures.tsv validation/golden/biogeobears-time-stratified-split.tsv DEC
Rscript validation/biogeobears-dec-ancestral-golden.R validation/time_stratified_fixtures.tsv validation/golden/biogeobears-time-stratified-ancestral.tsv DEC
Rscript validation/biogeobears-dec-optim-golden.R validation/time_stratified_fixtures.tsv validation/golden/biogeobears-time-stratified-optim.tsv DEC optim
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/time_stratified_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/time_stratified_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-split.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/time_stratified_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-ancestral.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/time_stratified_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-optim.tsv -Command dec-optimize
```

两组 fixture 覆盖二地区与三地区、终端枝和内部枝跨 epoch。固定 lnL 最大差为
`3.24e-8`，split weight 零差，split posterior 最大差为 `8.39e-9`，node-state
posterior 最大差为 `8.29e-9`，最优 lnL 最大差为 `2.17e-7`。

### Raw time-stratified anagenesis

扩展格式让每个时期分别引用原始修饰输入：

```text
oldest_age  matrix            distance_matrix    environment_distance_matrix  area_sizes
0.25        young_manual.tsv  young_distance.tsv young_environment.tsv        young_areas.tsv
0.70        middle_manual.tsv middle_distance.tsv middle_environment.tsv      middle_areas.tsv
1.20        old_manual.tsv    old_distance.tsv   old_environment.tsv           old_areas.tsv
```

五列列名必须完全匹配；未提供的输入用 `-` 或 `none`。路径相对 schedule 文件解析。
`x/n/u` 由 CLI 单独给出固定值或优化边界，因此候选参数变化时会从每个时期的原始文件
重新计算 `manual * distance^x * envdistance^n` 与 `area_size^u`。固定似然、祖先和
split posterior、`d/e` 优化、三个单指数优化、联合五维优化和三个二维 profile 都走
同一个 schedule loader 与 branch propagator。

生成与检查：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/time_stratified_raw_fixtures.tsv validation/golden/biogeobears-time-stratified-raw.tsv DEC
Rscript validation/biogeobears-dec-split-golden.R validation/time_stratified_raw_fixtures.tsv validation/golden/biogeobears-time-stratified-raw-split.tsv DEC
Rscript validation/biogeobears-dec-ancestral-golden.R validation/time_stratified_raw_fixtures.tsv validation/golden/biogeobears-time-stratified-raw-ancestral.tsv DEC
Rscript validation/biogeobears-dec-optim-golden.R validation/time_stratified_raw_fixtures.tsv validation/golden/biogeobears-time-stratified-raw-optim.tsv DEC optim
powershell -ExecutionPolicy Bypass -File validation/check-rust-dec-fixtures.ps1 -Manifest validation/time_stratified_raw_fixtures.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/time_stratified_raw_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-raw.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/time_stratified_raw_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-raw-split.tsv -Command dec -ProbabilityTolerance 2e-5
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/time_stratified_raw_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-raw-ancestral.tsv -Command dec -ProbabilityTolerance 2e-5
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/time_stratified_raw_fixtures.tsv -Golden validation/golden/biogeobears-time-stratified-raw-optim.tsv -Command dec-optimize -LnLTolerance 2e-5
```

合成全时变 fixture 的固定 lnL 差为 `1.75e-10`，`d/e` 最优 lnL 差为 `9.95e-8`。
用于 posterior 的合成 fixture 让地理距离和面积随时期变化、环境距离保持不变；祖先
posterior 最大差为 `1.29e-8`，split posterior 最大差为 `1.86e-8`，split weight
零差。环境距离保持不变不是 Rust 功能限制，而是因为 BioGeoBEARS stratified uppass
源码在环境距离处固定读取 `list_of_envdistances_mats[[1]]`；全时变环境输入仍用于固定
似然与优化 golden。

官方 Psychotria M4b 五时期输入也已冻结。其五组输入在该示例中相同，所以另保留一个
数学等价的静态行：Rust 分段与静态固定 lnL 相差 `1.86e-9`；Rust 分段优化与
BioGeoBEARS 静态优化 lnL 相差 `1.16e-7`。BioGeoBEARS 自己的 stratified 高迁移率
优化比静态结果低 `0.00161`，该值保留为审计结果，不作为放宽严格参考容差的理由。

### Time-stratified range-state constraints

七列 schedule 在五类原始修饰后追加两个可选路径：

```text
oldest_age  matrix  distance_matrix  environment_distance_matrix  area_sizes  areas_allowed  areas_adjacency
0.1         -       -                -                            -           young.tsv      -
100         -       -                -                            -           old.tsv        -
```

矩阵采用与 named dispersal table 相同的行列区域格式，但值只能是 0 或 1。约束会改变
每期 Q、节点 split table、root prior 和时期边界投影，不等同于 dispersal multiplier。
生成与检查：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/state_constraint_fixtures.tsv validation/golden/biogeobears-state-constraints.tsv DEC
Rscript validation/biogeobears-dec-ancestral-golden.R validation/state_constraint_fixtures.tsv validation/golden/biogeobears-state-constraints-ancestral.tsv DEC
Rscript validation/biogeobears-dec-split-golden.R validation/state_constraint_fixtures.tsv validation/golden/biogeobears-state-constraints-split.tsv DEC
Rscript validation/biogeobears-dec-optim-golden.R validation/state_constraint_fixtures.tsv validation/golden/biogeobears-state-constraints-optim.tsv DEC optimx
powershell -ExecutionPolicy Bypass -File validation/check-rust-dec-fixtures.ps1 -Manifest validation/state_constraint_fixtures.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/state_constraint_fixtures.tsv -Golden validation/golden/biogeobears-state-constraints.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/state_constraint_fixtures.tsv -Golden validation/golden/biogeobears-state-constraints-ancestral.tsv -Command dec -ProbabilityTolerance 2e-5
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/state_constraint_fixtures.tsv -Golden validation/golden/biogeobears-state-constraints-split.tsv -Command dec -ProbabilityTolerance 2e-5 -WeightTolerance 1e-8 -IgnoreZeroProbabilityPlaceholders
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/state_constraint_fixtures.tsv -Golden validation/golden/biogeobears-state-constraints-optim.tsv -Command dec-optimize -LnLTolerance 5e-5 -MultiStartPoints 2
powershell -ExecutionPolicy Bypass -File validation/check-fossil-tip-bsm.ps1
```

split golden 生成器不能把时期 COO 的局部状态编号当成主状态编号；它按范围 bitset 映射
局部状态，再用同一 BGB 结果的节点 posterior 和左右 downpass likelihood 重建 scenario
概率。官方 BSM 3-taxon areas-allowed 案例固定 lnL 差 `1.15e-8`，祖先 posterior 最大差
`8.53e-9`，16 个有效 split 的最大概率差 `4.27e-9` 且权重零差。合成 adjacency 案例固定 lnL
差 `1.20e-8`，祖先 posterior 最大差 `1.58e-10`。其 BGB stratified split 表与同一
结果对象的 node posterior 自相矛盾，因此 manifest 以独立的
`biogeobears_split_ready=false` 保留风险，而不关闭 fixed、ancestral 或 optimization
验证。adjacency 优化中 Rust lnL 比 `optimx/bobyqa` 高 `2.65e-5`，且 BGB 报告
`KKT1=FALSE`；优化门限记录该求解器状态，固定参数语义仍保持约 `1e-8` 对齐。

同一 manifest 还包含官方 `M3areas_allowed_wFossilBranch` 非超度量树。`human` 是年龄
`0.09` 的普通古老末端，不启用直接祖先特例。固定 lnL 差 `3.62e-8`，节点 posterior
最大差 `3.11e-9`，16 个有效 split 最大差 `1.56e-9` 且权重零差。BGB 优化点固定重算
差 `8.13e-7`；其 `KKT1=FALSE` 点比 Rust 最优 lnL 低 `7.69e-5`，仅该 manifest 行显式
允许更高 Rust 终点，其他优化案例仍使用双边容差。随机历史门禁抽取
20,000 条历史，严格检查 `human` 枝只包含 `0.90 + 0.01` 两段、总占据时间为 `4.91`，
再对照节点和 split posterior；最大 z 为 `1.60`，最大总变差分别为 `0.00436/0.00475`。
详细输入契约见 [`../docs/tree-input-and-fossil-tips.md`](../docs/tree-input-and-fossil-tips.md)。

## BioGeoBEARS 超短枝直接祖先 golden

`direct_ancestor_fixtures.tsv` 使用官方三物种化石示例的树和范围数据，并通过
BioGeoBEARS 1.1.3 `add_hook()` 在 `chimp` 谱系生成两棵派生树：

- `tree.nwk`：侧枝 `1e-7`，在默认 `min_branchlength=1e-6` 下是直接祖先 hook；
- `tree_threshold_equal.nwk`：侧枝恰为 `1e-6`，因为源码使用严格 `<`，仍按普通分裂处理。

重新生成与检查：

```powershell
Rscript validation/biogeobears-dec-golden.R validation/direct_ancestor_fixtures.tsv validation/golden/biogeobears-direct-ancestor.tsv DEC
Rscript validation/biogeobears-dec-ancestral-golden.R validation/direct_ancestor_fixtures.tsv validation/golden/biogeobears-direct-ancestor-ancestral.tsv DEC
Rscript validation/biogeobears-dec-split-golden.R validation/direct_ancestor_fixtures.tsv validation/golden/biogeobears-direct-ancestor-split.tsv DEC
Rscript validation/biogeobears-dec-optim-golden.R validation/direct_ancestor_fixtures.tsv validation/golden/biogeobears-direct-ancestor-optim.tsv DEC optim

powershell -ExecutionPolicy Bypass -File validation/check-rust-dec-fixtures.ps1 -Manifest validation/direct_ancestor_fixtures.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec.ps1 -Manifest validation/direct_ancestor_fixtures.tsv -Golden validation/golden/biogeobears-direct-ancestor.tsv -Command dec
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/direct_ancestor_fixtures.tsv -Golden validation/golden/biogeobears-direct-ancestor-ancestral.tsv -Command dec -ProbabilityTolerance 2e-5
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/direct_ancestor_fixtures.tsv -Golden validation/golden/biogeobears-direct-ancestor-split.tsv -Command dec -ProbabilityTolerance 2e-5 -WeightTolerance 1e-8 -IgnoreZeroProbabilityPlaceholders
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-optim.ps1 -Manifest validation/direct_ancestor_fixtures.tsv -Golden validation/golden/biogeobears-direct-ancestor-optim.tsv -Command dec-optimize -LnLTolerance 2e-5 -MultiStartPoints 2
```

直接祖先树固定 lnL 差为 `3.97e-9`，节点 posterior 最大差为 `1.04e-8`，非 hook
节点 split posterior 最大差为 `8.87e-9`；阈值相等控制树对应为 `2.15e-8`、
`2.28e-9` 和 `2.28e-9`。split weight 均为零差。两棵树的 `d/e` 优化都收敛到相同
边界点，最优 lnL 差约 `2.98e-8`。

BioGeoBEARS 在 stochastic mapping 的 hook 步骤会令左右子枝状态等于祖先状态，但其
`add_cladogenetic_events_to_trtable()` 后处理仍把该恒等关系显示为 `sympatry (y)`。Rust
按 likelihood 源码和“无物种形成事件”的注释，不把 hook 写入 cladogenetic split 表；
核心和 CLI 测试同时检查状态复制与事件计数。

这组 golden 只声明 BioGeoBEARS 默认 `1e-6` 的兼容性。BioGeoBEARS 1.1.3 非分时期
`bears_optim_run()` 的辅助 likelihood 路径不会稳定转发 run object 中的非默认阈值；Rust
允许显式自定义阈值，但在建立独立的低层 R 对照前，不把其他阈值宣称为包装层 golden。

## BioGeoBEARS BSM distribution golden

完整 BSM 不比较相同 seed 下的逐条路径。BioGeoBEARS 使用前向模拟加终点拒绝，Rust
使用条件 uniformization bridge；只要实现的是同一个条件随机过程，两批独立大样本的
统计分布就应一致。

正式 fixture 使用 BioGeoBEARS 官方 `examples/BSM_3taxa/M3areas_allowed` 树、范围和
两时期 `areas_allowed`，参数取该案例已冻结的 ML 结果：

```text
d = 5.98044354276819
e = 1.31300515961732
j = 0
```

相关文件：

- `biogeobears-bsm-distribution.R`：在项目独立 R library 中分批运行 `runBSM`，逐条
  验证事件链和占据时间守恒，并写 BioGeoBEARS 摘要；
- `extract-rust-bsm-distribution.R`：从 Rust 兼容单文件输出或版本化流式目录读取四张
  BSM 汇总表，并转成同构宽表；
- `compare-bsm-distributions.R`：比较均值 Monte Carlo 标准误、经验 CDF 最大差和聚合
  时期占比；
- `check-bsm-distributions.ps1`：构建 release CLI、抽取独立 Rust 样本并执行门禁；
- `benchmark-bsm-parallel.ps1`：在官方三物种和 197-tip Conifer 两种负载下运行
  1/2/4/8/16 worker 扩展基准，并强制比较八张数据表的跨线程 SHA-256 指纹；
- `golden/biogeobears-bsm-distribution-samples.tsv`：5000 条 BioGeoBEARS 生物地理随机历史的冻结摘要；
- `golden/biogeobears-bsm-distribution-samples-metadata.tsv`：来源、参数、seed、版本和耗时。

日常门禁不重跑耗时的 R 端采样：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-bsm-distributions.ps1

# 可显式指定 Rust worker、最大在途随机历史数、耐久检查点间隔、固定分片大小和输出级别
powershell -ExecutionPolicy Bypass -File validation/check-bsm-distributions.ps1 `
  -RustThreads 8 -RustMaxInFlight 16 -RustCheckpointSamples 1024 `
  -RustShardSamples 1000 -RustOutputLevel summary
```

`-RustShardSamples 0` 保持单目录八表；正整数启用 `biogeo-bsm-sharded-tsv-v1`。提取脚本会
校验根 metadata 与 manifest 指纹、固定区间连续性和分片目录，再按 manifest 顺序读取汇总表。
`-RustOutputLevel` 接受 `legacy/full/compact/summary`；抽取器同时支持 v1 和三种 v2，并会把
compact/summary 稀疏占据表中缺失的组合补为精确 0。2026-08-11 使用 summary v2 重跑官方
5000 对 5000 门禁，39 项全部通过。

CLI 交互暂停可在固定模型命令中增加 `--bsm-interactive`。标准输入逐行接受
`pause/resume/status/cancel`，控制消息写标准错误。2026-07-16 的 Windows release 真实进程
门禁使用百万样本目标和 1000 样本分片：首次暂停稳定为 0，恢复后推进到 2866，第二次暂停
保持稳定，cancel 以 130 退出且 metadata 为 `cancelled/2866`；同一目录随后关闭交互、改用
1 worker 和 0 秒上限恢复，仍准确停在 2866。最新 release 又从 2866 恢复到 5000 后暂停/
取消；暂停后直接关闭 stdin 的测试会自动恢复，并由 1.5 秒 deadline 在 sample 12000 以
`time_limit/124` 提交。期间观察到一次 Windows 目录重命名瞬时 code 5，完整活动分片下次
恢复可发布；writer 已增加仅针对该瞬时错误的有限退避重试，复验未再出现。上述门禁都只运行
数秒，没有完成百万样本任务。

需要审计或重建 golden 时显式运行：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-bsm-distributions.ps1 -RefreshBioGeoBEARS
```

每端 5000 条随机历史共检查 39 个分布，包括：沿枝事件总数、`d/e`、`y/s/v/j`、两时期
事件数、逐条随机历史和聚合时期占比、8 个状态占据时间，以及 16 个“时期 × 状态”占据时间。
标量分布同时要求绝对均值差不超过 5 个合并 Monte Carlo 标准误，且经验 CDF 差不超过
`2*sqrt((n1+n2)/(n1*n2))`；5000 对 5000 时后者为 `0.04`。聚合时期占比另要求绝对差
不超过 `0.02`。

冻结结果 39 项全部通过：最大均值差为 `2.43` 个标准误，最大 CDF 差为 `0.0368`；
两个额外 Rust seed 各 5000 条的复核也全部通过。BioGeoBEARS 样本没有 manual history，
最大分支重试数 2862，低于 `maxtries_per_branch=40000`。当前机器上 BioGeoBEARS 随机历史采样
5000 条耗时 `550.62` 秒。2026-07-23 当前 release 的单目录重跑中，1/2/4/8/16 worker
中位数分别为 `1.268/1.025/0.948/0.991/0.979` 秒；相对 BioGeoBEARS 约为
`434x/537x/581x/556x/562x`。更早的 6 轮单目录/5 分片交替热运行中位数为
`0.756/0.946` 秒，用于估算分片约 25% 的目录创建、同步和发布开销。两种格式的 39 项
分布检查均全部通过；加入耐久检查点前的历史基线为 `0.398` 秒。

本机扩展基准：

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-bsm-parallel.ps1
powershell -ExecutionPolicy Bypass -File validation/benchmark-bsm-parallel.ps1 `
  -Workload conifer-197tip
```

本轮官方三物种 10,000 条在 1/2/4/8/16 worker 下的中位数为
`2.086/1.559/1.557/1.498/1.501` 秒；197-tip、41-state 的 100 条复杂负载为
`2.188/1.485/1.587/1.649/1.236` 秒。后者 16 worker 加速约 `1.77x`，轻负载约
`1.39x`。两组各档三次运行的八表指纹完全一致；剩余扩展瓶颈主要在
串行格式化和磁盘写出，而不是随机结果对线程调度的依赖。

### Ponerinae 真实分层 BSM pilot

`benchmark-biogeobears-ponerinae-bsm.R` 使用 1534-tip short-name MCC 树、7 区域 `.data`、
7 个时期边界和由论文 adjacency block 导入的 7 份显式允许范围表，固定在已对齐的
BioGeoBEARS 最优 `d/e` 上运行 `runBSM`。它把全局状态号映回 120-state master space，
逐条检查事件链和总枝长，并额外输出：

- `manual_fallback_branches` 和 `max_branch_tries`；
- `forbidden_state_transitions`、`forbidden_state_endpoints` 和 `forbidden_state_time`；
- 7 个时期的事件数、120 个状态占据时间和 840 个“时期 × 状态”占据时间。

`compare-ponerinae-bsm-pilot.R` 将一条 BioGeoBEARS 诊断历史与 Rust 多条分片/非分片历史作
描述性比较：

```powershell
Rscript validation/compare-ponerinae-bsm-pilot.R `
  <bgb-samples.tsv> <rust-bsm-output-dir> <report.tsv>
```

2026-08-10 的实测中，BioGeoBEARS setup 为 `145.94 s`，单条采样为 `68.35 s`；该条历史
在 22 个分支达到 40000 次尝试后启用 manual fallback，并经过时期状态表禁止的中间范围。
诊断记录 24 次禁用状态进入和 `11.0676` 时间单位的禁用状态占据。因此该数据只用于性能、
事件量级和 fallback 风险 pilot，不能替代上面的无-fallback 官方 5000 对 5000 分布 golden。

Rust 同配置 100 条、16 workers、每 25 条一个分片，legacy v1 完整运行
`10.848 s`、输出 `575,694,792` bytes。summary v2 在相同模型、seed 和资源参数下运行
`3.065 s`、输出 `705,641` bytes。两者的样本事件、时期事件和全部非零占据行逐值一致；
summary v2 的禁止状态转移、端点和占据时间均为 0。`compare-ponerinae-bsm-pilot.R`
已按根目录 `states.tsv`/`periods.tsv` 对稀疏 v2 表补精确零值。Rust 1 worker 和 10 workers
生成的同 10 条历史八表 SHA-256 全部一致。

### Ponerinae 统一工作流中断与恢复验收

下面的门禁不调用 BioGeoBEARS，也不重新建立模型公式。它把上面的真实 Ponerinae 输入复制到
隔离目录，生成 portable `biogeo-analysis-request-v1`，然后完整覆盖 plan、d/e 优化、compact
分片随机历史、事件预算停止、深度检查和恢复：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File validation/check-ponerinae-analysis-workflow.ps1 `
  -DatasetDir E:\RASP\examples\phase1_reference_data\Dore_2025_Ponerinae
```

默认第一次运行把任务总沿枝事件预算设为 2500。脚本要求留下大于 0 且小于 10 的完整样本前缀，
随后临时移走请求侧树、范围、参数和时期目录，以 50000 事件预算和 `--resume` 继续。最终再从同一
分析结果执行一次性 BSM 基线，对全部相对路径、字节数和 SHA-256 逐文件比较。运行目录保留：

- `acceptance.tsv`：版本化验收摘要；
- `workflow-interrupted-error.tsv` 和 `bsm-interrupted-inspection.tsv`：预期退出码 3 及未完成目录检查；
- `workflow-resumed.tsv` 和 `bsm-final-inspection.tsv`：恢复摘要和最终深度检查；
- `bsm-byte-comparison.tsv`：恢复目录与一次性目录的逐文件 SHA-256；
- `source-provenance.tsv`：四个外部科学输入的字节数和 SHA-256。

2026-08-21 本机验收得到 120 个 master states 和时期状态数
`36,36,27,20,24,20,38`；优化 126 次评估后 lnL 为 `-3049.873438616853`，
`d=0.027730761730114538`、`e=0.021323516561213916`。中断点为 2 条/2047 个沿枝事件，
恢复终点为 10 条/10352 个沿枝事件，0 个诊断违规；恢复与一次性基线的 35 个文件逐字节一致。
脚本参数可调整样本数、线程、分片、检查点和预算，但科学状态及恢复身份仍由 CLI 自身校验。

## DEC stress benchmark

速度压测脚本会生成合成树和 tip range 矩阵，输出放在已忽略的
`validation/benchmark-runs/` 下，不作为 golden fixture：

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-stress.ps1
```

默认案例为 `12 areas / 64 tips / max_range_size=3 / include_null_range=true`，
共 299 个 DEC 状态。脚本会先编译 release 版 Rust CLI，再分别测：

- Rust release CLI 的固定参数 DEC likelihood。
- 同一个 R 会话内 BioGeoBEARS 固定参数 likelihood 的热启动重复运行时间。

Rust 的 wall time 包含每次 CLI 进程启动、输入解析和模型构建；BioGeoBEARS 在同一
R 会话中预热后计时。脚本会把 `mx01` 同时传给 Rust 和 BioGeoBEARS 的
`mx01y/mx01s/mx01v/mx01j`，并在 lnL 绝对差超过 `LikelihoodTolerance` 时失败，
避免比较语义不同的模型。复杂 daughter-range 权重可这样运行：

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-stress.ps1 -Areas 8 -Tips 128 -MaxRangeSize 5 -Mx01 0.5 -RustRepeats 10 -BioGeoBEARSRepeats 3
```

如果只想测试 Rust 大规模压力而不启动 BioGeoBEARS，可以传
`-BioGeoBEARSRepeats 0`：

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-stress.ps1 -Tips 1000 -MaxRangeSize 5 -RustRepeats 3 -BioGeoBEARSRepeats 0
```

20/30 区域成功预检与超大组合状态数的分配前拒绝门禁：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-large-state-space-resources.ps1
```

该门禁复用 100-tip 压力输入，验证 21,700 和 174,437 状态可真实构造，并验证
614,429,672 状态在显式 1,000,000 上限处返回 `resource_limit`，不依赖内存耗尽来判定失败。

当前机器 2026-07-23、当前 release 代码的参考结果：

- `8 areas / 128 tips / max_range_size=5 / mx01=0.0001`：219 个状态，Rust
  7 次平均 `0.028369s`、中位数 `0.029225s`；BioGeoBEARS 热会话 3 次平均
  `5.303333s`、中位数 `5.32s`，按均值约 `186.94x`；
  lnL 绝对差 `4.31e-6`，相对差 `3.33e-9`。
- 同一输入、`mx01=0.5` 的复杂拆分权重：Rust 7 次平均 `0.044049s`、中位数
  `0.037142s`；BioGeoBEARS 热会话平均 `6.08s`、中位数 `6.03s`，按中位数约
  `162.35x`；lnL 绝对差
  `1.91e-6`，相对差 `1.80e-9`。
- `12 areas / 64 tips / max_range_size=3`：Rust 平均 `0.017568s`，
  BioGeoBEARS 热启动平均 `6.74s`，约 `383.7x`；两边 `lnL` 差异约 `5e-8`。
- `14 areas / 96 tips / max_range_size=3`：Rust 平均 `0.023382s`，
  BioGeoBEARS 热启动单次 `35.02s`，约 `1497.8x`；两边 `lnL` 差异约 `5.7e-8`。
- `12 areas / 64 tips / max_range_size=4`：Rust 平均 `0.033994s`，
  BioGeoBEARS 热启动单次 `100.21s`，约 `2947.9x`；两边 `lnL` 差异约 `9.5e-7`。
- `12 areas / 1000 tips / max_range_size=5`：1586 个状态，Rust release CLI
  5 次平均 `0.639512s`；统一 `analysis-run` 端到端 `0.668341s`，Windows 实测
  working-set 高水位约 `68.27 MiB`，平均逻辑核使用量约 `0.96`。BioGeoBEARS 同规模
  测试运行 30 分钟仍未产出单次 timing 结果，
  用户已有类似数据经验约为 2-3 小时，因此这里先记录 Rust 压力结果和 BioGeoBEARS
  未完成状态，不作为精确 speedup golden。

### DEC d/e 参数优化 benchmark

完整参数优化使用独立脚本，输入可以是上面生成的合成数据，也可以是真实 tree/range
文件：

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-dec-optimization.ps1 -Tree validation/benchmark-runs/dec-stress-8a-32t-m5-mx0p0001/tree.nwk -Ranges validation/benchmark-runs/dec-stress-8a-32t-m5-mx0p0001/ranges.tsv -MaxRangeSize 5 -Mx01 0.0001 -RustRepeats 3 -BioGeoBEARSRepeats 1
```

当前机器上 `8 areas / 32 tips / max_range_size=5 / 219 states` 的默认 DEC：

- Rust 自定义 log-rate Nelder-Mead：3 次平均 `1.158489s`、中位数 `1.104484s`，
  222 次 likelihood 评估。
- BioGeoBEARS `optim/L-BFGS-B`：`325.53s`，报告 23 次 objective 和 23 次
  gradient 评估。
- 端到端优化耗时按均值约为 **281.00x**、按中位数约为 **294.73x**；两边均收敛，最优 lnL 绝对差
  `5.93e-6`，`d` 差 `3.19e-5`，`e` 差 `1.40e-4`。
- 同一输入的固定参数 likelihood 本轮按均值加速约为 `52.00x`。完整优化的倍率还包含数据结构
  复用和优化器策略差异，不能全部解释为单次 likelihood 内核加速。

BioGeoBEARS 的 L-BFGS-B 使用数值梯度，gradient 内部还有未计入 `counts["function"]`
的 likelihood 调用。因此脚本保留“按 reported evaluation 折算”的诊断值，但它不是
严格的核心性能指标；正式比较应优先看固定 likelihood 和最终 lnL 对齐后的总耗时。

`max_range_size=4` 曾暴露过一个 DEC cladogenesis 语义差异：BioGeoBEARS 默认
`mx01v=0.0001` 只保留较小 daughter range 为 singleton 的 vicariance，因此四区域
祖先范围不包含 `2+2` 平衡分裂。Rust 侧已按这个 BioGeoBEARS 默认 DEC 语义修正。

本轮完整口径、原始命令和遥测解释见
[`../docs/performance-benchmark.md`](../docs/performance-benchmark.md)。

## LAGRANGE-ng 独立参考

为避免直接依赖 RASP 安装目录，可以把本机已有的 LAGRANGE-ng 复制到项目内：

```powershell
powershell -ExecutionPolicy Bypass -File validation/copy-local-lagrange-ng.ps1
```

默认来源：

```text
E:/RASP/engines/lagrange-ng
```

默认项目副本：

```text
validation/tools/lagrange-ng/
```

`validation/tools/` 已经被 `.gitignore` 忽略，不会把二进制和 DLL 提交进仓库。
后续 LAGRANGE-ng 对照脚本会优先使用项目副本；如果项目副本不存在，再回退到
RASP 的安装目录。

修复后的本地二进制已经能在 `mode = evaluate` 下正确使用配置中的固定
`dispersion/extinction`。运行当前输出并与独立基线比较：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-lagrange-ng-reference.ps1 -ScratchRoot C:\tmp
```

冻结的独立语义参考位于：

```text
validation/reference/lagrange-ng-dec.tsv
```

本次运行输出位于已忽略的：

```text
validation/lagrange-ng-output.tsv
```

这个检查只回答“当前 LAGRANGE-ng 是否仍保持已记录的 LAGRANGE-ng 行为”，
不比较 Rust lnL，也不会要求 Rust 改成 LAGRANGE-ng 的 split scenario 语义。

独立性能采样：

```powershell
powershell -ExecutionPolicy Bypass -File validation/benchmark-lagrange-ng-reference.ps1 -ScratchRoot C:\tmp -Repeats 3
```

输出写入 `validation/lagrange-ng-benchmark.tsv`，记录每个 LAGRANGE-ng fixture
的单次进程 wall time 均值、最小值和最大值；这个时间包含可执行文件启动成本。
它是性能参考，不是语义 golden。

测试本地 LAGRANGE-ng 与官方教程/示例的差异：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-lagrange-ng-official-reference.ps1 -ScratchRoot C:\tmp
```

当前审计输出写入 `validation/lagrange-ng-official-output.tsv`；修复后二进制的
冻结摘要放在 `validation/reference/lagrange-ng-official.tsv`。

详细结论见：

```text
validation/lagrange-ng-local-audit.md
```

## 当前案例

- `two_tip_zero_split`：零分支手工案例，主要验证 cladogenesis 权重。
- `two_tip_unit_split_null`：两 tip 非零分支，包含 null range，适合先和
  BioGeoBEARS DEC 默认思路对照。
- `three_tip_nested_null`：三 tip 嵌套树，验证递归 pruning 和分支传播。
- `three_area_tri_tip_null`：三地区三 tip，`max_range_size=2`，每个 tip 分布在不同单区。
- `three_area_widespread_tip_null`：三地区三 tip，包含一个双区 widespread tip。
- `four_tip_asymmetric_rates_null`：四 tip 非对称树，使用第二组固定 `d/e` 参数。
- `four_area_balanced_m4_null`：四区域四 tip 平衡树，`max_range_size=4`，用于锁定
  BioGeoBEARS 默认 DEC 中 `mx01v=0.0001` 的 singleton vicariance 语义，避免错误加入
  `2+2` 平衡分裂。
- `four_area_six_tip_mixed_null`：四地区六 tip，混合单区和双区 tip，`max_range_size=3`。
- `five_area_eight_tip_mosaic_null`：五地区八 tip，26 个 DEC 状态，覆盖更大的
  Q 矩阵和优化搜索空间。

## `mx01r` 兼容语义审计

BioGeoBEARS 1.1.3 的参数表包含 `mx01r`，但标注 `note=no`。源码追踪显示 Q、C、MaxEnt
和 root prior 路径均未消费它；非分时期和分时期入口都把
`probs_of_states_at_root=NULL` 传给 pruning。运行时审计命令：

```powershell
& 'C:\Program Files\R\R-4.5.0\bin\x64\Rscript.exe' `
  validation/biogeobears-mx01r-audit.R
```

日常门禁不覆盖 golden，而是生成临时结果并逐字节比较：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-biogeobears-mx01r.ps1
```

脚本在 `mx01r=0.0001/0.5/0.9999` 下运行 5-area/8-tip 复杂静态 DEC 和官方
Psychotria M4b 五时期案例，并要求 lnL、根后验、uppass/downpass、节点 split probability
及各时期 cladogenesis 签名逐元素零差。冻结结果位于
`golden/biogeobears-mx01r-audit.tsv`；Rust 参数入口另有回归，要求该行保持
`fixed(0.5)`。完整判定见 `docs/mx01r-audit.md`。

## 对照阈值

`validation/dec_fixtures.tsv` 里区分两类阈值：

- `tolerance`：Rust CLI 自身回归测试阈值，应该很严格。
- `external_tolerance`：Rust 与 BioGeoBEARS golden 的阈值，用来容纳矩阵指数
  实现造成的小数值差异。

LAGRANGE-ng 的独立比较阈值由 `compare-lagrange-ng-reference.ps1` 单独管理，
不复用这个字段。

## Newick / NEXUS 输入等价门禁

`fixtures/biogeobears_official/bsm_3taxa_fossil/tree_ape.nex` 是用项目隔离 R 环境中的
`ape::write.nexus()` 从 BioGeoBEARS 官方三类群化石树生成的单树 NEXUS；同目录
`tree_ape_multi.nex` 包含枝长控制树和名称为 `official` 的原树。以下门禁使用同一范围表和
固定 DEC 配置运行原 Newick、单树 NEXUS 与显式 `--tree-name official` 的多树 NEXUS，要求
lnL、节点 posterior、split posterior 及诊断等 112 行模型语义输出逐字节一致，并单独确认
多树输出的选择记录：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-tree-input-equivalence.ps1
```

门禁还要求 `convert-tree --tree-name official` 产生规范官方 Newick，并确认省略树名时拒绝
多树输入。NEXUS 层只负责可靠提取树与执行 `TRANSLATE`；三种输入随后共用同一 Newick
解析、树结构和 likelihood 路径，因此这里不复制一套格式专用的生物地理公式。

同一官方 fixture 的只读输入摘要可直接检查：

```powershell
target/release/biogeo-cli.exe validate-inputs `
  --tree validation/fixtures/biogeobears_official/bsm_3taxa_fossil/tree_ape.nex `
  --ranges validation/fixtures/biogeobears_official/bsm_3taxa_fossil/ranges.tsv `
  --min-branch-length 0.000001
```

预期识别 3 个 tip、2 个二叉内部节点、根年龄 2、一个年龄约 0.09 的 `human` 古老末端，
且没有低于阈值的直接祖先边。

## BioGeoBEARS detection 官方对照

`fixtures/biogeobears_official/psychotria_detection/` 保存 BioGeoBEARS 1.1.3 源码包中的
Psychotria 树、detections 和 inclusive controls。固定参数对照同时检查整树 lnL 和所有
19 个 tip、16 个状态、8 组参数下的 2432 个相对末端似然值：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-profile.ps1
```

单独释放 `mf`、`dp`、`fdp` 以及三者联合释放的动态优化对照：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-optim.ps1
```

R 端冻结结果分别由 `biogeobears-detection-profile-golden.R` 和
`biogeobears-detection-optim-golden.R` 生成。联合案例在 `dp=fdp` 附近存在可识别性
ridge，因此验收似然和等价组合，不要求不同优化器返回同一个任意 `mf` 坐标。该现象不会
限制用户在参数表中自由、固定或联动这些参数。

跨模块固定组合（静态 `x/n/u`、节点参数、全栈静态、官方五时期）与五维
`x/j/y/v/mf` 联合优化分别运行：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-combinations.ps1
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-combination-ancestral.ps1
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-combination-split.ps1
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-combination-optim.ps1
```

联合优化门禁先在 BioGeoBEARS 最优参数处固定交叉重算，再分别让 R 与 Rust 从清单初值和
两个相反方向的内部角点搜索，只在收敛解中选择最高 lnL。这样把似然语义差异与局部优化器
搜索差异分开，也不会把 BGB 最终答案直接作为 Rust 起点。BioGeoBEARS stratified 优化
以 `optim_result$value` 为真实目标；不能用可能受运行后 uppass 影响的
`total_loglikelihood` 覆盖它。

直接五时期固定 lnL 保留为 BioGeoBEARS stratified 审计。由于官方五组输入完全重复且
BioGeoBEARS stratified uppass 有已记录差异，祖先范围和 split posterior 使用第一个时期
构成的静态等价模型作为严格 BGB 参考；比较脚本另行强制 Rust 分时期与静态等价输出一致。
联合优化采用相同双参考规则。BGB 直接/静态最佳 lnL 相差 `1.84e-6`；Rust 在两组 BGB
坐标处固定重算最大差 `7.90e-6`，Rust 最佳结果比严格参考高 `7.82e-6`，且 Rust
stratified/static-equivalent 在严格参考点仅差 `2.55e-12`。

受约束 full-stack 案例进一步让五个时期的可用状态数依次变为 `16/8/4/2/2`，同时保留
距离、环境、面积、手工倍率、founder event、非默认 `mx01*` 和 detection。严格后验不使用
已知错误的直接 BGB stratified uppass，而由逐状态 fixnode likelihood 与时期局部 C 表重建：

```powershell
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-detection-full-stack-fixnode.ps1
powershell -ExecutionPolicy Bypass -File validation/check-biogeobears-detection-full-stack-optimization.ps1
powershell -ExecutionPolicy Bypass -File validation/check-detection-full-stack-bsm-distribution.ps1
```

最后一个门禁先通过 `biogeo-analysis-result-v2` 重放，再抽取 20,000 条生物地理随机历史，
逐项比较 288 个节点状态和 408 个 split 的经验频率。完整数值、BGB 优化器劣化候选的拒绝
规则和 bridge 稀有端点修复见
[`../docs/detection-full-stack-validation.md`](../docs/detection-full-stack-validation.md)。

## BioGeoBEARS 不确定范围观测对照

`fixtures/biogeobears_official/psychotria_ambiguities/` 从官方 Psychotria M4 的 19-tip、
4-area 树和范围表衍生，只将部分原始 `0/1` 隐去为 `?`，不翻转任何已知值。完整 BGB
工作流案例覆盖精确、presence-only 和混合约束；另一个源码级微型案例直接调用
BioGeoBEARS 1.1.3 的 `tipranges_to_tip_condlikes_of_data_on_each_state()`，逐格覆盖标准文件
入口会提前拒绝的全未知和纯 absence-only 语义。

重新生成项目隔离 R 环境中的全部 golden：

```powershell
Rscript validation/biogeobears-ambiguity-golden.R
```

该脚本生成固定 lnL、304 个 tip-state likelihood、288 个内部节点 posterior、`d/e`
优化结果和底层边界语义。Rust 固定 lnL 与 BioGeoBEARS 的绝对差为 `1.34e-7`；优化 lnL
绝对差为 `3.25e-7`，两边均把 `e` 放在 `1e-12` 下界，`d` 相差约 `6.5e-6`。

日常门禁：

```powershell
cargo test -p biogeo-core --test biogeobears_ambiguity_golden
cargo test -p biogeo-cli ambiguous
cargo test -p biogeo-cli bsm_fingerprint_distinguishes_exact_and_ambiguity_observation_modes
```

CLI 默认仍严格只接受 `0/1`，必须用 `--use-ambiguities` 显式启用 `?`。分析结果会冻结
`ambiguous_ranges` 观测身份；重放和生物地理随机历史继续使用同一 tip likelihood，且运行
指纹区分 exact 与 ambiguity 模式。完整输入与数学契约见
[`../docs/ambiguous-ranges.md`](../docs/ambiguous-ranges.md)。

## BioGeoBEARS 模型比较公式 golden

`biogeobears-model-comparison-golden.R` 在项目隔离 R 环境中直接调用 BioGeoBEARS 1.1.3 的
`calc_AIC_vals()`、`calc_AICc_vals()` 和 `AkaikeWeights_on_summary_table()`，冻结三组
`lnL/k/n` 的 AIC、AICc 和两类权重：

```powershell
Rscript validation/biogeobears-model-comparison-golden.R
cargo test -p biogeo-cli model_batch
powershell -ExecutionPolicy Bypass -File validation/check-model-batch-psychotria.ps1
```

Rust 端使用相同的 `n = tips` 约定，并额外把 `n <= k + 1` 的非有限 AICc 明确写为 `NA`。
只有全部 AIC 候选模型都有有限 AICc 时才生成 AICc 权重，避免把可计算子集重新归一化为
看似完整的候选集。
首版端到端门禁以两张真实参数表运行批量优化，检查非覆盖、权重归一化、完整标记，以及删除
一个模型目录模拟中断后的恢复；已完成模型的 metadata 保持逐字节不变。官方 Psychotria M4
六模型 manifest 位于 `examples/model_batch/psychotria-six-models.tsv`，release 实跑 6 个模型
全部收敛并同时生成 AIC/AICc 模型平均祖先范围；同一目录恢复返回逐字节相同的比较与平均
结果。完整契约见
[`../docs/model-batch.md`](../docs/model-batch.md)。

## BioGeoBEARS 模型平均祖先范围 golden

`biogeobears-model-average-golden.R` 在隔离的 BioGeoBEARS 1.1.3 中，对三地区三末端数据分别
优化 DEC 和 DEC+J，读取
`ML_marginal_prob_each_state_at_branch_top_AT_node`，再调用官方 AIC 权重函数逐状态加权。
该例 `n <= k + 1`，BioGeoBEARS 的 `calc_AICc_vals()` 会直接报错，因此 golden 有意只包含
AIC，并用于验证 Rust 不产生虚假 AICc：

```powershell
Rscript validation/biogeobears-model-average-golden.R
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-model-average.ps1
```

冻结文件为：

- `golden/biogeobears-model-average-weights.tsv`；
- `golden/biogeobears-model-average-ancestral.tsv`。

当前 14 个内部节点状态概率的最大绝对差为约 `3.50e-6`。独立的 Psychotria 六模型门禁
进一步检查 AIC 与 AICc 各 6 个模型、36 个祖先范围归一化组、36 个 split scenario
归一化组、三组无 `+J`/有 `+J` 边界嵌套关系，以及恢复前后
`model-averaged-ancestral-ranges.tsv` 逐字节一致。格式契约见
[`../docs/model-average.md`](../docs/model-average.md)。

## 公开 CLI 示例门禁

`check-public-cli-examples.ps1` 真实运行 `examples/` 中的六个 preset 请求、Psychotria 五时期
任务和时间预算停止/恢复任务。六个优化结果必须收敛并通过科学重放；分层结果必须封存 15 个
时期依赖；恢复任务必须先返回退出码 124 与 `bsm_time_limit`，随后复用两项拟合结果并完成
8 条生物地理随机历史的深度检查：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-public-cli-examples.ps1
```

`-ExamplesRoot` 可改为 Windows 安装目录中的 `examples/`，因此同一脚本也验证发布包中的文件，
而不是只验证仓库副本。

## 多模型真实数据工作流门禁

`check-model-workflow-real-data.ps1` 对官方 Psychotria 19-tip 案例和确定性 Ponerinae
32-tip/7-area 子集分别运行六个正式 preset。每项工作流先以 0 秒 BSM 预算受控停止，再恢复
4 条生物地理随机历史；门禁按正式 schema 校验 plan、run 和结果目录，恢复前后重放全部
12 个模型结果，并要求两项最终深度检查均为 0 违规：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-model-workflow-real-data.ps1
```

Ponerinae 静态 fixture 由 `make-ponerinae-subset.py` 从完整来源确定性生成。生成器通过显式
`--ete3-vendor` 使用新版 RASP 内置的 vendor ETE3，只负责 fixture 维护，不进入运行时或科学核心。

## 六 preset 修饰组合矩阵

`preset-modifier-combination-matrix.tsv` 在同一 4-area/6-tip fixture 上冻结六个正式 preset 的
静态与两时期组合。每项都启用 manual dispersal、地理距离、环境距离、area size 和非默认
事件专属 `mx01*`，并检查 plan 中的时期数、Q/split/branch 规模、固定 lnL、可移植结果与重放。
`preset-modifier-rejection-matrix.tsv` 另冻结 12 项缺失原始输入、重复来源和非法配置诊断：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-preset-modifier-matrix.ps1
```

该门禁检查组合与公共接口，不用自身冻结值替代 BioGeoBEARS 数值 golden。距离、面积和分裂
公式仍由现有单因素、posterior、optimization 与 detection full-stack 外部对照负责。

## Windows 发布与 schema 契约门禁

Rust 集成测试读取 `schemas/registry.tsv`，用真实 CLI 进程生成优化结果、输入包、随机化石树
结果和两模型批量结果，执行带重放的检查、v1→v2 迁移、机器错误与逐行进度，再严格校验根目录
条目、分节表、键、条件字段、表头、列数、类型和固定值：

```powershell
cargo test -p biogeo-cli --test schema_contract
```

Windows 发布门禁另从 release exe 构建 `biogeo-windows-package-v3` 目录和 ZIP，核对 ZIP
sidecar SHA-256，从归档解压后用 payload 清单安装，再由安装后的 exe 完成真实 DEC 优化和
`analysis-result-inspect --replay`。发布包包含完整 `examples/`；安装版 exe 还会运行上述公开
示例门禁、两项六模型真实数据工作流门禁和六 preset 修饰组合矩阵。临时根目录必须位于仓库
`target/` 且满足固定名称模式才会递归清理：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File validation/check-windows-release.ps1
```

`-SkipBuild` 仅跳过 `cargo build --release`，不会跳过打包、解压、安装、哈希或科学重放。
同一门禁还把 `rasp_host_contract` 指向安装后的 exe。当前 5 项宿主测试包含中文/空格路径、
进度与错误分流、取消/预算恢复，以及只读源输入完成后删除源目录、跨项目移动结果、重放并
重新生成生物地理随机历史的组合场景：

```powershell
cargo test --locked -p biogeo-cli --test rasp_host_contract
```

v3 包还强制校验 `release-status.tsv`、locked 构建信息、引擎源码哈希清单、构建来源、签名或未签名状态、
版本说明、变更记录、项目 `GPL-3.0-or-later` 许可证全文，以及 18 个 Windows 目标依赖 crate 的
37 份许可证文本。包和安装记录均声明
`public_research_release_candidate/GPL-3.0-or-later/public_distribution_allowed=true`。未签名不会阻止
构建、安装或公开分发；当用户显式要求签名时，才执行 Authenticode 检查。

v0.1 总候选门禁把 locked 工作区测试、Clippy、完整框架语义和上述 Windows 安装门禁串成一次
不可拆分验收。只有全部通过才写版本化、不可覆盖的 evidence：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File validation/check-v0.1-release-candidate.ps1
```
发布结构和新版 RASP 使用边界见
[`../docs/windows-release.md`](../docs/windows-release.md)。

两小时 Windows PC 长稳验收使用：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File validation/check-windows-pc-stability.ps1
```

默认每轮运行六模型优化、4096 条 compact 分片随机历史和深度检查，并核对跨轮科学指纹。常规
发布门禁只运行一轮 8 条样本的安装版冒烟；数据行写入、表同步与 checkpoint 临时文件写入中的
`StorageFull` 恢复由 Rust 测试构建确定性注入，正式 CLI 不包含故障开关。详见
[`../docs/windows-pc-stability.md`](../docs/windows-pc-stability.md) 和
[`../docs/windows-trusted-distribution.md`](../docs/windows-trusted-distribution.md)。

证据会绑定实际 CLI SHA-256、输入和请求指纹、可见核心数及逐轮记录哈希。Windows 发布门禁还以
不可能满足的最小空间阈值验证：低空间任务必须在第一个分析轮次前失败，不能靠实际填满磁盘测试。

2026-08-24 的正式两小时结果通过 367 轮、1,503,232 条随机历史和约 17.66 GB 累计逻辑写入；
受测 EXE、首轮结果和机器证据保存在
`benchmark-runs/windows-pc-stability-2h-20260824T084700Z/`。
最终重建 EXE 的 10 轮指纹桥接证据保存在
`benchmark-runs/windows-pc-stability-final-release-10cycles-20260824/`。
