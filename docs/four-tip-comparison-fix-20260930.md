# 四端点六模型汇总崩溃修复

版本：`0.1.4-dev.20260930`，Windows x86_64 MSVC，本地开发交付，未发布到 GitHub。

## 问题与原因

使用 RASP5 保留的四端点、三区域、有范围约束的原始 `workflow.tsv`，
`0.1.3-dev.20260929` 在六个模型均完成并收敛后，以退出码 101 崩溃：

```text
internal error: entered unreachable code: model comparison criterion fields must be populated together
```

根因位于模型比较与模型平均之间的转换，不在似然、优化或祖先概率算法。
原来的转换函数错误地假设“有信息准则分数，就一定有差值和权重”。

以 tip 数作为 n 的现有定义保持不变：

```text
AICc = AIC + 2k(k+1)/(n-k-1)
```

当 n=4 时，标准无 J 模型的 k=2，其 AICc 有定义；标准 +J 模型的 k=3，
分母为零，AICc 不可用。引擎原本就规定，只有全部合格候选模型都有 AICc 时才计算
AICc 差值、权重和排名。于是无 J 模型的“有分数、无权重”是合法状态，却触发了错误断言。

## 修复范围

- 汇总转换接受“有分数、没有比较权重”，该模型不参与该准则的模型平均。
- 原比较表保留有定义的单模型分数，不伪造权重，也不只对无 J 的子集重新归一化。
- 六模型 AIC 比较和 AIC 加权祖先范围、分裂情景平均正常生成。
- 如明确要求按不可用的 AICc 排名自动选择 BSM 模型，返回正常的结构化错误及退出码 2，
  不崩溃，不自动改成 AIC。明确指定一个已收敛模型仍可运行生物地理随机历史。
- 不改变拟合参数、参数边界、优化策略、似然、后验、输入树、范围约束或来源分配规则。

仅修复合法缺失状态的转换，没有添加笼统的 panic 捕获或跳过模型机制。

## 实际复现结果

原始请求：
`D:/RASP5_Rust_Settings_20260930/151654_239872/six_False/biogeo-rs/workflow.tsv`。
原请求目录未修改；所有验证输出只写入本项目 `output/four-tip-comparison-20260930/`。

| 检查 | 结果 |
|---|---|
| 旧 exe + 原始请求 | 复现 panic，退出码 101；六个拟合此前均已完成并收敛 |
| 新 exe 恢复本项目内的崩溃复现目录 | 成功完成；78 个已有拟合文件的 SHA256 全部未变 |
| 新 exe 全新运行同一个请求 | 成功完成，六个模型仍收敛 |
| 恢复与全新运行的比较表、模型平均概率表 | 逐字节一致 |
| 直接 model-batch，include/exclude null | 两种设置都成功，均保留六模型 AIC 与部分单模型 AICc 分数 |
| 同设置的 model-batch 与 model-workflow | 比较表及模型平均概率表逐字节一致 |
| 完整 cargo test --workspace | 447 项通过，0 失败；2 项外部依赖测试默认忽略，本次未另行重跑 |
| release 模式小树、RASP 宿主和 schema 测试 | 9 项通过 |
| cargo clippy --workspace --all-targets -- -D warnings | 通过 |

新增测试检查：3 个 tip 的 AICc 全部缺失、4 个 tip 的部分缺失、5 个 tip 的全部可用；
两种 null 设置；范围约束；AIC/AICc 两种请求；恢复运行；按 AICc 自动选择时的正常拒绝；
明确指定模型后的 BSM。权重、节点祖先概率及节点分裂情景概率均检查归一化。

## RASP5 需要注意什么

不需要更换输入接口或结果 schema。原调用可继续使用，公开格式仍为：

- `biogeo-model-comparison-v3`；
- `biogeo-model-averaged-ancestral-ranges-v2`；
- 现有 model-workflow 格式。

本次原始四端点结果的关键字段为：

```text
models                 6
eligible_models        6
aicc_defined_models    3
aicc_eligible_models   0
```

模型平均表中为 `criteria=1, aic_models=6, aicc_models=0`。
RASP5 应把 AICc 权重和排名的 `NA` 展示为不可用，不能因为有三个 AICc 分数就据此选“最佳模型”，
也不能把缺少 AICc 平均误报为全部分析失败。

重新运行：

```powershell
.\biogeo-cli.exe model-workflow --request <原workflow.tsv> --output-dir <新目录>
```

从崩溃时已写出六个拟合的目录恢复：

```powershell
.\biogeo-cli.exe model-workflow --request <原workflow.tsv> --output-dir <原结果目录> --resume
```

恢复需要原请求、参数和依赖文件未变；不要为恢复而修改已有结果或其指纹。
本次未改 RASP5 的 GUI、R 后端、安装器或原始复现目录，也未做新的 R 数值对照。
