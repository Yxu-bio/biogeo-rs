# 自动检查

这里保存 PowerShell 自动门禁。脚本负责组合 Rust 测试、固定 fixture、公开 CLI 示例、
BioGeoBEARS 对照以及 Windows 发布包检查；它们不进入 `biogeo-cli.exe`。

由小到大的入口：

```powershell
# 公开示例和恢复流程
powershell -ExecutionPolicy Bypass `
  -File validation/checks/check-public-cli-examples.ps1

# 全部框架语义
powershell -ExecutionPolicy Bypass `
  -File validation/checks/check-framework-semantics.ps1

# v0.1 完整候选门禁
powershell -NoProfile -ExecutionPolicy Bypass `
  -File validation/checks/check-v0.1-release-candidate.ps1
```

单项脚本按 `check-<subject>.ps1` 命名。详细检查范围见父目录的
[验证说明](../README.md)。
