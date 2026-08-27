# Ponerinae 32-tip、7-area 验收 fixture

`tree.nwk` 是用户提供的 1534-tip Ponerinae short-name MCC 树的真实诱导子树，`ranges.tsv`
来自对应 7 区域矩阵。`provenance.json` 固定源文件 SHA-256、选择规则和 32 个 taxon；
`make-ponerinae-subset.py` 可重建它，但正式验收不依赖 Python、ETE、新版 RASP 目录或 `E:` 盘。
原始数据来自 Doré et al. (2025) 的公开仓库，论文引用、上游修订号和 MIT 许可全文见
[`LICENSE-NOTICE.md`](LICENSE-NOTICE.md)。

选择先保证每个区域至少一个代表和至少一个广布末端，再按枝长距离进行最远点遍历。诱导子树保留
合并枝长的 17 位浮点精度，因此仍是二叉、超度量树。

本目录同时提供六模型工作流请求。`workflow-stop.tsv` 在拟合和比较完成后以 0 秒随机历史预算
停止，`workflow-resume.tsv` 只将预算改为 120 秒并恢复 4 条生物地理随机历史。
