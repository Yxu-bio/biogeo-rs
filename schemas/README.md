# 机器接口 schema 契约

本目录是新版 RASP 与 `biogeo-cli` 之间的版本化机器契约，不是面向用户的示例输出。
`registry.tsv` 使用 `biogeo-schema-registry-v1`，把公开格式号映射到独立 schema 文件；Windows
发布包会原样携带整个目录。

每个 schema 文件使用 `biogeo-schema-contract-v1`，固定六列：

```text
record_kind  location  name  requirement  value_type  constraint
```

- `file/directory` 描述目录条目；`location=.` 表示根目录，`shards/*` 这类位置表示每个
  直接子目录。已声明的目录层级不允许静默增加未知条目。
- `key` 描述键值 TSV。目录内键值文件以 `key<TAB>value` 为表头；进程 stdout/stderr 键值块不带表头。
- `column` 按出现顺序描述普通 TSV 表头、分节 TSV 中指定 `location` 的表或逐行事件字段。
- `requirement` 为 `required`、`optional` 或 `when:<key>=<value>`；条件字段必须与条件同时出现或消失。
- `constraint` 为 `-` 时无额外常量限制；`literal` 使用固定值，`enum` 以 `|` 分隔允许值。
- `encoded_string` 沿用 CLI 的百分号编码：原始 `%`、制表符、回车和换行分别写为
  `%25/%09/%0D/%0A`。

schema 只定义可跨语言解析的结构、类型和固定值。文件指纹、计数相等、路径不得越界、模型身份和
科学重放等关系约束仍由 `analysis-result-inspect --replay`、`input-bundle-inspect` 和加载器执行。

兼容规则是：同一格式号不能删除、改名、重排或改变已声明字段的语义。需要破坏性变化时发布新的
artifact 格式号和 schema 文件，同时保留明确的只读加载或迁移路径。Rust 进程级契约测试会生成
真实分析结果、完成 v1 到 v2 迁移、生成随机化石树和两模型批量结果，并在中文且带空格的目录中
执行统一 request 的 plan/run，以及包含 compact 随机历史与深度检查的可恢复 workflow；同时
生成两候选模型的多模型 plan/run/result、显式选择模型后的随机历史，以及
full/compact/summary 的单目录与分片生物地理随机历史，再逐项对照这里的 schema，防止代码和
契约静默漂移。
`key_value_file` 表示带 `key<TAB>value` 表头的请求文件；`sectioned_tsv` 的首段是键值 preamble，
之后每个空行分隔块由节名、表头和数据行组成。`key_value_sectioned_tsv`
表示既有 shard manifest v1 的无空行键值前言与单表分节。

BSM v2 的六个目录格式号和 `biogeo-bsm-inspection-v1` 检查结果已进入 registry。full 与
compact/summary 分别共用结构 schema；单目录和分片布局由 `metadata.tsv` 的
`manifest_file` 条件决定。
`biogeo-analysis-workflow-v1` 只定义成功 stdout；其两个子目录继续分别使用分析结果和随机历史
目录 schema，工作流根目录不冒充第三种科学 artifact。
`biogeo-model-workflow-request-v1`、plan 和 run 是多模型宿主接口；
`biogeo-model-workflow-result-v1` 是可恢复编排目录，内部标准模型结果、模型比较/平均和随机历史
仍使用各自既有科学 artifact 格式，不因顶层工作流而复制 schema。其 `request_fingerprint` 是
恢复兼容身份：排除线程、在途任务、总事件/内存/时间预算、检查点、交互和检查深度等执行控制，
但包括样本定义、输出布局、模型选择及全部科学输入。
`biogeo-engine-capabilities-v1` 声明当前 exe 的版本、平台和功能集合。其 `public_formats` 与
registry 格式号集合由真实进程测试精确比对，不能单独修改其中一侧。

v0.1 使用 `biogeo-compatibility-policy-v1` 的严格规则：未在当前 schema 声明的字段和未识别的
格式号均拒绝，不做基于字段相似度的宽松读取。格式升级、迁移和最低一个次版本的弃用窗口见发布包
`docs/compatibility-policy.md`；`engine-info` 提供对应机器摘要。
