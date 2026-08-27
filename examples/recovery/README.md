# 可恢复错误示例

该示例演示机器可读的时间预算停止和安全恢复。`workflow-stop.tsv` 将生物地理随机历史预算设为
0 秒，因此模型拟合完成后进程以退出码 `124` 和错误码 `bsm_time_limit` 停止；这不是损坏的结果。

```powershell
biogeo-cli --error-format tsv model-workflow `
  --request examples/recovery/workflow-stop.tsv `
  --output-dir recovery-result

biogeo-cli model-workflow `
  --request examples/recovery/workflow-resume.tsv `
  --output-dir recovery-result `
  --resume

biogeo-cli bsm-inspect --bsm-result recovery-result/bsm-result --deep
```

两份请求只有 `bsm_time_limit_seconds` 不同。模型、数据、模型选择、样本数、seed 和输出布局完全
相同，因此恢复会复用已发布的拟合结果，只继续未完成的随机历史。不要删除中断目录后重跑。
