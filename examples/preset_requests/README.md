# 六个 preset 分析请求

本目录给出六个可直接执行的 `biogeo-analysis-request-v1` 请求。`inputs/` 保存共享的小数据，
`parameters/` 保存六张正式参数表；整个目录可移动，所有请求都通过同一个统一似然引擎运行。

以 DEC 为例：

```powershell
biogeo-cli analysis-plan --request examples/preset_requests/dec.tsv
biogeo-cli analysis-run --request examples/preset_requests/dec.tsv --output-dir dec-result
biogeo-cli analysis-result-inspect --analysis-result dec-result --replay
```

将 `dec.tsv` 替换为 `decj.tsv`、`divalike.tsv`、`divalikej.tsv`、
`bayarealike.tsv` 或 `bayarealikej.tsv` 即可运行其余 preset。输出目录不可预先存在。
