# DEC+J 对齐记录

本阶段实现 BioGeoBEARS 默认 DEC+J preset 的固定参数 likelihood、posterior、
`d/e/j` 三参数优化，以及静态/时间分层 pairwise dispersal modifier 对 founder-event
节点权重的影响。

## BioGeoBEARS 的 `j` 语义

BioGeoBEARS 的默认 DEC+J 不是简单使用：

```text
y = 1
s = 1
v = 1
j = user_j
```

它的参数表存在联动约束：

```text
ysv = 3 - j
y = ysv / 3
s = ysv / 3
v = ysv / 3
```

因此 Rust 的 `ModelConfig::preset_dec_j(d, e, j)` 按 BioGeoBEARS preset 解释：

```text
j = user_j
y = s = v = (3 - user_j) / 3
```

当 `j = 0` 时，这会退化回 DEC 默认的 `y=s=v=1`。

## Founder-Event Split 语义

对祖先范围 `A`，`j` 事件生成：

```text
A -> A + B
A -> B + A
A -> A + C
A -> C + A
```

也就是一个 daughter 复制祖先范围，另一个 daughter 跳到祖先范围外的 singleton
区域，左右方向都保留。

对祖先范围 `A+B`，`j` 事件生成：

```text
A+B -> A+B + C
A+B -> C + A+B
```

这个语义来自 BioGeoBEARS / cladoRcpp 的 COO cladogenesis table。

以上是默认 `mx01j=0.0001` 的行为。非默认 `mx01j` 会按最大熵 daughter-size
概率允许完全位于祖先范围之外的多区域 founder daughter。相关复杂 fixture
位于 `validation/maxent_fixtures.tsv`，golden 位于
`validation/golden/biogeobears-maxent*.tsv`。

## Pairwise Modifier 语义

BioGeoBEARS 把沿枝 Q 使用的同一有效矩阵也传给 cladoRcpp。有效矩阵为：

```text
effective[a,d] = manual[a,d] * distance[a,d]^x * envdistance[a,d]^n
```

对祖先范围 `A` 和位于祖先范围外的 founder daughter `D`：

```text
pairwise(A,D) = sum(effective[a,d] for a in A, d in D) / (|A| * |D|)
raw_j_weight = j * maxent01j(A,D) * pairwise(A,D)
```

方向是祖先区域 `a` 作为来源、founder daughter 区域 `d` 作为目标。多区域 founder
daughter 使用全部有向组合的算术均值，不使用 sum、max 或首个区域。这个因子先乘到
未归一化 j scenario，再与相同祖先范围下的 y/s/v/j scenarios 一起归一化。

全 1 矩阵严格保留原来的 DEC+J split table；0 会移除对应方向的 founder scenario。
静态模型用一套有效矩阵同时构建 Q/C；时间分层模型逐时期构建 C 表，节点按距今年龄
选择对应时期。area-specific extirpation 仍只作用于 Q。

## 固定参数验证

独立 fixture：

```text
validation/decj_fixtures.tsv
```

当前有三个案例：基础 all-one 语义、静态非对称矩阵，以及三时期非对称矩阵。后两例
专门锁定方向、零权重 scenario 和节点时期选择。

固定 likelihood：

| fixture | Rust lnL | BioGeoBEARS lnL | abs delta |
| --- | ---: | ---: | ---: |
| `three_area_tri_tip_decj_null` | -2.388547363659485 | -2.388547381056443 | 1.74e-8 |
| `three_area_tri_tip_decj_directional_null` | -2.401999749882900 | -2.401999701861462 | 4.80e-8 |
| `three_area_four_tip_decj_three_epoch_null` | -5.198603023684178 | -5.198603068867520 | 4.52e-8 |

posterior 对照：

| fixture | ancestral max delta | split rows | split probability max delta | split weight max delta |
| --- | ---: | ---: | ---: | ---: |
| baseline | 1.02e-8 | 78 | 2.31e-9 | 4.95e-9 |
| directional | 6.23e-9 | 74 | 1.83e-9 | 4.47e-9 |
| three-epoch | 1.24e-8 | 107 | 1.17e-8 | 8.11e-9 |

分层 split golden 生成器必须按节点年龄调用对应 `timeperiod_i` 的
`get_Qmat_COOmat_from_res`。若对所有节点错误使用最年轻时期，三时期案例会生成 117
行而 Rust 只有正确的 107 行；该生成器问题已修复并由当前 fixture 锁定。

split scenario weight 的最后几位差异来自 BioGeoBEARS 默认参数表使用接近 `3` 的数值边界；Rust 保留清楚的 preset 语义 `ysv = 3 - j`，不为这个数值细节改动模型定义。因此 DEC+J split weight 对照使用 `1e-8` 阈值，DEC 的 `j=0` 对照仍保持 `1e-12`。

## 参数优化验证

Rust 新增：

```text
biogeo_core::optimize_decj_dej()
biogeo_core::optimize_decj_dej_with_model()
biogeo-cli decj-optimize
```

优化变量为：

```text
ln(d), ln(e), logit(j)
```

其中 `j` 默认边界与 BioGeoBEARS 贴近：

```text
min_j = 1e-5
max_j = 2.99999
```

当前 BioGeoBEARS 优化 golden：

```text
validation/golden/biogeobears-decj-optim.tsv
```

三例 BioGeoBEARS 优化均从 `j=0.5` 起步并返回 convergence code 0；收敛码与消息写入
golden，比较器会拒绝非零收敛码。

| fixture | BioGeoBEARS lnL | Rust lnL delta | 主要参数差异 |
| --- | ---: | ---: | --- |
| baseline | -1.287155319182886 | 2.79e-8 | j delta 2.28e-4 |
| directional | -1.273719003423009 | 3.50e-8 | j delta 1.07e-4 |
| three-epoch | -3.927803442996674 | 3.25e-8 | d delta 1.17e-5, j delta 6.42e-5 |

`optimize_decj_dej_with_model` 在每次目标函数计算时通过同一 model factory 重建带固定
modifier 的 `ModelConfig`，不会在优化路径中绕过 C 修饰。优化对照以 lnL 为硬判据；
边界或平坦区域的参数点可略有差异，但非收敛的 BioGeoBEARS 结果不能进入 golden。

## 运行命令

```powershell
powershell -ExecutionPolicy Bypass -File validation/check-rust-decj-fixtures.ps1
Rscript validation/biogeobears-decj-golden.R
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-decj.ps1

Rscript validation/biogeobears-dec-ancestral-golden.R validation/decj_fixtures.tsv validation/golden/biogeobears-decj-ancestral.tsv
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-ancestral.ps1 -Manifest validation/decj_fixtures.tsv -Golden validation/golden/biogeobears-decj-ancestral.tsv

Rscript validation/biogeobears-dec-split-golden.R validation/decj_fixtures.tsv validation/golden/biogeobears-decj-split.tsv
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-dec-split.ps1 -Manifest validation/decj_fixtures.tsv -Golden validation/golden/biogeobears-decj-split.tsv -WeightTolerance 1e-8

Rscript validation/biogeobears-decj-optim-golden.R
powershell -ExecutionPolicy Bypass -File validation/compare-biogeobears-decj-optim.ps1
```
