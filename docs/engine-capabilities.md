# 引擎版本与能力发现

## 目的

新版 RASP 和命令行脚本不能通过解析 `--help`、文件名或英文日志判断某个 exe 支持哪些模型和
结果格式。`biogeo-cli` 提供两个不同层级的入口：

```powershell
biogeo-cli --version
biogeo-cli engine-info
```

`--version` 是简短的人类可读输出，例如 `biogeo-cli 0.1.0`。它没有机器 schema，不能代替能力
协商。`engine-info` 输出版本化的 `biogeo-engine-capabilities-v1` 键值 TSV，供新版 RASP、安装器
和自动化脚本读取。

## 能力记录

能力记录包括：

- 引擎版本、构建操作系统、架构、平台族、debug/release 和指针宽度；
- 当前进程可见并行度；
- schema registry 格式、已注册格式数量和完整格式号集合；
- `biogeo-compatibility-policy-v1`、严格格式号策略、未知格式/字段处理和最低弃用窗口；
- 已弃用但仍可读取的格式，以及当前没有弃用命令的明确记录；
- 六个正式 preset；
- 推荐命令和仍受支持的专用兼容命令；
- 树、范围、末端观测和生物地理随机历史输出类型；
- 参数表、优化、后验、batch、模型比较、模型平均、化石放置、BSM、分片、恢复和统一工作流
  的能力标志；
- 所有已宣告命令是否提供子命令级帮助；
- Windows 进程遥测以及尚未实现的 Linux/Slurm 资源发现标志。

`public_formats` 必须与相邻 `schemas/registry.tsv` 的格式号集合精确相同。Rust 真实进程测试和
Windows 安装后测试都会执行双向比较，因此新增或删除 schema 时必须同步升级能力记录。

`available_parallelism` 是查询时操作系统向当前进程报告的并行度，不是 BSM 最终 worker 数，
也不是固定硬件核心数。实际随机历史线程仍由样本数、`--bsm-threads`、在途窗口和内存预算共同
决定。

## 新版 RASP 启动握手

新版 RASP 启动计算任务前应：

1. 调用 `engine-info`，要求退出码 0、stderr 为空且 `status=ready`；
2. 按 `format` 选择能力 schema，未知能力格式必须明确拒绝；
3. 读取 `engine_version` 并记录到项目或任务诊断；
4. 校验发布目录中的 registry 格式，并检查其格式号集合与 `public_formats` 相同；
5. 根据具体任务检查 preset、输出级别和 `supports_*`，不要根据版本号范围猜功能；
6. 要求兼容政策版本可识别，并按 `unknown_*_policy` 拒绝未知内容；
7. 再生成请求并调用 `analysis-plan` 或高级工作流。

能力为 `false` 表示当前二进制明确没有该功能。例如 v0.1 的 Linux cgroup 和 Slurm 探测为
`false`；宿主不能把字段存在误解为功能已经实现。未来破坏性修改将发布新的 capabilities 格式，
不会悄悄改变 v1 字段语义。

完整格式升级与弃用规则见 [`compatibility-policy.md`](compatibility-policy.md)。终端用户可用
`biogeo-cli <command> --help` 查看命令专属参数；帮助文本不是机器协商接口，详见
[`command-line-help.md`](command-line-help.md)。
