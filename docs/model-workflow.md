# 多模型统一工作流

`model-workflow` 把已经稳定的 `model-batch`、模型比较、祖先结果模型平均、`model-bsm` 和
`bsm-inspect` 编排为一个可恢复任务。它不实现第二套似然、优化或随机历史算法。

## 请求文件

请求格式是 `biogeo-model-workflow-request-v1`。一个顶层文件引用：

- `models`：`biogeo-model-batch-manifest-v1` 候选模型清单；
- `config`：`biogeo-model-batch-config-v1` 共享树、分布数据、修饰输入和优化设置；
- `comparison_criterion`：主比较准则 `aic` 或 `aicc`；
- `bsm_selection`：是否生成生物地理随机历史，以及从哪个拟合模型采样。

示例：

```tsv
key	value
format	biogeo-model-workflow-request-v1
models	models.tsv
config	model-config.tsv
comparison_criterion	aic
bsm_selection	model_id
bsm_model_id	DEC
bsm_samples	1000
bsm_output_level	compact
bsm_threads	auto
bsm_shard_samples	100
bsm_deep_inspection	true
bsm_seed	20260822
```

所有相对路径都相对于各自所在文件解析：`workflow.tsv` 解析 `models`/`config`，manifest 解析
参数表，model config 解析树、范围矩阵和修饰文件。顶层请求使用绝对路径或 `..` 时仍可运行，
但 plan 会明确报告 `request_paths_portable=false`。

## 模型选择

`bsm_selection` 只有三种合法值：

- `none`：只拟合、比较和模型平均，不生成生物地理随机历史；此时不得保留其他 `bsm_*` 键。
- `model_id`：要求 `bsm_model_id` 与 manifest 中的 ID 大小写完全一致，并要求该模型至少有一个
  优化起点收敛。
- `best_by_criterion`：按 `comparison_criterion` 选择唯一 rank 1 的合格模型。没有可用分数或
  并列第一时拒绝采样，要求用户改用明确 `model_id`。

`best_by_criterion` 是明确的执行策略，不表示 AIC/AICc 第一名就是科学真相。模型平均结果仍然
使用完整候选集的 AIC/AICc 权重；生物地理随机历史必须来自一个具有完整 Q、节点分裂概率和拟合
参数的单模型结果，不能把模型平均概率误当成一套可直接采样的生成模型。

## 预检与运行

```powershell
biogeo-cli model-workflow-plan --request workflow.tsv

biogeo-cli --progress-format tsv model-workflow `
  --request workflow.tsv `
  --output-dir result
```

`model-workflow-plan` 不优化参数，但会真实解析树、末端观测、时期/距离/面积等修饰文件和每张参数
表，构建候选模型的 Q 与 cladogenetic split scenario，并报告状态数、节点数、自由参数维度、
随机历史 worker 解析结果和资源风险。

完成目录结构：

```text
result/
  metadata.tsv
  source-request.tsv
  source-models.tsv
  source-model-config.tsv
  model-batch/
    models/<model_id>/
    comparison.tsv
    model-averaged-ancestral-ranges.tsv
  selection.tsv
  bsm-result/                 # 仅在启用 BSM 时存在
  complete.tsv
```

`metadata.tsv`、`selection.tsv` 和 `complete.tsv` 都是机器记录。新版 RASP 应读取已注册的
`biogeo-model-workflow-run-v1` stdout 和 `biogeo-model-workflow-result-v1` 目录 schema，不应
解析人类帮助文本或根据目录名猜状态。

## 恢复与身份

中断后使用相同的科学请求和输出目录：

```powershell
biogeo-cli model-workflow --request workflow.tsv --output-dir result --resume
```

工作流保存首次请求、候选 manifest 和共享 config 的原始字节。候选 manifest、共享 config 以及
请求中的科学配置和输出布局必须保持不变；模型参数表和每个标准分析结果继续由 `model-batch` 的
jobs/输入指纹验证；随机历史继续由 BSM checkpoint 和运行指纹验证。已提交文件不覆盖，缺失模型
或未完成随机历史从其既有恢复边界继续。

恢复时可以修改不影响随机样本内容和输出布局的执行控制：`bsm_threads`、`bsm_max_in_flight`、
`bsm_max_events_total`、`bsm_memory_budget_mb`、`bsm_checkpoint_samples`、
`bsm_time_limit_seconds`、`bsm_interactive` 和 `bsm_deep_inspection`。例如首次因 10 分钟预算停止后，
可提高 `bsm_time_limit_seconds` 再执行 `--resume`。`source-request.tsv` 保留首次请求用于审计，实际
恢复预算记录在随机历史的 `metadata.tsv`；顶层 `request_fingerprint` 表示排除上述执行控制后的
恢复兼容身份。

`bsm_samples`、`bsm_seed`、`bsm_output_level`、`bsm_shard_samples`、
`bsm_max_events_per_sample`、模型选择、比较准则和输入配置仍属于不可变身份，修改后恢复会拒绝。

首次运行不会覆盖已有目录。AIC/AICc 并列、显式模型未收敛、请求身份变化、工作区出现未知文件或
随机历史深度检查失败时，不会发布顶层 `complete.tsv`。

## 可执行示例与真实数据验收

发布包的 `examples/model_workflow/` 提供两模型完整任务，`examples/recovery/` 提供可重复的
时间预算停止与恢复任务。六模型真实数据门禁使用两项独立数据：

- BioGeoBEARS 官方 Psychotria M4，19 tips、4 areas、16 states；
- 从完整 Dore Ponerinae 数据按冻结规则派生的 32-tip、7-area、120-state 子集，来源哈希与
  选择规则保存在 fixture 的 `provenance.json`。

两项任务均先完成六模型拟合后以 0 秒 BSM 预算受控停止，再在同一目录恢复 4 条生物地理随机
历史。门禁逐模型执行 `analysis-result-inspect --replay`，严格检查工作流 plan、run 和结果目录
schema，并对最终 BSM 执行深度检查：

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-public-cli-examples.ps1
powershell -ExecutionPolicy Bypass -File validation/check-model-workflow-real-data.ps1
```

## 当前边界

- 候选模型共享同一数据、状态空间和优化器设置，模型差异由各自参数表表达。
- BSM 目标只能是一个合格拟合模型；跨模型祖先范围和分裂情景平均仍保存在模型平均结果中。
- v0.1 先完成 Windows PC 单进程工作流；Linux/Slurm 资源发现和多进程协调属于后续阶段。
- CLI 负责计算与机器结果，新版 RASP 负责项目管理、可视化和报告，不参考旧版 RASP 接口。
