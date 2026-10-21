# 生物地理随机历史 v3：RASP5 接入契约

原 v3 历史保持不变。需要为既有事件补充条件来源抽样时，使用独立的
[`bsm-assign-sources` 后处理](bsm-source-attribution.md)，不要覆盖原事件表的来源列。

适用构建：`0.1.1-dev.20260928`。这是开发交付，不是覆盖 GitHub 的 v0.1.0。
它包含 9 月 27 日的单模型后验接口，并增加本页说明的 BSM 输出。
不改变似然、参数优化、后验算法、事件权重、情景顺序或随机数协议。

## 如何调用

```powershell
.\biogeo-cli.exe engine-info
.\biogeo-cli.exe model-bsm --analysis-result .\fit --bsm-samples 1000 --seed 20260928 --bsm-output-level compact --bsm-output-dir .\histories
.\biogeo-cli.exe bsm-inspect --bsm-result .\histories --deep
```

分片再加 `--bsm-shard-samples 100`；并行可加 `--bsm-threads 4`。
线程数只控制 BSM 采样，不代表单模型似然和优化已经多核化。
单模型计算仍为一个计算 worker；本次不作额外加速承诺。

RASP5 应先检查 `engine-info.public_formats`，而不是只判断 exe 文件名。
完整发布目录带 `engine-info.tsv`、全部 schema、测试说明及 SHA256。

## 版本与兼容

新输出格式共六种：

- `biogeo-bsm-full-tsv-v3` / `biogeo-bsm-full-sharded-tsv-v3`
- `biogeo-bsm-compact-tsv-v3` / `biogeo-bsm-compact-sharded-tsv-v3`
- `biogeo-bsm-summary-tsv-v3` / `biogeo-bsm-summary-sharded-tsv-v3`

`full` 和 `compact` 的抽样相同，只是后者通过字典引用省略重复标签并省略零占据时间行。
`summary` 只写汇总，逐路径表保留表头但没有数据；不能用于事件网络展示。
默认 `legacy` 仍为旧 v1。接入方必须显式选择 `compact` 或 `full`。

旧 v1/v2 的布局不改动，`bsm-inspect` 继续读取。
不能从旧数据反推出原来没有保存的事件类别，也不会为旧输出补造它。
v3 不能原地续写 v2 的任务；用旧 exe 完成旧任务，或在新目录按相同模型、输入和 seed 重跑。
原有检查报告和工作流报告仅扩展它们引用的 BSM 格式枚举，字段含义不变。

## 实际抽中的分裂事件

`cladogenetic_splits.tsv` 保留原列，在末尾增加：

| 字段 | 含义 |
| --- | --- |
| `event_kind` | 实际抽中情景的 `range_copying`、`subset_sympatry`、`vicariance` 或 `founder_event` |
| `founder_daughter_order` | founder 的目标子代：0 为左、1 为右；其他类别为 `NA` |
| `source_area_index` | 当前恒为 `NA`，没有抽样具体来源区域 |
| `source_status` | founder 为 `undetermined`；其他分裂类别为 `not_applicable` |

类别来自生成情景时保存的类型，抽样后直接携带到结果，不用状态三元组反推。
`sample_event_counts.tsv` 的四类计数也直接读取这个类型。

目标子代状态由 `founder_daughter_order` 选择 `left_state_index` 或 `right_state_index`，
再通过 `states.tsv` 的 bitset 得到目标范围。自定义拆分参数下，它可以是多个区域；
不要随意选出其中一个区域当作“抽中的落点”。

左右始终是输入树的子节点顺序，不是按区域、名称或概率排序。
直接祖先节点没有分裂事件，不输出占位或虚构的 range-copying 记录。

## 来源区域与网络路线

引擎抽样的是 CTMC 状态转换及分裂情景，**没有额外抽样扩散来源区域**。
地理修饰可能已经参与转换或 founder 权重，但加权求和/平均不等于抽中了某一条路线。

`anagenetic_events.tsv` 末尾增加：

| 事件 | source_area_index | target_area_index | source_status |
| --- | --- | --- | --- |
| range_expansion | NA | 新增区域编号 | undetermined |
| local_extirpation | NA | NA | not_applicable |
| range_switching | 转换前单区域编号 | 转换后单区域编号 | state_transition |

原有 `area_index` 仍表示变动区域：扩散新增区域、灭绝丢失区域、switching 的目标区域。
switching 的来源来自实际单区域状态转换，不是额外路线抽样。
扩散即便来自单区域范围也统一报告来源未抽样；调用方可以另行说明其状态蕴含的唯一候选来源。

RASP5 可以用前状态或祖先状态中的区域作为候选来源，并做等权分摊；
必须标记这是**可视化分配规则**，不是引擎抽样的路线或路线后验。
本次没有在 CLI 中实现网络布局、地图、筛选、图片导出或交互式地理参数编辑。

## 节点、状态与时间

字典在主目录，分片共享：

- `nodes.tsv`：原有 `node,label,kind` 后增加 `age,is_direct_ancestor`。
- `edges.tsv`：原有 `edge,parent,child,length` 后增加 `child_order,is_direct_ancestor_hook`。
- `child_order` 从 0 开始；二叉分裂时 0 为左、1 为右。
- `areas.tsv` / `states.tsv` 的索引与现有格式一致，状态身份以 bitset 为准，不能拆分显示名称。
- 标签使用与拟合结果相同的百分号编码，解码一次：`%25`、`%09`、`%0D`、`%0A`。中文无需额外编码。

节点 ID 的范围是**本次解析的树**，不是跨重新排序树的标准化编号，也不是 ape 的编号。
从保存拟合运行时，`analysis_result_fingerprint` 指向来源拟合；
BSM 的 `node` 等于该拟合 posterior 的 `node_id`，`edge` 等于 `edge_id`，
状态和区域索引也相同。不需要拆分 clade 字符串。
没有来源拟合的直接命令不能声称与任意另一份拟合编号相同。

`age` 使用核心现有定义：最长根到末端距离减去根到该节点距离。
对定年的树可解释为距最近末端（通常是现在）的年龄；不自动转换为 Ma，也不推断绝对年代。
非现生末端可以有正年龄；这不是外部化石注释。

事件 `time_from_parent` 和 segment 起止时间都是从父节点向子节点走过的相对时间，
单位与枝长一致，不是距现在时间。事件年龄：
`nodes[parent].age - time_from_parent`。
分裂事件发生在对应节点年龄。时期索引 `q_index` 仍查询 `periods.tsv`。

## 检查与测试范围

`bsm-inspect --deep` 对 v3 full/compact（含分片）核对：

- 每个 sample 的四类逐条分裂计数等于汇总，类别值合法；
- 每个真正分裂节点恰有一条记录，tip 和直接祖先节点没有伪分裂；
- 来源状态、目标字段及 founder 子代标记符合声明；
- 字典节点年龄、子节点顺序和 hook 标记与树边、枝长阈值一致；
- 沿用已有事件链、时期计数、占据时间和检查点完整性校验。

summary 没有逐事件记录，只能检验它实际保存的汇总，不能声称逐条对账。
旧 v1/v2 检查保留旧能力，不声称检验缺失的事件类别。
这些是一致性检查，不等于证明任意输出未经篡改；分发文件请同时检查 SHA256。

新增测试位于 `crates/biogeo-cli/tests/bsm_v3_contract.rs`。
旧 exe 对照为显式运行的测试：

```powershell
$env:BIOGEO_BSM_BASELINE_EXE = "<9月27日冻结的biogeo-cli.exe>"
cargo test -p biogeo-cli --test bsm_v3_contract -- --include-ignored --nocapture
```

同 seed 的对照比较旧版原有八张表的全部对应字段，不要求新版文件整体字节相同
（新格式当然增加了列）。具体构建的结果和测试数见交付目录 `TEST-REPORT.md`。
