# 单模型祖先概率：RASP5 接入说明

此接口于 2026-09-27 加入开发版本，**原有 v0.1.0 发布包不包含它**。
先运行 `engine-info`，确认 `public_formats` 包含
`biogeo-single-model-posteriors-v1` 和 `biogeo-analysis-result-v3`。
不要仅凭 `--version` 的软件版本判断开发构建是否具备此功能。

这次只导出已有计算结果，不改变似然、优化、祖先后验或生物地理随机历史算法。
以下是单个拟合模型的条件后验，**不是模型平均结果**。

## 1. 如何生成

在现有 `biogeo-analysis-request-v1` 请求中启用：

```tsv
ancestral_probabilities	true
split_probabilities	true
```

按原方式运行：

```powershell
& $exe analysis-run --request analysis.tsv --output-dir result
& $exe analysis-result-inspect --analysis-result result --replay
```

也支持 `model-evaluate` / `model-optimize` 的 `--ancestral-probs`、
`--split-probs` 与 `--analysis-result-dir` 组合。
仅输出 stdout、不指定结果目录时，仍保持原来的交互行为。

生成的目录：

```text
result/
  metadata.tsv                 # biogeo-analysis-result-v3
  inputs.tsv
  source-parameters.tsv
  resolved-parameters.tsv
  input-bundle/
  posteriors/
    metadata.tsv               # biogeo-single-model-posteriors-v1
    files.tsv
    nodes.tsv
    edges.tsv
    areas.tsv
    states.tsv
    node_posteriors.tsv         # 请求 ancestral_probabilities 时才存在
    split_posteriors.tsv        # 请求 split_probabilities 时才存在
```

父 metadata 的 `posteriors_available` 指明是否生成该目录。
两种概率都未请求时，不创建后验目录，不把“没有计算”伪装成零概率。
目录内 metadata 的两个布尔字段分别声明具体生成了哪类概率。
拟合结果与后验文件在同一临时目录中完成，检查成功后才整体提交。

## 2. 树节点映射

`nodes.tsv`：

| 字段 | 含义 |
|---|---|
| node_id | 从 0 开始的节点编号；覆盖全部 tips 和内部节点 |
| kind | `tip` 或 `internal`，根是否为根由独立字段表示 |
| label | 输入树解析后的原始标签；不包含 Newick 的引号语法 |
| label_present | 区分缺少标签和显式空标签；tip 保留实际名称 |
| is_root | 根节点标记 |
| is_direct_ancestor | 引擎当前枝长阈值下的直接祖先节点标记 |
| fossil_status | `direct_ancestor_tip` / `non_extant_tip` / `not_identified` |
| age | 距最年轻末端的年龄，单位与输入枝长相同 |

`edges.tsv` 的字段为
`edge_id,parent_id,child_id,length,child_order,is_direct_ancestor_hook`。
`child_order` 从 0 开始，明确记录输入树解析后的子节点顺序。

编号在**该次拟合结果内**固定，保存后不会变化；不能假定与 R 的 ape 编号相同，
也不能假定重新排序或重新定根后的树编号相同。
RASP5 应按原始 tip 名称、显式边和子节点顺序重建或对应树。
不要拆分 `A+B+C` 形式的文本，也不要使用内部标签作为唯一节点键。
节点和分支编号沿用当前引擎树对象，可与同一结果生成的 BSM 引用表对应。

**化石标记的边界：**这里没有额外输入古生物鉴定信息。
`direct_ancestor_tip` 来自引擎识别的短挂枝；`non_extant_tip` 表示 tip 年龄大于
metadata 中记录的 `tip_age_tolerance`。近似超度量树的舍入误差也可能触发后一项，
因此 RASP5 应显示为“非现生末端（据枝长）”，不能据此断言它一定是化石。
`not_identified` 也不等价于已证明是现生物种。不得在接入层偷偷改枝长来消除这些标记。

## 3. 区域和状态字典

- `areas.tsv`：`area_index,area`，区域位序从 0 开始。
- `states.tsv`：`state_index,range_bits,range,is_null`。
- `range_bits` 是十进制 u64，第 i 位对应 `area_index=i`；使用无损整数读取。
- `range` 是显示名称，不是可逆编码。即使区域名称包含 `+`，也只用 bitset 解码范围。
- null 的 bitset 为 0，`is_null=true`；是否存在该状态由 `include_null_range` 决定。

所有表使用 UTF-8 TSV。文本字段的 `%`、tab、CR、LF 分别编码为
`%25`、`%09`、`%0D`、`%0A`，只解码一遍。空格、`+` 和中文不拆分。
不要使用会把 `%xx` 自动按 URL 任意转义的标签解析规则。

## 4. 两种概率不要混淆

`node_posteriors.tsv`：`node_id,state_index,probability`。
覆盖每个内部节点和每个状态，包括概率为 0 的状态，按节点和状态编号排序。
每个内部节点的概率和应在 `1e-8` 绝对容差内为 1。
位置为 `branch_top_at_node`：节点分裂前的祖先范围，不是两条子分支各自的起始范围。
tips 在节点映射中保留，但此文件不重复导出 tip 的范围概率。

`split_posteriors.tsv` 单独给出：

```text
node_id left_node_id right_node_id ancestor_state_index left_state_index right_state_index event probability
```

实际分隔符为 tab。这一张表把分裂情景和情景概率放在一起。
左右子代分别是 `child_order=0/1`，事件为 `range_copying`、`subset_sympatry`、
`vicariance`、`founder_event`。每个发生分裂的节点，全部情景概率和为 1。
不输出不允许的情景；直接祖先节点不伪造一个分裂事件。

## 5. 来源与检查

后验 metadata 记录格式、生成状态、引擎版本、模型指纹、对应拟合结果指纹、
树输入指纹、输入清单指纹、区域与状态空间指纹、null 设置及概率位置。
`model=BIOGEOBEARS_LIKE_CONFIGURABLE` 表示统一可配置模型；实际科学模型由父目录中的
完整参数表及修饰输入定义，不能把这个字段当成某个 preset 的名称。
`analysis_result=..` 是相对引用，整体移动结果目录不会失效。
指纹沿用项目现有校验算法，用来发现意外变化，不是密码学签名。

`analysis-result-inspect` 自动检查声明的文件、文件指纹、拟合来源、树结构、
节点/状态编号、子节点顺序、直接祖先标记、概率范围、完整性和归一性。
`--replay` 另外用结果目录中的自包含输入和固定参数复算似然，不重新优化。
它不是再次独立抽样 BSM，也不声称仅靠校验和能证明数学正确。

RASP5 导入顺序：读父 metadata → 确认 v3 和 `posteriors_available` →
读子 metadata 并确认其格式 → 读字典和边 → 按编号读取两种概率。
重开项目时仍从保存的文件读取，不依赖终端输出或重新运行优化。

## 6. 旧版本与范围

- 旧 `biogeo-analysis-result-v1/v2` 继续可以读取、检查及重放。
- `analysis-result-migrate` 可以将 v1/v2 迁移到 v3，但不会凭空补算后验。
- 对旧拟合点需要概率时，可用其 `resolved-parameters.tsv` 和自包含输入运行固定参数
  `model-evaluate`，明确带上相同 null、范围上限、根先验、枝长处理和所有修饰文件。
  不能只拿树、矩阵和 d/e/j 重算复杂模型。
- 新摘要格式为 `biogeo-analysis-run-v3`、`biogeo-analysis-workflow-v2`、
  `biogeo-analysis-result-inspection-v2` 和 `biogeo-analysis-result-migration-v2`。
- BSM 表结构未变；其来源格式引用允许指向新 v3 拟合结果。
- 本次不实现 RASP5 绘图或 BSM 网络图，也不改模型平均格式。

字段级契约见 `schemas/biogeo-single-model-posteriors-v1.schema.tsv`，
父目录契约见 `schemas/biogeo-analysis-result-v3.schema.tsv`。
