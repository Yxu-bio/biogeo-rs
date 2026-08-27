# 随机化石放置示例

该示例同时生成一个 crown 类群内的普通侧枝化石和一个 stem 谱系上的直接祖先 hook：

```powershell
cargo run --release -q -p biogeo-cli -- fossil-place `
  --tree examples/fossil_placement/tree.nwk `
  --manifest examples/fossil_placement/fossils.tsv `
  --output-dir fossil-placement-result `
  --replicates 100 `
  --seed 20260723
```

结果树位于 `fossil-placement-result/trees/`，每次放置的年龄、枝和约束记录位于
`placements.tsv`。对包含 `direct_ancestor` 的结果做似然分析时，需显式传入大于 hook 长度的
`--min-branch-length`，默认 hook 为 `1e-7`，常用识别阈值为 `1e-6`。
