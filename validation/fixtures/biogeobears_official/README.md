# BioGeoBEARS 官方数据 fixture

这些文件由 `validation/import-biogeobears-official-fixtures.R` 从项目隔离 R 环境中的
BioGeoBEARS `extdata/examples` 导入，避免手工改写树、tip 分布和官方矩阵。

## BSM 3-taxon areas-allowed

- 来源：`examples/BSM_3taxa/M3areas_allowed`。
- 官方输入：3-tip 树、3-area tip ranges、`timeperiods.txt` 和
  `areas_allowed_noC.txt` 的两期矩阵。
- `anagenetic_strata.tsv` 仅把两个官方时期和矩阵整理为 Rust 七列 schedule；没有
  修改树、范围或约束数值。
- `adjacency_strata.tsv` 是明确标注的合成派生案例：老时期保留 A-B 连通并把 C 作为
  可存在但与 A/B 隔离的 singleton，用于独立验证 adjacency，不冒充官方输入。

## Psychotria M4

- 来源：`examples/Psychotria_M4_dists`。
- 官方输入：19-tip 树、4-area tip ranges、`Hawaii_KOMH_distances_max1.txt` 和
  `Hawaii_KOMH_area_of_areas.txt`。
- `area_sizes_geomean1.tsv` 将四个官方面积除以其几何均值。对自由 `u`，这只把
  `e` 重参数化为 `e / geometric_mean^u`，不改变 lnL、`x/n/u` 或区域间相对效应；
  它用于避免 BioGeoBEARS 以原始大面积联合优化时出现严重尺度问题。
- `island_age_distances.tsv` 是可审计的派生输入，不是 BioGeoBEARS 原文件：使用
  官方 `Hawaii_timeperiods.txt` 中与 K/O/M/H 对应的岛龄 5.1/3.7/1.9/0.5，定义
  非对角值 `1 + abs(age_i - age_j)`，对角线为 0。它作为 `n` 的第二距离协变量，
  不复制地理距离矩阵。
- `manual_exp_neg_distance.tsv` 也是明确标注的可审计派生输入：对官方
  `Hawaii_KOMH_distances_max1.txt` 的非对角距离逐元素计算 `exp(-distance)`，对角线置 1。
  它只用于 `w` 的手工扩散倍率 profile，不冒充 BioGeoBEARS 官方 manual 文件。

## Psychotria 不确定范围衍生案例

- `psychotria_ambiguities/ranges.tsv` 使用同一官方 Psychotria M4 树和范围表，只把预先选定的
  已知 `0/1` 单元格隐去为 `?`，不翻转任何 presence/absence 值。
- 完整案例覆盖精确、presence-only 和混合约束，并遵守 BioGeoBEARS 标准运行入口要求每个
  tip 至少保留一个已知 `1` 的限制。
- 全未知和纯 absence-only 不伪装成完整官方工作流，而由直接调用 BioGeoBEARS 1.1.3
  `tipranges_to_tip_condlikes_of_data_on_each_state()` 的源码级 golden 覆盖。
- 派生规则、观测语义和包装层限制详见 `psychotria_ambiguities/README.md`。

## Psychotria M4b stratified

- 来源：`examples/Psychotria_M4b_dists_stratified`。
- 官方输入：19-tip 树、4-area tip ranges、5 个 time period，以及每个时期对应的
  manual dispersal、地理距离和 area-of-areas 文件。
- `psychotria_m4_stratified/anagenetic_strata.tsv` 只把官方文件路径整理为 Rust 扩展
  schedule 格式，没有改写数值。该官方示例的五组输入内容相同，因此第一个时期还被
  冻结为 `psychotria_m4b_static_equivalent`，用于检查重复分段是否严格退化为静态模型。
- BioGeoBEARS stratified uppass 的环境距离索引和高迁移率优化存在已记录差异；直接
  stratified 结果与数学等价的静态参考都保留在 golden 中，二者用途不混用。

## Conifer DEC+x

- 来源：`examples/395lab/conifer_DEC+x_traits_models`。
- 官方输入：197-tip 树、6-area tip ranges 和 `modern_distances_subset.txt`。
- 原官方脚本使用自定义状态集合：null、全部 singleton/pair range，再加 DFG。
  当前 Rust 对照使用标准 `max_range_size=3` 完整状态空间时，BioGeoBEARS 端也必须
  使用同一完整状态空间，不能拿官方脚本保存结果直接比较。
- `simulate-biogeobears-conifer-xnu.R` 使用官方 197-tip 树和官方地理距离，在
  BioGeoBEARS 的 Q/C 模型中以固定随机种子模拟联合 `x/n/u` fixture。环境距离来自
  预先声明的二维验证坐标，面积为预先声明并归一化的正值；两者是合成实验协变量，
  不是自然数据。已知参数保存在 `sim_true_parameters.tsv`。

BioGeoBEARS 包和这些示例采用 GPL 许可；验证文档应保留包版本、导入脚本和派生规则。
完整来源、修订号和许可文本见 `LICENSE-NOTICE.md` 与 `COPYING-GPL-2.txt`。
