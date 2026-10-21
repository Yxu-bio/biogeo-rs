# RASP5 接入 biogeo-rs 全面手册

> 2026-09-30 开发版 `0.1.7` 新增 [v2 条件路线概率](bsm-joint-route-probabilities.md)。
> `bsm-assign-sources --allocation-mode probabilities` 支持多区域 founder，候选概率单独成表，
> 按原事件 ID 连接；默认 sample 模式保持 v1。

> 2026-09-28 开发版另增加 [BSM v3 逐事件输出](bsm-v3-contract.md)，包括实际分裂类别、
> 未抽样来源声明、节点年龄和子节点顺序。full/compact 可用于逐事件展示，summary 不含路径。

> 2026-09-27 开发版更新：已增加单模型祖先概率、显式节点/边映射及独立分裂概率文件。
> 新结果为 `biogeo-analysis-result-v3`，后验为 `biogeo-single-model-posteriors-v1`。
> 接入新构建请先阅读[单模型后验接口](single-model-posteriors.md)。下文以已发布 v0.1.0
> 为基线的缺口描述仍适用于旧 exe，不能据此判断新开发构建；以 `engine-info` 为准。

本文给 RASP5 开发者说明 `biogeo-rs` 的科学定位、进程接口、输入输出、任务生命周期、
结果导入、资源控制和当前边界。它不是命令行入门教程，也不要求 RASP5 复用旧版 RASP 的
任何内部代码。

命令行的逐步使用方法见[完整 CLI 教程](cli-tutorial.md)。本手册回答的是另一组问题：

- RASP5 应把引擎当成什么；
- 哪些接口已经稳定，可以直接依赖；
- 怎样启动、监视、取消和恢复子进程；
- 怎样把模型、参数和结果映射到 GUI；
- 哪些内容不能从日志或文件名猜测；
- 在完整接入前，`biogeo-rs` 还必须补哪几个机器接口。

## 1. 接入基线

本文以以下公开版本为基线：

| 项目 | 值 |
|---|---|
| 引擎 | `biogeo-cli 0.1.0` |
| Git 标签 | `v0.1.0` |
| Git 提交 | `bbf24f4` |
| 已验证平台 | 64 位 Windows，`x86_64-pc-windows-msvc` |
| 发布包 | `biogeo-cli-0.1.0-x86_64-pc-windows-msvc.zip` |
| 发布包 SHA-256 | `724d46fbb2d9a30159b42faa5518ca916bfeb4c4f14561ebda3e2f242003cfa6` |
| 许可证 | `GPL-3.0-or-later` |

GitHub 发布页：
[v0.1.0](https://github.com/Yxu-bio/biogeo-rs/releases/tag/v0.1.0)。上面的 SHA-256 只适用于
这个确切文件，未来版本必须读取各自随包发布的 `.sha256`，不能沿用旧值。

当前 Windows 包已经完成源码测试、科学 golden、安装后真实进程测试和 RASP 参考宿主测试。
Linux、Slurm、cgroup 和服务器资源发现尚未达到同等验证强度。RASP5 的第一轮集成应以
Windows 子进程为目标，不要把 Windows 测试结论自动外推到 Linux。

## 2. 一句话定位

`biogeo-rs` 是一个独立的历史生物地理计算引擎。RASP5 通过启动 `biogeo-cli.exe`、写入
版本化请求文件、读取版本化结果目录来使用它。

它不是：

- BioGeoBEARS R 包的运行时包装器；
- LAGRANGE-ng 的包装器；
- 需要 R、Python 或 PowerShell 才能运行的混合引擎；
- RASP5 内部必须链接的 Rust DLL；
- GUI、绘图或报告生成器。

普通用户运行发布版时只需要完整的 Windows 发布目录。仓库里的 R、PowerShell 和 Python
用于科学对照、打包和维护，不是引擎运行依赖。

## 3. 科学背景

### 3.1 统一框架，而不是六套独立算法

引擎把历史生物地理模型拆成共同组件：

```text
末端观测 + 状态空间
        |
        v
沿枝过程 Q：d/e/a、时期、距离、环境、面积和扩散倍率
        |
        v
节点分裂过程 C：y/s/v/j 与 mx01y/s/v/j
        |
        v
同一套 pruning、参数优化、祖先后验和生物地理随机历史
```

DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE 和 BAYAREALIKE+J 只是参数 preset。
RASP5 不应为六个名称实现六套调用器；它只需要一套参数表编辑器和一套任务提交器。

### 3.2 主要参数的含义

| 参数 | 含义 |
|---|---|
| `d` | 沿枝范围扩张或扩散速率 |
| `e` | 沿枝局部灭绝或范围收缩速率 |
| `a` | singleton range 之间的范围切换速率 |
| `b` | 非分时期模型中的枝长指数 |
| `x` | 地理距离指数 |
| `n` | 环境距离指数 |
| `w` | 手工 dispersal multiplier 的指数 |
| `u` | 区域面积指数 |
| `y` | range-copying 节点事件权重 |
| `s` | subset sympatry 节点事件权重 |
| `v` | vicariance 节点事件权重 |
| `j` | founder-event 节点事件权重 |
| `mx01y/s/v/j` | 对相应节点事件的 daughter range 大小分布进行控制 |
| `mf/dp/fdp` | detection 末端观测模型参数 |

参数表还包含 `ysv`、`ys` 和 `mx01` 等联动中间量。RASP5 必须保留用户提交的
`fixed/free/derived` 关系，不能只保存最终数值。

### 3.3 六个 preset 的关系

下表只用于帮助理解。真正默认值应从引擎的 `parameter-template` 或随包参数表取得，RASP5
不要在代码里复制一份长期维护的默认参数。

| preset | 节点过程概要 |
|---|---|
| DEC | `j=0`，启用 BioGeoBEARS DEC 的 `y/s/v` 关系 |
| DEC+J | 在 DEC 关系上释放 `j`，其余权重随 `j` 联动 |
| DIVALIKE | `y=1, s=0, v=1, j=0`，使用对应的 vicariance range-size 设置 |
| DIVALIKE+J | 释放 `j`，`y` 与 `v` 随 `j` 联动 |
| BAYAREALIKE | 只保留 exact range-copying：`y=1, s=v=j=0` |
| BAYAREALIKE+J | 在 BAYAREALIKE 上释放 founder event |

这些权重会在每个祖先范围的合法 split scenario 中归一化。不能把 `y/s/v/j` 简单解释为
跨整棵树直接可比的事件概率。

### 3.4 外部参考的角色

项目使用两个外部参照，但角色不同：

- BioGeoBEARS 1.1.3 是 BioGeoBEARS-like 统一框架语义的 golden；
- LAGRANGE-ng 是独立 LAGRANGE-ng 语义与性能参考，不决定 Rust preset 是否通过。

引擎不会为了最后几位小数加入案例特判，也不会把 LAGRANGE-ng 的 split 规则混入
BioGeoBEARS preset。已知的 BioGeoBEARS 上游不一致会记录为差异证据，而不是原样复制错误。

## 4. 已实现能力与职责边界

### 4.1 Rust 引擎负责

- Newick/NEXUS 和末端观测的严格解析；
- 状态空间和时期允许状态；
- 沿枝 Q、节点 split scenario 和树上似然；
- 固定参数评估及任意有效自由参数组合的优化；
- 祖先范围概率和节点分裂概率的计算；
- 六个 preset；
- AIC、AICc、Akaike weight、模型平均、嵌套关系和 LRT；
- 随机化石放置；
- 生物地理随机历史的采样、分片、检查点和恢复；
- 自包含分析结果、输入包、格式检查和科学重放；
- 稳定错误码、进度事件和退出码。

### 4.2 RASP5 负责

- GUI、项目文件和任务队列；
- 输入文件选择和参数编辑；
- 调用子进程及保存原始 stdout/stderr；
- 读取 schema 后把结果转换成 RASP5 内部对象；
- 树、祖先范围、节点分裂、时期和生物地理随机历史的可视化；
- 图形导出、表格导出和报告排版；
- 用户友好的科学说明、警告和错误展示；
- 管理引擎版本、项目迁移和结果归档。

RASP5 不应重新计算 lnL、AIC、模型权重、posterior 或随机历史，也不应在 GUI 侧复制一套
状态空间或节点分裂公式。

## 5. 为什么首选子进程，不直接链接 Rust

v0.1.0 已冻结的是 CLI 进程行为和版本化文件，不是 `biogeo-core` 的 Rust ABI。子进程边界有
以下实际好处：

- RASP5 可以使用任意 GUI 语言或框架；
- 引擎升级可以按目录并存，不要求 RASP5 与 Rust 编译器 ABI 同步；
- 长任务崩溃不会直接破坏 GUI 进程；
- 请求和结果本身就是可审计的科研记录；
- 用户可在 RASP5 外复现同一命令；
- 取消、超时、检查点和恢复已有真实进程测试。

因此第一版不要做 DLL、FFI、Rust static library 或把 `biogeo-core` 源码拷入 RASP5。只有未来
确认子进程开销成为真实瓶颈，并另外设计稳定 C ABI 后，才值得讨论 FFI。

## 6. 引擎包的部署方式

### 6.1 必须保留完整目录

不要只复制 `biogeo-cli.exe`。RASP5 至少要一起保存：

```text
biogeo-cli-0.1.0-x86_64-pc-windows-msvc/
  biogeo-cli.exe
  package.tsv
  files.tsv
  release-status.tsv
  build-info.tsv
  engine-source-manifest.tsv
  schemas/
  docs/
  examples/
  LICENSE
  THIRD-PARTY-NOTICES.md
  third-party-licenses/
```

`schemas/registry.tsv` 是 RASP5 的机器接口目录。只复制 exe 会失去宿主需要核对的 schema，
也会让许可证和构建来源与二进制分离。

### 6.2 建议在 RASP5 内按版本并存

```text
RASP5/
  engines/
    biogeo-rs/
      0.1.0/
        windows-x86_64/
          biogeo-cli.exe
          schemas/
          ...
```

升级时安装新目录，完成握手和冒烟测试后再切换默认版本。不要原地覆盖旧 exe，因为旧项目可能
需要旧引擎重放。项目记录至少应保存引擎名、软件版本、构建平台、包 SHA-256 和使用的 artifact
格式号。

### 6.3 公开分发

项目采用 `GPL-3.0-or-later`。RASP5 若捆绑并分发该引擎，应保留许可证、第三方说明、版本信息和
对应源码获取方式。`CITATION.cff` 可用于生成引用提示。代码签名不是科研发布的必要条件；
v0.1.0 包未签名，但有 ZIP SHA-256 和逐文件清单。

## 7. 启动前握手

RASP5 每次选择或更换引擎版本后，应先直接启动：

```powershell
biogeo-cli.exe engine-info
```

不要解析 `--help` 来判断能力。`engine-info` 的 stdout 是
`biogeo-engine-capabilities-v1`，至少检查：

1. `status=ready`；
2. `engine=biogeo-cli`；
3. `engine_version` 是 RASP5 允许的版本；
4. `compatibility_policy_version=biogeo-compatibility-policy-v1`；
5. `unknown_format_policy=reject`；
6. `unknown_field_policy=reject`；
7. `build_os/build_arch/pointer_width` 与当前系统匹配；
8. 本次任务所需命令出现在 `recommended_commands`；
9. 所需 artifact 出现在 `public_formats`；
10. `public_formats` 与同目录 `schemas/registry.tsv` 精确一致。

`available_parallelism` 是当前进程看到的可用并行度，只是运行信息，不是永久硬件配置。不要在
RASP5 中写死本机的 16 个逻辑核心。

握手失败时应阻止新任务启动，但仍允许用户浏览已经导入的旧结果。

## 8. 子进程调用规则

### 8.1 不要拼接 shell 命令字符串

RASP5 应直接使用操作系统的进程 API，并把可执行文件和参数数组分开传入。这样可以正确处理
中文目录、空格、引号和特殊字符，也避免把用户提供的文件名解释成 PowerShell 或 `cmd.exe`
语法。

概念上应当调用：

```text
executable = <绝对路径>/biogeo-cli.exe
arguments  = [
  "--error-format", "tsv",
  "--progress-format", "tsv",
  "analysis-plan",
  "--request", <绝对路径>/analysis.tsv
]
```

不要生成：

```text
cmd.exe /c "biogeo-cli.exe ...用户路径..."
```

`--error-format` 和 `--progress-format` 是全局选项，必须位于子命令之前。RASP5 应同时异步读取
`stdout` 和 `stderr`；如果只读一个管道，输出较多时可能因另一个管道写满而死锁。两路内容都
按 UTF-8 解码。

### 8.2 三条数据通道

| 通道 | 用途 | RASP5 处理方式 |
|---|---|---|
| `stdout` | 成功结果或计划的机器格式 | 根据首行格式号和 schema 解析 |
| `stderr` | 机器进度、最终机器错误及少量诊断 | 按每条记录的格式号分流 |
| `stdin` | 仅用于支持交互控制的长时间随机历史任务 | 由任务控制器独占，不与普通输入复用 |

退出码是最终进程状态，不能仅凭 stdout 出现 `status=...` 就判定任务成功。成功时也要先确认
退出码为 `0`，再导入结果目录。

### 8.3 路径和工作目录

统一分析请求中的相对路径以请求文件所在目录为基准，不以 RASP5 当前目录为基准。RASP5
最好仍传入请求和输出目录的绝对路径，并为每次运行创建独立、受控的工作目录。不要让两个任务
写入同一个输出目录，也不要让重试覆盖旧结果。

用户选择文件后，RASP5 应保存原始路径用于界面显示，但引擎运行和项目重放应依赖引擎生成的
自包含结果目录。完成后的结果目录可以整体移动；不能只取出其中一个 `metadata.tsv`。

## 9. 机器格式与 TSV 解析

### 9.1 四种结构

RASP5 至少需要实现四种通用解析器，而不是为每个命令写临时字符串切割：

| `artifact_kind` | 结构 | 示例 |
|---|---|---|
| `key_value_stdout` | 无表头的 `key<TAB>value` stdout | `engine-info`、`analysis-plan` |
| `key_value_file` | 首行为 `key<TAB>value` 的文件 | `analysis.tsv`、多数 `metadata.tsv` |
| 普通 TSV 表 | 一行表头，之后为同列数记录 | `states.tsv`、`nodes.tsv` |
| `sectioned_tsv` | 格式号后包含多个命名表段 | 模型比较、模型平均 |

目录型 artifact 还要按照对应 schema 检查必需文件、文件格式和目录关系。不要根据文件名猜测
含义，也不要把所有 `.tsv` 都当成同一种表。

### 9.2 字段转义

机器输出使用真实制表符分列。字符串中的特殊字符按以下规则编码：

| 原字符 | 编码 |
|---|---|
| `%` | `%25` |
| 制表符 | `%09` |
| 回车 | `%0D` |
| 换行 | `%0A` |

正确顺序是先按真实制表符分列，再逐字段解码。解码时不要把 `+` 当成空格，也不要使用 URL
表单解码器。未知或不完整的 `%xx` 转义应报格式错误。

### 9.3 类型约定

- 布尔值固定为小写 `true` 或 `false`；
- 空字段表示该事件没有此值，不等于数值 `0`；
- `NA`、`none`、`unlimited` 等字面值必须按 schema 保留，不能静默转成零；
- 浮点数按英文小数点解析，不受 Windows 地区设置影响；
- 引擎给出的 `dataset_id`、`model_id`、node ID、edge ID 和 state ID 应保持原样；
- 生物地理随机历史的 `sample` 从 `0` 开始；
- 参数优化的 `start` 从 `1` 开始；
- 不认识的格式号或字段应拒绝，而不是“尽量读取”。

首行或 `format` 字段决定解析器版本。RASP5 可以支持多个已知版本，但不能把未知的新版本当作
当前版本继续读。

## 10. RASP5 应使用的命令

| 目的 | 推荐命令 |
|---|---|
| 检查引擎能力 | `engine-info` |
| 生成规范请求和参数表 | `analysis-template` |
| 检查用户输入 | `validate-inputs` |
| 在运行前解析和估算 | `analysis-plan` |
| 固定参数评估或参数优化 | `analysis-run` |
| 检查或严格重放结果 | `analysis-result-inspect --replay` |
| 一次完成拟合和随机历史 | `analysis-workflow` |
| 多 preset 拟合、比较与模型平均 | `model-workflow-plan`、`model-workflow` |
| 从拟合结果抽取生物地理随机历史 | `model-bsm` |
| 深度检查随机历史目录 | `bsm-inspect --deep` |
| 恢复未完成随机历史 | `model-bsm --bsm-resume` |
| 迁移旧分析结果 | `analysis-result-migrate` |
| 检查输入包 | `input-bundle-inspect` |
| 导入常见树、范围和时期文件 | `convert-tree`、`convert-ranges`、`convert-biogeobears-strata` |
| 生成随机化石放置 | `fossil-place` |

`dec-*` 等旧的专用验证入口、直接 `model-evaluate/model-optimize` 以及底层 batch 命令可以供开发
和兼容性测试使用，但不应成为 RASP5 的主接口。主流程优先使用已经注册 schema 的
`analysis-*`、`model-workflow` 和结果检查命令。

“命令存在”不等于“它的全部 stdout 已经冻结为公共接口”。RASP5 只能依赖
`engine-info.public_formats` 与 `schemas/registry.tsv` 共同声明的 artifact。

## 11. 单模型标准流程

### 11.1 先生成请求

最稳妥的方式是让引擎生成模板，而不是 RASP5 自己拼一份可能过期的参数表：

```powershell
biogeo-cli.exe analysis-template `
  --preset dec `
  --mode optimize `
  --output-dir <request-directory>
```

请求目录中的 `analysis.tsv` 是 `biogeo-analysis-request-v1`。最小优化请求类似：

```text
key	value
format	biogeo-analysis-request-v1
mode	optimize
tree	tree.nwk
observation	exact_ranges
ranges	ranges.tsv
parameters	parameters.tsv
max_range_size	auto
include_null_range	false
root_prior	flat
min_branch_length	0
ancestral_probabilities	true
split_probabilities	true
optimization_initial_step	0.2
optimization_tolerance	1e-8
optimization_max_iterations	200
```

实际文件使用制表符。请求可进一步包含 `tree_name`、`max_states`、
`missing_branch_length_fill`、dispersal/extirpation 修饰、时期文件以及多起点优化设置。

### 11.2 请求字段的职责

| 类别 | 主要字段 |
|---|---|
| 运行模式 | `mode=evaluate` 或 `mode=optimize` |
| 树 | `tree`，NEXUS 多树时可加 `tree_name` |
| 末端观测 | `observation` 与 `ranges`，或 `detections`、`controls` |
| 模型 | `parameters` |
| 状态空间 | `max_range_size`、可选 `max_states`、`include_null_range` |
| 根节点 | `root_prior` |
| 枝长处理 | `min_branch_length`、可选 `missing_branch_length_fill` |
| 地理修饰 | 静态或分时期 dispersal、距离、环境距离、灭绝倍率、面积 |
| 输出计算 | `ancestral_probabilities`、`split_probabilities` |
| 优化 | 初始步长、容差、最大迭代数和可选附加起点 |

请求文件中的相对路径都以 `analysis.tsv` 所在目录解析。RASP5 保存请求时应采用 UTF-8、真实
制表符和 `\n` 或 `\r\n` 行尾，不应把表格保存成 CSV。

`root_prior` 目前只有 `flat` 和 `equal`。`flat` 给每个允许根状态权重 1，是当前默认；
`equal` 给每个允许根状态归一化权重 `1/N`。两者可能使 lnL 相差一个归一化常数，不能在
RASP5 中当作同义显示选项，也不能比较使用不同 root prior 的模型。

### 11.3 必须先执行计划

```powershell
biogeo-cli.exe --error-format tsv analysis-plan `
  --request <analysis.tsv>
```

计划不会做完整优化，但会真正读取所有输入并构建模型。RASP5 应向用户展示：

- 识别到的末端、区域和状态数量；
- 最大范围大小和 null range 设置；
- free/fixed/derived 参数；
- 时期数、Q 矩阵结构和节点分裂场景数量；
- 估计内存、风险等级、警告和输入可移植性；
- 请求最终解析到的模式和模型。

只有计划 `status=valid` 才允许点击“运行”。计划中的资源数字是运行前估计，不是保证的峰值
内存或剩余时间。

### 11.4 运行并发布结果

```powershell
biogeo-cli.exe `
  --error-format tsv `
  --progress-format tsv `
  analysis-run `
  --request <analysis.tsv> `
  --output-dir <new-result-directory>
```

输出目录必须尚不存在。引擎先在临时目录工作，完整成功后才原子发布
`biogeo-analysis-result-v2`。RASP5 不应预先创建该最终目录。

成功 stdout 是 `biogeo-analysis-run-v2` 摘要；权威数据是结果目录。结果目录包含：

```text
metadata.tsv
inputs.tsv
source-parameters.tsv
resolved-parameters.tsv
input-bundle/
```

`source-parameters.tsv` 保存用户的 fixed/free/derived 声明；`resolved-parameters.tsv` 保存最终
数值。`metadata.tsv` 同时记录 lnL、状态空间、模型身份、输入指纹和优化诊断。

`status=complete` 只表示目录完整发布，不表示优化已经收敛。RASP5 必须单独读取并显示
`optimization_converged`、迭代数、似然评估数、起点数和收敛起点数；未收敛结果可以查看和
重放，但不能在界面上标成“优化成功”。

### 11.5 导入前严格检查

```powershell
biogeo-cli.exe --error-format tsv analysis-result-inspect `
  --analysis-result <result-directory> `
  --replay
```

`--replay` 会重新构建状态空间和模型、重算 lnL，并核对输入、依赖、模型身份和文件指纹。只有
退出码为 `0` 且 inspection 状态有效，RASP5 才应把目录加入项目。快速重新打开项目时可先做
结构检查，但在继续随机历史、模型平均或论文导出前仍应完成一次 replay。

## 12. 当前最重要的接口缺口：单模型祖先结果

引擎已经能够计算单模型的祖先范围后验概率和节点分裂后验概率；底层
`model-evaluate/model-optimize` 也能把它们追加到 stdout。但是 v0.1.0 的
`biogeo-analysis-result-v2` 尚未把这些表保存成已注册、稳定的结果 artifact，
`analysis-run` 的摘要也不包含这些明细。

因此 RASP5 当前可以可靠接入：

- 拟合参数、lnL 和优化诊断；
- 状态空间与输入身份；
- 模型比较和模型平均；
- 生物地理随机历史及其节点、分支和事件表。

但在 biogeo-rs 增加正式的单模型 posterior artifact 之前，RASP5 **不应解析底层 stdout**
来实现单模型祖先重建页面。这样做会把未承诺稳定的开发输出变成事实 API，下一版很难兼容。

需要补充的稳定结果至少应包含：

1. 区域字典和完整状态字典；
2. 不依赖标签文本解析的 node、tip 和 edge 字典；
3. 每个内部节点的祖先状态概率；
4. 每个节点的 cladogenetic split scenario 及概率；
5. 直接祖先和化石节点标志；
6. 模型、树、状态空间和输入指纹；
7. 独立的格式号、schema、检查命令和版本迁移规则。

当前内部输出会把一个 clade 表示成“排序后的末端标签以 `+` 连接”。由于合法末端标签本身也
可能含有 `+`，RASP5 绝不能拆分该文本来恢复树节点。正式接口应使用稳定 node ID 和显式
node/edge/tip 字典。

多模型输出 `biogeo-model-averaged-ancestral-ranges-v2` 已注册，可以可靠读取，但它不能替代
没有进行多模型平均的单模型祖先结果。建议把上述 artifact 作为 RASP5 祖先结果界面上线前的
biogeo-rs 首要开发任务。

## 13. 参数编辑器

### 13.1 不要只做 d、e、j 三个输入框

统一框架当前认识以下 23 个参数名：

```text
d e a b x n w u j ysv ys y s v
mx01 mx01j mx01y mx01s mx01v mx01r
mf dp fdp
```

其中 d/e/a/b 控制沿枝变化，x/n/w/u 控制地理修饰，j/y/s/v 控制节点事件，
`mx01*` 控制分裂大小偏好，mf/dp/fdp 用于 detection 观测模型。具体可用性仍由参数表和
观测模式共同决定。

### 13.2 三种参数状态

- `fixed`：用户给定数值，优化器不改变；
- `free`：优化器在声明边界内估计；
- `derived`：由受限表达式从其他参数计算，不能独立优化。

优化尺度可以是 linear、log 或 logit。RASP5 应显示原值、边界、尺度、状态和 derived 表达式，
并把合法性判断交给引擎 plan。不要在 GUI 中复制一套日后会漂移的数学验证器。

### 13.3 preset 是模板，不是硬编码表单

RASP5 应调用 `analysis-template --preset ...` 获取每个 preset 的参数表，再允许用户编辑。这样
引擎升级时，默认值、边界和联动关系来自同一来源。特别是 `mx01r` 当前固定为 `0.5`，不能让
界面仅凭名字把所有 `mx01*` 都显示成相同的自由参数。

v0.1.0 的参数表解析器、教程和测试已经存在，但 `biogeo-parameter-table-v1` 尚未列入公共
schema registry。第一版 RASP5 可以把引擎生成的参数表视为请求目录内部文件并原样回传；若要
提供长期稳定、跨版本的任意参数编辑器，还应先把参数表格式正式注册。

## 14. 三种末端观测模式

| 模式 | 输入 | 适用情形 |
|---|---|---|
| `exact_ranges` | 每个末端一个确定范围 | 常规 presence/absence 分布 |
| `ambiguous_ranges` | `0/1/?` | 某些区域是否存在未知 |
| `detection` | detections + controls | 把未检出与真实缺失分开建模 |

RASP5 不应把 `?` 自动改成 `0`。detection 模式中的 mf/dp/fdp 是观测过程参数，不应与
生物地理过程参数混为同一组“扩散/灭绝”设置。无论哪种模式，树末端与观测表 taxon 都要由
引擎验证一一对应；GUI 的预检查只能用于尽早提示，不能代替 `analysis-plan`。

## 15. 树输入边界

### 15.1 Newick

当前严格解析器支持：

- UTF-8 未引号标签；
- 单引号标签，因此标签可包含空格、逗号、冒号和括号；
- 用两个连续单引号表示标签中的一个字面单引号；
- 平衡的方括号注释，包括嵌套注释；
- tip 标签和内部节点标签；
- 有限、非负枝长。

所有非根节点默认必须有枝长。只有用户明确选择填充值时，RASP5 才可写入
`missing_branch_length_fill`；不能为了“让分析跑起来”自动补 `0` 或 `1`。root edge 会被拒绝，
因为当前模型没有定义根上方的演化过程。

解析器可以读出多分叉，但当前 DEC 类节点分裂似然要求二叉树，所以正式分析会拒绝多分叉。
RASP5 可以另外提供有明确方法说明的树处理功能，但不能在提交引擎前静默二叉化。

### 15.2 NEXUS

当前支持一个 `BEGIN TREES` 块、rooted `TREE`、可选默认树标记和 `TRANSLATE`。单树无需选择
名称；多树必须让用户明确选择 `tree_name`，且名称区分大小写。`UTREE`、重复树名、重复
translate alias、未映射 alias 和选择不存在的树都会失败。

RASP5 的树选择器应先显示文件中可用树名，再把确切名称写入请求。不要默认选择第一棵，也不要
在 GUI 中更改标签以“匹配”分布矩阵。

### 15.3 古老末端与直接祖先

root-to-tip 较短的 tip 会按其实际采样年龄解释为古老末端。它的终端枝在该年龄结束，时期切分、
似然和生物地理随机历史都不会把这条枝补到现在。

`min_branch_length` 具有特殊科学语义：子枝长度严格小于该阈值时，其父节点按
BioGeoBEARS 的超短枝直接祖先规则处理，而不是普通物种形成节点。CLI 默认值为 `0`；若用户要
复现常见 BioGeoBEARS 默认，应明确设置 `1e-6`。RASP5 应把它放在“直接祖先规则”设置中，
不能伪装成普通数值稳定参数。

引擎在直接祖先节点不会记录 y/s/v/j 分裂事件。BioGeoBEARS 的部分后处理会把这种恒等复制显示
成 y，但这与其似然中的“无普通物种形成”语义不一致；RASP5 应采用引擎的显式直接祖先标志，
不要自行增加一个 y 事件。

## 16. 分布矩阵、时期与地理修饰

### 16.1 分布输入

规范输入是带 tip/taxon 和区域列的 TSV。转换入口还可读取 BioGeoBEARS/LAGRANGE `.data`，
以及首列为 `Name`、`tip`、`taxon` 或 `species` 的 CSV。`.data` 只是输入格式，不会使任务
自动切换成 BioGeoBEARS 算法。

引擎严格要求：

- 区域名唯一；
- taxon 名与树 tip 精确对应；
- 不缺 tip、不多未知 tip、不重复 tip；
- 行列数一致；
- 单元格符合 exact 或 ambiguity 模式。

RASP5 可以在导入时显示差集，但不能做大小写折叠、去空格、模糊匹配或自动替名。需要改名时，
应让用户确认并保存一份可审计的映射表。

### 16.2 时期文件和二级依赖

分时期分析可引用 dispersal multiplier、distance matrix、environment distance matrix、
extirpation multiplier、area size、允许区域和 adjacency 文件。引擎会把时期表及其二级文件
一起装入结果目录的 `input-bundle`，并重写包内执行副本的相对路径。

RASP5 应把这些文件视为一个依赖集合，不能只把顶层时期表加入项目。源时期表会保留作审计，
重放则使用包内规范副本。

### 16.3 adjacency 的两种含义不能混用

“所有区域两两相邻”和“范围内每个区域至少连接到另一个区域”不是同一个状态约束。项目已经
为相关 BioGeoBEARS 工作流实现明确的 all-pairs 与 edge-covered 语义。RASP5 导入 adjacency
时必须要求用户或导入模板选择语义，并在计划页显示每个时期实际允许的状态数。

不能仅看到一个 0/1 adjacency matrix 就猜测论文作者采用哪种状态生成规则。Ponerinae 官方
配置使用的是 edge-covered；这是一项已经验证的真实案例，不应被硬编码成所有数据的默认值。

### 16.4 修饰参数的分工

- `x`：距离对 dispersal 的影响；
- `n`：环境距离修饰；
- `w`：dispersal multiplier 的权重；
- `u`：面积相关修饰；
- 静态或分时期 extirpation multiplier：修饰局部灭绝；
- `areas_allowed` 和 adjacency：直接改变时期内允许的状态。

这些修饰可能共同作用。RASP5 不应因为某个示例对某参数信息不足就隐藏该参数；应允许参数表
表达 fixed/free/derived，再由计划、优化诊断和科学解释告知用户识别度风险。

`b` 是静态模型中的 `branch_length^b`，分时期模型要求 `b=1`。`w` 只有在提供手工
dispersal multiplier 时才能偏离 1。`x/n/u` 分别需要距离、环境距离和面积原始输入。静态与
时期表不能重复提供同一种来源；具体冲突由 `analysis-plan` 统一拒绝。

## 17. 状态空间与资源预检

### 17.1 状态数如何计算

若有 `A` 个区域、最大范围大小为 `M`，并且允许所有组合，则非空状态数为：

```text
sum(C(A, k), k = 1..min(A, M))
```

若 `include_null_range=true`，再加一个空范围状态。时期允许区域、adjacency 和显式状态表可以
减少某时期的允许状态，但引擎仍需维护统一 master state space。

因此区域数增加一点，状态数可能增加很多。例如 30 个区域并不等于 30 个状态。资源负载主要
随“树节点数 × 状态数”增长，节点数和区域数都不能单独代表难度。

请求中的 `max_range_size=auto` 表示 `M=A`，即允许范围最多覆盖全部区域；它不是根据已观测
最大范围自动选择较小上限。区域较多时，RASP5 应在计划页把解析后的具体数值和状态数醒目显示。
`include_null_range` 决定空范围是否进入状态空间，同样会改变模型，不能作为节省资源的隐藏开关。

### 17.2 不设隐藏默认上限

`max_states` 是用户可选的安全阈值，不是科学模型参数。省略它表示不人为限制状态数；引擎仍会
在实际大分配前计算组合数量并检查整数、地址空间和资源风险。RASP5 不应悄悄设置一个默认上限，
也不应自动减小 `max_range_size`，因为那会改变模型。

如果用户设置了 `max_states`，计划超过上限时应在分配大对象前返回 `resource_limit`。界面可
建议减小范围上限、增加生物学状态约束或换用更大内存机器，但最终改变必须由用户确认。

### 17.3 当前 PC 实测参考

同一棵 100-tip 树、`max_range_size=5`、包含 null range 的 release 固定似然：

| 区域数 | 状态数 | 中位运行时间 | 峰值 working set 中位数 |
|---:|---:|---:|---:|
| 5 | 32 | 0.0076 s | 5.18 MiB |
| 10 | 638 | 0.0247 s | 8.24 MiB |
| 20 | 21,700 | 1.6418 s | 112.16 MiB |
| 30 | 174,437 | 17.6232 s | 863.50 MiB |

这些数字只用于量级提示，不是另一台机器的保证。RASP5 应优先显示本次 `analysis-plan` 的估算，
并注明尚不包含所有分配器、树对象和临时工作区开销。

当前状态用 `u64` bitset 表示，所以区域数硬上限为 64。即使低于 64，组合状态数也可能远超
实际内存。100 万 tip、5 区域完整 32 状态的固定 DEC 已在本机完成；这不意味着 100 万 tip、
20 区域也可运行。

### 17.4 RASP5 的资源提示

建议把计划结果显示为普通语言：

- “预计状态较少，可以直接运行”；
- “状态较多，运行可能需要较多内存”；
- “预计超过你设置的安全阈值，尚未开始计算”。

不要制造精确剩余时间，也不要仅按区域数弹出固定警告。用户确认高风险任务后，RASP5 可以运行，
但必须继续保留取消能力和完整错误信息。

## 18. 单模型拟合后运行生物地理随机历史

生物地理随机历史必须从一个可重放的拟合结果出发：

```powershell
biogeo-cli.exe `
  --error-format tsv `
  model-bsm `
  --analysis-result <fit-result> `
  --bsm-samples 1000 `
  --bsm-output-dir <new-bsm-result> `
  --bsm-output-level compact `
  --bsm-threads auto `
  --bsm-max-in-flight 32 `
  --bsm-checkpoint-samples 100 `
  --seed 1
```

RASP5 不应从界面上显示的四舍五入参数重新拼模型。`model-bsm` 会从
`resolved-parameters.tsv` 和输入包重建模型、核对 lnL 与身份，再开始抽样。

常规 RASP5 项目默认建议：

- `compact` 输出；
- `bsm-threads=auto`；
- 有界 `bsm-max-in-flight`；
- 定期 checkpoint；
- 为长任务启用可恢复目录；
- 只在用户明确只要总体分布、不需要具体路径时选择 `summary`。

少量探索性任务可以先做 10--100 条；正式事件分布估计通常需要更多样本，并应由用户根据
Monte Carlo 稳定性决定，而不是由 GUI 固定一个“科学正确”的条数。

## 19. 多模型拟合、比较和模型平均

### 19.1 用一个工作流保存共同数据和候选模型

RASP5 推荐使用 `model-workflow`，而不是依次启动六个互不关联的命令。一个
`biogeo-model-workflow-request-v1` 引用候选模型 manifest、共享数据配置、比较准则和可选的
生物地理随机历史设置。

```text
key	value
format	biogeo-model-workflow-request-v1
models	models.tsv
config	model-config.tsv
comparison_criterion	aic
bsm_selection	best_by_criterion
bsm_samples	1000
bsm_output_level	compact
bsm_threads	auto
bsm_shard_samples	100
bsm_deep_inspection	true
bsm_seed	20260822
```

先运行：

```powershell
biogeo-cli.exe model-workflow-plan --request <workflow.tsv>
```

计划会真实构建每个候选模型，显示共享数据身份、各模型自由参数数目、Q 与 split 场景、状态数、
随机历史资源配置和风险。确认后再运行：

```powershell
biogeo-cli.exe `
  --error-format tsv `
  --progress-format tsv `
  model-workflow `
  --request <workflow.tsv> `
  --output-dir <new-workflow-result>
```

### 19.2 模型比较

参数数目 `k` 是参数表中真正声明为 `free` 的维度。AIC、AICc、delta 和 Akaike weight 由引擎
计算；RASP5 应直接显示结果，不应从四舍五入的 lnL 重新计算。

只有优化收敛且至少一个起点收敛的模型才进入权重。`n <= k + 1` 时 AICc 没有有限定义；
结果中的 `NA` 不能显示成 0。只要完整 AIC 候选集有模型缺少有限 AICc，引擎就不会对剩余子集
重新归一化并冒充完整 AICc 比较。

不同树、tip 观测、状态空间或 root prior 的模型不能放入同一比较。时期、距离和节点参数可以
不同，因为这些正是候选模型定义的一部分。

### 19.3 嵌套关系和似然比检验

`biogeo-model-comparison-v3` 从参数表表达式判断嵌套关系，不按模型名字猜测。关系可能是：

- `equivalent`：同一模型流形；
- `nested_interior`：约简模型位于完整模型内部；
- `nested_boundary`：约简模型只在边界或闭包上获得；
- `not_nested`：已证明不嵌套；
- `undetermined`：当前符号检查无法可靠证明。

只有严格嵌套且两边拟合均合格时才计算 likelihood-ratio test。边界嵌套会保留普通卡方值和
风险说明；恰好一个边界约束且自由度为 1 时，还给出 50:50 half-chi-square 值。RASP5 应同时
显示边界警告，不能只留下一个 p 值。

如果完整模型的 lnL 反而显著低于约简模型，引擎会报告 `likelihood_order_violation`，这通常
表示优化需要重跑。不能把负 LR 截成 0 后继续生成“正常”检验。

### 19.4 模型平均不是参数平均

`biogeo-model-averaged-ancestral-ranges-v2` 使用 AIC 或 AICc 权重，对各模型条件 posterior
逐状态加权：

```text
P_avg(z) = sum_m weight_m * P(z | data, model_m)
```

它同时覆盖祖先范围和 cladogenetic split scenario。不同模型缺少的 split 场景先按概率 0
处理，再进入加权和；不能只在共有场景中重新归一化。

RASP5 应按文件中的 node、state 和 scenario ID 连接八个表段，不能按各模型内部行号对齐，也
不能把不同模型的 d/e/j 参数做平均后当成一个可抽样模型。生物地理随机历史仍必须选择一个
完整的单模型结果。

### 19.5 生物地理随机历史模型选择

`bsm_selection` 有三种值：

- `none`：只拟合、比较和模型平均；
- `model_id`：由用户明确选择一个已收敛模型；
- `best_by_criterion`：使用比较准则唯一排名第一的模型。

排名第一不等于科学真相。若第一名并列、没有合格模型或显式模型未收敛，引擎会拒绝抽样；
RASP5 应让用户查看诊断或明确选择，不能暗中选 manifest 第一行。

## 20. 生物地理随机历史结果

### 20.1 四种输出级别

| 级别 | 能否重建具体历史 | 占据表 | 推荐用途 |
|---|---:|---|---|
| `legacy` | 是 | 稠密 | 兼容旧读取脚本 |
| `full` | 是 | 稠密 | 人工检查和完整归档 |
| `compact` | 是 | 稀疏 | RASP5 常规默认 |
| `summary` | 否 | 稀疏 | 只分析大量样本的统计分布 |

`compact` 与 `summary` 的稀疏占据表只写 `occupancy_time > 0` 的行。缺行表示精确的 0，不表示
数据缺失。RASP5 必须先读取 `metadata.tsv` 中的 `format`、`output_level`、`path_details` 和
`sparse_occupancy`。

`summary` 不保存可重建路径。选用后，界面必须禁用逐分支动画或单条历史查看，而不能根据空表
猜测“没有事件”。

### 20.2 v2 字典

每个 v2 目录都包含：

- `areas.tsv`：区域 ID 与名称；
- `states.tsv`：状态 ID、bitset 和范围名称；
- `nodes.tsv`：节点 ID、标签和节点类型；
- `edges.tsv`：边 ID、父子节点和枝长；
- `periods.tsv`：时期 ID、年龄上界和允许状态概要。

compact 样本表只存整数 ID，必须与字典一起移动和归档。RASP5 应在导入时建立只读索引，但不
应重写字典或给 ID 重新编号。

### 20.3 八张样本表

```text
node_states.tsv
cladogenetic_splits.tsv
branch_segments.tsv
sample_event_counts.tsv
sample_period_event_counts.tsv
sample_state_occupancy.tsv
sample_period_state_occupancy.tsv
anagenetic_events.tsv
```

其中：

- `node_states` 保存每条样本在节点上的状态；
- `cladogenetic_splits` 保存节点处的 range copying、subset sympatry、vicariance 和
  founder event；
- `branch_segments` 保存受时期边界切分的分支路径段；
- `anagenetic_events` 保存沿枝事件及时间；
- 其余四张表保存逐样本、逐时期和逐状态汇总。

沿枝事件标准名称为：

- `range_expansion`，对应 d；
- `local_extirpation`，对应 e；
- `range_switching`，对应 a。

节点事件标准名称为 `range_copying`、`subset_sympatry`、`vicariance` 和 `founder_event`。
RASP5 可在界面翻译名称，但项目文件中应保留原始枚举，不能用显示文本覆盖。

### 20.4 样本索引、随机数和线程

每条样本的随机流由 master seed 和 sample index 派生。因此同一模型、seed 和样本数下，
1、4 或 16 个 worker 得到的第 n 条历史相同，线程完成顺序不会改变输出。

实际 worker 数为用户设置或 `auto` 解析值与样本数的较小者，不存在固定 16 线程的软件上限。
v0.1.0 已在 Windows PC 上验证；Linux/Slurm 的 affinity、cgroup 和调度器资源发现尚未作为
正式能力发布，RASP5 当前不要对服务器自动扩展作承诺。

`bsm-max-in-flight` 控制同时在内存中等待写出的样本数，不是线程数。应保持有界，以免某条
历史事件很多时结果队列无限增长。`bsm-memory-budget-mb` 是执行窗口预算，不是对进程全部
内存的硬隔离；它不能代替系统级资源限制。

### 20.5 分片、checkpoint 和恢复

大任务建议按固定样本区间分片。每个 checkpoint 会：

1. 写完整、有序的样本前缀；
2. flush 并同步八张表；
3. 原子记录八个文件的精确字节长度；
4. 中断或恢复时把未提交残尾截断到最近 checkpoint。

分片改变文件组织，不改变样本内容。恢复时以下执行控制可以调整：线程数、在途数、总事件预算、
内存预算、checkpoint 间隔、时间限制、交互控制和是否深度检查。

以下属于随机历史身份，恢复时不能改变：样本总数、seed、输出级别、分片大小、单样本事件上限、
所选模型及科学输入。需要改变这些项目时应新建输出目录。

### 20.6 导入检查

快速检查：

```powershell
biogeo-cli.exe --error-format tsv bsm-inspect `
  --bsm-result <bsm-result>
```

归档、传输后或正式导入时：

```powershell
biogeo-cli.exe --error-format tsv bsm-inspect `
  --bsm-result <bsm-result> `
  --deep
```

快速检查验证目录事务、字典、checkpoint、分片和表头；`--deep` 流式扫描全部已提交样本，
进一步验证事件分类、事件链、时期、状态占据、总枝长及禁止状态诊断。深度检查不会重新抽样，
也不能替代拟合结果的 `analysis-result-inspect --replay`。

## 21. 进度显示

启用方式：

```powershell
biogeo-cli.exe `
  --error-format tsv `
  --progress-format tsv `
  analysis-run ...
```

每条 `biogeo-cli-progress-v1` 记录固定 14 列：

```text
format sequence event command dataset_id model_id completed total
start starts iteration max_iterations evaluations best_log_likelihood
```

实际为一行、制表符分隔。`sequence` 从 1 递增；缺失字段为空。事件包括任务开始/完成/取消、
批处理单元开始/完成/失败，以及优化起点、迭代和起点完成。

`analysis-run` 内部执行优化时，进度记录的 `command` 可能是 `model-optimize`。RASP5 应按
task ID 和当前子进程归属它，而不是要求 command 文本等于用户点击的菜单名。

优化迭代次数不能可靠换算成剩余时间。界面应显示起点、迭代、评估次数和当前最好 lnL，
不要显示虚假的精确百分比或预计结束时间。

生物地理随机历史目前不为每条样本发送 stderr 进度。RASP5 应低频读取结果目录
`metadata.tsv` 的 `completed_samples/samples`，显示已经 checkpoint 提交的数量。worker 已算完
但尚未提交的样本不算完成。

stdout、stderr 必须持续异步排空。进度行和最终错误都可能出现在 stderr，分别以
`biogeo-cli-progress-v1` 和 `format<TAB>biogeo-cli-error-v1` 识别；其他行作为诊断日志保存。

## 22. 错误与退出码

RASP5 应始终启用 `--error-format tsv`。最终错误是：

```text
format	biogeo-cli-error-v1
code	<stable_code>
message	<percent-encoded UTF-8 text>
exit_code	<2|3|124|130>
```

稳定 code 包括：

```text
bsm_cancelled
task_cancelled
bsm_time_limit
bsm_event_limit
resource_limit
invalid_arguments
io_error
invalid_input
optimization_error
analysis_result_error
analysis_workflow_error
fossil_placement_error
model_average_error
model_batch_error
dataset_batch_error
bsm_error
runtime_error
configuration_error
```

退出码约定：

| 退出码 | 含义 |
|---:|---|
| 0 | 成功 |
| 2 | 普通参数、输入、I/O、模型或运行错误 |
| 3 | 生物地理随机历史事件预算停止 |
| 124 | 生物地理随机历史时间预算停止 |
| 130 | 用户协作式取消 |

事件预算和时间预算停止不等于目录损坏。RASP5 应先快速检查已提交前缀，再把任务显示为“可恢复”
而不是“失败且结果不可用”。机器逻辑依据 `code` 和退出码；`message` 用于向用户解释和记录，
不能靠匹配英文句子决定行为。

## 23. RASP5 任务状态

建议宿主内部至少区分：

| 状态 | 含义 |
|---|---|
| `queued` | 请求已保存，尚未启动 |
| `checking_engine` | 正在握手并核对 schema |
| `planning` | 正在做输入和资源计划 |
| `running` | 引擎进程运行中 |
| `cancel_requested` | 已发送取消，等待安全点 |
| `rejected` | 运行前输入或配置被拒绝 |
| `cancelled` | 引擎以 130 确认取消 |
| `budget_stopped` | 事件或时间预算停止 |
| `failed` | 已识别的执行错误 |
| `completed` | 进程成功，尚未导入 |
| `importing` | 正在检查 artifact |
| `ready` | 已验证并进入项目 |
| `protocol_error` | 格式、schema、错误码或退出状态矛盾 |

标准路径是：

```text
queued -> checking_engine -> planning -> running
       -> completed -> importing -> ready
```

`cancelled` 和 `budget_stopped` 不自动等于“可恢复”。只有输出目录存在，且身份、attempt 或
checkpoint 能通过对应检查时，界面才显示恢复按钮。

未知格式、缺字段、未知错误码、非零退出却没有机器错误、或机器错误中的 `exit_code` 与真实退出
码不一致，都应进入 `protocol_error`。这表示 RASP5 与引擎接口不兼容或输出损坏，不应伪装成
普通输入错误。

## 24. 取消、暂停和强制终止

### 24.1 参数优化和批处理

RASP5 在 Windows 上向子进程发送控制台中断，相当于 `Ctrl+C`。引擎在完整似然评估之间检查
取消令牌，不会在矩阵计算中间破坏数值对象。因此用户点击取消后，状态先进入
`cancel_requested`，进程可能需要短暂时间到达安全点。

只有引擎最终返回 `task_cancelled` 和退出码 `130`，才能记为 `cancelled`。不能一发送信号就
删除目录或把任务标成已经取消。

### 24.2 生物地理随机历史交互控制

显式启用 `--bsm-interactive` 后，RASP5 可以向 stdin 写入一行：

```text
pause
resume
status
cancel
```

`p` 和 `r` 也可分别作为 pause/resume 简写，但 RASP5 建议发送完整单词。暂停只在进程内等待，
不发布一个新的 checkpoint，也不等于退出后可恢复。暂停期间仍要排空 stdout/stderr，并允许
继续发送 resume 或 cancel。

若宿主设定宽限时间后引擎仍未退出，可以强制终止进程。但强制终止不能记为“引擎确认取消”；
必须重新检查输出目录和 checkpoint，并向用户说明这是未确认中断。

## 25. 恢复身份

恢复不是“对同一文件夹再跑一次”，而是先证明新请求和已有工作完全属于同一科学任务。

单模型 `analysis-workflow --resume` 要求原请求仍存在且字节不变。已有完整
`analysis-result` 会复用，不重复优化；生物地理随机历史从最近 checkpoint 继续。

多模型 `model-workflow --resume` 会核对顶层请求、候选 manifest、共享 config、每张参数表和
各模型结果。已完成且身份有效的模型不重算，缺失或未完成部分继续。

通常可以改变而不改变随机样本的执行控制包括：

- worker 数；
- 最大在途样本数；
- 总事件、内存窗口和时间预算；
- checkpoint 间隔；
- interactive 开关；
- 深度检查开关。

不能在原目录中改变：

- 树、观测、时期和模型；
- 参数表的科学定义；
- 生物地理随机历史样本总数和 seed；
- 输出级别和分片大小；
- 单样本事件上限；
- 多模型选择和比较准则。

身份不兼容时应创建新任务目录，不能删除指纹、改 metadata 或手工复制 `complete.tsv` 来绕过
检查。

## 26. 结果检查与项目导入

### 26.1 单模型

导入顺序：

1. 子进程退出码为 0；
2. stdout 格式是本次命令声明的稳定格式；
3. 运行 `analysis-result-inspect`；
4. 首次正式导入或迁移后运行 `--replay`；
5. 读取 metadata、source/resolved parameters 和 inputs；
6. 整个结果目录作为一个只读项目资源登记。

### 26.2 多模型工作流

至少检查：

1. 根 metadata 为 `biogeo-model-workflow-result-v1`；
2. `complete.tsv` 存在且状态完整；
3. metadata、selection 和 complete 的请求指纹一致；
4. selected analysis result 是根目录内安全相对路径；
5. 每个要使用的模型结果均为完整 `biogeo-analysis-result-v2`；
6. comparison 和 model-average 格式已在 registry 注册；
7. 启用随机历史时，样本请求数等于完成数；
8. 正式导入前 `bsm-inspect --deep` 返回有效且 run status 完成。

只有全部通过后，任务才从 `importing` 进入 `ready`。

### 26.3 已经导入后的读取

RASP5 可以为界面性能建立数据库索引或缓存，但缓存必须可删除、可重建，并保存来源 artifact
格式和指纹。原结果目录仍是权威来源。不要把界面数据库当作唯一科研结果，也不要回写引擎目录。

对几百 GB 的随机历史结果，应流式读取、分页和按 sample/edge 查询；不能启动时把八张表全部
装入内存。summary 和 compact 的稀疏零值规则必须在索引层保留。

## 27. 项目可移植性与可重复性

完成的 `biogeo-analysis-result-v2` 自包含树、范围/观测、参数、时期表及其二级依赖。移动或复制
时必须整体保留目录。原始输入即使不再可用，结果仍可重放。

RASP5 项目建议保存：

- 使用的引擎目录和 `engine_version`；
- 发布包 SHA-256；
- artifact 格式号；
- 原始用户请求和引擎封存请求的指纹；
- 分析结果与随机历史结果的项目相对路径；
- 命令参数数组；
- 开始/结束时间、退出码、机器错误和完整诊断日志；
- 用户显示名与不可变内部 task ID；
- 是否通过 quick inspect、deep inspect 和 likelihood replay。

RASP5 可把完整 artifact 复制到项目的受管数据目录，或只登记外部路径。外部路径模式必须清楚
提示移动风险；论文归档和跨机器传输应使用受管副本。

当前结果指纹用于发现误改和保证重放，不是密码学签名。发布 ZIP 有 SHA-256，结果目录本身尚未
定义签名容器。RASP5 不应把“指纹一致”宣传成防恶意篡改证明。

## 28. 随机化石放置

`fossil-place` 是树预处理，不是似然中的隐藏步骤。manifest 为每个化石指定：

- 年龄区间；
- `side_branch` 或 `direct_ancestor`；
- `stem`、`crown` 或 `both`；
- 限制类群的 tip 列表。

命令按 replicate 生成独立 Newick 树和 `placements.tsv`：

```powershell
biogeo-cli.exe fossil-place `
  --tree <tree.nwk> `
  --manifest <fossils.tsv> `
  --output-dir <new-placement-result> `
  --replicates 1000 `
  --seed 20260723 `
  --direct-ancestor-hook-length 1e-7
```

输出为已注册的 `biogeo-fossil-placement-set-v1`。同一 seed 下，每个 replicate 由独立索引派生
随机流，增加 replicate 数不会改变已有编号的树。

若结果含 direct ancestor，后续分析必须显式设置大于 hook 长度的 `min_branch_length`，例如
hook 为 `1e-7` 时使用 `1e-6`。否则引擎会把它当成极短普通侧枝。RASP5 在把化石树送入分析时
应自动带出该建议并要求用户确认，而不是静默改变请求。

多棵随机放置树代表拓扑/化石位置不确定性。每棵树应成为独立数据任务；不同树的 lnL、AIC 和
posterior 不能直接混入同一普通模型比较表。如何跨树汇总属于 RASP5 更高层科研工作流，必须
明确统计定义。

## 29. 已注册接口与暂未注册输出

RASP5 的稳定边界由 `schemas/registry.tsv` 定义。v0.1.0 已注册的核心类别包括：

- 引擎能力、机器错误和机器进度；
- 分析请求、计划、运行摘要、结果目录、输入包、检查和迁移；
- 单模型与多模型工作流；
- 模型比较 v3 和模型平均 v2；
- fossil placement set；
- full/compact/summary 的单目录与分片随机历史 v2；
- Windows 发布包与安装目录。

以下能力已有命令、代码或测试，但其全部明细输出尚未全部注册为 RASP5 长期机器接口：

- `validate-inputs` 的详细 stdout；
- `convert-tree`、`convert-ranges` 和部分时期转换摘要；
- `biogeo-parameter-table-v1`；
- 底层 `model-evaluate/model-optimize` 的详细 stdout；
- 单模型祖先状态与 split posterior；
- 直接使用 `model-batch/dataset-batch` 的部分顶层结果。

第一版可把 `validate-inputs` 和转换命令作为用户主动执行的导入工具，但正式任务仍需
`analysis-plan` 兜底。不要让 GUI 的核心状态依赖未注册的 stdout 字段。多模型主线优先走已注册
的 `model-workflow`。

接入过程中若确实需要某项未注册数据，正确做法是在 biogeo-rs 中定义新 artifact、schema、
检查器和回归测试，再由 `engine-info` 宣告；不是在 RASP5 里临时抓日志。

## 30. RASP5 内部建议结构

不要求 RASP5 使用某种编程语言，但建议把接入拆成以下清楚模块：

| 模块 | 只负责 |
|---|---|
| `EngineInstallation` | 定位版本目录、校验包、运行握手 |
| `SchemaRegistry` | 加载 registry 和已知 schema |
| `BiogeoProcessRunner` | 参数数组、三路管道、退出码和进度 |
| `AnalysisRequestBuilder` | 管理模板和请求文件，不重做似然验证 |
| `BiogeoTaskController` | 状态、取消、暂停、恢复和日志 |
| `ArtifactInspector` | 调用引擎 inspect/replay/deep |
| `ArtifactImporter` | 流式读取已验证结果并建立可重建缓存 |
| `ProjectReference` | 保存相对路径、版本、格式号和指纹 |

`BiogeoProcessRunner` 不应知道 DEC 或 DIVALIKE 的科学含义；参数编辑器不应知道 Windows 进程
细节；结果可视化也不应直接启动命令。这样以后增加 Linux runner 或新 artifact 版本时，不需要
改写所有模型页面。

进程调用的语言无关伪代码：

```text
capabilities = engine.handshake()
registry.assert_exact_match(capabilities.public_formats)

plan = runner.run(
  executable,
  ["--error-format", "tsv", "analysis-plan", "--request", request]
)
require(plan.exit_code == 0 && plan.format == "biogeo-analysis-plan-v1")

run = runner.start(
  executable,
  ["--error-format", "tsv", "--progress-format", "tsv",
   "analysis-run", "--request", request, "--output-dir", new_output]
)
task.consume_stdout_and_stderr(run)
require(run.exit_code == 0)

inspection = runner.run(
  executable,
  ["--error-format", "tsv", "analysis-result-inspect",
   "--analysis-result", new_output, "--replay"]
)
require(inspection.exit_code == 0)
project.import_read_only(new_output)
```

## 31. 界面与命令的对应关系

| RASP5 界面动作 | 引擎动作 |
|---|---|
| 选择引擎版本 | `engine-info` + registry 核对 |
| 新建历史生物地理分析 | `analysis-template` |
| 选择树和观测数据 | 写请求路径，随后 `analysis-plan` |
| 编辑 preset/参数 | 编辑模板生成的参数表，再次 plan |
| 查看运行前摘要 | 显示 `analysis-plan` 结果 |
| 开始单模型分析 | `analysis-run` |
| 查看拟合信息 | 读取已检查的 analysis result |
| 生成生物地理随机历史 | `model-bsm` |
| 暂停/继续/取消随机历史 | stdin 控制或进程中断 |
| 恢复任务 | 原命令加 `--resume` 或 `--bsm-resume` |
| 比较六个 preset | `model-workflow-plan` + `model-workflow` |
| 显示 AIC/AICc/LRT | 读取 model comparison v3 |
| 显示模型平均祖先范围 | 读取 model average v2 |
| 随机放置化石 | `fossil-place` |
| 打开已有结果 | inspect，必要时 replay/deep，再导入 |

RASP5 可以把高级选项分组折叠，但必须允许用户看到最终请求、参数表、模型计划、引擎版本和完整
诊断。命令行复现信息应可以从项目记录导出，不要求用户理解内部目录结构才能复现。

## 32. 建议的接入顺序

### 第一步：只接通引擎和协议

1. 安装完整 v0.1.0 包；
2. 实现 `engine-info` 握手和 registry 核对；
3. 实现 UTF-8 TSV、percent decoding、机器错误和进度解析；
4. 用中文及空格路径跑通进程；
5. 建立任务状态、日志和取消。

这一阶段不做科学界面，先保证 RASP5 能可靠判断“成功、拒绝、失败、取消、预算停止和协议错误”。

### 第二步：完成单模型拟合

1. 用 `analysis-template` 创建请求；
2. 做树、观测、参数和资源表单；
3. 每次科学设置改变后重新 `analysis-plan`；
4. 用 `analysis-run` 发布结果；
5. `analysis-result-inspect --replay` 后导入参数、lnL 和诊断；
6. 明确显示优化是否收敛。

当前先显示拟合结果，不上线依赖未注册 stdout 的单模型祖先 posterior 页面。

### 第三步：完成生物地理随机历史

1. 从 analysis result 启动 `model-bsm`；
2. 默认 compact，支持样本数、seed、线程、在途数、分片和预算；
3. 实现 checkpoint 进度、暂停、取消和恢复；
4. quick inspect 后可预览，deep inspect 后正式导入；
5. 流式索引八张表和五张字典。

### 第四步：完成多模型工作流

1. 候选 preset/参数表管理；
2. `model-workflow-plan`；
3. 顺序拟合进度与恢复；
4. comparison v3；
5. nested/LRT 风险显示；
6. model average v2；
7. 明确选择一个模型运行生物地理随机历史。

### 第五步：补单模型 posterior 正式 artifact

先在 biogeo-rs 中完成第 12 节定义的 schema、写出、检查、迁移和测试，再把 RASP5 的单模型祖先
范围与节点分裂页面连接到该 artifact。这是接入过程中应当新增的 Rust 功能，不应由 GUI 绕过。

### 第六步：扩展输入和化石工作流

加入 NEXUS 多树选择、显式名称映射、BioGeoBEARS 时期转换、随机化石放置、多树任务组织和大型
结果索引。Linux/服务器部署在 Windows PC 主线稳定后另行验证。

## 33. RASP5 必须做的集成测试

### 33.1 协议与路径

- 完整发布包可定位，单独 exe 被判为不完整安装；
- `engine-info.public_formats` 与 registry 精确匹配；
- 中文、空格、单引号和长路径通过参数数组正确传入；
- stdout/stderr 同时大量输出时不死锁；
- percent encoding 往返不丢 `%`、tab、回车和换行；
- 未知格式、未知字段和未知错误码进入 `protocol_error`；
- 全局参数放错位置能显示可理解的 `invalid_arguments`。

### 33.2 输入与计划

- Newick 引号标签、NEXUS translate 和多树显式选择；
- 缺失枝长默认拒绝，显式 fill 可运行且进入结果身份；
- 多分叉、UTREE、root edge 稳定拒绝；
- exact、`?` ambiguity 和 detection 三种观测；
- 树与分布 tip 缺失、多余、重复和大小写不同；
- 古老末端年龄与直接祖先阈值；
- all-pairs 与 edge-covered 产生不同且正确的时期状态数；
- 30 区域大状态计划不会在 GUI 自己枚举组合；
- 用户设置 `max_states` 时在大分配前得到 `resource_limit`。

### 33.3 单模型

- evaluate 和 optimize 都能生成、检查、移动并重放结果；
- 优化收敛和未收敛在 UI 中有不同状态；
- 删除原源输入后仍可从结果目录重放；
- 修改结果文件后 inspect/replay 拒绝；
- 两个任务不能共享输出目录；
- 取消后不把半成品标成完成。

### 33.4 生物地理随机历史

- 1 worker 与多个 worker 的同 seed 样本内容一致；
- compact 可重建路径，summary 明确不可重建；
- 稀疏占据表缺行按 0 处理；
- 事件预算和时间预算停止后只读取已提交前缀；
- 改变 worker 和执行预算后恢复，结果与一次性运行逐文件一致；
- 改 seed、样本数、输出级别或分片大小时恢复被拒绝；
- stdin pause/resume/cancel 的状态转换正确；
- quick 和 deep inspect 的用途与耗时区别正确；
- 大文件导入保持流式，不随文件大小线性占满内存。

### 33.5 多模型和项目迁移

- 六 preset 工作流全部完成并产生 comparison/model average；
- 不收敛模型不进入 Akaike weight；
- AICc 无定义显示 NA；
- DEC 与 DEC+J 的边界嵌套及 half-chi-square 提示正确；
- 非嵌套模型不显示 LRT p 值；
- rank 1 并列时不自动选择随机历史模型；
- 中断恢复不重新拟合已完成模型；
- 整个项目移到另一中文路径后仍可检查、重放和继续抽样；
- 新旧引擎并存时，旧项目使用记录的兼容版本或明确迁移。

仓库已有无 GUI 的参考宿主测试
`crates/biogeo-cli/tests/support/rasp_host.rs` 和
`crates/biogeo-cli/tests/rasp_host_contract.rs`。RASP5 可以参照其判断顺序，但应在自身语言和
进程框架中重新做上述端到端测试。

## 34. 当前风险和限制

1. 单模型祖先状态和 split posterior 尚缺稳定 artifact，这是 RASP5 科学结果页最明确的阻塞项。
2. 部分转换/验证 stdout 和参数表尚未注册，不应被长期 GUI 接口依赖。
3. 单模型 likelihood 主线当前接近单核；生物地理随机历史可并行，但写出会限制扩展。
4. v0.1.0 的正式平台是 64 位 Windows；Linux、容器、Slurm 和多进程调度尚未发布验证结论。
5. 64 区域是 bitset 硬上限，实际可行区域数通常更早受组合状态数限制。
6. 优化是数值搜索，不保证找到全局最大值；应支持多起点并显示每个起点和收敛诊断。
7. AIC/AICc、模型权重和 LRT 是统计工具，不是自动替代科学模型选择的结论。
8. `summary` 大幅节省空间，但永久放弃逐条路径；选错后只能重新抽样。
9. `legacy/full` 在大树和大量样本上可能产生极大文件；必须在运行前估算磁盘并支持 compact。
10. BioGeoBEARS 和 Rust 在浮点算法、优化器与某些明确修正的边界语义上不会逐字节相同；科学
    门禁使用 lnL 容差和分布级随机历史对照。
11. 直接祖先事件计数采用显式“无分裂事件”语义，不复制 BioGeoBEARS 后处理把 hook 计作 y 的
    不一致行为。
12. 结果目录指纹不是密码学签名，不能把它当成防恶意修改机制。
13. 当前 CLI 负责数值与机器结果，不提供 RASP5 的绘图、数据库、项目浏览和报告界面。

## 35. 不应采用的接法

- 只捆绑 `biogeo-cli.exe`，丢掉 schemas、许可证和包元数据；
- 通过 PowerShell/cmd 字符串拼命令；
- 解析 `--help`、英文日志或底层未注册 stdout；
- 按文件扩展名或列数猜 artifact 版本；
- 遇到未知字段时忽略并继续；
- 把 `status=complete` 当成优化收敛；
- 从四舍五入显示值重建随机历史模型；
- 把 clade 字符串按 `+` 拆成 tip；
- 自动改 tip 名、补枝长、二叉化或减小最大范围；
- 将 `?` 当成 absence；
- 把模型平均 posterior 当成一套可直接抽样的模型；
- 在多个进程间共享一个随机历史输出目录；
- 所有样本完成后才写文件，或把全部随机历史先放进内存；
- 取消后立即删除目录；
- 在已有 artifact 内原地迁移、修补 metadata 或覆盖文件；
- 把当前 PC 的 16 个逻辑核心写成软件上限；
- 根据旧版 RASP 内部接口反推 RASP5 架构。

## 36. 最低接入完成标准

第一版 RASP5 后端可称为“可靠接入”至少要满足：

1. 完整安装包握手和 schema 协商通过；
2. 单模型模板、计划、运行、检查和可移植重放全通；
3. fixed/free/derived 参数和优化收敛诊断可见；
4. 三种 tip observation 均能正确提交；
5. compact 生物地理随机历史可暂停、取消、恢复、深度检查和流式导入；
6. 多模型比较、嵌套关系、LRT 和模型平均可读取；
7. 中文/空格路径和项目整体移动通过；
8. 所有失败由机器错误与退出码驱动，不抓英文日志；
9. 未注册的单模型 posterior 不被临时解析；
10. RASP5 自身端到端测试覆盖取消、损坏、未知版本和恢复身份冲突。

完成这些后，命令行版与 RASP5 会共享同一套科学计算和结果格式；RASP5 只增加交互、项目管理、
可视化和报告，不形成第二套数值语义。

## 37. 相关文档

- [命令行完整教程](cli-tutorial.md)：面向普通用户从安装到分析；
- [现有 CLI 接入说明](rasp-cli-integration.md)：较短的宿主调用概览；
- [宿主状态机](rasp-host-state-machine.md)：退出、取消、恢复和导入判断；
- [分析请求](analysis-request.md)：`analysis.tsv` 全字段；
- [版本化分析结果](analysis-result.md)：目录、重放和身份；
- [单模型统一工作流](analysis-workflow.md)：拟合加随机历史；
- [多模型统一工作流](model-workflow.md)：模型比较、选择和恢复；
- [参数表](parameter-table.md)：fixed/free/derived 与表达式；
- [模型平均](model-average.md)：祖先范围与 split scenario 平均；
- [生物地理随机历史格式](bsm-output-formats.md)：full/compact/summary；
- [随机历史检查](bsm-inspection.md)：quick 与 deep；
- [进度与取消](progress-and-cancellation.md)：机器进度协议；
- [输入验证](input-validation.md)：树和范围的稳定诊断；
- [树、古老末端和直接祖先](tree-input-and-fossil-tips.md)；
- [随机化石放置](random-fossil-placement.md)；
- [旧格式输入导入](legacy-input-import.md)；
- [兼容政策](compatibility-policy.md)；
- [性能基准](performance-benchmark.md)；
- [BioGeoBEARS 功能对照](biogeobears-parity-matrix.md)；
- [v0.1 版本范围](v0.1-release-notes.md)。

## 38. 给 RASP5 开发者的直接交接

```text
引擎：biogeo-cli 0.1.0
正式平台：x86_64 Windows MSVC
集成方式：完整版本目录中的子进程
握手：engine-info + schemas/registry.tsv 精确核对
全局机器选项：--error-format tsv --progress-format tsv
单模型：analysis-template -> analysis-plan -> analysis-run
单模型验收：analysis-result-inspect --replay
随机历史：model-bsm，RASP5 默认 compact
随机历史验收：bsm-inspect --deep
多模型：model-workflow-plan -> model-workflow
默认严格策略：未知格式、字段和错误码全部拒绝
首要待补 Rust 接口：注册并持久化单模型 ancestral/split posterior artifact
```

RASP5 不需要 BioGeoBEARS、R、Python 或 PowerShell 才能运行发布版计算。它只需要完整
biogeo-rs Windows 发布目录；R/Python/PowerShell 是本仓库生成 golden、维护 fixture、benchmark
和打包验证所用的开发工具。
