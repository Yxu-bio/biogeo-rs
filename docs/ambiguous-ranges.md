# 不确定范围观测

## 目标与入口

范围表可以显式使用 BioGeoBEARS 1.1.3 的 `0/1/?` 观测语义：

- `1`：真实范围必须包含该区域；
- `0`：真实范围不得包含该区域；
- `?`：该区域不施加 presence/absence 约束。

这是末端观测模型，不改变状态空间、Q、节点分裂表或 pruning 公式。为避免输入错误被静默
放宽，CLI 默认仍只接受精确 `0/1`；必须显式传入 `--use-ambiguities`：

```powershell
cargo run --release -q -p biogeo-cli -- dec `
  --tree tree.nwk `
  --ranges ranges.tsv `
  --use-ambiguities `
  --d 0.1 `
  --e 0.2
```

固定模型、专用优化器、二维 profile、通用 `model-evaluate/model-optimize`、版本化结果重放
和生物地理随机历史都消费同一组 tip likelihood。`--use-ambiguities` 与 detection 观测模型
互斥，不能在同一次分析中混用。

## 数学语义

对 tip `t`，记已知存在区域集合为 `R_t`，已知不存在集合为 `F_t`。状态 `s` 的观测
likelihood 为：

```text
L_t(s) = 1, 当 R_t 是 s 的子集，且 s 与 F_t 不相交
       = 0, 其他情况
```

这些 0/1 是 conditional likelihood，不是归一化概率。兼容状态可以同时有多个 1，归一化
只在整棵树的 pruning、posterior 或随机历史条件抽样中发生。

BioGeoBEARS 的底层函数还有两个明确特例：

- 部分未知行排除 null range；
- 整行全 `?` 对状态空间内每个状态取 1，包括启用时的 null range。

精确行即使在 ambiguity 模式中也保持 one-hot；不会因为分析中其他 tip 含 `?` 而改变。

## BioGeoBEARS 标准工作流边界

BioGeoBEARS 1.1.3 的底层
`tipranges_to_tip_condlikes_of_data_on_each_state(..., useAmbiguities=TRUE)` 支持 presence-only、
absence-only、混合约束和全未知行，但标准文件工作流还有更早的检查：

- `getranges_from_LagrangePHYLIP()` 拒绝整行全 `?`；
- `check_BioGeoBEARS_run()` 要求每个 tip 至少有一个已知 `1`，因此拒绝纯 absence-only 行。

Rust 选择保留底层正式函数的完整观测语义，并要求显式 opt-in，而不是复制这两个输入层限制。
完整端到端 BioGeoBEARS 对照 fixture 遵守其标准工作流；全未知和 absence-only 由直接调用
官方底层函数生成的源码级 golden 锁定。

## 校验与诊断

```powershell
cargo run --release -q -p biogeo-cli -- validate-inputs `
  --tree tree.nwk `
  --ranges ranges.tsv `
  --use-ambiguities
```

除普通树/范围摘要外，输出还包括：

- `tip_observation_model=ambiguous_ranges`；
- `ambiguous_tips`；
- `unknown_range_cells`；
- `all_unknown_tips`；
- `maximum_possible_range_size`。

`maximum_observed_range_size` 表示已知必含区域数的最大值；
`maximum_possible_range_size` 还计入 `?` 可能包含的区域。若状态空间的 `max_range_size` 使某行
没有任何兼容状态，分析会在 pruning 前明确失败。

## 外部对照

`validation/fixtures/biogeobears_official/psychotria_ambiguities/` 从 BioGeoBEARS 官方
Psychotria M4 的 19-tip、4-area 数据衍生，只把部分已知值隐去为 `?`。BioGeoBEARS 1.1.3
golden 固定了：

- 19 × 16 = 304 个 tip-state conditional likelihood；
- 固定 `d=0.1, e=0.2` 的 lnL；
- 18 × 16 = 288 个内部节点状态 posterior；
- BioGeoBEARS 的 `d/e` 优化结果及 Rust 在该坐标的交叉重算。

固定 lnL 的 Rust/BioGeoBEARS 绝对差为 `1.34e-7`。优化 lnL 绝对差为 `3.25e-7`，两边
都把 `e` 放在 `1e-12` 下界，`d` 相差约 `6.5e-6`。源码级微型 golden 另逐格覆盖全未知、
absence-only 和混合约束。

自动门禁：

```powershell
cargo test -p biogeo-core --test biogeobears_ambiguity_golden
cargo test -p biogeo-cli ambiguous
```

analysis-result 会把观测模式冻结为 `ambiguous_ranges`；重放时重新解析原范围表、重算 lnL，
再交给同一生物地理随机历史采样器。BSM 运行指纹也包含观测模式，所以 exact 与 ambiguity
模式即使在一个全为 `0/1` 的表上数值相同，也不会被误判为同一个可续跑任务。
