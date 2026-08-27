# Psychotria 五时期分层分析

这是 BioGeoBEARS 官方 Psychotria M4b 分层输入的可移植命令行示例。目录内包含 19-tip 树、
4 区域范围矩阵、5 个时期的 manual dispersal、距离和面积输入，以及完整 DEC 参数表。
示例优化 `d/e`，并固定 `x=-1`、`u=-0.5`，因此每次似然计算都会经过时期、距离和面积修饰。

```powershell
biogeo-cli analysis-plan --request examples/stratified_analysis/analysis.tsv
biogeo-cli analysis-run --request examples/stratified_analysis/analysis.tsv --output-dir psychotria-stratified-result
biogeo-cli analysis-result-inspect --analysis-result psychotria-stratified-result --replay
```

输入数值来自仓库中冻结的 BioGeoBEARS 官方 fixture；本目录是独立副本，便于直接复制运行。

