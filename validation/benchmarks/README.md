# 性能测试

这里保存可重复运行的性能脚本。它们调用 release 模式的 `biogeo-cli`，部分任务还调用本地
BioGeoBEARS 或 LAGRANGE-ng 作为参考。

常用入口：

```powershell
powershell -ExecutionPolicy Bypass `
  -File validation/benchmarks/benchmark-dec-stress.ps1

powershell -ExecutionPolicy Bypass `
  -File validation/benchmarks/benchmark-dec-optimization.ps1

powershell -ExecutionPolicy Bypass `
  -File validation/benchmarks/benchmark-bsm-parallel.ps1
```

性能结果依赖树、区域数、最大范围大小、状态数、线程数和输出级别。不能把一个 fixture 的倍率
当成所有分析的固定加速比例。已有记录与解释见[性能基准](../../docs/performance-benchmark.md)。
