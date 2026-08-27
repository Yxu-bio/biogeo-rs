# Detection 全栈组合验证

## 目的

该案例不是为某个输出写特殊分支，而是把此前分别验证过的模块放进同一次计算：

- BioGeoBEARS 官方 Psychotria detection 与 inclusive control 数据；
- 五个时期的手工扩散倍率、地理距离、环境距离和面积；
- `d/e/a/b/x/n/w/u` 沿枝参数；
- `y/s/v/j` 与事件独立的 `mx01y/mx01s/mx01v/mx01j`；
- `mf/dp/fdp` 末端观测模型；
- 随历史时期依次只允许 `K/O/M/H`、`K/O/M`、`K/O`、`K`、`K` 的范围约束；
- `max_range_size=4`、包含 null range、flat root prior。

fixture 位于 `validation/fixtures/detection_full_stack/`。五期局部状态数依次为
`16/8/4/2/2`，因此该案例会真实改变时期状态空间，而不只是重复同一矩阵。

## 固定似然

固定参数下：

```text
BioGeoBEARS lnL  -72.754230882411846
Rust lnL         -72.754230167807100
absolute delta     0.000000714604746
```

差值低于 `2e-5` 门限，量级来自两套矩阵指数与分时期数值路径，不通过 fixture 特判消除。

## 祖先状态与分裂概率

BioGeoBEARS 1.1.3 的直接 stratified uppass 在这个会改变时期状态空间的案例上不可靠。
直接结果与独立条件似然的最大概率差为 `0.2936639527`，不能通过放宽容差接受。

严格参考改用 BioGeoBEARS 自身的 fixnode likelihood。对节点 `v` 的每个合法状态 `i`
分别固定后重算整树似然，再归一化：

```text
P(X_v=i | data) proportional to L(data, X_v=i)
```

因硬约束导致 BioGeoBEARS 返回 NaN 的不可能状态记为 `-Inf` likelihood，也就是零概率。
18 个内部节点乘 16 个主状态共冻结 288 行。Rust 与 fixnode 的最大概率差为
`9.291646e-8`。

节点分裂 posterior 使用同一 fixnode 节点边缘分布、两个 daughter downpass likelihood
和该节点实际时期的 C 表重建，共 408 行。BioGeoBEARS 各时期 COO 索引是时期局部状态
索引，生成器先按 range bits 映射回 16 个主状态，不能把局部索引直接当全局索引。Rust
与校正分裂分布的最大概率差为 `9.383145e-8`，scenario weight 最大差为
`9.727940e-10`。

## 五维优化

`validation/detection_full_stack_optimization_fixtures.tsv` 同时释放 `d/e/x/n/u`，其余
非默认 detection 和 cladogenesis 参数仍参与同一个 likelihood。BioGeoBEARS BOBYQA
曾返回 `convergence=0`，却把第一起点从 `-72.754` 降到 `-435.735`；这种结果现在会被
生成器拒绝。每个候选必须满足：

1. 起点固定 likelihood 有限；
2. 优化器返回参数有限且声明收敛；
3. 返回参数重新固定后的 likelihood 可重放；
4. 重放 likelihood 不低于该候选自己的起点。

第二起点得到可重放的 BioGeoBEARS 候选：

```text
lnL  -20.121763189568259
d     0.10954653897055
e     1e-12
x    -1.29125534071989
n     6.0652965825974
u     0.59568343910554
```

Rust 在该点固定重算差为 `2.802485e-7`。Rust 两起点搜索得到
`lnL=-19.8575023959577`，比该 BGB 候选高 `0.2642607936`；门禁要求 Rust 不低于可重放
BGB 候选，不要求不同优化算法返回相同坐标。

## 生物地理随机历史

直接 BioGeoBEARS `runBSM()` 依赖上述 stratified uppass，因此不把它在该案例上的路径
分布误当严格 golden。Rust 先写出 `biogeo-analysis-result-v2`，由 `model-bsm` 完成输入、
模型身份和 lnL 重放，再抽取 20,000 条生物地理随机历史。经验节点状态与分裂频率直接
对照 fixnode 和校正分裂的精确分布：

```text
sampling + eight-table streaming  10.596 s
maximum standardized difference    3.030086
maximum node total variation        0.001799049
maximum split total variation       0.003254839
```

288 个节点状态项和 408 个分裂项全部通过；精确零概率项必须零命中。运行还暴露并修复了
稀有端点 bridge 风险：当端点概率约为 `2.6e-8` 时，`1-accumulated_poisson` 会停在机器
精度而无法达到相对容差。实现只在累计质量进入浮点精度区时改用递推的剩余 Poisson 尾部
上界，并以至少 17 次跃迁的稀有端点回归锁定。

## 复现

```powershell
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-combinations.ps1
powershell -ExecutionPolicy Bypass -File validation/biogeobears/compare-biogeobears-detection-full-stack-fixnode.ps1
powershell -ExecutionPolicy Bypass -File validation/checks/check-biogeobears-detection-full-stack-optimization.ps1
powershell -ExecutionPolicy Bypass -File validation/checks/check-detection-full-stack-bsm-distribution.ps1
```

重新生成 R 端严格参考：

```powershell
Rscript validation/biogeobears/biogeobears-stratified-node-posterior-audit.R `
  validation/golden/biogeobears-detection-full-stack-fixnode-posterior.tsv `
  psychotria_detection_constrained_full_stack all `
  validation/golden/biogeobears-detection-full-stack-fixnode-split.tsv

Rscript validation/biogeobears/biogeobears-detection-combination-optim-golden.R `
  validation/detection_full_stack_optimization_fixtures.tsv `
  validation/golden/biogeobears-detection-full-stack-optim.tsv
```
