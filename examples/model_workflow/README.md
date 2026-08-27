# 多模型统一工作流示例

`workflow.tsv` 是 `biogeo-model-workflow-request-v1` 请求。它引用候选模型 manifest 和共享
数据配置，一次执行模型拟合、AIC/AICc 比较、祖先范围及分裂情景模型平均，并可从明确选择的单个
拟合模型生成生物地理随机历史。

先预检：

```powershell
cargo run --release -q -p biogeo-cli -- model-workflow-plan `
  --request examples/model_workflow/workflow.tsv
```

再运行：

```powershell
cargo run --release -q -p biogeo-cli -- model-workflow `
  --request examples/model_workflow/workflow.tsv `
  --output-dir two-tip-model-workflow
```

任务中断后在相同科学请求和输出目录后追加 `--resume`。恢复时可调整线程、在途任务、总事件/内存/
时间预算、检查点频率、交互模式和检查深度；样本数、seed、输出布局及模型配置不能改变。请求中的
`bsm_selection=model_id` 和
`bsm_model_id=DEC` 表示明确从 DEC 结果采样；也可使用 `best_by_criterion`，但 AIC/AICc 并列第一
时会拒绝自动选择，要求改为明确模型 ID。信息准则选择只是执行策略，不代表该模型为科学真相。
