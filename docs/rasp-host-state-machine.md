# 新版 RASP 子进程宿主状态机

本文定义新版 RASP 调用 `biogeo-cli` 时的宿主侧状态和判断顺序。它是语言无关的接入契约，
不引用旧版 RASP，也不要求新版 RASP 使用 Rust。

本状态机作为 v0.1 接入边界于 2026-08-24 冻结。状态名称可以映射为新版 RASP 自己的类型，
但退出码、机器错误、恢复条件和导入顺序不能被宿主侧猜测或改写。

## 状态

| 状态 | 含义 | 可否恢复 |
|---|---|---:|
| `queued` | 请求已保存，尚未启动引擎 | 是 |
| `checking_engine` | 运行 `engine-info` 并核对 schema registry | 否 |
| `planning` | 运行 `analysis-plan` 或 `model-workflow-plan` | 否 |
| `running` | 子进程正在拟合、比较或采样 | 否 |
| `cancel_requested` | 已发送协作式取消，等待安全点 | 否 |
| `rejected` | 参数、输入或配置在执行前被拒绝 | 否 |
| `cancelled` | 引擎确认取消，退出码 130 | 取决于工作目录 |
| `budget_stopped` | 达到总事件或时间预算，退出码 3/124 | 取决于工作目录 |
| `failed` | 已识别的计算、I/O 或 artifact 失败 | 通常否 |
| `completed` | 进程成功并发布完整结果，尚未导入 | 否 |
| `importing` | 宿主正在按 schema 验证和读取结果 | 否 |
| `ready` | 结果已验证，可供项目和界面使用 | 不适用 |
| `protocol_error` | 未知格式、字段、错误码、流损坏或退出码矛盾 | 否 |

`cancelled` 和 `budget_stopped` 只有在预期输出目录存在且相应工作流的身份文件、attempt 或 BSM
checkpoint 可验证时才显示“可恢复”。仅凭退出码不能承诺恢复。

## 标准流程

```text
queued
  -> checking_engine
  -> planning
  -> running
  -> completed
  -> importing
  -> ready
```

任何阶段遇到未知格式、schema 不匹配或机器记录自相矛盾都进入 `protocol_error`。`planning`
只接受 `status=valid`；风险警告由界面展示，但不能擅自改写请求。

启动分析前，宿主必须：

1. 运行 `engine-info`；
2. 要求 `format=biogeo-engine-capabilities-v1`、`status=ready`；
3. 要求严格兼容政策和未知内容拒绝策略；
4. 将 `public_formats` 与同一安装目录的 `schemas/registry.tsv` 精确比较；
5. 确认本次调用所需命令和 artifact 格式均已宣告。

## stderr 分流

机器调用同时启用：

```text
--error-format tsv --progress-format tsv
```

宿主按记录前缀分流：

- `biogeo-cli-progress-v1<TAB>...`：单行 14 列进度事件，`sequence` 必须从 1 连续递增；
- `format<TAB>biogeo-cli-error-v1`：四行最终机器错误块；
- 其他行：仅作为诊断日志保存，不参与状态判断。

启用 BSM stdin 交互控制时会出现人类可读诊断行。宿主可以显示或记录它们，但不能从
`BSM status: ...` 文本推断取消是否成功；最终状态仍由机器错误、退出码和结果目录共同决定。

优化与 batch 的实时进度来自 `biogeo-cli-progress-v1`。当前 BSM 已提交进度从结果目录
`metadata.tsv` 的 `status`、`completed_samples` 和 `samples` 读取；只显示已提交 checkpoint，
不把 worker 内尚未写入的样本计为完成。

## 退出与错误映射

| 退出码/机器错误 | 宿主终态 |
|---|---|
| `0` 且成功 stdout 与完整 artifact 均通过 | `completed` |
| `invalid_arguments`、`invalid_input`、`configuration_error` | `rejected` |
| `bsm_cancelled`、`task_cancelled`，退出码 130 | `cancelled` |
| `bsm_event_limit`，退出码 3 | `budget_stopped` |
| `bsm_time_limit`，退出码 124 | `budget_stopped` |
| 其他已注册 code，退出码 2 | `failed` |
| 非零退出但没有机器错误，或两处退出码不同 | `protocol_error` |
| 未注册 code、格式或字段 | `protocol_error` |

`message` 只供用户阅读。宿主不得解析英文消息来区分错误。

## 取消

- 优化和 batch：发送操作系统 `Ctrl+C`/中断信号，然后进入 `cancel_requested`；
- BSM：可发送相同信号，或在启用交互控制时向 stdin 写入 `cancel\n`；
- 收到取消后继续读取 stdout/stderr，等待引擎在数值安全点以 130 退出；
- 只有超过宿主设置的宽限期才强制终止。强制终止不是“已取消”，应重新检查工作目录并显示
  未确认中断状态。

宿主不能在发送取消后立即删除输出目录，也不能把尚未返回的子进程标为 `cancelled`。

## 恢复

恢复使用同一输出目录并追加 `--resume`。科学输入、样本定义、seed、模型选择和输出布局必须
保持身份一致。`model-workflow` 允许调整其文档列出的线程、在途任务、总事件/内存/时间预算、
检查点、交互和检查深度；这些变化不改变随机样本序列。

恢复前先检查：

- 顶层目录格式和身份文件可读；
- 已完成子结果仍通过其 schema；
- 未发布顶层 `complete.tsv` 时不能导入为成功结果；
- 修改了不可恢复身份时创建新输出目录，不覆盖旧目录。

## 导入

退出码 0 只是进入 `completed` 的必要条件。导入多模型工作流还必须验证：

1. `metadata.tsv` 为 `biogeo-model-workflow-result-v1`；
2. `complete.tsv` 状态为 `complete`；
3. `metadata.tsv`、`selection.tsv` 和 `complete.tsv` 的请求指纹一致；
4. `selected_analysis_result` 是工作流根目录下的安全相对路径；
5. 选中结果为完整 `biogeo-analysis-result-v2`；
6. BSM metadata 为 `complete`，请求与完成样本数相同；
7. 正式导入前运行 `bsm-inspect --deep` 并要求 `status=valid`、`run_status=complete`。

全部通过后状态才从 `importing` 进入 `ready`。

## 可执行证据

参考宿主位于 `crates/biogeo-cli/tests/support/rasp_host.rs`，只启动 CLI 子进程并读取公开机器接口，
不链接科学核心。`rasp_host_contract` 使用中文和空格路径覆盖：

- 能力协商及 schema registry 精确匹配；
- 普通机器错误分类；
- plan、提交、实时优化进度、完成和深度导入；
- 时间预算停止后提高预算恢复；
- stdin 取消 4096 条 BSM 任务后改变线程数恢复；
- 已完成模型结果不重复拟合；
- 只读中文源输入完成后删除源目录、跨项目移动结果、重放 lnL 并新生成生物地理随机历史。

Windows 发布门禁会把同一参考宿主指向安装目录中的 release `biogeo-cli.exe` 再执行一次。
