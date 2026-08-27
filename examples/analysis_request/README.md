# 统一分析请求示例

`analysis.tsv` 使用 `biogeo-analysis-request-v1`，相对路径以请求文件所在目录为基准。

运行前检查：

```powershell
biogeo-cli analysis-plan --request examples/analysis_request/analysis.tsv
```

执行并生成可移植分析结果：

```powershell
biogeo-cli analysis-run --request examples/analysis_request/analysis.tsv --output-dir result
```

`analysis-run` 继续调用通用参数表模型引擎。请求文件不包含结果目录，因此同一请求可以在不同输出目录运行，结果目录仍保持非覆盖语义。
示例给予优化器 200 次最大迭代，避免把仅供快速冒烟的未收敛结果当作教程输出。
