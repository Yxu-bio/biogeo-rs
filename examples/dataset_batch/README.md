# 多数据集批任务示例

本目录用同一组 DEC/DEC+J 参数表分别分析两末端和三末端数据。两个数据集各自产生独立的
模型比较，Akaike weight 不跨数据集混合。

```powershell
cargo run --release -q -p biogeo-cli -- dataset-batch `
  --manifest examples/dataset_batch/datasets.tsv `
  --output-dir dataset-batch-example
```

中断或某个输入暂时不可用时，以相同参数追加 `--resume`。逐数据集状态位于根目录
`attempts/`，具体模型状态位于各 `datasets/<dataset_id>/attempts/`。
