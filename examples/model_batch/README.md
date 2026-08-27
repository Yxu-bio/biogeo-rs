# 六模型批量拟合示例

`psychotria-six-models.tsv` 列出统一框架现有的六个正式 preset 参数表。路径相对于 manifest
文件所在目录解析，因此从项目根目录或其他工作目录调用时语义一致。

使用 BioGeoBEARS 官方 Psychotria M4 输入运行：

```powershell
cargo run --release -q -p biogeo-cli -- model-batch `
  --manifest examples/model_batch/psychotria-six-models.tsv `
  --output-dir psychotria-models `
  --tree validation/fixtures/biogeobears_official/psychotria_m4/tree.nwk `
  --ranges validation/fixtures/biogeobears_official/psychotria_m4/ranges.tsv `
  --include-null-range `
  --max-range-size 4 `
  --max-iterations 1000
```

任务中断后使用完全相同的 manifest 和模型参数，再追加 `--resume`。已经完整发布且通过输入、
参数表和结果格式校验的模型不会重算；缺失模型从头优化。批量层不在单个优化器内部制造检查点。

完成目录中的 `comparison.tsv` 是信息准则表，`model-averaged-ancestral-ranges.tsv` 是
`biogeo-model-averaged-ancestral-ranges-v2` 数值结果，包含 AIC/AICc 模型权重、祖先范围以及
跨模型 cladogenetic split scenario 平均。`comparison.tsv` 的 v3 关系表还会自动确认三组
无 `+J`/有 `+J` 模型为边界嵌套并生成可用的似然比检验。图形由新版 RASP 读取这些表后生成。
