# 条件路线概率：给 RASP5 的接入说明

适用开发版：0.1.7-dev.20260930。输出格式：biogeo-bsm-source-attribution-v2。
这是已有生物地理随机历史的后处理，使用保存拟合中的有效权重。无需重新拟合或重抽历史。

## 如何调用

从解压后的目录运行：

~~~powershell
.\biogeo-cli.exe bsm-assign-sources --analysis-result fit --bsm-result histories --allocation-mode probabilities --threads auto --output-dir routes
.\biogeo-cli.exe bsm-sources-inspect --analysis-result fit --bsm-result histories --source-result routes --deep
~~~

输入为绑定到同一保存拟合的 BSM v3 full 或 compact，支持单目录和原生分片。
probabilities 模式不使用随机数，省略 --seed；传入 seed 会明确报错。
--threads auto|N 控制后处理并行度。v2 每批 256 个事件，限制的是批内任务数，不是机器核心数。
中断后使用相同输入、模式和输出目录追加 --resume；可以修改线程数。
续跑重新计算并核对已写分片，补齐缺失文件。检查器从 metadata 自动识别 v1/v2。

旧命令省略 --allocation-mode 时仍为 sample 模式，要求独立 --seed，输出 v1。
v1 的多区域 founder 仍报告 unsupported_multi_area_founder_target；需要此功能的宿主应显式请求 probabilities 并读取 v2。

## 四条概率从哪里来

设已经抽中的 founder 分裂为 AB -> (AB, CD)，CD 是 founder 子代。
当前时期有效矩阵中：

|区域对|权重|条件概率|
|---|---:|---:|
|A -> C|1|0.10|
|A -> D|2|0.20|
|B -> C|6|0.60|
|B -> D|1|0.10|

每条概率为该区域对的有效权重除以四个区域对权重之和。
这里的矩阵已包含拟合时使用的地理修饰，不能用尚未应用参数的原始距离矩阵替代。

Rust 的 founder 整体原始权重为：
j × 目标范围大小权重 × sum(W[source,target]) / (祖先区域数 × 目标区域数)。
对一个已经给定的分裂，j、范围大小权重、区域对数量、祖先行归一化均为共同因子，
因此在条件分配中抵消。距离、环境距离、手动扩散倍数等区域对差异已包含在有效 W 中。

BioGeoBEARS 的 get_spPmat_inputs_from_BGB 将有效矩阵传给 cladoRcpp，
后者也按区域对求平均。参考：

- https://github.com/nmatzke/BioGeoBEARS/blob/master/R/BioGeoBEARS_extract_Qmat_COOmat_v1.R
- https://github.com/nmatzke/cladoRcpp/blob/master/src/calc_anclikes_sp.cpp
- Rust：crates/biogeo-core/src/cladogenesis.rs 的 add_founder_event、founder_event_pairwise_mean。

这些数值是给定整体分裂后的“条件贡献概率”。整体模型没有显式抽中某一个微观区域对。
例如 A -> C 获得 0.1 的贡献不意味着子代范围中的 D 消失，也不表示已经重建了先到 C 再到 D 的过程。
新增输出保留全部条件概率，不额外抽取一条路线。

## 输出文件

|文件|用途|
|---|---|
|metadata.tsv|v2 格式、算法、原拟合/历史/字典指纹、时期规则。rng_protocol=none，source_seed=NA。|
|files.tsv|原事件记录分片清单及校验值。|
|route-files.tsv|候选路线分片清单及校验值，与 files.tsv 按 chunk 对应。|
|shards/sources-NNNNNNNN.tsv|每个原事件一行，保留 event_id、sample、状态、节点/边、事件时间、q_index 和 founder 子代顺序。|
|shards/route-candidates-NNNNNNNN.tsv|每个有效候选区域对一行，按原事件顺序、source_area_index、target_area_index 升序排列。|
|complete.tsv|完成状态、事件数、候选行数、分片数、状态统计和清单指纹。|

候选表字段：
sample, event_id, source_area_index, target_area_index, conditional_probability。
用 (sample, event_id) 连接到同分片的原事件表。区域索引沿用绑定 BSM 的 areas.tsv。
节点、边、状态字典同样沿用绑定 BSM，不通过标签或 clade 文本猜测映射。

v2 原事件表中的 source_area_index、target_area_index、conditional_probability 均为 NA，
包括 switching；具体区域对和概率统一读取候选表。
positive_candidates 表示实际输出的正概率区域对数量。
概率采用科学计数法保留小数值，读取方应按浮点数解析。

分片可能没有候选行，但仍有表头，例如其中只有灭绝或不涉及扩散的分裂。
单端点、无事件的历史产生零个分片，两个清单仅有表头。

## 事件及时期规则

- range_expansion：来源为扩散前范围，目标为新增的唯一区域。
- founder_event：目标严格取原 split 指定的 founder_daughter_order；允许多区域目标。
- range_switching：直接输出状态转换确定的唯一区域对，概率 1。
- local_extirpation、range_copying、subset_sympatry、vicariance：not_applicable，无候选行。
- 有正权重的扩散/founder：probability_distribution，每个事件的候选概率之和为 1。
- 有效权重全部为零：undetermined / zero_total_weight，无候选行，不作均匀回退。
- 未提供矩阵意味着模型采用单位权重，此时等权是模型本身的含义。
- 直接祖先处没有原始分裂记录时，不产生额外分裂事件。

沿枝事件使用历史中保存的 segment/q_index。节点事件使用引擎在节点年龄处选定的时期；
在当前边界容差 1e-12 下归较年轻时期，与 Rust 抽样保持一致。

当前实现使用 f64；小到无法表示的归一化贡献不输出。全零权重与此情况不同：
最大正权重对应的候选始终保留。范围数目最多 64，单个不相交 source × target 最多 1024 对。

## RASP5 如何绘图和计数

1. 校验 metadata/complete 及输入指纹，确认格式为 v2。engine-info.public_formats 已注册 v2。
2. 对每个事件，将候选概率加到相应方向的网络边；四个候选仍只代表一个原始事件。
3. 每份历史内按区域对累加。若展示多份历史的平均事件贡献，再除以总历史样本数；
   某条路线未出现在某份历史时，其贡献为零。
4. 图例使用“条件分配的事件贡献”或“期望事件贡献”。不要标为实际抽中的路线或新增事件次数。
5. 筛选 founder 与沿枝扩散时，使用原事件表的 event_kind。保留 undetermined 的独立统计。

深度检查会使用保存拟合和历史重新计算每个事件的概率并逐文件比较。
普通检查核对清单、文件指纹、事件连接、候选数量和每事件概率和（容差 1e-12）。

## 可复现实例

包内 examples/bsm_joint_routes 提供四端点、四区域、两时期的固定参数示例。
放开 mx01j 使多区域 founder 可以发生。较年轻时期 AB -> CD 是 1:2:6:1，
较老时期为 6:1:1:2。这是验证功能的明确构造例子。

~~~powershell
.\biogeo-cli.exe analysis-run --request examples/bsm_joint_routes/request.tsv --output-dir joint-fit
.\biogeo-cli.exe model-bsm --analysis-result joint-fit --bsm-samples 512 --seed 1234 --bsm-output-level compact --bsm-shard-samples 73 --bsm-threads 3 --bsm-output-dir joint-histories
.\biogeo-cli.exe bsm-assign-sources --analysis-result joint-fit --bsm-result joint-histories --allocation-mode probabilities --threads 3 --output-dir joint-routes
.\biogeo-cli.exe bsm-sources-inspect --analysis-result joint-fit --bsm-result joint-histories --source-result joint-routes --deep
~~~

开发期 R 对照脚本位于 validation/biogeobears/compare-bsm-joint-routes.R。
它逐条比较 Rust 与 R 的有效矩阵归一化概率，并直接调用 cladoRcpp 的原始分裂权重构造函数，
分别仅保留一个区域对的权重，验证各贡献之和与整体分裂权重一致。
R 侧再按这些概率做条件分类抽样以检验频率。原版 BGB 来源后处理没有多目标接口，
因此这里没有把扩展后的条件抽样声称为原版 helper 的现有输出。
