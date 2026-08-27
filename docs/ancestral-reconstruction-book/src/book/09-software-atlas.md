# 第九章：方法和软件全景图

## 9.1 先按问题选模型，再按模型选软件

软件名不能代替方法名。RASP 是多方法图形平台，RevBayes 是概率编程平台，`phytools` 是工具箱，BEAST X 是联合时间树平台，BioGeoBEARS 是历史生物地理条件似然框架。问“哪个更高级”没有统计含义。

本章状态核查截止 2026-08-10。版本号只帮助复现，不保证新版本一定更可靠。仓库有提交、CRAN 有包、二进制有日期和有正式方法论文，是四件不同的事。

## 9.2 普通离散与连续性状

| 工具 | 核查状态 | 强项 | 推断范式 | 最锋利的边界 | 官方入口 |
|---|---|---|---|---|---|
| Mesquite | 4.03，2026-04 发布 | 形态矩阵、简约/似然祖先状态、交互可视化 | 多模块 | GUI 操作难完整脚本化 | [官网](https://www.mesquiteproject.org/) |
| `ape` | 稳定 5.8-1，开发分支仍更新 | `ace`、PIC、树基础设施 | 最大似然/REML | 某些“边缘”算法与严格全边际不同，固定树 | [CRAN](https://cran.r-project.org/package=ape) |
| `phytools` | CRAN 2.5-2，开发版 2.6 系 | `fitMk`、`make.simmap`、`fastAnc`、阈值模型 | ML、经验贝叶斯、随机映射 | 函数回答不同问题，默认根与速率需核查 | [仓库](https://github.com/liamrevell/phytools) |
| `phangorn` | 2.12.1，3.0 开发中 | 序列/形态建树、简约和似然祖先状态 | MP、ML | 通常不传播比对和树不确定性 | [文档](https://klausvigo.github.io/phangorn/) |
| `geiger` | 2.0.11，低频维护 | BM、OU、早期爆发、离散/连续模型比较 | ML、部分 MCMC | 不是完整祖先历史器，OU/EB 可弱辨识 | [CRAN](https://cran.r-project.org/package=geiger) |
| `corHMM` | CRAN 2.8，开发 2.10 系 | 隐藏速率、多状态、相关演化 | ML 隐马尔可夫 | 隐类别不等于真实性状，参数膨胀 | [仓库](https://github.com/thej022214/corHMM) |
| castor | 1.8.7，2026-08 发布 | 超大树 Mk、简约、连续祖先估计 | 高效 ML/算法 | 规模不修复模型偏差 | [CRAN](https://cran.r-project.org/package=castor) |
| PastML | 1.9.51，服务在线 | 超大树离散状态、病原地点、压缩 HTML | MP、边际/联合 ML 近似 | 固定树、各性状多独立处理 | [官网](https://pastml.pasteur.fr/) |
| BayesTraits | 5.0.2，2025-07 | 离散相关、连续、变速率、多树、可逆跳跃 | ML、MCMC、RJ-MCMC | 先验、树尺度、节点定义和混合 | [官网](https://www.evolution.reading.ac.uk/BayesTraits.html) |
| RevBayes | 1.4.1，2026-07 | 可编程 Mk、随机历史、树、SSE、DEC、化石 | 贝叶斯概率图 | 能运行不等于模型语义正确 | [官网](https://revbayes.github.io/) |
| MrBayes | 稳定 3.2.7a，源码仍维护 | 分子/形态树、祖先状态、总证据 | 贝叶斯 MCMC | 祖先输出常依赖约束节点和根 | [官网](https://nbisweden.github.io/MrBayes/) |
| phyddle | 文档 0.3.0；祖先节点扩展仍属研究前沿 | 可模拟但似然难处理的模型、摊销式祖先预测 | 模拟训练的深度学习 | 强依赖训练分布、模拟器、校准与树编码 | [文档](https://phyddle.org/) |

### 选择建议

- 先做可解释固定树基线：`ape` 或 `phytools`。
- 要枝上完整历史：`phytools::make.simmap` 或 RevBayes。
- 要隐藏速率：`corHMM`，并设置简单模型和充分性模拟。
- 超大树展示：PastML；超大树计算：castor。
- 要跨多树、先验和模型平均：BayesTraits 或 RevBayes。
- 形态矩阵教学和人工检查：Mesquite，同时记录完整操作和版本。
- 只有在生成模型可模拟但似然确实难处理，且能承担大量模拟和严格校准时，才把 phyddle 作为研究路线；简单 Mk 先保留精确剪枝基线。

## 9.3 连续性状与适应峰

| 工具 | 状态 | 适合问题 | 主要输出 | 关键风险 | 入口 |
|---|---|---|---|---|---|
| `ouch` | 活跃维护 | 预先指定制度的多峰 OU | \(\alpha,\sigma,\theta\)、似然 | 制度位置作为已知 | [官网](https://kingaa.github.io/ouch/) |
| OUwie | 3.0.2，2026-07 | 多制度 BM/OU、测量误差 | 参数、AIC、诊断 | 两步制度映射、不辨识 | [CRAN](https://cran.r-project.org/package=OUwie) |
| `mvMORPH` | 1.2.1/开发 1.2.2 | 多变量 BM/OU、速率变化、高维惩罚 | 协方差、祖先均值、峰 | 参数按性状数平方增长 | [仓库](https://github.com/JClavel/mvMORPH) |
| `bayou` | 2.3.2，2026-02 | 未知 OU 峰数和转变位置 | RJ-MCMC 后验 | 峰数量和位置先验敏感 | [CRAN](https://cran.r-project.org/package=bayou) |
| SURFACE | 成熟方法、维护较低 | 逐步搜索趋同 OU 制度 | 制度树、AIC | 逐步选择、忽略制度不确定性 | [方法论文](https://doi.org/10.1111/j.1558-5646.2011.01479.x) |
| `l1ou` | 专门工具 | 稀疏 OU 转变检测 | 转变位置与峰 | 惩罚选择和大树近似 | [仓库](https://github.com/khabbazian/l1ou) |
| `MCMCglmm` | 活跃 R 包 | 系统发育混合模型、多响应、误差 | 回归和随机效应后验 | 先验、尺度和因果解释 | [CRAN](https://cran.r-project.org/package=MCMCglmm) |

连续模型优先检查参数恢复、半衰期相对树高、测量误差和化石，而不是收集最多的 AIC 模型名。

## 9.4 祖先范围与历史生物地理

| 工具 | 状态 | 强项 | 主要概率对象 | 主要边界 | 入口 |
|---|---|---|---|---|---|
| DIVA | 历史软件 | 事件简约范围历史 | 最低代价事件配置 | 无枝长速率和概率区间 | [SourceForge](https://sourceforge.net/projects/diva/) |
| RASP | 源码 4.1；2025 二进制仍发布 | S-DIVA、BBM、DEC、BayArea、GUI、多树 | 随所选模块改变 | 菜单并列掩盖模型差异；BBM 不是贝叶斯 DEC | [下载](https://sourceforge.net/projects/rasp2/files/) / [源码](https://github.com/sculab/RASP) |
| 原版 Lagrange | 历史 Python/C++ 实现 | 经典 DEC | 固定树最大似然 | 构建、旧数值实现和维护 | [Python](https://github.com/rhr/lagrange-python) |
| Lagrange-NG | 0.7.2-7，2026-02 | 高速并行经典 DEC | 固定树 DEC 条件似然 | 更快但仍不含 SSE/树不确定性 | [仓库](https://github.com/computations/lagrange-ng) |
| BioGeoBEARS | 描述版本 1.1.3；正式标签较旧，主支维护 | 六模型、修饰、化石约束、后验、BSM | 给定树范围条件似然 | 安装、状态爆炸、`e`/`mu` 混淆、`+J` 争议 | [仓库](https://github.com/nmatzke/BioGeoBEARS) |
| 原始 BayArea | 历史独立实现 | 数百区域、数据增广历史 | 贝叶斯范围占据路径 | 旧工具链，分叉语义与 DEC 不同 | [论文](https://doi.org/10.1093/sysbio/syt040) |
| RevBayes DEC | 1.4.1 教程持续更新 | 贝叶斯 DEC、时期、树样本、随机历史 | 可编程范围模型 | 脚本复杂、需自行验证 | [DEC 教程](https://revbayes.github.io/tutorials/biogeo/biogeo_intro) |
| PhyBEARS | 活跃研究仓库，无稳定正式发行 | 大状态地理 ClaSSE/SSE、BGB 类模型验证 | 范围与树生成过程 | 接口/文档变化，不是 BGB 外围功能完整替代 | [仓库](https://github.com/nmatzke/PhyBEARS.jl) |
| PyRate/DES | 活跃研究软件 | 化石出现、扩散、局部消失、采样 | 化石记录生成过程 | 保存和出现过程假设 | [PyRate](https://github.com/dsilvestro/PyRate) |

### 对 BGB Rust 的直接参照顺序

1. **BioGeoBEARS：** 模型语义和外部金标准。
2. **Lagrange-NG：** 经典 DEC 的现代数值与性能参照。
3. **RevBayes：** 贝叶斯 DEC、ClaSSE 和模型组合参照。
4. **PhyBEARS：** 地理 SSE、`D/E` ODE 与不可见支系参照。
5. **RASP：** GUI、工作流和结果消费层，不作为 Rust 核心公式标准。

## 9.5 SSE 与多样化

| 工具 | 状态 | 实现 | 最适合 | 主要警告 | 入口 |
|---|---|---|---|---|---|
| `diversitree` | 0.10-1，维护较低但可用 | BiSSE、MuSSE、QuaSSE、GeoSSE、ClaSSE | 经典复现、模拟、方法开发 | 假阳性、灭绝弱辨识 | [CRAN](https://cran.r-project.org/package=diversitree) |
| `hisse` | CRAN 2.1.11、开发 2.1.14 | HiSSE、GeoHiSSE、MuHiSSE、MiSSE/CID | 隐藏速率和复杂零模型 | 隐状态不可直接命名 | [官网](https://speciationextinction.info/) |
| FiSSE | R 实现 | 二元性状枝长统计 | 独立稳健性补充 | 不是完整生灭估计 | [论文](https://doi.org/10.1111/evo.13227) |
| RevBayes SSE | 1.4.1 | BiSSE/HiSSE/GeoSSE/ClaSSE、化石、随机历史 | 贝叶斯和自定义模型 | 先验与模型脚本审计 | [SSE 教程](https://revbayes.github.io/tutorials/) |
| PhyBEARS | 研究代码 | 大状态地理 SSE ODE | 范围与不可见侧枝联合 | 成熟度和可辨识性 | [仓库](https://github.com/nmatzke/PhyBEARS.jl) |

任何 SSE 软件都不能绕过以下证据要求：复杂性状无关零模型、独立转变次数、取样敏感性、模拟充分性、参数区间和化石支持。

## 9.6 祖先序列

| 工具 | 状态 | 强项 | 输出 | 关键边界 | 入口 |
|---|---|---|---|---|---|
| PAML | 4.10.10，2026-01 | 核酸/密码子/蛋白、选择模型 | `rst` 祖先序列与概率 | 固定比对和树，插入缺失弱 | [仓库](https://github.com/abacus-gene/paml) |
| FastML | 服务在线，公开核心版本较旧 | 边际/联合序列、插入缺失、候选样本 | FASTA、位点概率、树 | 规模、旧依赖、固定比对 | [服务](https://fastml.tau.ac.il/) |
| IQ-TREE | 3.1.3，2026-06 | 大规模 ML 树、模型选择、`-asr` | `.state` 位点概率 | 条件于一棵树/模型 | [官方文档](https://iqtree.github.io/doc/) |
| RAxML-NG | 2.0.2，2026-05 | 高性能 ML 与 `--ancestral` | 节点序列和概率 | 不积分拓扑与模型 | [仓库](https://github.com/amkozlov/raxml-ng) |
| HyPhy | 2.5.101，2026-06 | 密码子、选择、重组、自定义模型 | 结构化 JSON、枝替换 | 不是普通表型工具；脚本复杂 | [官网](https://hyphy.org/) |
| BAli-Phy | 4.2，2026-05 | 比对、树、祖先序列联合 | 联合后验样本 | 计算昂贵、模型复杂 | [官网](https://www.bali-phy.org/) |
| `phangorn` | 2.12.1 | R 内序列和形态祖先 | `phyDat`/概率 | 固定比对和树为主 | [文档](https://klausvigo.github.io/phangorn/articles/Ancestral.html) |

## 9.7 时间树、系统发育地理与化石

| 工具 | 状态 | 强项 | 不等于什么 | 入口 |
|---|---|---|---|---|
| BEAST X | 10.5.0，2025-07，仓库活跃 | 时间树、群体历史、离散/连续地点、复杂性状 | 不是 BEAST 2，也不是多地区物种范围模型 | [官网](https://beast.community/) |
| BEAST 2 | 2.7.8，2025-06，插件生态活跃 | 时间树、物种树、FBD、采样祖先、结构化溯祖 | 插件功能不是核心自动自带 | [官网](https://www.beast2.org/) |
| MrBayes | 3.2.7a 稳定版 | 分子+形态树、FBD/总证据能力 | 不是专用地理范围引擎 | [手册](https://nbisweden.github.io/MrBayes/manual.html) |
| RevBayes | 1.4.1 | 显式概率图、FBD、总证据、SSE、DEC | 灵活不等于自动正确 | [教程](https://revbayes.github.io/tutorials/) |
| MASCOT/BASTA | BEAST 2 插件/方法 | 结构化溯祖近似 | 不等于物种宏观范围 | [MASCOT](https://taming-the-beast.org/tutorials/Mascot-Tutorial/) |
| PyRate | 活跃 | 化石出现、保存、多样化与 DES | 不等于总证据树推断 | [仓库](https://github.com/dsilvestro/PyRate) |

## 9.8 一分钟决策树

```text
你要的是内部节点的类别吗？
  是 -> 固定树基线：ape/phytools；隐速率：corHMM；完整贝叶斯：BayesTraits/RevBayes
  否
你要的是整条枝的变化次数和时间吗？
  是 -> phytools/RevBayes；祖先范围则 BioGeoBEARS BSM 或 RevBayes
  否
你要的是物种可以同时占多个地区的范围吗？
  是 -> Lagrange-NG / BioGeoBEARS / RevBayes DEC
  且要整条谱系灭绝和不可见分叉 -> GeoSSE/ClaSSE/PhyBEARS
  否
你要的是样本或基因谱系的地点、时间和迁移吗？
  是 -> BEAST X / BEAST 2；有种群结构则 MASCOT/BASTA
  否
你要的是古 DNA/蛋白字母吗？
  是 -> PAML/FastML/IQ-TREE/RAxML-NG/HyPhy；比对也不确定则 BAli-Phy
  否
你要化石参与树生成和定年吗？
  是 -> BEAST 2 / RevBayes / MrBayes 的 FBD/总证据流程
```

## 9.9 复现时必须冻结什么

- 软件正式版本、提交哈希和下载来源；
- R/Julia/Python 及依赖锁文件；
- BEAST 2 全部插件版本；
- 完整命令、配置、脚本和随机种子；
- 树、矩阵、区域顺序和状态索引；
- 根先验、参数先验、优化边界和多起点；
- 链长、舍弃、有效样本量和重复链；
- 所有失败、取消和未收敛结果；
- 输出模式和后处理脚本。

对本仓库，ETE3 使用项目内置的 vendor 版，不能让系统环境中的另一个 ETE3 静默改变树处理结果。

本章最短结论：**软件是问题与概率模型的实现，不是方法层级排行榜。正确工具是能直接回答目标问题、且边界和验证最清楚的那个。**
