# 输入验证契约

`validate-inputs` 在拟合、优化或生物地理随机历史采样之前，只读检查树与范围表。`--ranges`
可直接读取规范 TSV、BioGeoBEARS/LAGRANGE `.data` 和带 `Name/tip/taxon/species` 列的 CSV；
三种格式进入同一个严格的树名对应检查，不按相似名称自动替换。默认要求精确 `0/1`；显式加入
`--use-ambiguities` 后使用 BioGeoBEARS 的 `0/1/?` 约束语义：

```powershell
target/release/biogeo-cli.exe validate-inputs `
  --tree tree.nex `
  --tree-name selected_tree `
  --ranges ranges.tsv `
  --use-ambiguities `
  --fill-missing-branch-length 0.25 `
  --min-branch-length 0.000001
```

成功输出以 `format=biogeo-input-validation-v1` 和 `status=valid` 开头，稳定报告：

- Newick/NEXUS 格式及 NEXUS 树名；
- tip、内部节点、边和二叉性；
- 根年龄、总/最小/最大枝长、零长度枝和是否超度量；
- 实际末端年龄容差及其为自动还是显式模式；
- 古老末端数量与逐 tip 采样年龄；
- 区域名、范围行数、最大观测范围大小和 null range 数量；
- ambiguity 模式下的未知 tip 数、`?` 单元格数、全未知 tip 数和最大可能范围大小；
- `--min-branch-length` 阈值、直接祖先节点和命中边明细。
- 缺失枝长策略；默认输出 `missing_branch_length_fill=reject`，显式填充时记录精确数值。

`--tree-name` 仅在多树 NEXUS 中需要，并要求名称完全匹配；单树 Newick/NEXUS 可省略。
验证使用与正式分析完全相同的树选择、范围观测解析和直接祖先标记逻辑。它不重新定年、
不二叉化、不自动选择多棵 NEXUS 树，也不修改标签。缺失枝长默认拒绝；只有显式使用
`--fill-missing-branch-length <非负值>` 才填充，并与正式分析采用同一策略。

默认末端年龄容差为 `root_age * 1e-9 + 1e-12`，只用于诊断由十进制枝长舍入造成的极小
root-to-tip 差异，不修改节点年龄、枝长、似然或生物地理随机历史。输出会记录
`tip_age_tolerance` 和 `tip_age_tolerance_mode`；需要逐位严格检查时使用
`--tip-age-tolerance 0`。普通化石或非现生末端仍按原始采样年龄报告。

以下情况直接失败并返回具体位置、标签或节点：

- Newick/NEXUS 语法错误、缺失枝长、root edge、未显式选择的多树、未知/重复树名或 `UTREE`；
- 非二叉内部节点；
- 范围表重复区域名、未知/重复 tip、缺失 tip、列数错误，或当前模式不允许的单元格值；
- ambiguity 约束在指定状态空间中没有任何兼容范围。

外部范围和时期文件的显式转换见 [`legacy-input-import.md`](legacy-input-import.md)。

detection 数据使用独立观测模型入口，不能与 `--use-ambiguities` 混用，也不会被悄悄转换
成范围表。完整 `?` 契约和 BioGeoBEARS 标准工作流的输入限制见
[`ambiguous-ranges.md`](ambiguous-ranges.md)。
