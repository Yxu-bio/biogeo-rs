# Psychotria 多模型工作流验收 fixture

输入树和范围矩阵来自 BioGeoBEARS 官方 Psychotria M4 示例。该 fixture 使用统一参数框架的六个
preset，先由 `workflow-stop.tsv` 在模型比较后以 0 秒随机历史预算停止，再由
`workflow-resume.tsv` 恢复。两份请求仅执行时间预算不同，身份指纹必须相同。

