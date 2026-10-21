# 参数边界、优化与数值精度核查

构建：`0.1.3-dev.20260929`，Windows x86_64 MSVC。本地开发交付，未发布到 GitHub。
本次没有重写似然、祖先概率、生物地理随机历史或来源分配算法。

## 结论

原始依据是 RASP5 的 `20260929/REPORT.md` 及其保存输入、参数和结果。
本次直接使用原请求的参数边界、树、null 设置、步长和单起点 200 次迭代预算。
没有修改树的枝长精度，没有降低科学参数下界，也没有为追平 R 添加偏移量。

| 检查 | 结果 |
|---|---|
| BAYAREALIKE+J，j=0.99999 | include/exclude 均通过；j 原值不变，派生 y/ys/ysv 合法落在 1e-5 |
| 24 组小数据拟合 | 原审计 14/24 收敛；本次 24/24 收敛；逐组保存结果均重放通过 |
| 4 组已有 Rosa 请求 | 原审计 3/4 收敛；本次 4/4 收敛；没有重跑耗时的 R 优化 |
| 固定参数精度检查 | 52 组输入，各测 1e-14、1e-15 两档传播容差，共 104 次对照 |
| 收紧容差后的最大 lnL 变化 | 7.8444e-12 |
| 收紧容差后的最大节点概率变化 | 2.5247e-12 |
| 对数缩放因子改用补偿累加后的最大变化 | 1.1369e-13 |
| 固定拟合下 BSM 与来源分配 release 回归 | 15 项通过，包括冻结旧 exe 的相同 seed 原历史对照 |
| 完整 cargo test --workspace | 443 项通过，0 失败；2 项外部依赖测试默认忽略，已分别显式执行通过 |
| cargo clippy --workspace --all-targets -- -D warnings | 通过 |

部分有实际改善的拟合如下。lnL 越大越好，但“收敛”不代表证明了全局最大值。

| 数据 / 模型 / null | 原审计 Rust lnL | 新版 lnL |
|---|---:|---:|
| Psychotria / DEC+J / exclude | -32.5895597372 | -30.9666552666 |
| toy / BAYAREALIKE+J / include | -14.9359624642 | -13.4123100967 |
| toy / DIVALIKE+J / exclude | -12.6451888915 | -12.6408249940 |
| Rosa / DEC+J / exclude | -280.4479481160 | -279.7038314340 |
| Rosa / DIVALIKE+J / exclude | -284.5939576189 | -282.2683738461 |

所有小数据和这 4 组 Rosa 的新结果均不低于旧结果，除浮点末位差异（最多约 3e-14）。
这不表示与 R 独立优化后必须得到相同参数，也不表示已经测试所有真实数据。

## 改了什么

1. 派生参数越过边界时，只允许 `8 * f64::EPSILON * max(1, |value|, |该边界|)`
   以内的舍入差吸附到该边界。原本在界内的值不变，自由参数输入仍严格检查。
   下界附近相差 1e-12 等真正越界仍报错。没有改变 j 或参数表上下界。
2. Nelder-Mead 同时检查似然差和**解码后的参数距离**，不再要求近零参数的 log/logit
   坐标继续缩到极小。参数距离使用 `|a-b| / max(1, |a|, |b|)`；似然容差不变。
3. 修正外侧收缩的接受条件，且在最后一次允许的迭代之后再次检查是否已经收敛。
   log 解码后的指数舍入限制在原边界内。
4. 自由参数恰为 d/e/j、且 j=0 的联动表有效时，先拟合相同配置的 j=0 模型。
   原始起点及显式额外起点全部保留；另加入两个起点，沿用该拟合的 d/e，
   j 分别取原初值与其允许区间的 10% 位置。Logit 热起点若恰在端点，
   只为建立可搜索的初始单纯形略向内移动；不改变边界或最终解。
5. 若所有已尝试起点中**最佳有限解**仍未收敛，最多从该解重建一次单纯形继续搜索。
   不是把未收敛解强行归类为成功，也不是只保留收敛但更差的解。

默认每个起点仍最多 200 次迭代，多起点及辅助拟合会增加总计算量。
`iterations/evaluations` 是包含辅助拟合的总数；`starts/converged_starts`
仅计目标模型。此处的“更稳健”不是承诺单次拟合更快。
额外自由参数的自定义模型仍支持显式多起点，但不自动套用 d/e/j 的嵌套起点策略。

## RASP5 怎么读取

原来的 `analysis-run` 调用不需要改。新优化结果目录额外提供：

```text
optimization-starts.tsv
```

它是注册格式 `biogeo-optimization-starts-v1`，先有 format 键，再有 `starts` 表。
主结果仍是 `biogeo-analysis-result-v3`，没有改变旧必需字段的含义。
metadata 新增可选的 `optimization_diagnostics_format` 和
`optimization_diagnostics_fingerprint`，`analysis-result-inspect --replay`
会校验诊断文件完整性，并按原方式验证拟合及后验。
旧的、没有这两个字段的结果仍可读取。

表中每个起点的每个自由参数占一行：

- `stage=target` 是目标模型；`nested_j_zero` 是只用于提供初值的辅助拟合。
- `(stage,start)` 标识一次优化。该起点的 lnL、迭代次数等在各参数行重复，统计时应去重。
- `initial/final` 是参数本身的值，不是 log/logit 坐标，保留可往返解析的浮点精度。
- `selected=true` 仅标记最终使用的那个目标起点；有多个参数行，不是多个被选中的解。
- `stop_reason` 为 `converged`、`iteration_limit` 或 `numerical_failure`。
- `lnL=NA` 表示该起点没有找到有限似然；这样的起点不会替代已有有效解。
- `failed_evaluations` 计候选点构模失败、非法联动值或非有限似然次数，
  不意味着整个起点必然失败。
- `at_boundary` 是近边界标记，与停止原因独立。正常收敛也可能在边界。
  它不是置信区间，不应显示成“参数已被精确估计”。

RASP5 应继续根据最终 `optimization_converged` 决定是否提示用户，而不是要求全部起点都收敛。
完全没有有效解时命令仍失败，不会写出一个伪造的已完成拟合。

调用示例：

```powershell
.\biogeo-cli.exe analysis-run --request analysis.tsv --output-dir fit-new
.\biogeo-cli.exe analysis-result-inspect --analysis-result fit-new --replay
```

显式额外起点沿用 `optimization_additional_starts`，顺序与参数表自由行一致。
需要关闭无 J 辅助起点时，可在 optimize 请求中设置
`optimization_nested_j_start=false`，或给 `model-optimize` 传 `--no-nested-j-start`。
这不会关闭显式额外起点或最佳未收敛解的一次重启。

## 数值精度的结论与限度

核查覆盖同树的 48 组小数据交叉固定参数，以及 4 组 Rosa 固定参数。
正式路径仍使用原来的 1e-13 传播容差；提高精度的路径仅编译到测试中。
在这些输入上，传播截断和对数普通累加产生的变化远小于报告中的 1e-6 至 1e-5 差异。
因此没有证据需要改似然公式、祖先概率算法或默认传播精度。

**剩余 Rust/R 差异的全部来源尚未确定。** 本轮没有重新调整 R 的积分器再拟合，
也没有证明其中一方“绝对正确”。后续如需进一步归因，应在相同枝长、固定参数下
改变 R 的积分容差或使用独立高精度参考，而不是只让 Rust 数字贴近当前 R 输出。

BSM 来源分配没有改动：时期归属沿用 Rust 抽样的约定；全零权重仍为 `undetermined`，
不增加均匀分配兜底。固定拟合和相同 seed 的历史保持不变；如果重新优化找到了更好的参数，
之后产生的历史当然可能改变，不能把这种变化误认为 RNG 回归。

## 复现与文件

源码中的复测脚本：`validation/checks/check-rust-optimization-audit.ps1`。
它读取外部审计输入但不修改原审计目录，输出到用户指定的新目录。
本机证据位于 `output/optimizer-audit-20260929/`：
`final-pass/summary.csv`、各组拟合及逐起点表、`precision-manifest.tsv`、
`precision-results.tsv`、完整测试与静态检查日志。

```powershell
.\validation\checks\check-rust-optimization-audit.ps1 `
  -AuditRoot D:\RASP5_Rust_R_Full_Audit\20260929 `
  -OutputDirectory output\another-audit -IncludeRosa

$env:BIOGEO_PRECISION_MANIFEST = '<absolute-path-to-precision-manifest.tsv>'
$env:BIOGEO_PRECISION_OUTPUT = '<absolute-path-to-new-precision-results.tsv>'
cargo test --release -p biogeo-core fixed_parameter_precision_audit --lib -- --ignored
```

精度清单使用八列 TSV：case、tree、ranges、parameters、max、include、min_branch、reference_r。
这项测试限于无时间分层、w=1 的固定参数输入，不冒充所有修饰组合的数值证明。
