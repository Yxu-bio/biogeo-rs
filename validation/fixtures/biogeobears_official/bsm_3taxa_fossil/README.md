# BioGeoBEARS 官方三类群化石末端案例

来源：本地 BioGeoBEARS 包
`extdata/examples/BSM_3taxa/M3areas_allowed_wFossilBranch/`。

- `tree.nwk` 原样保留官方非超度量树；`human` 的末端年龄为 `0.09`。
- `tree_ape.nex` 由项目隔离 R 环境中的 `ape::write.nexus()` 从同一棵官方树导出；
  仅移除了 APE 自动写入的运行日期，以便 fixture 可重复。它用于验证 NEXUS `TAXA`、
  单树 `TREES`、`TRANSLATE` 和 `[&R]` 根注释，不作为新的生物地理模型 golden。
- `tree_ape_multi.nex` 同样由 APE 导出，包含一个枝长减半的输入控制和原样 `official` 树。
  它只验证必须用 `--tree-name official` 显式选择多树 NEXUS；生物地理 golden 仍来自原始
  官方树，不把减半控制当成官方案例。
- 范围数据来自官方 `geog.data`。
- 两个时期和允许区域矩阵来自官方 `timeperiods.txt` 与
  `areas_allowed_noC.txt`。
- 对照脚本将 `min_branchlength` 设为 `0`，因此这里验证普通古老末端，
  不启用 BioGeoBEARS 的超短枝直接祖先特例。
