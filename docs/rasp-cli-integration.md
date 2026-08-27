# 新版 RASP 与 Rust CLI 的集成边界

## 定位

本项目交付的是高性能、可独立运行的命令行计算引擎。新版 RASP 将它作为子进程调用并消费
版本化结果；Rust 核心不实现 GUI、绘图或 HTML 报告。本契约不参考旧版 RASP，也不假定
新版 RASP 使用何种语言、GUI 框架、项目文件或内部目录结构。

本文的 **v0.1 接入边界已于 2026-08-24 冻结**。冻结的是公开进程行为和已注册机器格式；以后
若要破坏这些约定，必须发布新的格式号或明确的新兼容版本，不能在同一格式号下静默改变字段。
新版 RASP 的界面、项目数据库和内部任务实现不属于本契约。

## 职责划分

Rust CLI 负责：

- 科学输入的严格解析与校验；
- 状态空间、Q、节点分裂情景、pruning、后验和参数优化；
- DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE、BAYAREALIKE+J 及通用参数表；
- 模型比较、嵌套关系/似然比检验以及祖先范围和分裂情景模型平均的版本化数值结果；
- 年龄区间与类群约束下的随机化石树生成结果；
- 生物地理随机历史采样、确定性并行、检查点、恢复、分片与资源限制；
- 版本化、可校验、可重放的机器结果和稳定进程状态。

新版 RASP 负责：

- GUI、项目管理、交互式参数编辑和任务队列展示；
- 树、祖先范围、分裂、时期和生物地理随机历史的可视化；
- 图形导出、报告排版及用户工作流；
- 把自身数据转换为已公开的 CLI 输入，并把版本化结果转换为界面对象。

因此，图形和报告不再计作 Rust CLI 的 BioGeoBEARS 功能缺口。会影响似然、后验、模型比较
或随机历史分布的数值能力仍属于 Rust 引擎。

## 子进程契约

新版 RASP 应直接启动发布版 `biogeo-cli`，初期不依赖 DLL/FFI。建议传入绝对路径，并把
标准输出、标准错误和退出码分别捕获：

启动任何分析前先调用 `engine-info`，按 `biogeo-engine-capabilities-v1` 读取版本、preset、格式
和能力标志，并核对发布目录 `schemas/registry.tsv`。不要解析 `--help` 或只根据版本号推测功能。
握手步骤和字段语义见 [`engine-capabilities.md`](engine-capabilities.md)。
宿主还必须识别 `compatibility_policy_version`，按 `unknown_format_policy` 和
`unknown_field_policy` 严格拒绝未知内容；不要通过忽略字段实现模糊向前兼容。完整规则见
[`compatibility-policy.md`](compatibility-policy.md)。

- 退出码 `0`：命令成功；
- 退出码 `2`：参数、输入、模型配置、优化、I/O 或其他普通失败；
- 退出码 `3`：生物地理随机历史任务达到总事件预算；
- 退出码 `124`：生物地理随机历史任务达到耗时上限；
- 退出码 `130`：生物地理随机历史任务被协作式取消。

机器调用时把全局选项放在子命令之前：

```powershell
biogeo-cli --error-format tsv --progress-format tsv analysis-run `
  --request <analysis.tsv> `
  --output-dir <result>
```

拟合后立即生成生物地理随机历史的单任务也可使用：

```powershell
biogeo-cli --error-format tsv --progress-format tsv analysis-workflow `
  --request <analysis.tsv> `
  --output-dir <workflow-result> `
  --bsm-samples <N> `
  --bsm-threads auto
```

新版 RASP 的首选入口是 `biogeo-analysis-request-v1`。RASP 先调用 `analysis-plan` 校验完整
任务并读取状态空间、自由参数顺序和资源风险，再调用 `analysis-run`。旧的
`model-evaluate/model-optimize` 仍是受支持的底层与兼容入口，但 RASP 不应自行重复拼接其
全部选项。当前 `analysis-run` stdout 为 `biogeo-analysis-run-v2`；RASP 可直接保存其中的
Windows 峰值 working set、CPU 时间和实际 worker 配置作为任务诊断。请求格式、路径规则和
遥测作用域见 [`analysis-request.md`](analysis-request.md)。

`analysis-workflow` 是同一请求的便捷编排入口，固定写入 `analysis-result/` 和 `bsm-result/`，
默认使用 compact 并在成功前调用 `bsm-inspect`。中断后由 RASP 以相同请求、随机历史配置和
`--resume` 恢复。工作流根目录本身不是新的科学 artifact；RASP 仍按两个子目录各自的 schema
读取。需要在拟合后修改采样设置、延迟生成或单独展示每阶段状态时，继续使用分阶段命令。完整
规则见 [`analysis-workflow.md`](analysis-workflow.md)。

用户已有的 BioGeoBEARS/LAGRANGE geography `.data` 可直接作为 `--ranges` 或请求中的
范围路径，不必先改写扩展名或生成临时 TSV。用户需要检查转换结果时可调用 `convert-ranges`。
BioGeoBEARS 的 `time_boundaries + dispersal block matrices + adjacency block matrices` 可由
`convert-biogeobears-strata` 生成内部相对路径的 `strata.tsv` 和逐时期矩阵；RASP 应把生成目录
作为一个整体保存。转换和约束语义见 [`legacy-input-import.md`](legacy-input-import.md)。

失败时 `stderr` 是 `biogeo-cli-error-v1` 键值 TSV，例如：

```text
format	biogeo-cli-error-v1
code	invalid_arguments
message	missing required option --ranges
exit_code	2
```

`message` 中的 `%`、制表符、回车和换行分别按 `%25/%09/%0D/%0A` 编码。RASP 应按
`format + code + exit_code` 分支，`message` 只用于向用户显示，不应解析英文句子判断错误。
不指定该选项时维持原有人工可读错误和帮助文本。

`--progress-format tsv` 为通用模型优化和两级 batch 在 `stderr` 输出逐行
`biogeo-cli-progress-v1` 事件。它与错误块可按首字段区分，字段和取消语义见
[`progress-and-cancellation.md`](progress-and-cancellation.md)。
新版 RASP 的完整宿主状态、退出映射、恢复判定和导入顺序见
[`rasp-host-state-machine.md`](rasp-host-state-machine.md)。

上述公开边界的机器定义位于发布包 `schemas/registry.tsv`。新版 RASP 应按 artifact 的
`format` 选择对应 schema，遇到未知格式号时明确拒绝，不按字段相似度猜测兼容。Rust 的
进程级测试会用真实优化、统一工作流、检查、v1→v2 迁移、错误、进度输出以及六种 BSM v2
目录布局验证这些契约。

成功时 `stdout` 保留当前版本化 TSV 摘要。对于拟合、批量和大规模生物地理随机历史，结果
目录才是权威数据源：

- `biogeo-analysis-result-v2`：单模型拟合点、参数、自包含输入和重放信息；
- `biogeo-input-bundle-v1`：树、观测、修饰和时期二级依赖的包内相对路径清单；
- model-batch 目录：同一数据集内的逐模型分析结果、信息准则比较、版本化模型平均祖先范围
  和逐次失败汇总；
- dataset-batch 目录：多个独立数据集/树的分层任务状态和各自 model-batch 结果；
- `biogeo-bsm-tsv-v1`：既有随机历史八表兼容格式；
- `biogeo-bsm-full/compact/summary[-sharded]-tsv-v2`：可检查点恢复、可分片的新版随机历史
  格式。新版 RASP 默认应使用 compact；只做分布统计时使用 summary。详见
  [`bsm-output-formats.md`](bsm-output-formats.md)。

## 推荐调用顺序

1. 用 `analysis-template` 或 RASP 自身生成请求与参数表。
2. 用 `analysis-plan` 在启动昂贵任务前完整校验树、观测、参数、修饰和状态空间，并展示风险。
3. 用户确认一次完成拟合和随机历史时调用 `analysis-workflow`；需要阶段级控制时调用
   `analysis-run`，再由分析结果调用 `model-bsm`。
4. RASP 读取结果目录中的数值表进行展示和可视化，不抓取人类可读日志；批量任务继续使用
   `model-batch` 或 `dataset-batch`。
5. 分阶段生成随机历史时，大任务启用流式目录和检查点；完成后先用
   `bsm-inspect` 快速验收，归档或正式导入时使用 `--deep`。
6. 导入外部分析结果时先调用 `analysis-result-inspect`；旧 v1 在原输入仍完整时用
   `analysis-result-migrate` 生成新 v2 目录。

## v0.1 冻结接口

| 边界 | v0.1 约定 |
|---|---|
| 引擎发现 | 调用安装目录中的 `biogeo-cli` 绝对路径，先运行 `engine-info` 并核对同目录 schema registry |
| 任务输入 | 单模型首选 `biogeo-analysis-request-v1`；多模型使用 `biogeo-model-workflow-request-v1` |
| 运行反馈 | stdout 只消费已注册成功摘要；stderr 分流 `biogeo-cli-progress-v1`、`biogeo-cli-error-v1` 和普通诊断 |
| 终态判断 | 联合退出码、机器错误和结果目录判断，不能只看其中一项 |
| 取消与恢复 | 使用协作式取消；仅在已有可验证检查点或工作流目录时提供恢复 |
| 结果导入 | 按 artifact 自身格式号和 schema 读取；未知格式或字段明确拒绝 |
| 科学计算 | 新版 RASP 不自行复算 lnL、后验、模型权重或随机历史，只展示 Rust 结果 |

该表不冻结按钮、页面、数据库表或任务队列类名。RASP 只要满足这些进程边界，就可以自由调整
内部实现。

## 项目目录与迁移

- 引擎安装目录和 RASP 项目目录相互独立；项目中记录所用引擎版本，不把结果内部路径改写成
  安装目录路径。
- 新分析保存完整 `biogeo-analysis-result-v2` 或工作流目录。移动项目时整体移动结果目录，
  RASP 只更新自己保存的项目相对路径，不改写结果内部文件。
- v2 分析结果完成后可以删除原树、范围、参数及时期输入；`analysis-result-inspect --replay` 和
  `model-bsm` 从结果内的输入包重放。旧 v1 必须在源输入仍存在时先迁移。
- `analysis-workflow --resume` 仍要求提交时的请求文件存在且字节不变；如果只保留了完成的 v2
  分析结果，应直接调用 `model-bsm` 继续或重新生成生物地理随机历史。
- 源输入可以是只读文件，但结果父目录必须可写。引擎不会为了分析而改写用户输入。

上述规则由 Windows 参考宿主组合测试验证：在中文和空格目录读取只读输入，完成后删除源目录，
把工作流移入另一个项目目录，再从移动后的分析结果重算 lnL 并新生成生物地理随机历史。源码
测试和安装版 EXE 使用同一个测试宿主。

## Windows 引擎分发

当前 64 位 Windows PC 发布包使用 `biogeo-windows-package-v3`，包含 exe、SHA-256 payload
清单、locked 构建信息、发布/许可证状态、第三方许可证、schema 和接入文档。ZIP 条目使用标准
`/` 路径并且只有一个顶层包目录。新版 RASP 可以把完整目录放入自身资源目录并记录 exe 绝对路径，
也可以调用包内 `install.ps1` 安装到一个新的版本目录。安装器不修改 `PATH`，拒绝覆盖现有
目录，并在发布前校验全部 payload 和实际启动 exe。新版 RASP 可以直接捆绑 GitHub 发布的未签名
科研包，但应保留包版本和 SHA-256 以便复现与排查。只有项目未来主动采用代码签名时，RASP 才需要
校验发布者指纹。详见
[`windows-release.md`](windows-release.md)。

软件发布 ZIP 只分发计算引擎，不是分析结果容器。`biogeo-analysis-result-v2` 仍应作为完整目录
移动、归档或纳入 RASP 项目；RASP 自行压缩项目时不得修改目录内部的相对路径和文件字节。

## 冻结边界之外的后续工作

- v2 分析结果可整目录复制和重定名；RASP 不应缓存 `input-bundle/` 内部文件的绝对路径。
- 工作流恢复要求科学身份和输出布局不变；线程、时间/内存/总事件预算等执行控制仅可按对应
  工作流文档的白名单调整，其他修改必须使用新的输出目录。
- v1 只读兼容仍需原机器输入；迁移是新目录发布，不会覆盖原结果。
- 通用优化和两级 batch 已输出版本化实时进度并响应同一 `Ctrl+C` 取消令牌；取消后的 batch
  v2 attempt 区分 `complete/failed/cancelled/not_started`。单次似然评估仍需执行到安全点。
- `dataset-batch` 已支持多数据集/多树分层调度，但当前顺序执行，尚无跨进程并发调度。
- Windows PC 发布、指定目录安装和 schema 契约门禁已完成。Linux 包、Slurm/cgroup 资源
  探测和多进程协调属于后续服务器阶段，不阻塞当前新版 RASP 的 Windows 进程级集成。

模型平均数值已升级为 `biogeo-model-averaged-ancestral-ranges-v2`；RASP 应连接其中 `nodes`、
`split_nodes`、`areas`、`states`、`split_scenarios` 和两张概率表，不解析人类日志。
`biogeo-model-comparison-v3` 提供全部有向模型对的嵌套关系和可用 LRT，边界警告必须随 p 值展示，
不能只取一个数字。随机化石工作流读取 `biogeo-fossil-placement-set-v1`，由用户选定或批量提交
具体生成树后再拟合。实时任务状态按 `biogeo-cli-progress-v1` 消费，不参考旧版 RASP 的任务管理
行为。
