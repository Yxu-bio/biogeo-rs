# Psychotria ambiguous-range fixture

这是一个 **BioGeoBEARS 官方案例衍生 fixture**，不是 BioGeoBEARS 原样附带的独立示例。

- 树使用 `../psychotria_m4/tree.nwk`，来源于 BioGeoBEARS `examples/Psychotria_M4_dists/Psychotria_5.2.newick`。
- `ranges.tsv` 由同目录官方 Psychotria M4 的四区域 tip ranges 衍生。
- 仅将原始 `0` 或 `1` 隐去为 `?`；没有把 `0` 改成 `1`，也没有把 `1` 改成 `0`。
- 行类型覆盖精确范围、presence-only 以及同时含已知 presence/absence 的部分未知约束。
- golden 生成时显式设置 `BioGeoBEARS_run_object$useAmbiguities = TRUE`、`max_range_size = 4` 和 `include_null_range = TRUE`。

BioGeoBEARS 1.1.3 的约束语义为：`1` 表示状态必须包含该区域，`0` 表示状态不得包含该区域，`?` 不施加约束。兼容状态的 tip conditional likelihood 均为 1，不做概率归一化。部分未知行排除 null range。

底层 `tipranges_to_tip_condlikes_of_data_on_each_state()` 将整行 `????` 定义为允许全部状态（包括 null），也支持只有 `0/?` 的 absence-only 约束；但标准输入检查要求每个 tip 至少有一个已知 `1`，会在似然计算前拒绝这两类行。因此完整 Psychotria 运行 fixture 不包含它们；这些底层语义由单独的源码级微型 golden 覆盖，Rust 保留这一能力。
