# 树输入与化石末端契约

本文定义 Rust 引擎当前接受的树输入、古老末端的时间语义，以及 BioGeoBEARS
超短枝直接祖先特例。

## Newick 输入

`parse_newick()` 默认采用严格策略：

- 支持 UTF-8 的未引号标签；
- 支持单引号标签，因而可包含空格、逗号、冒号和括号；
- 单引号标签中的两个连续单引号表示一个字面单引号，例如
  `'O''Brien taxon'`；
- 支持并忽略平衡的方括号注释，包括嵌套注释；
- 保留 tip 标签和内部节点标签，CLI 输出优先使用输入的内部标签；
- 所有非根节点都必须有有限、非负的枝长；
- 缺失枝长默认报错，不再静默填成 `0.0`；
- 核心 API 只有显式传入
  `MissingBranchLengthPolicy::Fill(value)` 时才会填充缺失枝长；
- CLI 只有显式传入 `--fill-missing-branch-length <非负值>` 时才会采用相同填充策略；
- root edge 会明确报错，因为当前 likelihood 没有根上方的演化过程，不能静默忽略。

Newick 解析器可以保留多分叉结构，但 DEC 类 cladogenesis pruning 要求二叉内部节点，
遇到多分叉会明确报错，不会静默二叉化。

## NEXUS 树输入

CLI 和公共 `parse_tree_input()` 会按 `#NEXUS` 文件头自动识别 NEXUS，并继续把提取出的树
交给同一个严格 Newick 解析器。当前契约为：

- 支持大小写不敏感的单个 `BEGIN TREES` 块、一个或多个 rooted `TREE` 和可选默认树标记 `*`；
- 支持 `TRANSLATE`，包括单引号标签、双单引号转义、UTF-8 和嵌套方括号注释；
- `TAXA` 等非树块可以保留，分析只消费 `TREES` 块；
- 单树文件无需树名；多树文件默认报告所有树名并拒绝继续，只有显式传入
  `--tree-name <exact-name>` 才选择名称完全匹配的一棵树；
- 树名选择区分大小写，重复名称、空名称和不存在的名称均明确报错；
- `UTREE` 明确拒绝，因为当前 likelihood 必须知道根；
- `TRANSLATE` 中重复 alias、重复最终标签或树上未映射 alias 均明确报错。

`--tree-name` 已贯通校验、固定似然、全部优化器、通用参数表模型、分析结果重放和生物地理
随机历史运行指纹。结果只在显式选择时记录 `tree_name`，未选择树名的既有输出不变。

官方 `M3areas_allowed_wFossilBranch` 树已经由项目隔离环境中的 `ape::write.nexus()` 导出为
单树 `tree_ape.nex` 和多树 `tree_ape_multi.nex`。后者包含一个枝长控制树以及名称为
`official` 的原树。原 Newick、单树 NEXUS 和显式选择 `official` 后的固定 DEC、节点
posterior、split posterior 共 112 行模型语义输出逐字节一致，lnL 都是
`-3.365375376083962`；多树结果另有一行 `tree_name=official` 选择记录。可重复门禁为：

```powershell
powershell -ExecutionPolicy Bypass -File validation/checks/check-tree-input-equivalence.ps1
```

正式分析前可用 `biogeo-cli validate-inputs --tree <path> --ranges <path>` 运行同一解析路径；
多树文件同时传入 `--tree-name <exact-name>`，
并查看树规模、枝长、古老末端和直接祖先诊断。稳定字段及失败契约见
[`input-validation.md`](input-validation.md)。

## 显式树转换

`convert-tree` 把 Newick 或选定的 NEXUS `TREE` 输出为规范 Newick：

```powershell
target/release/biogeo-cli.exe convert-tree `
  --tree trees.nex `
  --tree-name official
```

命令只写标准输出，保留拓扑、子节点顺序、tip/内部标签和枝长，并应用 `TRANSLATE`。必要时
标签会用单引号表示，字面单引号写成两个单引号。NEXUS 块、注释、默认树标记及其他非树
元数据不会进入 Newick。转换默认不补枝长；只有显式加入
`--fill-missing-branch-length <非负值>` 才统一填充所有缺失的非根枝长。转换不会重新定年、
二叉化、随机放置化石或自动选择树；同一解析契约下无效的输入仍然失败。

该选项同样适用于 `validate-inputs`、`model-evaluate`、`model-optimize`、`model-batch` 和版本化
分析请求。请求键为 `missing_branch_length_fill`。它会进入分析计划、可移植结果元数据和
模型身份，并以十六进制浮点位模式辅助精确重放；未提供时明确记录为 `reject`。固定兼容命令
仍保持严格输入，必要时先用 `convert-tree` 生成规范 Newick。

D2 进程门禁额外覆盖 UTF-8 BOM、大小写混合 NEXUS 关键字、非树 `TAXA` 块、嵌套注释、
带空格树名、`TRANSLATE` 中带空格及转义单引号的标签、规范 TSV 中带空格的 tip/区域名，
以及 `UTREE` 和多分叉树的稳定拒绝诊断。分析完成后删除源输入，结果目录仍能按同一填充值
独立重放。

需要按年龄区间、stem/crown 和类群约束生成化石树时，使用独立的 `fossil-place` 命令；其
抽样规则、manifest 和可复现多树结果见
[`random-fossil-placement.md`](random-fossil-placement.md)。

## 古老末端

引擎以所有 tip 中最长的 root-to-tip 深度定义现在。节点年龄为：

```text
age(node) = max(root_to_tip_depth) - root_to_node_depth
```

因此 root-to-tip 路径较短的 tip 自然得到正年龄，并被视为已在该年龄采样的古老末端。
该 tip 的终端枝只传播到它自己的采样年龄；时期切分、生物地理随机历史事件和状态占据
时间都不会把这条枝补到现在。

官方验证案例来自 BioGeoBEARS
`examples/BSM_3taxa/M3areas_allowed_wFossilBranch`：

```text
((human:0.91,chimp:1):1,gorilla:2);
```

其中 `human` 年龄为 `0.09`。`0.1` 的时期边界把其 `0.91` 长终端枝切成
`0.90 + 0.01` 两段。固定 `d=0.1, e=0.2` 时：

- BioGeoBEARS lnL：`-24.673663554693370`；
- Rust lnL：`-24.673663590873062`，差 `3.62e-8`；
- 16 个节点状态概率最大差 `3.11e-9`；
- 16 个有效 split 概率最大差 `1.56e-9`，权重零差；
- 20,000 条 Rust 生物地理随机历史的节点/分裂经验分布最大 z 值 `1.60`；
- 每条历史的总枝长占据时间严格为 `4.91`，`human` 枝严格终止于其采样年龄。

`d/e` 优化也已覆盖。BioGeoBEARS `bobyqa` 返回
`d=8.61149989135817, e=1.58707661729099`，但报告 `KKT1=FALSE`；Rust 在该坐标固定
重算与 BGB lnL 差 `8.13e-7`，并找到高 `7.69e-5` 的似然点。因此优化门禁同时要求
BGB 终点可交叉重算，并只对该 manifest 案例显式允许 Rust 最优值高于 BGB；其他案例仍
执行双边 lnL 容差，不把这一例外扩散成全局放宽。

## 直接祖先不是普通化石 tip

BioGeoBEARS 可用 `min_branchlength` 把超短化石侧枝解释为直接祖先，并取消该位置的普通
物种形成事件。Rust CLI 用显式的 `--min-branch-length <x>` 启用同一语义：

- 判定条件与 BioGeoBEARS 1.1.3 源码一致，是子枝长度严格 `< x`，不是 `<= x`；
- 任一子枝命中阈值时，其父节点标记为直接祖先节点；
- 两条子枝仍按各自真实枝长传播 Q，节点处改为同状态逐元素相乘，不调用普通 split table；
- uppass/posterior 使用恒等节点关系，节点状态概率仍然输出；
- split posterior 和生物地理随机历史的 cladogenetic split 表不记录该节点；
- 生物地理随机历史在该节点把祖先状态同时复制为两条子枝的起始状态。

CLI 默认 `x=0`，用于保持已有普通短枝分析的兼容性；需要复现 BioGeoBEARS 默认行为时应
显式传入 `--min-branch-length 1e-6`。阈值和命中的节点/边会进入标准输出、结果目录元数据、
模型指纹和 `model-bsm` 重放检查。

BioGeoBEARS `runBSM()` 的抽样步骤同样在 hook 节点复制状态，但后续事件分类器会把这个
恒等关系显示为 `sympatry (y)`。这与其 likelihood 源码和“该处没有物种形成”的注释不一致。
Rust 保留节点状态和 hook 诊断，但不把它计入 `y/s/v/j`，避免把直接祖先伪装成分裂事件。

官方派生 fixture 从 `M3areas_allowed_wFossilBranch` 输入出发，用 BioGeoBEARS 1.1.3
`add_hook()` 在 `chimp` 谱系加入侧枝。`1e-7` 树触发直接祖先；第二棵树的侧枝恰为
`1e-6`，用于锁定严格小于的边界。固定 `d=0.1, e=0.2` 时：

- 直接祖先树 lnL 差 `3.97e-9`，节点 posterior 最大差 `1.04e-8`；
- 阈值相等控制树 lnL 差 `2.15e-8`，节点 posterior 最大差 `2.28e-9`；
- 非 hook 节点的 split posterior 最大差分别为 `8.87e-9` 和 `2.28e-9`，权重零差；
- `d/e` 优化都收敛到相同边界点，最优 lnL 差约 `2.98e-8`；
- 核心与 CLI 回归确认每条生物地理随机历史都复制 hook 状态且不增加 split 事件。

## 验证命令

```powershell
powershell -ExecutionPolicy Bypass -File validation/checks/check-rust-dec-fixtures.ps1 `
  -Manifest validation/state_constraint_fixtures.tsv -Command dec

powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-dec.ps1 `
  -Manifest validation/state_constraint_fixtures.tsv `
  -Golden validation/golden/biogeobears-state-constraints.tsv -Command dec

powershell -ExecutionPolicy Bypass -File validation/checks/check-fossil-tip-bsm.ps1

powershell -ExecutionPolicy Bypass -File validation/checks/check-rust-dec-fixtures.ps1 `
  -Manifest validation/direct_ancestor_fixtures.tsv -Command dec

powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-dec.ps1 `
  -Manifest validation/direct_ancestor_fixtures.tsv `
  -Golden validation/golden/biogeobears-direct-ancestor.tsv -Command dec
```

最后一个命令会用 release CLI 抽取 20,000 条生物地理随机历史，检查古老末端的时期边界、
占据时间和观测状态，再与 BioGeoBEARS 节点及 split posterior 做分布级对照。
