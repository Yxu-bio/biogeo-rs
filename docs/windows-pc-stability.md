# Windows PC 长时间稳定性测试

## 测试目标

该门禁验证同一 Windows PC 在持续计算、频繁写盘和重复启动下不会产生科学结果漂移，也不会在
取消、写盘失败或恢复后保留未提交的随机历史行。它不代替 Linux/Slurm 服务器测试。

## 常规门禁与长稳验收

常规 Windows 发布门禁只运行一轮短检查，确认安装版 EXE 能执行六模型优化、随机历史和深度
检查。多小时测试独立运行，避免每次普通开发回归都等待数小时：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File validation/checks/check-windows-pc-stability.ps1
```

默认持续 120 分钟。每轮使用确定性的 Ponerinae 32-tip、7-area、120-state fixture：

- 优化六个正式 preset；
- 为 DEC 生成 4096 条 compact、分片、带 checkpoint 的生物地理随机历史；
- 对输出执行逐行深度检查；
- 对六模型参数、比较/模型平均结果和八张随机历史数据表分别计算 SHA-256 聚合指纹；
- 记录每轮耗时、逻辑输出量、进程峰值 working set、样本数和沿枝事件数。

第一轮完整结果保留用于审计；后续成功轮次核对指纹后删除大目录，只保留逐轮记录和日志。脚本
在每轮开始前检查可用空间，默认低于 2048 MiB 时停止，不用长稳测试主动填满用户磁盘。
常规 Windows 发布门禁会用一个不可能满足的阈值验证该预检确实在第一个分析轮次前拒绝。

可用固定轮数做快速检查：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File validation/checks/check-windows-pc-stability.ps1 `
  -Cycles 2 -BsmSamples 64
```

结果目录包含 `evidence.tsv`、`cycles.tsv`、逐轮 stdout/stderr 和保留的第一轮结果。证据同时
绑定 CLI 版本与 SHA-256、完整输入指纹、实际请求指纹、可见逻辑核心数和逐轮记录哈希。目录严格
非覆盖；失败时保留现场。

## 取消、磁盘不足与恢复

取消测试由新版 RASP 无 GUI 参考宿主通过 stdin 发送 `cancel`，要求退出码 130、状态
`cancelled`、已提交 checkpoint 可读，并在改变线程数后恢复到逐字节相同结果。

真实填满系统磁盘既危险又不可重复，因此磁盘不足采用测试构建内的一次性 `StorageFull` 故障：

1. 八张表写到一半时失败；
2. checkpoint 将部分表同步到磁盘时失败；
3. 表同步完成后，检查点临时文件写入时失败；
4. 三种情况都必须清理临时检查点并回滚到最后一个已发布 checkpoint；
5. 取消故障后恢复，八张表必须与一次性基线逐字节一致。

故障注入代码只存在于 `#[cfg(test)]`，正式 CLI 没有隐藏环境变量或故障开关。真实文件系统的
空间、权限和硬件故障仍可能使回滚本身失败；此时 CLI 返回组合恢复错误并保留目录，不宣称结果
完整。

## 2026-08-24 正式结果

本机 16 线程正式门禁使用受测 EXE
`02164857823c378117f5b037eec1baf3386f0547b0d0cd7be1f656718313daf7`，连续运行
7,210.72 秒并通过：

- 367 轮，每轮六个 preset 优化和 4096 条随机历史；
- 合计 1,503,232 条随机历史、17,664,484,003 bytes 累计逻辑写入；
- 每轮平均 15.341 秒，最短 14.415 秒，最长 17.413 秒；
- 最高记录 working set 41,398,272 bytes，367 份 stderr 均为空；
- 优化指纹和八张随机历史表指纹在全部轮次中零漂移；
- 保留首轮再次独立深度检查 295 个文件、934,758 行，诊断违规为 0。

证据位于
`validation/benchmark-runs/windows-pc-stability-2h-20260824T084700Z/`。其中保留受测 EXE、
首轮结果、`evidence.tsv`、`cycles.tsv` 和 `run-context.tsv`；上下文文件 SHA-256 为
`b2d2c097f8c153c7ef233d76669471d73e23d83a8fff616a263f581da3004bc1`。

随后加入的故障测试代码只在 `#[cfg(test)]` 下编译，但重建仍会改变 EXE 文件哈希。最终 release
EXE `049b23e8bc29469652739b45cca71f3da76171c7843e79ef320190a6ca16a7be` 因此另跑 10 个同规格
轮次、40,960 条随机历史；输入、请求、优化指纹和随机历史指纹均与两小时证据完全相同。桥接证据
位于 `validation/benchmark-runs/windows-pc-stability-final-release-10cycles-20260824/`。
