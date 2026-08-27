# Windows 科研软件发布与文件校验

## 当前结论

本项目按公开科研软件准备 GitHub 发布。Windows 代码签名和专用生产 CI 都不是公开发布的
必要条件。当前 `0.1.0` 可以保持：

```text
build_origin = local_worktree
authenticode_status = unsigned
project_license_status = GPL-3.0-or-later
public_distribution_allowed = true
```

整个项目已经采用 `GPL-3.0-or-later`，许可证全文、第三方许可证和来源记录均随源码及 Windows
包提供。未签名状态不影响公开发布、安装、计算或新版 RASP 接入。

## GitHub 发布需要什么

科研软件公开版本应提供：

1. 源代码、明确的项目许可证和第三方许可证说明；
2. 版本号、变更记录、已知限制和可引用信息；
3. 安装与命令行示例；
4. 自动测试以及与 BioGeoBEARS 的对照结果；
5. 若发布预编译 ZIP，同时提供 SHA-256 校验值。

GitHub Actions、Zenodo DOI 和 Windows 代码签名都可以以后增加，但不阻止首个公开科研版本。

## 包内记录

`biogeo-windows-package-v3` 仍记录 Rust/Cargo 版本、`Cargo.lock`、源码 revision、构建来源、
源码清单和 EXE 的 SHA-256。这些信息用于复现计算和排查拿错版本的问题。

安装器会校验包内全部文件的大小和 SHA-256，并实际启动 EXE。ZIP 外的 `.sha256` 用于确认下载
或复制后的归档没有变化。这些校验不证明发布者身份，但对 GitHub 科研软件的常规分发已经足够。

## 可选代码签名

打包脚本保留 `-SigningCertificateThumbprint` 和 `-TimestampServer`，仅供未来自愿签名使用。
未提供这两个参数时会正常生成未签名包，未签名不影响公开状态、安装、计算或新版 RASP 接入。

如果用户明确提供 `-ExpectedSignerThumbprint`，安装器才要求 EXE 具有匹配的有效签名；未提供时
不要求签名。这样保留了可选能力，但不把商业分发条件强加给科研软件。

## 自动验证

Windows 自动检查覆盖：

- 未签名包可以直接构建和安装；
- 全部文件的大小和 SHA-256 必须与清单一致；
- 被修改的包不会安装；
- 用户主动请求签名时，不存在的证书不会留下半成品；
- 用户主动指定发布者指纹时，未签名或签名不匹配的 EXE 会被拒绝；
- 安装后的 EXE 会完成命令行和科学计算冒烟测试。
