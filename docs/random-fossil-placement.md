# 年龄与类群约束下的随机化石放置

## 定位

`fossil-place` 是树预处理命令，不是似然模型的一部分。它根据年龄区间和类群约束生成一组普通
Newick 树；后续 `model-evaluate/model-optimize/model-batch` 仍在每棵固定树上运行同一套似然。
随机抽样不会藏进 pruning，也不会为了对齐 BioGeoBEARS 修改 lnL 公式。

实现语义已对照本地隔离安装的 BioGeoBEARS 1.1.3：
`get_possible_branches_to_add_fossils_to()`、`add_random_side_branch()`、
`add_random_direct_ancestor_hook()` 和 `add_fossils_from_xls_randomly()`。

## Manifest

```text
biogeo-fossil-placement-manifest-v1
fossil_id<TAB>min_age<TAB>max_age<TAB>attachment<TAB>stem_or_crown<TAB>clade_tips
F1<TAB>5<TAB>8<TAB>side_branch<TAB>crown<TAB>A,B,C
F2<TAB>2<TAB>3<TAB>direct_ancestor<TAB>stem<TAB>D
```

- 年龄单位与树枝长一致，`0 <= min_age <= max_age`；固定年龄可令两者相等。
- `attachment` 为 `side_branch` 或 `direct_ancestor`；兼容读取 `ancestor` 别名。
- `stem_or_crown` 为 `stem/crown/both`。
- 一个 tip 的约束只有 stem；`crown` 会明确报错。两个以上 tip 先求 MRCA：stem 是进入 MRCA
  的枝，crown 是 MRCA 以下全部后代枝，both 是两者并集。根类群没有 stem。
- `clade_tips` 用逗号分隔。标签中的 `%`、制表符、回车、换行和列表内逗号分别写为
  `%25/%09/%0D/%0A/%2C`。
- 化石按 manifest 顺序放置，后续行可以在 `clade_tips` 中引用已经放入树中的化石。
- 化石标签不得与原树或其他化石 tip 重复。

## 抽样规则

普通 side branch 先在用户年龄区间与约束枝可行年龄的交集中均匀抽化石年龄。给定年龄后，每条
候选枝的权重等于该枝上比化石更老、仍可作为连接点的长度，再在选中区段内均匀抽连接年龄。
这与 BioGeoBEARS 的 side-branch 核心权重一致，同时避免先抽到不可能年龄后静默跳过该化石。

direct ancestor 先抽化石年龄，再从跨过该连接年龄的约束枝中按枝长抽枝。输出树用短侧枝 hook
表示直接祖先。默认 hook 长度为 `1e-7`，本实现令：

```text
attachment_age = fossil_age + hook_length
fossil_branch_length = hook_length
```

因此输出 fossil tip 的年龄精确等于抽样年龄。BioGeoBEARS 原函数在化石年龄处连接再加短枝，tip
会年轻一个 hook 长度；这里保留相同直接祖先拓扑语义，但修正这一个可审计的时间偏移，而不是为
逐字节对齐制造特殊分支。

## CLI 与结果

```powershell
biogeo-cli fossil-place `
  --tree tree.nwk `
  --manifest fossils.tsv `
  --output-dir fossil-placement-result `
  --replicates 1000 `
  --seed 20260723 `
  --direct-ancestor-hook-length 1e-7
```

NEXUS 和 `--tree-name` 使用与似然入口相同的严格树解析器。输出目录不可覆盖，并通过同一父目录的
暂存目录原子发布：

```text
fossil-placement-result/
  metadata.tsv
  source-tree.nwk
  source-manifest.tsv
  placements.tsv
  trees/
    tree-000001.nwk
    tree-000002.nwk
```

格式号为 `biogeo-fossil-placement-set-v1`。`placements.tsv` 记录每个 replicate、化石年龄、
连接年龄、hook/侧枝长度、约束类型及选中枝的后代类群。每个 replicate 从
`master_seed` 与 `replicate_index` 的稳定混合值独立派生 ChaCha12 seed；增加 replicate 数不会改变
已有编号的树。

对含 `direct_ancestor` 的结果运行似然时，必须显式设置
`--min-branch-length` 大于 hook 长度，例如 hook 为 `1e-7` 时使用 `1e-6`。否则它只是一棵带
极短普通侧枝的树。结果机器契约见
`schemas/biogeo-fossil-placement-set-v1.schema.tsv`，可运行示例见
`examples/fossil_placement/`。
