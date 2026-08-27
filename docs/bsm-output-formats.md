# 生物地理随机历史输出格式

本文定义 Rust CLI 的生物地理随机历史（biogeographic stochastic history）目录格式。
输出级别只改变存储表示，不改变似然模型、条件历史抽样、随机数协议或样本索引。

## CLI

```text
--bsm-output-dir <dir>
--bsm-output-level <legacy|full|compact|summary>
```

`--bsm-output-level` 只用于目录输出；单独调用 `model-bsm` 且不提供时默认为 `legacy`，以兼容
既有脚本。`analysis-workflow` 默认使用 `compact`。无目录的标准输出继续使用既有兼容格式。

## 输出级别

| 级别 | 格式 | 路径可重建 | 占据表 | 用途 |
|---|---|---:|---|---|
| `legacy` | `biogeo-bsm-tsv-v1` | 是 | 稠密 | 兼容既有脚本 |
| `full` | `biogeo-bsm-full-tsv-v2` | 是 | 稠密 | 人工检查、完整归档 |
| `compact` | `biogeo-bsm-compact-tsv-v2` | 是 | 稀疏 | 新版 RASP 和大型任务 |
| `summary` | `biogeo-bsm-summary-tsv-v2` | 否 | 稀疏 | 大量重复样本的分布统计 |

分片格式在名称中增加 `sharded`，例如
`biogeo-bsm-compact-sharded-tsv-v2`。分片只改变文件组织，不改变样本行。

八张样本表的文件名保持稳定：

- `node_states.tsv`
- `cladogenetic_splits.tsv`
- `branch_segments.tsv`
- `sample_event_counts.tsv`
- `sample_period_event_counts.tsv`
- `sample_state_occupancy.tsv`
- `sample_period_state_occupancy.tsv`
- `anagenetic_events.tsv`

`summary` 的四张路径明细表只有表头：节点状态、节点分裂、分支段和逐事件表。这样
checkpoint 和分片 writer 仍使用同一八表事务，但消费者必须根据 `path_details=false`
判断这些文件不能用于重建具体历史。

## v2 字典

所有 v2 根目录都包含不可变字典：

- `areas.tsv`：`area_index` 到区域名；
- `states.tsv`：`state_index` 到 bitset 和范围名；
- `nodes.tsv`：节点 ID、标签和节点类型；
- `edges.tsv`：边 ID、父子节点和枝长；
- `periods.tsv`：`q_index`、时期上界、约束标志和允许状态数。

文本字段使用项目统一的 percent encoding。compact 表只保存整数 ID，必须与同目录字典
一起移动和归档。恢复时 CLI 会重新生成并逐字验证字典；树、区域、状态空间或时期不一致会
在追加样本前失败。

## v2 样本汇总

v2 的 `sample_event_counts.tsv` 显式记录：

- `d` 范围扩张、`e` 局部灭绝和 `a` singleton range switching；
- `y/s/v/j` 四类节点事件；
- 总枝长；
- 分支段数和受时期状态约束的分支段数；
- 最小条件端点概率；
- 最大虚拟跳数；
- 单段最大沿枝事件数；
- 禁止状态转移、禁止端点和禁止状态占据时间。

每条历史在写出前独立执行结构和时期状态约束审计。任何禁止状态计数或禁止占据时间为正时，
CLI 返回稳定 BSM 错误并按最近 checkpoint 回滚，不发布该样本。

compact 和 summary 的两张占据表只写 `occupancy_time > 0` 的组合。缺失的
`sample × state` 或 `sample × q_index × state` 行表示精确的 0，不表示缺失数据。
`metadata.tsv` 用 `sparse_occupancy=true` 明确该规则。

## 确定性与恢复

输出级别进入运行指纹。同一模型、seed 和样本索引在不同级别或不同线程数下抽到同一条历史；
但不同级别不能在同一目录中混合恢复。八张样本表仍由同一个 checkpoint 提交：

1. 完整写入一个有序样本前缀；
2. flush 并同步八张表；
3. 原子发布记录八个精确字节长度的 checkpoint；
4. 失败或恢复时统一截断到最近 checkpoint。

字典不随样本增长，因此不进入八表字节长度数组，但属于 v2 目录身份校验的一部分。

## 消费建议

- 新版 RASP 默认使用 `compact`，按 ID 连接字典后展示或重建历史。
- 只做事件数量、事件类型、时期占比和状态占据时间分布时使用 `summary`。
- 需要便于人工查看的自包含行时使用 `full`。
- 既有读取器尚未升级时使用 `legacy`；新代码不得根据列数猜测格式。
- 消费者必须先读 `metadata.tsv` 的 `format`、`output_level`、`path_details` 和
  `sparse_occupancy`，遇到未知格式号时明确拒绝。
- 读取或导入前调用 `bsm-inspect --bsm-result <dir>`；归档和传输后验收使用 `--deep`。
  检查范围与性能边界见 [`bsm-inspection.md`](bsm-inspection.md)。
- 六个 v2 格式号均已注册到 `schemas/registry.tsv`；发布包中的 schema 固定元数据、
  引用表、八张样本表、分片 manifest 和 shard 内部表头。

官方三末端时期约束案例的 5000 对 5000 分布门禁已经直接读取 summary v2，39 项全部通过。
2026-08-11 在同一 Windows release、16 worker、同一 seed 的单轮直接对照中，legacy 为
1.513 秒和 21.44 MB，summary v2 为 0.836 秒和 3.17 MB。该结果用于说明写出规模差异，
不是跨机器稳定性能承诺。
