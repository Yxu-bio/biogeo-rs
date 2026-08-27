# 第三方依赖声明

Windows `0.1.0` 发布候选使用 `Cargo.lock` 锁定依赖。下表来自
`cargo tree -p biogeo-cli --target x86_64-pc-windows-msvc --locked`；完整许可证文本随包放在
`third-party-licenses/<crate-version>/`。项目自身采用 `GPL-3.0-or-later`，完整条款见 `LICENSE`，
说明见 `LICENSE-STATUS.md`。

| crate | 版本 | SPDX/上游声明 | 上游仓库 |
|---|---:|---|---|
| cfg-if | 1.0.4 | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| chacha20 | 0.10.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/stream-ciphers |
| cpufeatures | 0.3.0 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| crossbeam-deque | 0.8.7 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-epoch | 0.9.20 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | 0.8.22 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| ctrlc | 3.5.2 | MIT OR Apache-2.0 | https://github.com/Detegr/rust-ctrlc |
| either | 1.16.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/either |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | https://github.com/cryptocorrosion/cryptocorrosion |
| rand | 0.10.2 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_chacha | 0.10.0 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | https://github.com/rust-random/rand_core |
| rayon | 1.12.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/rayon |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/rayon |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| zerocopy | 0.8.54 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |

该清单只覆盖当前 Windows 目标实际进入依赖图的第三方 crate。目标平台、feature 或
`Cargo.lock` 变化后必须重新生成并核对，不能沿用旧清单。

## 源码仓库中的 BioGeoBEARS 验证数据

`validation/fixtures/biogeobears_official/` 包含从 BioGeoBEARS `1.1.3` 官方示例导入的树、
分布范围和矩阵，以及明确标记的派生测试数据。BioGeoBEARS 的 `DESCRIPTION` 声明
`GPL (>= 2)`。来源、导入时修订和完整许可文本见：

- `validation/fixtures/biogeobears_official/LICENSE-NOTICE.md`
- `validation/fixtures/biogeobears_official/COPYING-GPL-2.txt`

这些验证数据不会进入 Windows 运行包。本地 BioGeoBEARS/R 安装目录和 LAGRANGE-ng 二进制也已由
`.gitignore` 排除。

## Ponerinae 派生验收数据

`validation/fixtures/ponerinae_32tip_7area/` 和两个 Ponerinae 名称映射表来自 Doré et al. (2025)
公开研究仓库的确定性整理，上游采用 MIT License。论文 DOI、上游修订号、版权声明和完整许可
条款见 `validation/fixtures/ponerinae_32tip_7area/LICENSE-NOTICE.md`。完整 1534-tip 本地参考数据
不进入本项目仓库。
