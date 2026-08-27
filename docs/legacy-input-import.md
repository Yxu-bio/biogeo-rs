# BioGeoBEARS 与 RASP 输入导入

## 范围数据

所有接受 `--ranges` 的分析、优化、规划和验证入口都按文件内容识别三种格式：

- 项目规范 TSV：`tip` 后跟区域列；
- BioGeoBEARS/LAGRANGE geography `.data`：首行 `<taxa> <areas> (area names)`，随后为 taxon 和
  固定长度 `0/1` 编码；
- 常见 CSV：自动寻找唯一的 `Name`、`tip`、`taxon` 或 `species` 列，并把其后的列视为区域。

`.data` 是正式输入，不是只为某个 fixture 保留的兼容路径。需要查看规范化结果时运行：

```powershell
biogeo-cli convert-ranges --ranges ranges.data
biogeo-cli convert-ranges --ranges ranges.csv --taxon-column Name
biogeo-cli convert-ranges --ranges ranges.csv `
  --taxon-map taxon-map.tsv `
  --area-map area-map.tsv
```

输出是 `biogeo-range-table-v1` 规范 TSV。转换会检查声明的 taxon/区域数、编码长度、重复名称、
CSV 引号和单元格值；不会按前后缀、同物异名或字符串相似度修改 taxon。需要改名时必须提供
显式映射。taxon 映射表头为 `source_taxon<TAB>target_taxon`，地区映射表头为
`source_area<TAB>target_area`。转换器拒绝未知 source、重复 source/target 和映射后冲突，
并在输出注释中记录实际应用数。树与范围名称不一致时，正式分析仍由核心范围解析器明确拒绝。

## 分时期矩阵

BioGeoBEARS 常把时间边界、dispersal multiplier 和 adjacency 分成三个文件。转换命令为：

```powershell
biogeo-cli convert-biogeobears-strata `
  --time-boundaries time_boundaries.txt `
  --dispersal-matrices dispersal_matrices.txt `
  --adjacency-matrices adjacency_matrices.txt `
  --output-dir converted-strata
```

输出目录包含 `strata.tsv`、逐时期的矩阵或允许状态表以及 `metadata.tsv`。目录采用非覆盖和
临时目录原子发布；`strata.tsv` 只引用同目录相对路径，因此移动时必须整体移动。转换要求时间
边界严格递增、矩阵数完全对应、区域顺序在所有时期一致、dispersal 值有限且非负、adjacency
值只能为 `0/1`。BioGeoBEARS 文件末尾的单独 `END` 被正式支持，但 `END` 后出现其他内容会失败。

不提供 `--adjacency-matrices` 时只导入 dispersal，不会伪造全相邻矩阵。提供后，adjacency
进入 BioGeoBEARS 状态约束语义：一个多区域范围只有在其任意两个成员区域之间矩阵值都为 1 时
才允许，单纯通过中间区域连通并不够。末端观测若在其采样时期被约束完全排除，似然引擎会在
剪枝前报告冲突末端和时期；不会归一化、丢弃 taxon 或自动放宽约束。

部分 BioGeoBEARS 脚本不使用上述内置 adjacency 检查，而是自行生成每时期允许状态列表。
这种情况必须显式选择另一条转换规则：

```powershell
biogeo-cli convert-biogeobears-strata `
  --time-boundaries time_boundaries.txt `
  --adjacency-matrices adjacency_matrices.txt `
  --adjacency-range-rule edge-covered `
  --max-range-size 5 `
  --output-dir converted-strata
```

`edge-covered` 对 singleton 和 null range 直接放行；多区域范围要求每个成员至少与范围内另一个
成员相邻。因此链式 `A-B-C` 允许 `A+B+C`，但不允许 `A+C`。转换器会写出版本化
`allowed-ranges-NNN.tsv`，八列 schedule 的 `allowed_ranges` 列引用它；不会改写
`areas_adjacency` 的 all-pairs 定义。该模式的 dispersal 文件是可选的，因为有些分析只使用
邻接生成状态集合，并未把 dispersal multiplier 乘入 Q。

## Ponerinae 实测

`Ponerinae_MCC_phylogeny_1534t_short_names.tree` 与
`lagrange_area_data_file_7_regions_PaleA.data` 已直接验证为 1534 个一一对应 taxon、7 个区域、
二叉且在舍入容差内超度量。`.data` 是原 RASP 后台调用 BioGeoBEARS 所需的原始格式；Rust CLI
可直接读取，但不要求用户继续使用该格式。

同目录 CSV 与 short-name 树存在一个真实名称差异：CSV 的
`NewGenus_bucki_EX2455_DZUP549431` 对应树和 `.data` 中的 `Neoponera_bucki`；CSV 使用地区全名，
时期文件使用 `A/U/I/R/N/E/W`。验证目录提供两张显式映射表，不在代码中内置替换：

- `validation/reference/ponerinae-short-tree-taxon-map.tsv`
- `validation/reference/ponerinae-area-map.tsv`

[论文的官方分析脚本](https://github.com/MaelDore/Ponerinae_Historical_Biogeography)将
`lists_of_states_lists_0based` 直接写入各模型，并未把 adjacency 文件交给 BioGeoBEARS 的
all-pairs 检查。其自定义算法正是 `edge-covered`，`max_range_size=5` 时七时期状态数（含 null）
为 `36,36,27,20,24,20,38`。Rust 转换器逐期得到相同计数，所以 `U+I+E` 可通过 U-I 和 I-E
两条边合法存在。

在 1534-tip short-names 树上，以 `d=e=0.01`、DEC、120 个 master states 和该七时期状态表
固定计算，CSV+两张映射表与 `.data` 均得到逐位相同的
`lnL=-3279.174634278399026`；`analysis-plan` 报告 4073 个实际分支时期片段。这个结果验证两条
Rust 输入路径等价，不冒充论文最优参数或新的 BioGeoBEARS lnL golden。

可重复检查命令（`DatasetDir` 指向包含 `final_inputs` 和 CSV 的整理后数据目录）：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-ponerinae-official-inputs.ps1 `
  -DatasetDir E:\RASP\examples\phase1_reference_data\Dore_2025_Ponerinae
```

脚本默认使用临时运行目录并在成功或失败后清理；加 `-KeepRun` 可保留转换产物用于审计。
