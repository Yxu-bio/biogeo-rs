# biogeo-cli

`biogeo-cli` 是一个用 Rust 实现的历史生物地理命令行引擎。它使用一套可配置的统一似然引擎，
支持 DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE 和 BAYAREALIKE+J，并可以独立运行或作为
新版 RASP 的计算后端。

当前版本是 `0.1.0` 公开科研发布候选版。已完成 64 位 Windows 测试；Linux 和服务器调度尚未系统验证。

## 主要能力

- 六个 BioGeoBEARS-like 模型配置，共用同一套状态空间、沿枝过程、节点分裂和似然计算。
- 23 行参数表，支持固定、自由估计、参数联动、多起点优化和模型比较。
- 时间分层、扩散倍率、距离、环境距离、面积、邻接和允许分布范围约束。
- 祖先分布范围、节点分裂情景、AIC/AICc、模型平均和似然比检验。
- 生物地理随机历史，支持确定性并行、分片输出、检查点、取消和中断后继续。
- Newick/NEXUS、古老末端、直接祖先、随机化石放置和常见 BioGeoBEARS/RASP 分布数据转换。
- 版本化的 TSV 和结果目录，供命令行、批处理脚本和新版 RASP 稳定读取。

## 快速开始

需要已安装 Rust 工具链。在仓库根目录构建：

```powershell
cargo build --release --locked -p biogeo-cli
```

先检查示例的输入、状态数和估计资源，不进行优化：

```powershell
.\target\release\biogeo-cli.exe analysis-plan `
  --request examples\analysis_request\analysis.tsv
```

运行 DEC 参数估计并写入一个可移动的结果目录：

```powershell
.\target\release\biogeo-cli.exe analysis-run `
  --request examples\analysis_request\analysis.tsv `
  --output-dir output\quickstart-dec
```

核对结果，并用结果内保存的输入重新计算 lnL：

```powershell
.\target\release\biogeo-cli.exe analysis-result-inspect `
  --analysis-result output\quickstart-dec `
  --replay
```

从已拟合模型生成 100 条生物地理随机历史：

```powershell
.\target\release\biogeo-cli.exe model-bsm `
  --analysis-result output\quickstart-dec `
  --bsm-samples 100 `
  --bsm-output-dir output\quickstart-dec-bsm `
  --bsm-output-level compact `
  --bsm-threads auto `
  --bsm-shard-samples 50

.\target\release\biogeo-cli.exe bsm-inspect `
  --bsm-result output\quickstart-dec-bsm `
  --deep
```

结果目录不会覆盖已有目录。重跑示例时应换用新的 `--output-dir`。

## 多模型分析

下面的示例会拟合 DEC 和 DEC+J，比较 AIC，并从明确选定的 DEC 结果生成生物地理随机历史：

```powershell
.\target\release\biogeo-cli.exe model-workflow-plan `
  --request examples\model_workflow\workflow.tsv

.\target\release\biogeo-cli.exe model-workflow `
  --request examples\model_workflow\workflow.tsv `
  --output-dir output\quickstart-models
```

## Windows 打包

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File packaging\build-windows-package.ps1
```

输出位于 `dist/`，包含 EXE、文件清单、SHA-256、schema、示例和文档。未签名包可以正常
构建和安装；Windows 代码签名是可选项。

## 文档

- [v0.1 版本范围](docs/v0.1-release-notes.md)
- [分析请求格式](docs/analysis-request.md)
- [参数表](docs/parameter-table.md)
- [生物地理随机历史输出](docs/bsm-output-formats.md)
- [新版 RASP 接入](docs/rasp-cli-integration.md)
- [与 BioGeoBEARS 的功能对照](docs/biogeobears-parity-matrix.md)
- [源码与许可证工程审计](docs/source-and-license-audit.md)
- [验证与性能结果](validation/README.md)

## 与 BioGeoBEARS 和 LAGRANGE-ng 的关系

本项目是独立的 Rust 实现，不是 BioGeoBEARS 的 R 包装层。BioGeoBEARS 1.1.3 用作数值和模型语义
对照；LAGRANGE-ng 仅作为独立 LAGRANGE 语义和性能参考。随机历史用大量独立样本比较统计分布，
不要求不同实现在同一随机种子下生成逐条相同的路径。

## 项目状态与许可证

整个项目采用 [GNU General Public License v3.0 or later](LICENSE)，即
`GPL-3.0-or-later`。你可以使用、研究、修改和再分发本项目；再分发本项目或其修改版本时，需要
继续遵守 GPL 并提供相应源代码和许可证说明。第三方依赖和验证数据来源见
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) 和 [LICENSE-STATUS.md](LICENSE-STATUS.md)。
