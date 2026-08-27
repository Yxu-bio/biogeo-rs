# 项目许可证状态

项目所有者已选择 `GPL-3.0-or-later`。完整许可条款见仓库根目录的 `LICENSE`。

这意味着用户可以运行、研究、修改和再分发本软件。对项目或其派生版本进行再分发时，
应按 GPL-3.0-or-later 提供对应源码并保留许可说明。

当前 Rust 运行时依赖均使用与 GPL-3.0-or-later 兼容的宽松许可。BioGeoBEARS 官方验证数据
使用 `GPL-2.0-or-later`，可在 GPL-3.0-or-later 项目中分发。来源、版本和第三方许可详见：

- `docs/source-and-license-audit.md`
- `THIRD-PARTY-NOTICES.md`
- `third-party-licenses/`
- `validation/fixtures/biogeobears_official/LICENSE-NOTICE.md`
- `validation/fixtures/ponerinae_32tip_7area/LICENSE-NOTICE.md`

本地 R 环境、构建产物、完整 Ponerinae 参考数据和 LAGRANGE-ng 二进制由 `.gitignore` 排除，
不作为本项目 GitHub 源码发布的一部分。公开的 32-tip Ponerinae 派生测试子集保留其上游 MIT
许可与论文引用。Windows 代码签名仍然是可选项，与 GPL 许可无关。
