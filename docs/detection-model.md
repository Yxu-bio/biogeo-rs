# Detection 末端观测模型

## 模型位置

`mf`、`dp` 和 `fdp` 不改变沿枝 Q 矩阵或节点分裂表。它们把每个末端、每个地区的
detection/control 计数转换为“真实范围状态给定时观测到这些计数的相对似然”，再把该向量
作为 pruning 的末端输入。因此它属于观测模型，而不是 DEC 的第四种地理事件。

三个参数沿用 BioGeoBEARS 含义：

- `mf`：目标在真实存在地区被纳入可检测材料的平均频率；
- `dp`：目标存在且进入材料时的检测概率；
- `fdp`：目标不存在，或未进入材料时的假阳性检测概率。

对真实存在的地区，单次检测概率为：

```text
p_present = mf * dp + (1 - mf) * fdp
```

对真实不存在的地区：

```text
p_absent = fdp
```

若一个地区有 `detections` 次阳性和 `controls` 次总控制计数，其对数观测似然是：

```text
detections * ln(p) + (controls - detections) * ln(1 - p)
```

`controls` 包含阳性次数，因此必须满足 `0 <= detections <= controls`。一个范围状态的
对数似然是所有地区贡献之和。

## BioGeoBEARS 相对缩放

BioGeoBEARS 对每个 tip 的状态对数似然减去该 tip 的最大值，再取指数；null range 的值
固定为 0。这个缩放常数不会加回整树 lnL。Rust 严格复刻这一行为，而不是改成另一种在
数学上看似等价、但数值 lnL 不同的标准化方式。

优化时不能只在初始参数计算一次末端向量。每个 `mf/dp/fdp` 候选值都会重新生成全部
tip-state 观测似然，然后由同一套 `LikelihoodEngine` 执行 pruning。

## 输入契约

detections 和 controls 都是制表符分隔矩阵。第一列是 tip/OTU，后续列是地区；两份文件的
地区名和顺序必须相同。官方 BioGeoBEARS 文件允许第一列表头留空，Rust 也接受 `tip` 或
`OTU`。解析器还会拒绝：

- 树中 tip 缺失、重复或出现未知 tip；
- 非有限、负数或 `detections > controls`；
- 两张表地区不一致；
- 超过当前 64 地区的状态位掩码上限。

通用固定评估示例：

```powershell
cargo run --release -q -p biogeo-cli -- model-evaluate `
  --tree validation/fixtures/biogeobears_official/psychotria_detection/tree.nwk `
  --use-detection-model `
  --detections validation/fixtures/biogeobears_official/psychotria_detection/detections.tsv `
  --controls validation/fixtures/biogeobears_official/psychotria_detection/controls.tsv `
  --parameters <fixed-parameters.tsv> `
  --max-range-size 4
```

`model-optimize` 使用同一组输入参数。`--ranges` 与 `--use-detection-model` 是互斥的末端
观测模式；计数文件不会在未显式启用 detection 模式时被静默解释。

## 官方 golden

官方 Psychotria 对照覆盖：

- 8 组固定 `mf/dp/fdp` profile；
- 19 个 tips x 16 个范围状态 x 8 组参数，共 2432 个相对末端似然值；
- 单独释放 `mf`、`dp`、`fdp` 和三者联合释放的四组优化。

运行：

```powershell
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-profile.ps1
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-optim.ps1
```

固定 profile 的 Rust/BioGeoBEARS 最大绝对 lnL 差约为 `1.59e-7`。优化对照最大差约为
`4.45e-6`，低于 `2e-5` 门限。

跨模块组合另有四个固定门禁：静态 `x/n/u`、非默认 `y/s/v/j + mx01*`、包含
`a/b/w/x/n/u/j` 的全栈静态点，以及官方 Psychotria 五时期 manual/distance/area-size
输入。前三者最大差 `3.19e-7`，五时期点差 `1.94e-6`：

```powershell
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-combinations.ps1
```

联合优化门禁同时释放 `x/j/y/v/mf`，覆盖全栈静态输入以及官方重复五时期输入。R 与
Rust 都使用清单初值加两个显式附加起点，并只在收敛解中选取最高 lnL；BGB 最优参数
不会被回填为 Rust 起点：

```powershell
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-combination-optim.ps1
```

全栈静态案例在 BGB 最优点的 Rust 固定重算差为 `2.16e-7`。五时期静态等价与直接
stratified 的 BGB 最佳 lnL 相差 `1.84e-6`；Rust 在两组 BGB 坐标处的固定重算差分别为
`7.80e-6` 和 `7.90e-6`，Rust 最佳 lnL 比严格静态 BGB 参考高 `7.82e-6`。这项门禁先
比较同一点语义，再比较各自搜索质量，不要求局部优化器返回相同参数末位或相同轨迹。

组合 posterior 也已独立冻结：四个严格参考共包含 1152 个祖先范围概率和 12,942 个节点
分裂 scenario。祖先范围最大差 `8.58e-8`，split 概率最大差 `9.47e-8`，scenario weight
最大差 `1.28e-8`：

```powershell
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-combination-ancestral.ps1
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-combination-split.ps1
```

官方五时期输入的五组矩阵完全重复，但 BioGeoBEARS stratified uppass 不严格退化到静态
posterior，这是项目已有的参考实现缺陷审计项。直接 stratified lnL 继续冻结；posterior
使用第一个时期矩阵构成的数学等价静态模型作为严格 BioGeoBEARS 参考。同时，Rust
stratified 与 static-equivalent 的祖先范围最大差 `2.0e-15`，split 概率最大差
`1.01e-15`，split weight 完全相同。Rust 不复制该 uppass 缺陷，也不删除差异证据。
优化输出同样区分目标函数与后处理字段：BioGeoBEARS stratified 运行以
`optim_result$value` 作为优化 lnL，不以可能受 uppass 影响的运行后
`total_loglikelihood` 覆盖它。Rust 在严格 BGB 最优点的 stratified/static-equivalent
lnL 差为 `2.55e-12`。

## 可识别性风险

当 `dp == fdp` 时，真实存在与不存在地区的检测概率相同，观测数据不再提供范围信息；
此时 `mf` 可以沿 ridge 变化而几乎不改变似然。不同优化器返回不同的 `mf` 不代表实现
不一致，也不能据此禁止联合优化。输出必须保留完整参数、边界状态和最终 lnL，用户可用
profile 或多起点判断可识别性，golden 则比较等价最优似然和 ridge 条件。
