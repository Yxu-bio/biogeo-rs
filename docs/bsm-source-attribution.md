# 为已有生物地理随机历史分配扩散来源

从开发版 `0.1.7-dev.20260930` 起，可用 `--allocation-mode probabilities` 保存全部区域对的条件概率，
包括多区域 founder 目标。使用 v2 格式且不需要 seed，见 [条件路线概率接入说明](bsm-joint-route-probabilities.md)。
本页下文描述仍然兼容的默认 sample 模式（v1）。

适用版本：`0.1.2-dev.20260928`。新结果格式：`biogeo-bsm-source-attribution-v1`。

这一步只回答“这次已发生的扩散更可能从哪个区域出发”。它不会重新拟合模型，不会重抽生物地理随机历史，也不会改动历史中的状态、事件类型、事件时间或事件次数。

例如，已保存的事件是 `AB -> ABC`，事件所在时期的有效权重为 `A -> C = 9`、`B -> C = 1`，则分别以 0.9 和 0.1 的概率抽取来源。结果是给定既有历史后的条件来源分配，不是重新估计的一条完整历史，更不代表已经观察到的真实迁移路线。

## 1. 直接运行

已有保存拟合 `fit` 和由该拟合生成的完整 BSM 目录 `histories`：

```powershell
.\biogeo-cli.exe bsm-assign-sources --analysis-result fit --bsm-result histories --seed 29 --threads auto --output-dir sources
.\biogeo-cli.exe bsm-sources-inspect --analysis-result fit --bsm-result histories --source-result sources --deep
```

- `--seed` 必须显式指定，是来源分配专用 seed，不是原 BSM seed。
- `--threads auto|N` 控制本次后处理，auto 使用当前进程可用并行度；没有固定 16 线程上限。每批最多处理 4096 条事件，因此更多 worker 无实际用途。
- 输入必须是已完成的 **v3 full 或 compact** 生物地理随机历史，单目录或原生分片均可。summary 和旧版历史缺少必要信息，会被拒绝。
- BSM 必须带有与保存拟合一致的 `analysis_result_fingerprint`。直接由固定模型命令产生但未绑定保存拟合的 BSM，不能仅凭参数看起来相同就接入。
- 输出必须是新的空目录，且不能放进只读的拟合或 BSM 输入目录。
- 不需要安装 R。R 仅用于开发期的对照验证。

中断后，用原 seed、原输入和原输出目录续跑，可修改线程数：

```powershell
.\biogeo-cli.exe bsm-assign-sources --analysis-result fit --bsm-result histories --seed 29 --threads 4 --output-dir sources --resume
```

续跑会重新读取和检查输入，重新计算确定性来源结果以核对已完成分片，仅写缺失分片；不会重抽历史，也不会改写已完成分片。Ctrl+C 在输入校验完成后或事件批次之间生效，输入深度校验本身还不支持即时取消。异常中止留下的未完成分片由同一次续跑覆盖；没有 `complete.tsv` 时不可导入为完整结果。

## 2. 不同事件如何处理

|事件|处理方式|
|---|---|
|range_expansion|目标为新增加的唯一区域，候选来源为扩散前范围中的区域，按有效方向性矩阵抽样。|
|founder_event，单区域目标|依据原 split 的 `event_kind` 和 `founder_daughter_order` 确定目标子代，候选来源为祖先范围，按有效矩阵抽样。|
|range_switching|来源和目标由两个单区域状态直接确定，不消耗额外随机数。|
|local_extirpation、range_copying、subset_sympatry、vicariance|`not_applicable`，不构造扩散路线。|
|自定义多区域 founder 目标|保留整个目标状态和子代顺序，输出 `undetermined / unsupported_multi_area_founder_target`，来源和目标 area 均为 `NA`。|
|全部候选权重为零|输出 `undetermined / zero_total_weight`，不退回均匀抽样。|

只剩一个正权重候选时为 `unique_candidate`，概率为 1；两个以上正权重候选为 `weighted_sampled`。负数、NaN 或无穷权重会被模型输入校验拒绝。`undetermined` 不等于“没有事件”，网络图不能把它伪装成已确定路线。

## 3. 权重与时期

恢复的是保存拟合的参数和地理修饰，直接使用模型构造后的有效 dispersal matrix，不是原始手工矩阵。已应用的 `w`、距离 `x`、环境距离 `n` 等不会被忽略或再次重复应用。只影响灭绝率的面积修饰不会被错误加入来源分配。

对目标区域 c、祖先范围 S，条件概率为：

```text
P(source=i | 已有事件, 时期) = W[i,c] / sum(W[k,c], k in S)
```

共同的 d 或 j 系数及同一事件中共同的归一化因子会约掉。矩阵按“来源行、目标列”使用，不能转置。

- 沿枝事件使用原历史记录的 `q_index`，即生成该枝段时实际使用的时期。
- 节点 founder event 使用节点年龄和现有引擎的 `stratum_index_at_age`，与原节点分裂模型保持一致。
- 时间单位继承输入树，不能自行称为百万年。沿枝绝对年龄为父节点年龄减 `time_from_parent`；节点年龄沿用 BSM 字典的定义。

**与 R helper 的一个明确差异：** 恰好处在时期边界时，BioGeoBEARS 来源 helper 使用较老一期；现有 Rust 节点分裂引擎使用较年轻一期，比较时带 `1e-12` 容差。来源后处理必须服从原事件实际使用的模型，而不能为了对照修改事件年龄或历史。测试在非边界调用原版 R 分时期 helper；在边界另用实际选中的矩阵做条件概率对照，并单列原 R 边界行为。

## 4. RASP5 应怎样连接结果

目录内容：

```text
sources/
  metadata.tsv
  files.tsv
  complete.tsv
  shards/sources-00000000.tsv
  shards/sources-00000001.tsv
  ...
```

首先确认 `complete.tsv` 存在且 `status=completed`，再通过 `files.tsv` 逐个读取分片。没有任何事件时，分片目录可以为空。不要靠扫描文件名猜完成情况。

每条来源记录对应一条原事件，包括不适用的事件；直接祖先节点没有原 split 时，不会创建新 split。

**稳定事件标识：**

- 沿枝事件：`s{sample}:a:e{edge}:i{event_ordinal}`。
- 分裂事件：`s{sample}:c:n{node}`。
- sample、node、edge、state 和 area 均为原字典中的零基索引。
- `event_ordinal` 从 0 开始，是该 sample、该 edge 内的所有沿枝事件序号，跨 segment 连续计数，**灭绝和 switching 也参与计数**。以原事件表顺序计数；同一时间的事件仍按原表顺序区分。不能只给扩散事件编号，也不能用时间字符串或物理行号当键。
- 这是同一次拟合及历史中的稳定身份，不保证重新排序输入树后仍有相同 node/edge ID。

主要列：

|列|含义|
|---|---|
|sample、event_id、event_scope、event_kind|连接原事件及其真实类别。|
|edge、node、event_ordinal、segment|沿枝或节点定位；不适用项为 `NA`。|
|q_index、time_from_parent、age_before_present|实际时期、原沿枝相对时间及绝对年龄。|
|source_state_index|沿枝事件的前状态，或 split 的祖先状态。|
|target_state_index|沿枝事件的后状态；founder 的整个目标子代状态；其他 split 为 `NA`。|
|founder_daughter_order|原输入树顺序：0 为左，1 为右；其他事件为 `NA`。|
|source_area_index、target_area_index|已确定或已条件抽样的来源和单一区域目标；无法确定时为 `NA`。|
|assignment_status、reason|区分加权抽样、唯一候选、状态直接确定、不适用和无法分配。|
|conditional_probability|本次被选中来源的条件概率，不是整个事件或历史的后验概率。|
|positive_candidates|正权重来源数；不适用或多区域目标未支持时为 0。|

只有 `weighted_sampled`、`unique_candidate`、`state_transition` 可直接形成来源到目标的网络边。RASP5 可展示这些后处理抽样来源；不能再把它们称为等权分摊。未分配的事件应单独统计。

原 v3 历史中的 `source_status=undetermined` 不会被改写。新 artifact 是来源信息的补充，界面应明确使用了哪个来源 artifact 和 seed。

## 5. 可复现与检查范围

每个事件使用独立 ChaCha12 随机流，协议为 `chacha12-source-event-v1`。32 字节 seed 的四个小端 u64 段为 `[source_seed XOR scope_domain, sample, location, ordinal]`；anagenetic domain 为 `0x616e6167656e6574`，location 是 edge；cladogenetic domain 为 `0x636c61646f67656e`，location 是 node，ordinal 为 0。均匀抽样采用锁定依赖中的 `rand 0.10.2` / `rand_chacha 0.10.0`。

来源 RNG 不调用原 BSM RNG。相同 source seed 和同一逻辑事件，在不同线程数、原历史 full/compact 或分片布局下结果相同。

metadata 绑定保存拟合、全部原历史表内容以及 area/state/node/edge/period 字典，并声明算法、随机数协议和 seed。输入指纹使用现有 FNV-1a 64 协议，用于一致性校验，不作为密码学防篡改承诺；发布 exe 另提供 SHA256。

不同物理布局的 BSM 文件内容指纹不同，所以同一来源 artifact 不可直接挂接到重新导出的一份 BSM。可对新布局以相同 seed 再运行命令，逐事件来源保持相同。

- 普通 inspect：检查输入绑定、输入 BSM 完整性、文件清单、来源分片校验和、行数及完成统计。
- `--deep`：进一步独立重算每条来源分配并逐项核对。即使错误结果被重新计算过文件校验和，也会被发现。仍不会计算似然或重新抽历史。
- 写出和分配按 4096 条事件处理，结果不全部驻留内存。输入校验复用已有 BSM 检查器；大数据仍需时间读取全部输入。

## 6. 随包示例

发布目录包含 `example-input`、`example-fit`、`example-bsm`、`example-sources`。示例是固定参数的 DEC+J 两时期模型，不是新科研数据：年轻期 `A -> C : B -> C = 9 : 1`，较老期为 `1 : 9`。

直接核验交付结果：

```powershell
.\biogeo-cli.exe bsm-sources-inspect --analysis-result example-fit --bsm-result example-bsm --source-result example-sources --deep
```

生成另一份来源抽样：

```powershell
.\biogeo-cli.exe bsm-assign-sources --analysis-result example-fit --bsm-result example-bsm --seed 30 --threads auto --output-dir another-source-draw
```

重新生成同一示例历史时：

```powershell
.\biogeo-cli.exe analysis-run --request example-input/request.tsv --output-dir new-fit
.\biogeo-cli.exe model-bsm --analysis-result new-fit --bsm-samples 4096 --seed 1234 --bsm-threads 3 --bsm-output-level compact --bsm-shard-samples 512 --bsm-output-dir new-bsm
.\biogeo-cli.exe bsm-assign-sources --analysis-result new-fit --bsm-result new-bsm --seed 29 --threads 3 --output-dir new-sources
```

对应 schema 在 `schemas/biogeo-bsm-source-*.schema.tsv`，并已注册到 `engine-info.public_formats`。实际 R 对照数据和完整测试日志随包提供。
