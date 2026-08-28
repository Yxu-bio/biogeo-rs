# Windows PC 发布与安装

## 范围

当前发布流程面向本机同类的 64 位 Windows PC，使用 Rust MSVC host 构建独立
`biogeo-cli.exe`。它是新版 RASP 的子进程计算引擎，不修改注册表、不安装服务，也不把目录加入
用户或系统 `PATH`。Linux、Slurm 和 cgroup 资源探测仍属于后续服务器阶段。

## 构建发布包

在仓库根目录运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File packaging/build-windows-package.ps1
```

脚本执行 `cargo build --release --locked -p biogeo-cli`，从 `cargo metadata` 和 `rustc -vV`
读取版本及 host target。默认输出到被 Git 忽略的 `dist/`，且不会覆盖同名目录或归档：

```text
dist/
  biogeo-cli-<version>-<target>/
  biogeo-cli-<version>-<target>.zip
  biogeo-cli-<version>-<target>.zip.sha256
```

目录和 ZIP 含同一顶层包：

```text
biogeo-cli-<version>-<target>/
  biogeo-cli.exe
  README.md
  CITATION.cff
  install.ps1
  package.tsv
  files.tsv
  release-status.tsv
  build-info.tsv
  engine-source-manifest.tsv
  LICENSE
  LICENSE-STATUS.md
  CHANGELOG.md
  THIRD-PARTY-NOTICES.md
  third-party-licenses/
  schemas/
  docs/
  examples/
```

`package.tsv` 使用 `biogeo-windows-package-v3`；`files.tsv` 固定记录每个 payload 的相对路径、
字节数和 SHA-256。ZIP 外的 `.sha256` 用于下载或复制后的整包校验。`--SkipBuild` 只适用于已经
明确构建过 `target/release/biogeo-cli.exe` 的本地重复打包。

ZIP 内部条目按相对路径排序，统一使用标准 `/` 分隔符并且只有一个顶层包目录，避免新版 RASP
或其他解压库把 Windows 反斜杠误当成文件名字符。发布门禁会直接检查这一契约。

`examples/` 包含统一分析请求、六个 preset、Psychotria 五时期分析、多模型工作流和可恢复
错误等可移植任务。示例中的相对路径都在该目录内部解析，安装目录整体移动后仍可运行。

`release-status.tsv` 是权威发布状态。当前源码快照是 `public_research_release`，项目
许可证是 `GPL-3.0-or-later`，且 `public_distribution_allowed=true`。`build-info.tsv` 使用
`biogeo-windows-build-info-v2`，记录
Rust/Cargo、locked 构建命令、lockfile、源码清单、构建来源、源码 revision 与 Git HEAD
匹配状态、CI run、Authenticode/时间戳状态和签名后 exe 哈希；`engine-source-manifest.tsv`
逐文件冻结引擎构建输入。
当前本地发布包如实记录 `local_worktree/unsigned`。当前 ZIP 时间戳未归一化，
所以这些记录支持功能和来源复现，但不声称不同机器上的 ZIP 必然逐字节相同。

## 安装到指定目录

解压 ZIP 后运行包内脚本：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 `
  -InstallDir C:\Tools\biogeo-cli-0.1.0
```

安装器先校验包根条目、metadata、全部清单路径、字节数和 SHA-256，再复制到目标父目录中的
暂存目录，运行 `biogeo-cli.exe --help`，最后以目录重命名发布。目标目录只要已经存在就拒绝
覆盖；升级应安装到新的版本目录，由新版 RASP 显式切换 exe 绝对路径。成功目录额外包含
`biogeo-windows-installation-v3` 的 `installation.tsv`，记录版本、target、payload 已验证状态、
发布类别、许可证状态和公开分发标志。

公开 GitHub 科研包可以未签名，也不要求必须由 CI 构建。代码签名和构建来源信息仅是可选增强，
详见 [`windows-trusted-distribution.md`](windows-trusted-distribution.md)。

新版 RASP 也可以直接捆绑完整发布目录而不执行安装器。不能只复制 exe 后再假定 schema 仍可从
相邻目录发现；RASP 应随自身版本保存所使用的完整 package 或至少保存 package 版本和 schema。

## 自动验证

发布门禁在临时目录完成构建、ZIP 解压、SHA-256 安装，并在带中文和空格的路径中用已安装 exe
执行统一请求的 `analysis-plan`、`analysis-run`、Windows 进程遥测、分析结果重放，以及
`--version`/`engine-info` 能力握手、命令专属帮助与兼容政策、`analysis-workflow` 的 compact 随机历史、深度检查、恢复
和机器 schema 检查。门禁还会直接从安装目录运行六个 preset、五时期示例、预算停止/恢复示例，
并用安装后的 exe 完成 Psychotria 与 Ponerinae 子集的六模型中断恢复、结果 schema 验收，
以及六 preset 的 12 项静态/两时期修饰组合与 12 项拒绝规则检查。参考宿主还会删除只读中文
源输入、移动工作流结果，再用安装后的 exe 重放并重新生成生物地理随机历史：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File validation/checks/check-windows-release.ps1
```

完整 v0.1 候选门禁还会先运行 locked 工作区测试、Clippy 和全部科学语义 golden：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File validation/checks/check-v0.1-release-candidate.ps1
```

只有全部步骤通过才会在 `validation/benchmark-runs/` 写入不可覆盖的版本化检查记录。该流程验证
文件完整性和可执行性，并验证未签名包的正常构建安装、包损坏拒绝，以及可选签名参数的错误
路径。独立两小时稳定性检查见 [`windows-pc-stability.md`](windows-pc-stability.md)。面向公开下载时
应保留项目许可证、第三方来源记录和相应源代码，但不需要代码签名证书。
