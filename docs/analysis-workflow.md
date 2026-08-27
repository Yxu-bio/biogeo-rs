# 统一分析工作流

## 定位

`analysis-workflow` 面向命令行用户和新版 RASP，把一次单模型任务的四个既有步骤可靠串联起来：

1. 解析统一请求并执行 `analysis-plan`；
2. 用 `analysis-run` 写出可移植分析结果；
3. 从该分析结果执行 `model-bsm`，生成生物地理随机历史；
4. 用 `bsm-inspect` 验收完成的结果目录。

它不是新的似然实现，也不复制模型、优化器或随机历史采样代码。工作流 stdout 使用
`biogeo-analysis-workflow-v1`；真正的科学结果仍是两个已注册的子目录。

## 基本命令

```powershell
biogeo-cli --error-format tsv --progress-format tsv analysis-workflow `
  --request request/analysis.tsv `
  --output-dir result `
  --bsm-samples 1000 `
  --bsm-threads auto `
  --seed 20260821
```

工作流支持 `model-bsm` 的资源和持久化选项：

- `--bsm-output-level <legacy|full|compact|summary>`；
- `--bsm-threads <auto|N>` 和 `--bsm-max-in-flight <N>`；
- `--bsm-max-events-per-sample <N>`、`--bsm-max-events-total <N>`；
- `--bsm-memory-budget-mb <N>`；
- `--bsm-shard-samples <N>` 和 `--bsm-checkpoint-samples <N>`；
- `--bsm-time-limit-seconds <seconds>`、`--bsm-interactive` 和 `--seed <u64>`。

工作流默认使用 `compact`，适合新版 RASP 和普通命令行分析；这与单独调用 `model-bsm` 时为
兼容旧脚本而保留的 `legacy` 默认值不同。完成后默认执行快速检查，加入 `--deep` 才逐行检查
事件、状态占据和路径状态链。

## 目录结构

```text
result/
  analysis-result/
  bsm-result/
```

根目录只负责固定这两个阶段的位置，不是第三种科学结果容器。`analysis-result/` 是
`biogeo-analysis-result-v2`，`bsm-result/` 是八种已发布生物地理随机历史格式之一。移动、归档
或导入时应按各自 schema 读取这两个完整目录，不能只保存工作流 stdout。

用户不能在该命令中传入 `--analysis-result`、`--bsm-output-dir` 或 `--bsm-resume`；这些位置和
恢复行为由工作流管理。需要自行指定阶段目录时，应分别调用 `analysis-run`、`model-bsm` 和
`bsm-inspect`。

## 首次运行与恢复

首次运行不覆盖任何已有工作流目录。任务因取消、耗时上限、事件预算或其他错误停止后，使用
原命令并加入 `--resume`：

```powershell
biogeo-cli analysis-workflow `
  --request request/analysis.tsv `
  --output-dir result `
  --bsm-samples 1000 `
  --seed 20260821 `
  --resume
```

恢复时执行以下规则：

- 根目录必须已存在，且只能包含固定的两个子目录；
- 没有完整分析结果时重新计划并执行分析；
- 已有分析结果时直接复用，不重复拟合；
- 当前请求文件必须与分析结果中封存的请求逐字节相同，摘要不同会拒绝恢复；
- 已有生物地理随机历史目录时自动进入其检查点恢复路径，writer 继续验证模型、seed、输出级别、
  字典和其他运行身份；
- 只有请求样本数全部完成且检查器返回 `valid`，工作流才输出 `status=complete`。

请求的逐字节身份检查是审计边界，不是模型名称比较。即使只修改空白，也应把它视为新请求并
使用新的输出目录。分析结果完成后，原树、范围、参数和时期文件可以被移走；恢复 BSM 使用
`analysis-result/` 内的可移植输入包，但当前请求文件本身仍须存在并保持相同字节。

## stdout 契约

成功输出包含请求摘要、两个结果目录、分析是否复用、lnL、状态/区域/tip 数、随机历史格式、
请求和完成样本数、事件数、是否恢复、检查级别、检查规模以及三个阶段和总耗时。字段定义位于
`schemas/biogeo-analysis-workflow-v1.schema.tsv`。

失败时不伪造完成摘要。配合 `--error-format tsv`，普通工作流结构和请求身份错误使用稳定的
`code=analysis_workflow_error`；取消、耗时上限和事件预算保留生物地理随机历史执行层既有退出码。
已经提交的分析结果和随机历史检查点不会因失败被删除。

## 新版 RASP 的选择

新版 RASP 的单模型“一次拟合并生成随机历史”任务可直接调用 `analysis-workflow`，并把固定的
两个子目录纳入项目。若界面需要让用户在拟合后查看参数、修改随机历史资源配置或稍后再采样，
则继续使用分阶段的 `analysis-plan`、`analysis-run`、`model-bsm` 和 `bsm-inspect`。两种入口
复用完全相同的执行器和结果 schema，不应维护两套结果解析代码。

## Ponerinae 真实验收

`validation/checks/check-ponerinae-analysis-workflow.ps1` 使用 1534-tip、7 区域、7 时期的 Ponerinae
输入做发布级验收。脚本生成 portable 优化请求，以任务总事件预算确定性停止在非空样本前缀，
深度检查未完成目录；随后临时移走请求侧树、范围、参数和时期目录，只依赖
`biogeo-analysis-result-v2` 恢复全部样本。最后用同一分析结果、seed 和资源配置执行一次性
基线，并对两个 BSM 目录逐文件比较长度和 SHA-256。

2026-08-21 当前 Windows release 的 10 条 compact 分片验收中，2500 事件预算提交 2 条历史和
2047 个沿枝事件后以退出码 3 停止；恢复后完成 10 条和 10352 个沿枝事件，深度检查为 0 违规。
恢复目录与一次性基线的 35 个文件逐字节一致。该脚本只在 validation 层锁定真实数据行为，
核心执行器不读取案例名称或这些期望值。
