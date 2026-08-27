# 统一分析请求

## 目的

`biogeo-analysis-request-v1` 把一次单模型固定评估或参数优化所需的输入和选项写入一个严格、
版本化的 TSV 文件。命令行用户和新版 RASP 使用同一份请求，不再分别维护长串命令行参数。

请求只是执行说明，不包含输出目录。同一请求可以写入不同的新结果目录；`analysis-run`
仍遵守非覆盖语义，并生成现有 `biogeo-analysis-result-v2`，没有另造似然或结果实现。

## 基本格式

```text
key	value
format	biogeo-analysis-request-v1
mode	optimize
tree	tree.nwk
observation	exact_ranges
ranges	ranges.tsv
parameters	parameters.tsv
max_range_size	auto
max_states	1000000
include_null_range	false
root_prior	flat
min_branch_length	0
ancestral_probabilities	true
split_probabilities	true
optimization_initial_step	0.2
optimization_tolerance	1e-8
optimization_max_iterations	200
```

相对路径以请求文件所在目录为基准。绝对路径和包含 `..` 的相对路径可以运行，但
`analysis-plan` 会把请求标记为 `portable=false`。空字段不能代替省略可选字段，未知字段、
重复字段和不适用于当前模式的字段都会被拒绝。

`max_states` 是可选的执行资源上限，不是模型参数。引擎先按
`sum(C(areas, k), k=1..max_range_size) + null` 计算状态数，不分配状态对象；估计值超过该
正整数时返回 `code=resource_limit`。省略表示不设置人为上限，不会把当前 PC 的能力写死为
软件上限。调整该值不改变似然公式或模型身份。

`observation` 支持：

- `exact_ranges`：必须提供 `ranges`。
- `ambiguous_ranges`：必须提供 `ranges`，并按 BioGeoBEARS `0/1/?` 语义解析。
- `mf_dp_fdp_detection`：必须提供 `detections` 和 `controls`，不能同时提供 `ranges`。

可选修饰输入包括 `dispersal_multipliers`、`dispersal_strata`、`distance_matrix`、
`environment_distance_matrix`、`extirpation_multipliers` 和 `area_sizes`。指数值、事件权重
和固定/自由/联动关系仍由 `parameters` 指向的 23 行参数表决定。

多个优化起点写在一个字段中，向量内部用逗号，向量之间用分号：

```text
optimization_additional_starts	0.01,0.01;0.1,0.02
```

向量顺序是参数表的自由参数顺序，`analysis-plan` 会明确报告该顺序。

## 四个入口

生成 DEC 优化请求和参数表骨架：

```powershell
biogeo-cli analysis-template --preset dec --mode optimize --output-dir request
```

骨架目录包含 `analysis.tsv` 和 `parameters.tsv`。把树和范围文件放入该目录，或修改请求中的
路径后，再执行计划检查。`ready_to_plan=false` 是有意的：模板不会伪造科学输入。

运行前检查：

```powershell
biogeo-cli analysis-plan --request request/analysis.tsv
```

计划阶段会完整解析树、观测、参数表、修饰、状态空间和初始模型结构，但不会执行完整
pruning 或优化。输出包括：

- 树、区域、状态、时期、逐时期实际允许状态数、Q 转移和分裂情景规模；
- `state_count_estimate` 和 `state_space_limit`，前者在状态分配前即可计算；
- 自由参数顺序；
- 保留数值载荷的明确参考量；
- 分裂 posterior 行数上界；
- 当前进程可见并行度和 `low/moderate/high` 风险级别。

计划阶段还会用初始模型构造 tip likelihood，并在 pruning 前检查每个 tip 的采样时期状态约束。
普通范围模型及 detection evaluate 会报告具体 taxon 与时期；detection optimize 的末端支持依赖
待优化的 `mf/dp/fdp`，因此保留为每次目标函数求值时检查，避免把初值的零支持误报为结构冲突。
`stratum_allowed_state_counts` 按从年轻到年老的时期顺序输出逗号分隔计数；无分层模型输出单个
master state count。

`dense_q_reference_bytes` 只是密集矩阵对照尺度，Rust 引擎不会因此构造密集 Q。
`combined_numeric_payload_bytes_reference` 也不是进程 RSS 估计；输出固定声明
`process_rss_estimate_available=false`，因为传播临时缓存、分配器和系统库开销尚未进入统一统计。

执行请求：

```powershell
biogeo-cli --error-format tsv --progress-format tsv analysis-run `
  --request request/analysis.tsv `
  --output-dir result
```

优化进度继续使用既有 `biogeo-cli-progress-v1` 和底层命令名 `model-optimize`。成功 stdout
是 `biogeo-analysis-run-v2`，提供 lnL、端到端命令耗时、结果字节数、收敛状态和结果目录；
科学数据的权威来源仍是结果目录。原始请求以 `analysis_request` 角色进入结果输入包，便于审计。

需要在一次命令中完成拟合、生物地理随机历史生成和结果检查时：

```powershell
biogeo-cli --error-format tsv --progress-format tsv analysis-workflow `
  --request request/analysis.tsv `
  --output-dir result `
  --bsm-samples 1000 `
  --bsm-threads auto `
  --seed 20260821
```

该入口复用 `analysis-plan`、`analysis-run`、`model-bsm` 和 `bsm-inspect`，默认写出 compact
随机历史，并提供严格的非覆盖和 `--resume` 语义。它没有独立的似然或结果实现；详细目录、
请求身份和失败恢复规则见 [`analysis-workflow.md`](analysis-workflow.md)。

Windows 版 v2 还通过系统进程 API 报告：

- `process_peak_working_set_bytes`：当前 CLI 进程从启动到查询时的真实 working-set 高水位，
  包含启动、输入解析、拟合和结果写出，不是分析阶段增量；
- `process_cpu_user_seconds/process_cpu_kernel_seconds`：`analysis-run` 内部作用域的 CPU 时间差；
- `average_logical_cores_used`：总 CPU 时间除以 wall time，可理解为平均占用的逻辑核数；
- `analysis_worker_threads=1`：当前单模型 likelihood/优化执行器的实际 worker 配置；
- `available_parallelism`：操作系统向当前进程报告的可用并行度，不等于实际已使用线程数。

`telemetry_scope=process_lifetime_peak_and_analysis_run_cpu_delta` 明确记录上述两类测量的不同
作用域。当前非 Windows 构建保持相同 v2 字段，但 provider 为 `unavailable`、数值为 `NA`；
Linux `/proc`、cgroup 和 Slurm 资源统计将在服务器阶段实现。

## 新版 RASP

新版 RASP 应生成请求文件。需要分阶段控制时，先调用 `analysis-plan`，只有计划通过后才调用
`analysis-run`；一次完成拟合和随机历史时可以调用 `analysis-workflow`。不要重新拼接
`model-optimize` 的几十个选项，也不要解析人类可读日志。RASP 应按 `schemas/registry.tsv`
中的 request、template、plan、run 和 workflow 契约读取格式，并继续通过进度事件实现取消和
任务状态显示。
