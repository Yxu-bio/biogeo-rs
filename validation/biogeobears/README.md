# BioGeoBEARS 对照

这里保存 BioGeoBEARS 1.1.3 的 golden 生成器、R 环境配置，以及把 Rust 输出与冻结结果逐项
比较的 PowerShell 脚本。它们是科学验证代码，不是 `biogeo-cli` 的运行依赖。

## 本地 R 环境

从仓库根目录安装隔离依赖：

```powershell
Rscript validation/biogeobears/setup-local-r-biogeobears.R
```

包安装到被 Git 忽略的 `validation/r-lib/R-<major>.<minor>`，不会写入用户全局 R library。

## 最小 DEC 对照

```powershell
Rscript validation/biogeobears/biogeobears-dec-golden.R
powershell -ExecutionPolicy Bypass `
  -File validation/biogeobears/compare-biogeobears-dec.ps1
```

完整案例、容差和已知语义边界见父目录的 [验证说明](../README.md)。
