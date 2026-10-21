# biogeo-rs

用 Rust 编写的历史生物地理命令行工具，可单独运行，也可作为 RASP5 的计算后端。
可执行程序名为 `biogeo-cli`。

它支持 DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE 和 BAYAREALIKE+J，
可以估计参数、比较模型、计算祖先范围概率，并抽取生物地理随机历史。

**当前预发布版：[0.1.7-dev.20260930](https://github.com/Yxu-bio/biogeo-rs/releases/tag/v0.1.7-dev.20260930)。**
它包含 0.1.0 之后的修复和 RASP5 接口扩展。改动见[发布说明](docs/v0.1.7-dev-release-notes.md)和
[变更记录](CHANGELOG.md)。旧版 [0.1.0](https://github.com/Yxu-bio/biogeo-rs/releases/tag/v0.1.0) 仍可下载。

目前验证的平台是 **64 位 Windows**。Linux 和服务器环境还没有完成同等测试。
项目仍在开发，BioGeoBEARS 的绘图及 R 辅助函数没有逐函数移植。

## 安装

Windows 用户从上面的发布页面下载 `biogeo-cli-0.1.7-dev.20260930-x86_64-pc-windows-msvc.zip`，
解压后在目录里打开 PowerShell，运行：

~~~powershell
.\biogeo-cli.exe --version
.\biogeo-cli.exe --help
~~~

预编译包不需要安装 Rust、R 或 Python。这是命令行程序，需要在终端中带参数运行。

从源码构建：

~~~powershell
git clone https://github.com/Yxu-bio/biogeo-rs.git
cd biogeo-rs
cargo build --release --locked -p biogeo-cli
.\target\release\biogeo-cli.exe --version
~~~

源码构建需要 Rust MSVC 工具链和 Microsoft C++ 构建工具。完整步骤见[安装教程](docs/installation.md)。

## 十分钟跑通第一个分析

包内的 `examples/analysis_request` 是一个两末端、两区域的 DEC 示例。
它很小，适合检查安装和熟悉输出。

下面的命令在解压目录中运行。若从源码构建，将第一行改为
`$exe = ".\target\release\biogeo-cli.exe"`。

~~~powershell
$exe = ".\biogeo-cli.exe"

& $exe analysis-plan --request examples\analysis_request\analysis.tsv
& $exe analysis-run --request examples\analysis_request\analysis.tsv --output-dir output\first-dec
& $exe analysis-result-inspect --analysis-result output\first-dec --replay
~~~

`analysis-plan` 检查输入并估计状态数；`analysis-run` 估计 d、e 和祖先概率；
`--replay` 从保存的输入和参数重新计算，核对结果是否完整。

~~~mermaid
flowchart LR
    T["树"] --> P["检查输入"]
    R["分布矩阵"] --> P
    M["模型参数表"] --> P
    P --> F["拟合参数与祖先概率"]
    F --> C["模型比较"]
    F --> B["生物地理随机历史"]
    B --> S["条件来源分配"]
~~~

主要结果位于：

~~~text
output/first-dec/
  metadata.tsv                 # lnL、参数优化状态、输入身份
  source-parameters.tsv        # 用户提交的参数设置
  resolved-parameters.tsv      # 最终参数值
  optimization-starts.tsv      # 各优化起点的初值、结果和停止原因
  input-bundle/                # 本次分析使用的输入副本
  posteriors/                  # 节点、边、状态字典和祖先/分裂概率
~~~

示例已经启用祖先和分裂概率。自己的请求若没有启用相应选项，就不会生成对应概率表。
看结果时请同时检查 `lnL` 和 `optimization_converged`；有一个数值结果不等于优化已经收敛。

再从该拟合抽取 100 条生物地理随机历史：

~~~powershell
& $exe model-bsm --analysis-result output\first-dec --bsm-samples 100 --seed 1 --bsm-output-level compact --bsm-threads auto --bsm-shard-samples 50 --bsm-output-dir output\first-dec-bsm
& $exe bsm-inspect --bsm-result output\first-dec-bsm --deep
~~~

这 100 条是拟合模型下可能的历史，包含沿枝事件、节点分裂和状态占据时间。
线程数可以通过 `--bsm-threads` 调整；相同输入、seed 和样本编号的结果不随线程数改变。
再次运行示例时请使用新目录，或在支持恢复的命令中明确指定 `--resume`。

从入门到时间分层、六模型比较和批量任务的完整示例，见[命令行教程](docs/cli-tutorial.md)。

## 使用自己的树和分布数据

先生成请求和参数表：

~~~powershell
& $exe analysis-template --preset dec --mode optimize --output-dir my-analysis
~~~

把树和分布矩阵放进该目录，在 `analysis.tsv` 中设置文件路径，再运行
`analysis-plan` 和 `analysis-run`。模板缺少数据时会报告缺失项。

树支持有根 Newick 和受支持的 NEXUS。末端名称必须与分布矩阵一致。
推荐的范围矩阵是 TSV，第一列为类群名，其余列用 0/1 表示是否出现在对应区域。
下面用空格对齐展示；文件中各列以制表符分隔。

~~~text
tip          AreaA  AreaB  AreaC
Species_1    1      0      1
Species_2    0      1      0
~~~

已有 CSV 或 BioGeoBEARS/LAGRANGE 的 `.data` 文件也可以读取；需要转换时用：

~~~powershell
& $exe convert-ranges --ranges input.data
~~~

缺失枝长、不确定范围、非现生末端和化石各有明确的输入规则，见
[输入检查](docs/input-validation.md)、[树与化石末端](docs/tree-input-and-fossil-tips.md)、
[不确定范围](docs/ambiguous-ranges.md)和[随机化石放置](docs/random-fossil-placement.md)。

区域数与 `max_range_size` 一起决定计算规模。A 个区域、最大范围 K、不含 null 时：

~~~text
states = C(A,1) + C(A,2) + ... + C(A,K)
~~~

例如 7 区域、最大范围 5 为 119 个状态，包含 null 时为 120 个。
区域多时，先看 `analysis-plan` 的状态数估计。程序没有写死本机专属的状态数上限；
可以用 `max_states` 设置任务自己的限制。

## 模型与参数

六个 preset 共用同一个似然引擎：

| preset | 主要设置 |
|---|---|
| DEC | d/e 沿枝变化与 DEC 节点分裂规则 |
| DEC+J | DEC 加 founder-event 参数 j |
| DIVALIKE | DIVA-like 的节点分裂规则 |
| DIVALIKE+J | DIVALIKE 加 j |
| BAYAREALIKE | 更强调 range-copying 的分裂规则 |
| BAYAREALIKE+J | BAYAREALIKE 加 j |

23 行参数表支持固定、释放、边界和表达式联动。时间分层、扩散倍率、距离、环境距离、
面积、邻接和允许范围约束可以参与同一分析。具体参数含义见[参数表说明](docs/parameter-table.md)。

[多模型工作流](docs/model-workflow.md)可运行模型比较和 AIC/AICc 模型平均。
小树的 AICc 可能没有定义，此时相应权重和排名为 `NA`。
批量拟合标准模型时，请把无 J 模型排在对应 +J 模型之前，便于复用前者的拟合结果；
+J 仍会保留其他优化起点。

## 来源抽样与条件路线概率

已有 BSM 事件可以继续做来源分配。比如 `AB -> ABC`，
若当前时期 A→C、B→C 的有效权重为 9:1，两者的条件来源概率就是 0.9 和 0.1。

按独立 seed 抽取来源：

~~~powershell
& $exe bsm-assign-sources --analysis-result output\first-dec --bsm-result output\first-dec-bsm --seed 29 --output-dir output\sources
~~~

保留所有候选路线概率：

~~~powershell
& $exe bsm-assign-sources --analysis-result output\first-dec --bsm-result output\first-dec-bsm --allocation-mode probabilities --output-dir output\routes
& $exe bsm-sources-inspect --analysis-result output\first-dec --bsm-result output\first-dec-bsm --source-result output\routes --deep
~~~

概率模式支持多区域 founder 目标，例如 `AB -> CD` 的 A→C、A→D、B→C、B→D 四种贡献。
它不使用随机数，所以不传 seed。同一个事件的候选概率之和为 1，候选表多行仍只代表一个事件。
这些是给定已抽中事件后的条件分配，不能当作已观测到的迁移路线。

调用、文件和网络计数方法见[来源抽样](docs/bsm-source-attribution.md)和
[条件路线概率](docs/bsm-joint-route-probabilities.md)。包内 `examples/bsm_joint_routes`
提供四区域、两时期的非均匀权重示例。

## 接入 RASP5

RASP5 通过子进程调用 CLI，读取版本化结果。单模型后验提供完整的节点与边映射；
BSM v3 保存实际抽中的分裂类别、founder 子代顺序和事件时间。

接入方应读取 `engine-info` 和包内 `schemas/registry.tsv`，
并按文件声明的格式解析。节点身份使用显式 ID 和边关系，区域状态使用 bitset。

0.1.0、0.1.3 与本次构建的升级差异，见[本版发布说明](docs/v0.1.7-dev-release-notes.md)。
字段和例子见[单模型后验](docs/single-model-posteriors.md)、
[BSM v3](docs/bsm-v3-contract.md)及[RASP5 接入手册](docs/rasp5-integration-handbook.md)。

## 验证与限制

BioGeoBEARS 1.1.3 用于模型语义和数值对照，LAGRANGE-ng 用作独立的语义与性能参考。
生物地理随机历史按事件数、事件类型、时期和占据时间做分布比较；两种实现不用相同 seed 逐条复现。

本次构建的回归结果为 457 项通过、2 项既有测试默认跳过。
多区域 founder 示例中，6,030 条候选概率与 R 的最大差为 `1.11e-16`；
单线程、多线程和分片结果也做了比较。详见[测试报告](docs/bsm-joint-route-test-report.md)。

性能取决于状态数、树、模型和输出量，[基准记录](docs/performance-benchmark.md)给出了具体配置。
目前单模型似然与优化使用 1 个计算 worker；BSM 支持多线程。
CLI 提供计算与数据输出，绘图和交互由 RASP5 或其他程序完成。

普通分析只需 exe。R、PowerShell、Python 文件分别用于 BioGeoBEARS 对照、
Windows 测试与打包、数据维护。开发验证保留在 `validation/`，
本地 R 安装、大型运行结果和其他引擎二进制不进源码仓库。

~~~powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
~~~

## 文档

- [安装教程](docs/installation.md) · [命令行教程](docs/cli-tutorial.md)
- [分析请求](docs/analysis-request.md) · [参数表](docs/parameter-table.md) · [结果目录](docs/analysis-result.md)
- [多模型工作流](docs/model-workflow.md) · [模型平均](docs/model-average.md) · [批量数据集](docs/dataset-batch.md)
- [时间分层示例](examples/stratified_analysis) · [生物地理随机历史输出](docs/bsm-output-formats.md)
- [BioGeoBEARS 功能对照](docs/biogeobears-parity-matrix.md) · [中文 BioGeoBEARS 教程](docs/biogeobears-chinese-tutorial.md)
- [验证记录](validation/README.md) · [变更记录](CHANGELOG.md)

## 许可证与引用

本项目采用 [GPL-3.0-or-later](LICENSE)。第三方依赖和验证数据的说明见
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)。

论文中请记录实际使用的版本、输入、参数设置和 seed。当前版本可引用为：

~~~text
Xu, Y. (2026). biogeo-rs (Version 0.1.7-dev.20260930) [Computer software].
https://github.com/Yxu-bio/biogeo-rs
~~~

GitHub 的 **Cite this repository** 读取 [CITATION.cff](CITATION.cff)。
