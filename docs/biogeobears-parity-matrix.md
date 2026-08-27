# BioGeoBEARS 功能兼容矩阵

## 基准版本

- BioGeoBEARS 版本：`1.1.3`
- 本地隔离源码提交：`7d2092f94a5d2b598807771379ef6c58a84b4fb3`
- 默认参数运行时导出：`validation/biogeobears/export-biogeobears-parameter-table.R`
- 逐参数机器可读矩阵：`validation/biogeobears-parameter-parity.tsv`

兼容目标是用户可见的统计与工作流能力，不是逐行翻译 R 内部辅助函数。若 BioGeoBEARS
参数表声明了参数，但源码或运行时没有实际消费，则先标记为“待核实”，不能把参数名存在
直接计为功能完成；完成源码和运行时审计后，可标记为版本限定的“兼容空操作”。

本矩阵评估 Rust 命令行计算引擎。图形、祖先范围可视化和报告呈现由新版 RASP 负责，
不作为 CLI 兼容缺口；模型平均等会改变数值结果的计算仍属于本矩阵。

## 状态定义

- **已实现**：计算路径、公共入口和 BioGeoBEARS 对照均存在。
- **部分实现**：底层数学或专用入口存在，但通用参数表、组合验证或用户入口尚未闭合。
- **待实现**：当前 Rust 路径没有等价语义。
- **待核实**：BioGeoBEARS 自己的有效行为尚不明确，先用运行时案例确认。
- **兼容空操作**：已证明冻结的 BioGeoBEARS 版本不消费该参数；保留表结构，但拒绝制造
  无意义的自由优化维度。

## 1. 通用参数表

状态：**已实现主体**。

本阶段已新增：

- `ParameterSpec`：参数名称、优化边界、模式和 `Linear/Log/Logit` 优化坐标。
- `Fixed`、`Free`、`Derived` 三种模式。
- 受限的 `+ - * /`、括号、常数和参数引用表达式。
- 未知引用、重复名称、循环依赖、除零、非有限值、自由参数越界和联动结果越界检查。
- 稳定的自由参数向量顺序以及固定、释放、重新联动 builder。
- BioGeoBEARS 默认 23 行参数和六个正式 preset 参数表。
- 从解析值构造当前统一 `ModelConfig` 的核心适配器。
- 动态维度 Nelder-Mead 优化器：直接按参数表发现任意数量的 `Free` 参数，按声明的坐标
  变换搜索，并在每次似然计算前解析全部固定值和联动表达式。
- 模型值空间的显式多起点、稳定自由参数顺序、逐参数边界分类，以及包含最终
  `ResolvedParameters`、`ModelConfig` 和 pruning 结果的统一返回值。
- `biogeo-analysis-result-v2` 原子非覆盖结果目录，保存原始/冻结参数表、自包含输入、lnL
  位值、优化诊断和 `biogeo-model-identity-v1`。
- `model-bsm` 对结果执行输入、状态空间、模型身份和 lnL 重放校验，再复用既有生物地理
  随机历史 sampler、确定性并行、检查点和分片 writer。

六个参数表已经与原有 DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE、
BAYAREALIKE+J preset 做结构等价回归。此检查锁定的是 `d/e`、`y/s/v/j` 和
`mx01y/s/v/j`。此外，`a/b/w` 已进入同一模型构造器和动态优化器：官方 Psychotria M4
树/范围上的 10 点固定 profile（含联合点）与 BioGeoBEARS 最大 lnL 差为 `3.33e-7`；
`a` 的 Q 与生物地理随机历史原子事件分类也有内部回归。

`mf/dp/fdp` detection/observation likelihood 也已进入通用参数表。官方 Psychotria
案例锁定了 8 个固定 profile 点、2432 个 tip-state 相对似然值以及单参数和三参数联合
优化；Rust 在每个优化候选点重新生成末端似然，不使用只在初值计算一次的静态权重。
此外，四个固定跨模块点已覆盖静态 `x/n/u`、非默认 `y/s/v/j + mx01*`、全栈静态修饰和
官方五时期输入；`x/j/y/v/mf` 五维联合优化在 BioGeoBEARS 最优点固定交叉重算后，也由
Rust 显式 3 起点搜索达到同一最优 lnL。该联合优化现已扩展到官方重复五时期输入：R 与
Rust 使用相同三起点策略，静态等价/直接 BGB 最佳 lnL 相差 `1.84e-6`，Rust 两条目标路径
在严格参考点相差 `2.55e-12`。
组合祖先范围 posterior 与 split posterior 也已闭合；官方重复五时期输入使用数学等价
静态 BGB posterior 作为严格参考，并额外要求 Rust 分时期结果与静态结果一致，不复制
已记录的 BioGeoBEARS stratified uppass 缺陷。

进一步的受约束全栈 Psychotria 案例同时启用了五时期 `x/n/u/w`、逐期状态空间、非默认
`d/e/a/b`、`y/s/v/j + mx01*` 和 `mf/dp/fdp`。固定 lnL 差为 `7.15e-7`；288 个
fixnode 祖先状态和 408 个校正 split 项的最大概率差分别为 `9.30e-8` 和 `9.39e-8`。
`d/e/x/n/u` 五维优化通过返回点固定重算，20,000 条生物地理随机历史也通过精确分布门禁。
详见 [`detection-full-stack-validation.md`](detection-full-stack-validation.md)。

通用优化器已经与原有 DEC `d/e`、DEC+J `d/e/j` 专用优化器交叉回归；相同起点和边界下，
最终参数、lnL 与 pruning 结果一致。另有自定义 `y/v` 自由、`s=y/2` 联动和两起点回归，
证明节点分裂参数不再依赖硬编码的 `d/e[/j]` 维度。独立释放
`y/s/v/mx01/mx01y/mx01s/mx01v/mx01j` 的 BioGeoBEARS 优化与 240 个固定剖面点
也已通过；其中 MaxEnt 对照复刻 `rexpokit::maxent` 的停止容差和三位舍入，并区分
连续点估计与台阶平台。

`mx01r` 审计已经闭合：BioGeoBEARS 1.1.3 只定义该参数并标注 `note=no`，实际 root prior
入口始终为 `NULL`；复杂静态和官方五时期运行时扰动也全部为零差。Rust 因此保留默认
`fixed(0.5)` 兼容行并拒绝释放，详见 [`mx01r-audit.md`](mx01r-audit.md)。新增 `a`
汇总列已在 BSM v2 显式发布，包含 `a`、困难分支诊断和状态约束审计；
`biogeo-bsm-tsv-v1` 仍保持原表头。

## 2. 修饰组合

状态：**已实现核心语义与全栈组合门禁**。

已对齐距离 `x`、环境距离 `n`、面积 `u`、手工扩散矩阵、按区域局部灭绝、时期分层、
`areas_allowed`、邻接约束、founder-event 有向权重和 `mx01*`。现有 golden 覆盖固定
似然、节点后验、分裂后验和主要优化路径。

六 preset 公共入口组合矩阵进一步让每个正式 preset 各运行静态与两时期全修饰任务，共
12 项固定似然、祖先/分裂后验、可移植结果和重放；另冻结 12 项缺失原始输入、重复来源、
分时期 `b`、`mx01r` 和无节点事件等拒绝规则。该门禁验证统一组合器没有只在 DEC 路径生效，
但不替代下述 BioGeoBEARS 单因素与全栈数值 golden。

验收采用“单因素 + 两两覆盖 + 全栈案例”，不枚举无限数值组合。全栈案例现已同时启用
时期、距离、环境、面积、状态约束、`j` 和非默认 `mx01*`，并完成 lnL、两类后验、
`d/e/x/n/u` 优化和生物地理随机历史分布对照。邻接约束等单项仍由独立 golden 锁定；后续
新增修饰类型时必须扩展同一组合策略，不能把“已有全栈案例”解释成对未来参数自动兼容。

## 3. 化石和非现生末端

状态：**已实现**。

当前以最长 root-to-tip 深度定义现在，较短路径的 tip 得到正年龄。官方
`M3areas_allowed_wFossilBranch` 案例已覆盖固定似然、`d/e` 优化、节点后验、split 后验和
20,000 条生物地理随机历史。化石终端枝按自身采样年龄结束，不会被补齐到现在。

`--min-branch-length` 已实现 BioGeoBEARS 严格 `< min_branchlength` 的微小侧枝约定。
直接祖先节点使用恒等状态关系，保留节点 posterior，但不生成 split posterior 或
`y/s/v/j` 随机历史事件。官方三物种案例派生的 `1e-7` hook 与恰好 `1e-6` 的边界控制已
覆盖固定 lnL、节点/split posterior、`d/e` 优化和 BSM 结构不变量。

独立 `fossil-place` 命令还支持年龄区间、stem/crown/both、MRCA 类群约束、side branch、
direct-ancestor hook 和确定性多 replicate。随机放置生成候选树，不混入给定树的固定似然。

化石末端是已观察到的古老样本，不等于出生死亡模型中的谱系灭绝。
详细契约和数值见 [`tree-input-and-fossil-tips.md`](tree-input-and-fossil-tips.md)。

## 4. 树与范围输入

状态：**CLI 核心输入与显式迁移已实现**。

当前支持 UTF-8 标签、Newick 单引号标签及双单引号转义、平衡方括号注释、内部标签、
非负枝长、严格 `0/1` TSV 范围表，以及通过 `--use-ambiguities` 显式启用的 `0/1/?`
范围表。`1/0/?` 分别表示必含、必不含和无约束；该观测层为每个 tip 生成任意状态 likelihood，
固定推断、后验、优化、结果重放和生物地理随机历史共用同一路径。ambiguity 与 detection
观测模型明确互斥。缺失非根枝长默认报错，核心 API 只有显式选择填充值时才继续；
root edge 因没有根上方过程而明确拒绝。多分叉可解析但 likelihood 明确拒绝，不静默二叉化。
单树 NEXUS、`TRANSLATE`、APE `write.nexus()` 输出、多树精确名称选择和明确的默认多树/
`UTREE` 拒绝已接入所有 CLI 分析及结果重放路径。`convert-tree` 可显式输出规范 Newick；
缺失枝长只有在用户提供统一填充值时才会补齐，不做其他自动修复。`validate-inputs` 已用正式
解析路径检查树/范围对应、二叉性、枝长、古老末端
和直接祖先；范围表重复区域、重复/缺失 tip 也在解析层拒绝。BioGeoBEARS 底层函数支持的
全未知和纯 absence-only 语义已直接锁定，Rust 不复制标准 BGB 文件入口对它们的前置拒绝。
`.data`、常见 CSV 和规范 TSV 可进入同一路径；显式 taxon/area 映射、BioGeoBEARS block
时期转换、all-pairs adjacency 与脚本自定义 allowed-state list 已接入。完整契约见
[`ambiguous-ranges.md`](ambiguous-ranges.md) 和 [`legacy-input-import.md`](legacy-input-import.md)。

## 5. 数据整理、批处理与数值模型比较

状态：**部分实现**。

计划按依赖顺序提供：

1. `validate-inputs` 的精确/不确定范围诊断和显式 `convert-tree` 已完成，均不做静默自动修复。
2. 已完成“一个共享数据配置 × 多张模型参数表”的 manifest 批量优化、原子逐模型结果、
   首错后继续、不可变失败 attempt 和恢复；`dataset-batch` 已进一步支持每组独立数据、树、
   观测和修饰配置，并确保模型权重只在数据集内部计算。跨进程并发调度尚未实现。
3. 已完成 lnL、自由参数数目、AIC/AICc、delta、Akaike weight 和 rank；v3 比较表还会从
   23 参数表达式证明全部有向模型对的嵌套关系，并对可用严格嵌套对计算带边界说明的
   likelihood-ratio test。
4. 已完成 AIC/AICc 模型平均祖先范围和 cladogenetic split scenario 机器结果；严格重放最终
   posterior，按完整候选集权重累加。split 以 ancestor/ordered daughters/event 稳定键取并集，
   缺失情景概率为 0，并由 BioGeoBEARS 两模型 golden 与官方 Psychotria 六模型门禁覆盖。

祖先范围、分裂状态、逐区域概率、时期边界和生物地理随机历史的可视化，以及 HTML、
SVG/PNG 报告属于新版 RASP 的展示层，不在 Rust CLI 内重复实现。

版本化 `AnalysisResult` 已升级为 `biogeo-analysis-result-v2`，并已接通通用参数结果
到生物地理随机历史。批量与模型比较契约见 [`model-batch.md`](model-batch.md) 和
[`dataset-batch.md`](dataset-batch.md)，新版 RASP 的宿主无关进程边界见
[`rasp-cli-integration.md`](rasp-cli-integration.md)。Linux/Slurm 资源
发现和跨进程 writer 协调仍属于后续服务器工程阶段。可移植输入打包和 v1→v2 迁移已完成；
Windows MSVC v3 发布目录/ZIP、非覆盖安装和 32 项新版 RASP schema 契约也已有端到端门禁；
公开 GitHub 科研版本还需最终项目许可证、第三方许可说明、版本文档和 SHA-256。
代码签名和 CI 是可选增强，不是发布条件。
单模型工作流现可由 `biogeo-analysis-request-v1` 统一描述；`analysis-plan` 在不执行完整似然的
前提下解析模型规模与资源风险，`analysis-run` 继续生成原有 v2 可移植分析结果；
`analysis-workflow` 进一步把分析、随机历史和结果检查串联起来，但不改变任何模型公式。
状态空间现在可在分配前精确计算组合状态数；可选 `max_states` 资源上限贯通单模型、模型批次、
数据批次和多模型工作流，超限返回稳定机器错误，不设置固定的软件上限。
多模型层现由 `biogeo-model-workflow-request-v1` 描述候选 manifest、共享数据配置、AIC/AICc
准则和可选随机历史目标；`model-workflow-plan` 与 `model-workflow` 复用既有 model-batch、模型
平均、model-bsm 和检查器。随机历史来源必须明确为模型 ID 或显式准则策略，rank-1 并列不会
按 manifest 顺序静默选择。

## 完成判据

每项功能必须依次通过内部结构检查、固定参数 BioGeoBEARS 对照、后验对照、优化交叉重算
和必要的随机分布检验。不得根据 fixture 名称、区域数、树大小或参考工具名称切换公式。
