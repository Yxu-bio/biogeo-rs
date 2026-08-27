# 机器进度与协作式取消

## 适用范围

新版 RASP 面向的 `model-optimize`、`model-batch` 和 `dataset-batch` 已接入同一个
`ExecutionCancellationToken`。生物地理随机历史继续复用该令牌以及既有检查点、恢复、暂停和
耗时上限。旧的专用验证命令（如 `dec-optimize`）不是新版 RASP 的标准调用入口，当前不输出
本协议的优化迭代事件。

## 启用机器进度

全局选项必须放在子命令之前；`--error-format` 和 `--progress-format` 的先后顺序不限：

```powershell
biogeo-cli --error-format tsv --progress-format tsv model-optimize `
  --tree <tree.nwk> `
  --ranges <ranges.tsv> `
  --parameters <parameters.tsv>
```

默认值为 `--progress-format none`，不会改变原有 `stdout`。启用后，每个事件立即写入并刷新
`stderr`，一行就是一个完整记录；没有多行消息，也不要求等任务结束后再解析。

格式号为 `biogeo-cli-progress-v1`，固定 14 列：

```text
format  sequence  event  command  dataset_id  model_id  completed  total  start  starts  iteration  max_iterations  evaluations  best_log_likelihood
```

实际分隔符为制表符。缺失值是空字段；字符串中的 `%`、制表符、回车和换行分别编码为
`%25/%09/%0D/%0A`。`sequence` 从 1 严格递增；`start` 是从 1 开始的多起点序号；
`evaluations` 是当前模型跨全部起点的累计似然评估数。`best_log_likelihood` 只是截至该事件的
当前最好值，不是最终收敛保证。

当前事件类型包括：

- `task_started`、`task_completed`、`task_cancelled`；
- `unit_started`、`unit_completed`、`unit_failed`，用于模型或数据集批处理；
- `optimization_start`、`optimization_iteration`、`optimization_start_complete`。

批处理中，优化事件会同时携带 `dataset_id` 和/或 `model_id`，RASP 不需要从人类日志推断
任务层级。迭代次数不能可靠换算成剩余时间，因此协议报告实际计数，不制造虚假的百分比。

若同时启用 `--error-format tsv`，正常进度行以 `biogeo-cli-progress-v1` 开头；最终错误块以
`format<TAB>biogeo-cli-error-v1` 开头。RASP 应按此前缀分流同一个 `stderr`，不要解析英文文本。

BSM 当前不把每条样本写成 stderr 进度事件。宿主应轮询结果目录 `metadata.tsv`，使用其中
`completed_samples` 和 `samples` 展示已经提交 checkpoint 的进度；worker 内尚未提交的样本
不能计为完成。启用 stdin 交互控制时的其他 stderr 行是诊断文本，只记录或显示，不参与机器
状态判断。

## 取消语义

`Ctrl+C` 设置共享取消令牌。参数优化在每次完整似然评估之间检查令牌，因此正在执行的单次
矩阵传播会先安全返回，不会在数值对象内部强行中断。取消不会发布半成品
`biogeo-analysis-result-v2`，退出码为 `130`，机器错误码为 `task_cancelled`。

`model-batch` 和 `dataset-batch` 收到取消后不会启动后续任务。它们会写不可变的 v2 attempt：

- 已完成任务记为 `complete`；
- 正在运行时收到取消的任务记为 `cancelled`；
- 尚未启动的任务记为 `not_started`；
- 取消前发生的普通错误仍记为 `failed`。

根 `complete.tsv` 不会发布。之后使用 `--resume` 时，已经完成并通过身份校验的结果继续复用，
其余任务重新执行。取消不属于模型拟合失败，也不会触发“首错后继续”。

## 当前边界

- 参数优化尚不支持暂停，只支持协作式取消；生物地理随机历史另有交互暂停。
- 一个正在运行的似然评估暂时没有树内更细粒度的取消安全点。
- 两级 batch 仍是顺序调度；本协议不等同于跨进程并发执行器。
- 机器进度是瞬时事件，权威结果仍是版本化结果目录和不可变 attempt。
