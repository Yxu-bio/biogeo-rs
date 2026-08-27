# LAGRANGE-ng 本地二进制审计

测试对象：

```text
源文件: E:\RASP\engines\lagrange-ng\lagrange-ng.exe
项目隔离副本: validation/tools/lagrange-ng/lagrange-ng.exe
文件大小: 38870588 bytes
源文件时间: 2026-07-08 22:35:13
运行时版本: v0.7.2-7
```

项目脚本会优先使用 `validation/tools/lagrange-ng/lagrange-ng.exe`，所以测试前已用：

```powershell
powershell -ExecutionPolicy Bypass -File validation/lagrange-ng/copy-local-lagrange-ng.ps1 -Source E:\RASP\engines\lagrange-ng
```

把修复后的 RASP 版本复制到项目内隔离目录。

## 结论

这次修复后的 LAGRANGE-ng 已经能在 `mode = evaluate` 下正确使用配置文件里的固定参数：

```text
dispersion = 0.1
extinction = 0.2
```

实际输出：

```text
Period: default, Dispersion: 0.1, Extinction: 0.2
```

把参数改成：

```text
dispersion = 2.0
extinction = 3.0
```

实际输出：

```text
Period: default, Dispersion: 2, Extinction: 3
```

因此之前记录的 “evaluate 模式忽略 dispersion/extinction，始终使用 0.01/0.01” 问题已经修复。

## 官方示例结果

审计命令：

```powershell
powershell -ExecutionPolicy Bypass -File validation/lagrange-ng/audit-lagrange-ng-official.ps1 -ScratchRoot C:\tmp
```

本次运行结果写入：

```text
validation/lagrange-ng-official-output.tsv
```

不含临时目录字段的冻结参考摘要位于：

```text
validation/reference/lagrange-ng-official.tsv
```

当前结果摘要：

```text
readme_minimal_optimize:
  Initial LLH = -66.49851
  Final LLH   = -33.2499

repo_example_optimize:
  Initial LLH = -72.99674
  Final LLH   = -37.76632

readme_evaluate_d0.1_e0.2:
  LLH = -41.38613
  actual d/e = 0.1 / 0.2
  status = requested_rates_used

readme_evaluate_d2_e3:
  LLH = -40.16422
  actual d/e = 2 / 3
  status = requested_rates_used
```

README 中展示的旧示例值仍是：

```text
Initial LLH = -66.235818
Final LLH   = -31.424296
```

当前源码仓库的 `example/example.conf` 与 README 的最小配置并不完全一致，因此优化模式数值不一致不能直接判定为本地二进制错误。固定参数 evaluate 模式已经通过参数生效检查。

## 项目 DEC Fixture 结果

审计命令：

```powershell
powershell -ExecutionPolicy Bypass -File validation/lagrange-ng/run-lagrange-ng-dec.ps1 -ScratchRoot C:\tmp
```

本次运行结果写入：

```text
validation/lagrange-ng-output.tsv
```

独立 LAGRANGE-ng 语义基线位于：

```text
validation/reference/lagrange-ng-dec.tsv
```

当前结果：

```text
two_tip_unit_split_null:
  LAGRANGE-ng lnL = -1.729076
  actual d/e = 0.1 / 0.2
  status = requested_rates_used

three_tip_nested_null:
  LAGRANGE-ng lnL = -3.676267
  actual d/e = 0.1 / 0.2
  status = requested_rates_used
```

对应 BioGeoBEARS DEC 固定参数 golden 是：

```text
two_tip_unit_split_null:
  BioGeoBEARS lnL = -1.967899379931347

three_tip_nested_null:
  BioGeoBEARS lnL = -3.767920515476409
```

这说明修复后的 LAGRANGE-ng 可以作为独立 LAGRANGE 语义和性能参考，但不能直接替代 BioGeoBEARS DEC preset 的 golden。

## 与 BioGeoBEARS/Rust DEC 的差异

差异不只是 null range。对 `two_tip_unit_split_null`：

```text
LAGRANGE-ng lnL                 = -1.729076
Rust DEC include_null flat lnL  = -1.967899400953594
Rust DEC no-null flat lnL       = -1.597842771081793
```

进一步看 JSON split 输出，LAGRANGE-ng 对两区域根节点输出了 10 个 split 条目，而当前 BioGeoBEARS/Rust DEC preset 输出 8 个 split 条目。也就是说，两者的 cladogenesis split scenario 语义和/或权重并不完全相同。

因此后续使用策略应该是：

```text
BioGeoBEARS 对照: 用来锁定 BioGeoBEARS-like 统一框架语义
LAGRANGE-ng 对照: 作为独立 LAGRANGE-ng 语义/性能/命令兼容性的辅助参考
```

自动检查命令：

```powershell
powershell -ExecutionPolicy Bypass -File validation/lagrange-ng/compare-lagrange-ng-reference.ps1 -ScratchRoot C:\tmp
powershell -ExecutionPolicy Bypass -File validation/lagrange-ng/compare-lagrange-ng-official-reference.ps1 -ScratchRoot C:\tmp
powershell -ExecutionPolicy Bypass -File validation/benchmarks/benchmark-lagrange-ng-reference.ps1 -ScratchRoot C:\tmp -Repeats 3
```

这两个脚本都不会把 LAGRANGE-ng 的 lnL 当作 Rust BioGeoBEARS-like 语义的
通过条件。
