# 安装 biogeo-rs

本文面向第一次使用 Rust 命令行软件的用户。完成安装后，你会得到一个名为
`biogeo-cli` 的可执行程序。

## 当前可用的安装方式

当前版本是 `0.1.0` 公开科研发布候选版，GitHub Releases 暂时还没有预编译安装包。
目前推荐从源码构建：

1. 安装 Rust 和 Windows C++ 构建工具。
2. 下载 biogeo-rs 源码。
3. 用一条 `cargo build` 命令生成 `biogeo-cli.exe`。

本流程已在 64 位 Windows 和 Rust MSVC 工具链上验证。Linux 和服务器环境尚未完成与
Windows 同等强度的系统测试，相关说明见[其他操作系统](#其他操作系统)。

## Windows 安装

### 第 1 步：安装 Rust

打开 [Rust 官方安装页面](https://www.rust-lang.org/tools/install)，下载并运行
`rustup-init.exe`。保持默认的 stable MSVC 工具链即可。

Rust 在 Windows 上还需要 Microsoft C++ 构建工具和 Windows SDK。新版 `rustup-init`
可以引导安装这些依赖；也可以自行安装 Visual Studio 2022 Community 或 Build Tools，
并选择 **Desktop development with C++** 工作负载。详细要求见
[Rust 官方 Windows MSVC 说明](https://rust-lang.github.io/rustup/installation/windows-msvc.html)。

安装结束后关闭并重新打开 PowerShell，然后运行：

```powershell
rustc --version
cargo --version
rustup show active-toolchain
```

三条命令都应正常输出版本。最后一条通常包含：

```text
x86_64-pc-windows-msvc
```

如果 PowerShell 报告找不到 `rustc` 或 `cargo`，先查看
[常见安装问题](#常见安装问题)。

### 第 2 步：获取源码

#### 方法 A：使用 Git

先安装 [Git for Windows](https://git-scm.com/download/win)，然后在希望保存项目的目录中运行：

```powershell
git clone https://github.com/Yxu-bio/biogeo-rs.git
cd biogeo-rs
```

#### 方法 B：不安装 Git

1. 打开 [biogeo-rs GitHub 仓库](https://github.com/Yxu-bio/biogeo-rs)。
2. 点击 **Code**，再点击 **Download ZIP**。
3. 解压 ZIP。
4. 在解压后的 `biogeo-rs-main` 目录空白处按住 Shift 并单击鼠标右键，选择在终端中打开。

后面的命令都要在仓库根目录运行。这个目录中应当能看到 `Cargo.toml`、`README.md`、
`crates` 和 `examples`。

### 第 3 步：构建正式版本

```powershell
cargo build --release --locked -p biogeo-cli
```

第一次构建会从 crates.io 下载 Rust 依赖，需要网络连接，也可能需要几分钟。`--release`
会启用优化，实际分析不要使用未优化的 debug 构建。`--locked` 确保使用仓库已记录的依赖版本。

构建成功后，可执行文件位于：

```text
target\release\biogeo-cli.exe
```

### 第 4 步：验证程序

```powershell
.\target\release\biogeo-cli.exe --version
.\target\release\biogeo-cli.exe engine-info
```

第一条命令应报告 `biogeo-cli 0.1.0`。第二条会输出版本化的引擎能力表，包括支持的模型、
输入、结果格式和生物地理随机历史能力。

再用仓库自带的小数据检查完整的输入读取过程：

```powershell
.\target\release\biogeo-cli.exe analysis-plan `
  --request examples\analysis_request\analysis.tsv
```

看到 `status` 为 `valid` 后，说明程序、示例树、分布数据和参数表可以共同工作。这个命令
只检查并规划任务，不会开始参数优化。

## 完成一次最小分析

安装验证通过后，可以直接运行示例 DEC 分析：

```powershell
.\target\release\biogeo-cli.exe analysis-run `
  --request examples\analysis_request\analysis.tsv `
  --output-dir output\first-dec

.\target\release\biogeo-cli.exe analysis-result-inspect `
  --analysis-result output\first-dec `
  --replay
```

`analysis-run` 拟合模型并创建可移动的结果目录。`--replay` 使用结果目录中保存的输入和参数
重新计算似然，用于确认结果没有缺文件或意外改变。输出目录不会覆盖已有目录；再次运行时请换
一个目录名。

完整的新手分析步骤见项目 [README](../README.md#十分钟跑通第一个分析)。

## 可选：制作并安装 Windows 发布包

如果需要把程序交给另一台 Windows PC、保存一个固定版本，或供新版 RASP 捆绑，可以在仓库
根目录生成自包含 ZIP：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File packaging\build-windows-package.ps1
```

输出位于 `dist`：

```text
dist\biogeo-cli-<version>-<target>.zip
dist\biogeo-cli-<version>-<target>.zip.sha256
```

ZIP 包含 EXE、许可证、schema、文档、示例、文件清单和 SHA-256。当前科研发布包可以不做
Windows 代码签名，安装和使用不需要购买证书。

将 ZIP 解压后，在包的顶层目录运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 `
  -InstallDir C:\Tools\biogeo-cli-0.1.0
```

安装脚本会先校验包内所有文件，再复制和试运行程序。它不会修改注册表、安装服务或自动更改
`PATH`。目标目录已存在时会拒绝覆盖。详细的包结构和验证规则见
[Windows 发布与安装](windows-release.md)。

## 更新版本

使用 Git 获取源码时，在仓库根目录运行：

```powershell
git pull
cargo build --release --locked -p biogeo-cli
```

使用 ZIP 源码时，重新下载并解压新版本，再重新构建。固定发布包建议安装到新的版本目录，
不要覆盖旧目录。分析结果会记录引擎和格式版本，旧结果应先用对应版本检查和重放。

## 卸载

biogeo-rs 不写注册表、不安装服务，也不会自动修改系统 `PATH`。删除下面实际存在的目录即可：

- 源码目录；
- 手动安装的 `C:\Tools\biogeo-cli-<version>` 目录；
- 不再需要的分析输出目录。

Rust 是独立开发工具。如果还用于其他项目，不要卸载。确认完全不再需要 Rust 时，可运行
`rustup self uninstall`。

## 其他操作系统

### Linux

按 [rustup 官方说明](https://rust-lang.github.io/rustup/installation/)安装 stable Rust，
再在仓库根目录运行：

```bash
cargo build --release --locked -p biogeo-cli
./target/release/biogeo-cli --version
./target/release/biogeo-cli engine-info
```

Linux 上的程序路径没有 `.exe`。编译通常还需要发行版提供的 C 编译器和 linker，例如
Ubuntu/Debian 的 `build-essential`。目前尚未完成 Linux、Slurm、cgroup 配额和超大型服务器
任务的正式验证，因此当前版本不把这些环境标为与 Windows 等级相同的已验证平台。

### macOS

安装 Xcode Command Line Tools 和 stable Rust 后，可以使用与 Linux 相同的 `cargo build`
命令。macOS 当前也没有完成正式测试。

## 常见安装问题

### 找不到 `rustc` 或 `cargo`

安装 Rust 后先关闭并重新打开 PowerShell。默认可执行文件位于：

```text
%USERPROFILE%\.cargo\bin
```

如果该目录没有加入用户 `PATH`，重新运行 `rustup-init.exe`，或按 rustup 安装器提示修复
环境变量。不要在同一个尚未刷新的终端窗口中反复测试。

### 报告 `link.exe`、MSVC 或 Windows SDK 缺失

打开 Visual Studio Installer，确认已经安装 **Desktop development with C++**，并包含
MSVC x64/x86 build tools 和 Windows SDK。安装完成后重新打开 PowerShell 再构建。

### 下载 Rust 依赖失败

第一次构建必须访问 crates.io。先确认浏览器可以联网，然后重试 `cargo build`。单位代理或
防火墙环境需要按本地网络规则配置 Cargo 代理；不要删除 `Cargo.lock` 来绕过下载错误。

### PowerShell 阻止运行 `.ps1`

源码构建本身不依赖 PowerShell 脚本。只有制作或安装发布包时才需要它们，可以使用文档中的：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File <script.ps1>
```

该参数只对当前 PowerShell 进程生效，不会永久降低系统执行策略。

### 输出目录已经存在

这不是安装失败。biogeo-rs 为避免覆盖科研结果，要求新分析、工作流和生物地理随机历史使用
不存在的输出目录。更换 `--output-dir`，或在确认不需要旧结果后由用户自行归档它。

### 路径中有中文或空格

Windows 发布测试覆盖带中文和空格的路径。PowerShell 中遇到此类路径时使用引号：

```powershell
& "C:\研究项目\biogeo cli\biogeo-cli.exe" --version
```

如果仍有问题，请在 GitHub Issue 中附上完整命令、错误输出、`biogeo-cli --version` 和
`rustc -vV`，但不要上传尚未公开的研究数据。

## 安装完成后读什么

1. [命令行完整教程](cli-tutorial.md)
2. [README 新手教程](../README.md#十分钟跑通第一个分析)
3. [分析请求格式](analysis-request.md)
4. [参数表](parameter-table.md)
5. [分析结果目录](analysis-result.md)
6. [生物地理随机历史输出](bsm-output-formats.md)
