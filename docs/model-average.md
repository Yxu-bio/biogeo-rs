# 跨模型祖先范围与分裂情景平均

## 统计语义

`model-batch` 先分别拟合每个模型，再按 AIC 或 AICc 权重对条件于模型的 posterior 求和：

```text
w_m = exp(-delta_IC_m / 2) / sum_r exp(-delta_IC_r / 2)
P_avg(z) = sum_m w_m * P(z | data, model_m)
```

这里的 `z` 可以是节点顶端的祖先范围，也可以是某个 cladogenetic split scenario。结果是对
模型不确定性积分后的 posterior，不是把 `d/e/j` 等参数平均成一个新模型，也不是对每个模型的
最大概率状态投票。

候选模型规则与 `comparison.tsv` 完全相同：只纳入优化器报告收敛且至少一个起点收敛的模型；
AIC 与 AICc 是两套独立权重。只要一个 AIC 候选模型没有有限 AICc，完整候选集的 AICc 平均就
不生成，AIC 结果不受影响。

## Split scenario 对齐

不同 preset 的分裂情景集合并不相同，不能按各模型内部行号相加。v2 使用以下稳定键：

```text
node + ancestor_state_index + left_state_index + right_state_index + event
```

- `left/right` 按输入树的两个子节点顺序固定，不按范围大小重新排序。
- `event` 为 `range_copying/subset_sympatry/vicariance/founder_event`，由祖先和子代范围关系验证。
- 所有入选模型的键取并集；某个模型没有某一情景时，该模型对此键的概率定义为 0，然后再乘模型权重。
- 不会只在共有情景中重新归一化，也不会把 founder event 错配给 vicariance。
- 每个模型、每个分裂节点的输入概率和必须为 1；模型平均后，每个准则、每个分裂节点的并集概率和也必须为 1。
- 直接祖先 hook 没有物种形成事件，因此只保留节点状态 posterior，不进入 split 表。

## 结果格式

结果文件为 `model-averaged-ancestral-ranges.tsv`，格式号为
`biogeo-model-averaged-ancestral-ranges-v2`。前部键值元数据明确记录：

- 候选模型数及 AIC/AICc 可用数量；
- 节点、区域、状态和 split scenario 数量；
- `missing_split_scenario_semantics=zero_probability_before_weighted_sum`；
- `ordered_daughters=input_tree_child_order`；
- 加权公式和百分号字段编码。

随后有八张 TSV 表：

```text
model_weights
nodes
split_nodes
areas
states
ancestral_state_probabilities
split_scenarios
cladogenetic_split_probabilities
```

`split_nodes` 保存左右子节点及各自后代类群；`split_scenarios` 给每个稳定键分配文件内
`scenario_index`；最终概率表只保存 `criterion/scenario_index/probability`。新版 RASP 应先读取
元数据表再做连接，不应从范围名称反推 bitset，也不应混合 AIC 和 AICc 后重新归一化。

v1 只包含祖先范围平均。v2 是增加 split 语义后的新契约，不在原格式号下静默追加表。机器契约见
`schemas/biogeo-model-averaged-ancestral-ranges-v2.schema.tsv`。

## 数值与验证

祖先范围和 split 都使用补偿求和；模型权重、输入 posterior 和最终每节点概率均检查有限、非负
与归一化。已有 BioGeoBEARS 1.1.3 小数据 golden 继续约束祖先范围平均；六模型 Psychotria
回归同时覆盖 DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE、BAYAREALIKE+J 的情景并集、
事件类型、零填充和 AIC/AICc 两套归一化。

```powershell
Rscript validation/biogeobears-model-average-golden.R
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-model-average.ps1
powershell -ExecutionPolicy Bypass -File validation/check-model-batch-psychotria.ps1
```

BioGeoBEARS 提供各模型的条件 posterior 和 Akaike weight；跨模型 split 并集及缺失为 0 的语义是
本项目为统一模型框架定义的显式统计契约，不声称 BioGeoBEARS 有一张可逐行照搬的同名输出表。
