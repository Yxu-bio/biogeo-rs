# 源码与许可证工程审计

这是发布前的工程记录，不是法律意见。

## Rust 运行时

- `crates/biogeo-core` 和 `crates/biogeo-cli` 是本项目的 Rust 实现。
- 运行时不链接 BioGeoBEARS、R、rexpokit、cladoRcpp 或 LAGRANGE-ng。
- 当前 Windows 依赖图中的 Rust crate 均声明 MIT、Apache-2.0、BSD-2-Clause 或这些许可证的组合。
- 完整 crate 版本、上游仓库和许可文本见 `THIRD-PARTY-NOTICES.md` 与 `third-party-licenses/`。

## BioGeoBEARS 对照环境

- 项目隔离环境使用 BioGeoBEARS `1.1.3`。
- 本地源码修订为 `7d2092f94a5d2b598807771379ef6c58a84b4fb3`，上游为
  `https://github.com/nmatzke/BioGeoBEARS.git`。
- 该版本 `DESCRIPTION` 声明 `License: GPL (>= 2)`。
- `validation/r-cache/` 和 `validation/r-lib/` 是可重建的本地测试环境，已由 `.gitignore` 排除，不进入
  GitHub 源码或 Windows 运行包。
- `validation/fixtures/biogeobears_official/` 包含从 BioGeoBEARS 官方示例导入的树、范围和矩阵，
  以及明确标记的派生测试数据。这些文件应按 BioGeoBEARS 的 GPL-2.0-or-later 条款保留来源和许可说明。

## LAGRANGE-ng

- LAGRANGE-ng 只是独立语义和性能参考，不决定 BioGeoBEARS-like 模型的通过标准。
- 本地可执行文件位于被 `.gitignore` 排除的 `validation/tools/`，不进入 GitHub 源码或 Windows 运行包。
- 公开仓库只需保留运行与比较脚本；不应上传 RASP 中的本地 LAGRANGE-ng 二进制副本。

## Ponerinae 验收数据

- 完整 1534-tip 本地参考数据不进入源码仓库。
- 仓库保留一个确定性派生的 32-tip、7-area 验收子集和两个名称映射表。
- 上游为 Doré et al. (2025) 的公开研究仓库，采用 MIT License；论文 DOI、核对修订号和完整许可
  条款见 `validation/fixtures/ponerinae_32tip_7area/LICENSE-NOTICE.md`。

## 项目许可证

项目所有者已经决定整个仓库使用 `GPL-3.0-or-later`。许可证全文位于仓库根目录的 `LICENSE`，
Cargo 工作区及两个 Rust crate 也使用相同的 SPDX 标识。

该许可证与 BioGeoBEARS 官方测试数据的 `GPL-2.0-or-later` 条款兼容，也与当前 Rust 运行时的
MIT、Apache-2.0 和 BSD 系列依赖兼容。分发本项目的源码或二进制时，应同时提供 GPL 许可证、
对应源代码以及第三方许可证说明。

## 当前结论

1. 项目许可证已经确定为 `GPL-3.0-or-later`。
2. 二进制依赖许可文本已齐备。
3. 官方 BioGeoBEARS 验证数据的 GPL 来源已经明确。
4. 本地 R 测试环境和 LAGRANGE-ng 可执行文件不进入源码仓库或发布包。
5. Windows 代码签名和专用 CI 是可选的发布增强，不影响 GPL 科研软件公开分发。
