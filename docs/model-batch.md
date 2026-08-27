# 批量拟合与模型比较

## 定位

`model-batch` 是通用 `model-optimize` 和 `biogeo-analysis-result-v2` 上方的工作流层。它不包含
另一套似然、参数解析或优化公式：同一次调用共享树、末端观测、状态空间、时期/距离/面积输入
和优化设置，manifest 的每一行只替换参数表与模型标识。

该命令覆盖最常见的 BioGeoBEARS 工作流：**一个数据配置 × 多个模型参数表**。任意参数
释放、固定和联动仍由参数表决定，所以 manifest 不需要认识 DEC、DEC+J 等 preset 名称。
多组独立数据由上层 [`dataset-batch`](dataset-batch.md) 调度，不能混入同一比较表。

## Manifest

```text
biogeo-model-batch-manifest-v1
model_id<TAB>parameters
DEC<TAB>../parameter_tables/dec.tsv
DEC+J<TAB>../parameter_tables/decj.tsv
```

- 第一行是严格格式版本；第二行必须为 `model_id` 和 `parameters` 两列。
- 参数表路径相对于 manifest 所在目录解析。
- `model_id` 按大小写不敏感方式判重，并限制为可安全用作 Windows/Linux 目录名的 ASCII
  字符；尾随句点和 `CON/NUL/COM1` 等 Windows 保留名会被拒绝。
- `--parameters` 和 `--analysis-result-dir` 由批量层管理，不能再作为命令参数传入。

六模型示例位于
[`../examples/model_batch/psychotria-six-models.tsv`](../examples/model_batch/psychotria-six-models.tsv)。

## 运行与恢复

```powershell
cargo run --release -q -p biogeo-cli -- model-batch `
  --manifest models.tsv `
  --output-dir batch-result `
  --tree tree.nwk `
  --ranges ranges.tsv `
  --max-iterations 1000
```

模型当前按 manifest 顺序运行，避免多个高内存优化任务在 PC 上互相争用。每个模型先通过
既有 `model-optimize` 原子发布为标准分析结果。一个模型失败后仍继续后续模型；已发布目录
保留，批量根目录没有 `comparison.tsv` 或 `complete.tsv`，进程最终以非零退出码报告失败。

恢复时追加：

```powershell
--resume
```

恢复会逐字节校验原 manifest、规范任务表和共享调用身份。已有模型还必须通过分析结果内部
指纹、外部输入指纹、原始参数表字节及 `mode=optimize` 校验，否则明确失败；只有缺失模型
会重新优化。修改 manifest、共享选项或参数表后不能借旧目录续跑，应使用新的输出目录。

每次调用都在 `attempts/` 写一个不可覆盖的 `biogeo-model-batch-attempt-v2`。记录包含每个
模型的 `complete/failed/cancelled/not_started`、结果路径、稳定错误分类和经过 TSV 百分号
编码的错误信息。失败或取消后恢复会新增下一次 attempt，不会改写历史。v1 只包含
`complete/failed`，已有 v1 文件仍作为历史记录保留。

## 结果目录

```text
batch-result/
  run.tsv
  source-manifest.tsv
  jobs.tsv
  comparison.tsv
  model-averaged-ancestral-ranges.tsv
  complete.tsv
  attempts/
    attempt-000001.tsv
  models/
    DEC/
      metadata.tsv
      inputs.tsv
      source-parameters.tsv
      resolved-parameters.tsv
    DEC+J/
      ...
```

初始化目录先在同一父目录暂存并原子发布。当前批量完成格式为
`biogeo-model-batch-result-v2`。`comparison.tsv`、模型平均祖先范围和 `complete.tsv` 只在
所有模型都有完整分析结果后发布，`complete.tsv` 最后写入并记录前两者的指纹。重复恢复只
接受逐字节相同的数值结果，不覆盖旧结果。

## AIC、AICc 与权重

参数数目 `k` 是原始参数表中真正声明为 `free` 的维度；通用模型入口已经拒绝不影响似然的
自由参数。与 BioGeoBEARS 1.1.3 一致：

```text
AIC  = 2k - 2 lnL
AICc = AIC + 2k(k+1) / (n-k-1)
```

其中 `n` 使用 tip 数，这与 BioGeoBEARS 官方 Psychotria 示例中的
`samplesize = length(tr$tip.label)` 相同。Akaike weight 由
`exp(-delta/2)` 在可比较模型内归一化。

只有优化器报告收敛且至少一个起点收敛的结果才标为 `eligible=true` 并进入权重归一化。
非收敛模型仍保留 lnL 和诊断，但 AIC/AICc 字段写 `NA`，不会用一个未完成搜索产生虚假的
精确模型权重。当 `n <= k + 1` 时 AICc 修正没有有限定义，该模型的 AICc 写 `NA`。只有
**全部 AIC 候选模型**都有有限 AICc 时才归一化 AICc 权重；若只有一部分模型有定义，不会
把该子集重新归一化并伪装成完整候选集。`aicc_defined_models` 和
`aicc_eligible_models` 分别记录这两个数量。

比较前还严格要求树字节与所选 NEXUS 树、末端观测输入和模式、直接祖先阈值、状态空间、
root prior、区域数与 tip 数一致。距离、时期和节点参数可以不同，因为它们正是待比较模型的
组成部分。

官方数值门禁由 `validation/biogeobears/biogeobears-model-comparison-golden.R` 直接调用 BioGeoBEARS 的
`calc_AIC_vals()`、`calc_AICc_vals()` 和 `AkaikeWeights_on_summary_table()` 生成。

## 嵌套关系与似然比检验

`comparison.tsv` 已升级为 `biogeo-model-comparison-v3`。除了信息准则表，还包含全部有向模型对
的 `nested_model_relationships`，以及所有严格嵌套对的 `likelihood_ratio_tests`。

嵌套关系不按 `DEC/DEC+J` 名称猜测。检查器把约简模型的自由参数写成符号变量，将完整模型的
自由参数替换为约简模型中同名目标的固定、自由或联动表达式，再对全部 23 个 canonical
BioGeoBEARS 参数做有理多项式恒等检验，并检查嵌入值域与优化边界。结果分为：

- `equivalent`：表达式流形相同且自由维度相同；
- `nested_interior`：约简模型是完整模型的内部低维子模型；
- `nested_boundary`：约简模型只在边界或闭包上得到；
- `not_nested`：已证明目标表达式、维度或值域不相容；
- `undetermined`：遇到当前符号/区间检查无法证明的表达式，不会强行判成嵌套。

六个正式 preset 中，DEC、DIVALIKE、BAYAREALIKE 分别是对应 `+J` 模型在 `j=0` 的
`nested_boundary`；不同模型家族之间不是嵌套关系。参数表任意释放、固定和联动后会重新从表本身
判断，不依赖 preset 名称。

严格嵌套且两个优化结果都 `eligible` 时计算：

```text
LR = 2 * (lnL_full - lnL_reduced)
df = k_full - k_reduced
```

普通内部嵌套报告卡方上尾概率。边界嵌套同时保留普通卡方值和风险说明；只有恰好一个边界约束且
`df=1` 时才额外给出 50:50 point-mass/chi-square 的 `p_value_half_chi_square`。这两个值都依赖
渐近正则性，不应被当作小数据下的精确概率。若完整模型 lnL 反而比约简模型低超过容差，输出
`likelihood_order_violation` 并拒绝计算 p 值，提示需要重新优化，而不是把负 LR 截成一个看似
正常的检验。

## 模型平均祖先范围

比较完成后，CLI 会严格重放每个进入相应信息准则的模型，在其最终 ML 参数点计算节点顶端
祖先范围 posterior，并按 Akaike weight 逐元素求和：

```text
P_avg(state at node) = sum_m w_m * P(state at node | data, model_m)
```

结果写入版本化的 `biogeo-model-averaged-ancestral-ranges-v2`。AIC 始终独立计算；AICc 仅在
完整候选集可用时增加第二套结果。非收敛模型不进入任一平均。树、观测、root prior 和状态
空间身份已经在比较前严格校验，因此不会按节点序号强行拼接不同树，也不会对不同范围集合
补零。

文件除原有祖先范围五表外，增加 `split_nodes`、`split_scenarios` 和
`cladogenetic_split_probabilities`。不同模型的分裂情景按节点、祖先范围、有序左右子范围和事件
类型取并集；模型缺失情景先按概率 0 处理再加权。新版 RASP 通过元数据表恢复标签、clade、区域
bit 和范围名称。完整字段、语义与外部门禁见
[`model-average.md`](model-average.md)。

需要把共享数据配置、候选拟合、比较/平均、显式 BSM 模型选择和恢复组合成一次任务时，使用
[`model-workflow.md`](model-workflow.md) 的版本化请求；高层工作流直接复用本命令，不维护另一套
优化或模型比较实现。

## 当前边界

- 一个 `model-batch` 有意只处理一个共享数据配置；多数据集、多树和每组独立修饰输入由
  `dataset-batch` 分层调度，避免跨数据集错误比较 lnL。
- 当前按模型顺序执行，不并行启动多个优化器。
- `--progress-format tsv` 输出模型层和优化迭代层事件；`Ctrl+C` 会停止当前优化且不启动后续
  模型，详细语义见 [`progress-and-cancellation.md`](progress-and-cancellation.md)。
- 图形与 HTML 报告由新版 RASP 读取版本化数值结果后实现，不属于 Rust CLI 的职责。
- 单模型 v2 分析结果已自包含全部顶层输入和时期二级依赖；整个 batch 目录移动后仍应
  逐模型执行 `analysis-result-inspect --replay` 作为导入门禁。
