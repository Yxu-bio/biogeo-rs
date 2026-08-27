# 命令行帮助契约

## 用法

不带参数或使用全局 `--help` 时，CLI 显示全部命令的总览。查看一个命令时应把帮助标志放在
命令名之后：

```powershell
biogeo-cli analysis-workflow --help
biogeo-cli model-optimize --help
biogeo-cli model-bsm --help
```

子命令帮助只列出该命令可接受的参数，并同时说明：

- 完整调用形状和必需参数；
- 可选参数及其默认行为；
- 成功输出或权威结果目录的格式号；
- 该命令可能使用的稳定退出码；
- 是否属于推荐入口或受支持的底层兼容入口。

帮助请求在读取树、范围、参数表或结果目录前完成。因此，下面的命令只显示帮助，不会尝试打开
`missing.tree`：

```powershell
biogeo-cli model-evaluate --tree missing.tree --help
```

全局 `--error-format` 和 `--progress-format` 必须放在子命令之前。子命令帮助中的“Global prefix
options”只显示与该命令有关的全局前缀参数。

## 人工帮助与机器协商

`command --help` 面向终端用户，文本布局和措辞不是机器 schema。新版 RASP 不应解析帮助文本来
判断能力，而应调用 `engine-info`，读取 `recommended_commands`、`compatibility_commands` 和
`supports_subcommand_help`。帮助中列出的结果格式仍应按发布包 `schemas/registry.tsv` 解析。

所有 `engine-info` 宣告的推荐命令和兼容命令都必须存在子命令帮助。真实进程测试会逐个启动
`biogeo-cli <command> --help`，验证退出码为 0、stderr 为空，并对容易混淆的命令检查无关参数没有
泄漏。例如 `model-bsm` 不得显示 `--tree` 或 `--d`，`convert-tree` 不得显示范围和随机历史参数。

