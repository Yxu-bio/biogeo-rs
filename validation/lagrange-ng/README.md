# LAGRANGE-ng 参考

这里保存 LAGRANGE-ng 的二进制查找、固定案例运行和官方教程审计脚本。LAGRANGE-ng 是独立的
语义与性能参考，不是 BioGeoBEARS-like golden，也不是 Rust CLI 的运行依赖。

```powershell
powershell -ExecutionPolicy Bypass `
  -File validation/lagrange-ng/compare-lagrange-ng-reference.ps1 `
  -ScratchRoot C:\tmp
```

本机二进制和复制出的工作目录都被 Git 忽略，不会发布到源码仓库。
