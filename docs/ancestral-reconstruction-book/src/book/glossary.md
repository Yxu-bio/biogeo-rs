# 术语表：先说中文，再给检索词

本表把中文概念放在前面。括号内英文主要用于查论文和软件，不要求靠英文才能理解正文。

## A. 树与历史对象

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 系统发育树 | phylogeny | 表示共同祖先与分支关系的树；不自动等于时间树 |
| 时间树 | time-calibrated tree | 枝长按时间计量的系统树 |
| 基因树 | gene tree | 某段基因的谱系历史，可能不同于物种树 |
| 物种树 | species tree | 物种分化关系的模型，不等于每个基因的树 |
| 根 | root | 树中代表所研究类群最早共同祖先的一端 |
| 末端 | tip, terminal | 被实际采样的现生或化石对象 |
| 内部节点 | internal node | 分支汇合处，对应模型中的共同祖先或分叉事件 |
| 冠群 | crown group | 某组现生成员最近共同祖先及其全部后代 |
| 茎群 | stem group | 比冠群更早、但比其最近现生姊妹更接近冠群的支系 |
| 枝长 | branch length | 可表示时间、期望替换数或其他演化距离，必须先确认单位 |
| 超度量树 | ultrametric tree | 所有同时代末端到根距离相同的时间树 |
| 拓扑 | topology | 谁与谁更近，不包括枝长数值 |
| 多分叉 | polytomy | 一个节点有两个以上子代；可是真实快速分化或未解决关系 |
| 直接祖先 | sampled/direct ancestor | 自身被采样、又位于另一采样谱系祖先线上的对象 |
| 幽灵谱系 | ghost lineage | 没被直接观察到、但模型或化石间隙允许存在的谱系段 |
| 观察树 | reconstructed/observed tree | 删除未采样和无后代谱系后，数据中实际看见的树 |
| 完整树 | complete tree | 包含已观察与未观察分叉、灭绝支系的生成历史 |
| 谱系不完全排序 | incomplete lineage sorting | 祖先种群中的基因谱系未在物种分化间隔内完成合并 |
| 网状演化 | reticulation | 杂交、基因渗入、水平转移等不能只用普通树表达的历史 |

## B. 概率与计算

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 状态 | state | 模型在某时刻需要记住的类别；可是一种颜色，也可是 A+B 组合范围 |
| 隐藏状态 | latent/hidden state | 未直接观察、由模型推断或求和消去的类别 |
| 参数 | parameter | 控制过程分布的量，如转变率、方差、形成率 |
| 似然 | likelihood | 给定参数和模型后，观察数据出现的相对支持程度 |
| 后验 | posterior | 数据与先验结合后，对参数、状态、树或历史的概率分布 |
| 先验 | prior | 看数据前对未知量的概率描述 |
| 边际概率 | marginal probability | 只问一个节点/参数，把其他未知量求和或积分消去后的概率 |
| 联合概率 | joint probability | 多个节点、参数或历史同时取某组合的概率 |
| 条件概率 | conditional probability | 在某些量已给定的前提下计算的概率 |
| 最大似然 | maximum likelihood | 寻找使观察数据似然最大的参数 |
| 贝叶斯积分 | Bayesian integration | 按后验把参数、树或历史的不确定性保留下来 |
| 剪枝算法 | pruning algorithm | 从末端向根递推条件似然，避免枚举所有祖先组合 |
| 连续时间马尔可夫链 | continuous-time Markov chain, CTMC | 状态在连续时间中按瞬时速率随机跳转的模型 |
| 速率矩阵 | rate matrix, Q | 记录各状态每单位时间怎样转变的矩阵 |
| 矩阵指数 | matrix exponential | 把瞬时速率矩阵变成一段时间后的转移概率 |
| 常微分方程 | ordinary differential equation, ODE | 描述一个量随连续时间怎样变化的方程 |
| 数值积分 | numerical integration | 用离散计算近似求解积分或微分方程 |
| 容差 | solver tolerance | 数值求解器允许的误差尺度，不是生物学置信区间 |
| 归一化 | normalization | 让一组非负权重和为 1，变成条件概率 |
| 对数似然 | log-likelihood | 似然取对数，便于数值稳定和相加 |
| 数值缩放 | numerical scaling | 在递推中重标概率并记录尺度，防止下溢 |
| 均匀化 | uniformization | 用泊松跳数表示 CTMC 转移的数值方法 |
| 条件马尔可夫桥 | conditional CTMC bridge | 已知枝两端状态时，抽取中间跳转路径 |
| 蒙特卡洛 | Monte Carlo | 用大量随机样本近似分布、积分或不确定性 |
| 马尔可夫链蒙特卡洛 | MCMC | 构造相关样本链来近似后验分布 |
| 有效样本量 | effective sample size, ESS | 考虑样本相关后，相当于多少独立样本的信息量 |
| 收敛 | convergence | 数值算法稳定到目标附近；不保证模型正确或参数可辨识 |
| 可辨识性 | identifiability | 不同参数或过程能否由观察分布区分 |
| 似然脊线 | likelihood ridge | 一串不同参数组合给出近似相同似然的区域 |
| 边界估计 | boundary estimate | 最优参数落在允许区间端点，常提示弱信息或模型结构问题 |
| 自动微分 | automatic differentiation | 由计算图精确传播导数，帮助梯度优化和采样 |

## C. 祖先性状与比较方法

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 祖先状态重建 | ancestral-state reconstruction | 估计内部节点或过去时刻的性状状态 |
| 随机性状历史 | stochastic character mapping | 从条件分布中抽取整棵树上相互一致的状态路径 |
| 简约法 | parsimony | 偏好所需变化次数或代价最小的历史 |
| Mk 模型 | Mk model | 离散形态/性状在有限状态间连续时间转变的模型 |
| Mkv 修正 | Mkv ascertainment correction | 数据只保留可变字符时，对筛选过程进行条件修正 |
| 等速率模型 | equal-rates model, ER | 所有允许方向共享一个转变率 |
| 对称速率模型 | symmetric-rates model, SYM | 正反方向共享速率，不同状态对可不同 |
| 全不同速率模型 | all-rates-different, ARD | 每个有向转变可有独立速率 |
| 隐马尔可夫模型 | hidden Markov model | 观察性状之外增加未观察类别，以表示速率异质性 |
| 相关演化 | correlated evolution | 两个性状的联合转变结构不能由独立过程很好解释 |
| 阈值模型 | threshold model | 离散观察状态由隐藏连续倾向跨越阈值产生 |
| 布朗运动 | Brownian motion, BM | 连续性状随机游走，方差随时间线性增长 |
| 奥恩斯坦–乌伦贝克模型 | Ornstein–Uhlenbeck, OU | 连续性状随机变化，同时向长期均值回拉 |
| 适应峰 | adaptive optimum | OU 中长期均值的一种生物解释，不应仅凭参数名当成事实 |
| 半衰期 | phylogenetic half-life | OU 偏离长期均值的影响减半需要的时间 |
| 早期爆发模型 | early burst, EB | 演化速率随时间下降，早期变化更快的模型 |
| 跳跃模型 | jump model | 少数时点或枝发生较大突变的连续性状模型 |
| 系统发育独立对比 | phylogenetic independent contrasts, PIC | 在 BM 条件下把姐妹差异变换成近似独立对比 |
| 系统发育广义最小二乘 | PGLS | 用树诱导的协方差修正回归残差非独立性 |
| 系统发育信号 | phylogenetic signal | 近缘物种性状相似程度的一组统计描述 |
| 测量误差 | measurement error | 观察值围绕物种真实值的误差，应与演化方差区分 |
| 制度 | regime | 模型中共享某组速率或适应峰的枝/时期类别 |

## D. 历史生物地理

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 分布范围 | geographic range | 一个物种同时占据的地点集合、区域集合或连续空间 |
| 区域组合状态 | range state | 如 A+B，表示同一谱系同时分布于 A 和 B，不是两个物种 |
| 范围扩张 | range expansion, dispersal | 沿枝加入一个区域，如 A 到 A+B |
| 区域局部消失 | local extirpation | 沿枝失去一个区域，如 A+B 到 A；谱系仍活着 |
| 隔离分化 | vicariance | 广域祖先范围在分叉时被两个子代分割继承 |
| 同域复制 | sympatry/copy | 两个子代都继承祖先完整范围 |
| 子集同域 | subset sympatry | 一个子代继承祖先全范围，另一个继承其中一部分 |
| 创始者式分化 | founder-event speciation | 一个子代在分叉附近进入祖先范围外的新区域 |
| 分叉发生变化 | cladogenetic change | 状态变化与分叉事件绑定 |
| 沿枝发生变化 | anagenetic change | 状态在两个分叉节点之间改变 |
| DEC | dispersal–extinction–cladogenesis | 用范围扩张、局部消失和分叉继承描述祖先范围的模型族 |
| DIVA | dispersal–vicariance analysis | 偏重隔离分化事件的历史生物地理方法 |
| BayArea | Bayesian island biogeography model | 用区域占据变化描述大区域集合历史的一类模型 |
| 最大范围大小 | maximum range size | 状态空间允许同时占据的最多区域数 |
| 空范围 | null range | 不占据任何研究区域的状态；是否允许及含义需明确 |
| 扩散倍率 | dispersal multiplier | 按方向、距离、时期或地质连接调整扩张速率的权重 |
| 时间分层 | time stratification | 不同时期使用不同地理连接、状态或速率 |
| 生物地理随机历史 | biogeographic stochastic mapping, BSM | 条件于树和数据抽取完整范围变化历史 |
| 分叉事件表 | cladogenetic event table | 列出祖先范围可怎样分配给左右子代及其权重/速率 |
| 地理 SSE | geographic SSE | 让范围状态与物种形成、整条谱系灭绝和取样共同生成树 |

## E. 多样化、取样与幽灵谱系

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 状态依赖形成与灭绝模型 | state-dependent speciation and extinction, SSE | 性状状态可影响树的形成、灭绝与状态变化 |
| 物种形成率 | speciation rate, lambda | 一条谱系每单位时间发生分叉的瞬时率 |
| 整条谱系灭绝率 | lineage extinction rate, mu | 一条谱系每单位时间完全终止的瞬时率 |
| 取样比例 | sampling fraction, rho | 现在存在的谱系中被纳入数据的比例或概率 |
| 化石采样率 | fossil sampling rate, psi | 谱系每单位时间留下并被观察为化石的速率 |
| BiSSE | binary-state SSE | 两状态性状依赖形成和灭绝模型 |
| MuSSE | multistate SSE | 多状态版本的 SSE |
| QuaSSE | quantitative-state SSE | 连续性状版本的 SSE |
| GeoSSE | geographic SSE | 两地区范围与多样化共同建模的 SSE |
| ClaSSE | cladogenetic SSE | 允许分叉时状态改变的通用 SSE |
| HiSSE | hidden-state SSE | 增加未观察类别以表示多样化异质性 |
| FiSSE | fast, intuitive SSE test | 用枝长对比检验二元性状与多样化关联的非参数化补充 |
| 性状无关多样化模型 | character-independent diversification, CID | 多样化可异质，但不由目标性状解释的零模型 |
| 无采样后代概率 | extinction/no-sampled-descendant probability, E | 一条谱系到现在没有任何被观察后代的总概率 |
| 观察子树概率 | observed-subtree probability, D | 一条祖先谱系产生指定观察子树和末端数据的概率 |
| 数据增广 | data augmentation | 在后验中显式抽样一部分原本未观察的历史对象 |
| 生灭过程 | birth–death process | 用形成与灭绝率生成分支树的随机过程 |
| 化石化生灭过程 | fossilized birth–death, FBD | 把形成、灭绝、化石采样与现生取样统一的树生成过程 |
| 溯祖过程 | coalescent | 从样本向过去描述基因谱系在种群中合并的过程 |
| 结构化溯祖 | structured coalescent | 允许谱系在多个种群/地点间迁移的溯祖过程 |

## F. 序列、时间与空间

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 祖先序列重建 | ancestral sequence reconstruction | 估计祖先 DNA、RNA 或蛋白质字母及其不确定性 |
| 替换模型 | substitution model | 描述序列字母随时间改变概率的模型 |
| 分子钟 | molecular clock | 把替换量与时间联系起来的速率模型 |
| 严格钟 | strict clock | 所有枝共享同一替换速率 |
| 松弛钟 | relaxed clock | 枝间速率可按某个分布变化 |
| 比对不确定性 | alignment uncertainty | 哪些字符同源本身不确定 |
| 插入缺失 | insertion/deletion, indel | 序列片段的加入或删除；不能总当作普通缺失数据 |
| 重组 | recombination | 序列不同片段拥有不同谱系历史 |
| 祖先蛋白复活 | ancestral protein resurrection | 合成推断的祖先序列并实验测量其功能 |
| 系统发育地理 | phylogeography | 研究基因/样本谱系的空间历史，尺度常低于物种宏观范围 |
| 离散地点模型 | discrete phylogeography | 把采样地点作为有限类别，在树上建模迁移 |
| 连续空间扩散 | continuous phylogeography | 用坐标和空间随机过程估计谱系移动 |
| 群体动力学 | phylodynamics | 用时间树推断群体规模、传播或流行过程 |
| 总证据定年 | total-evidence dating | 联合现生分子、现生/化石形态、年龄和树过程估计时间树 |
| 节点定年 | node dating | 用节点年龄约束校准时间树 |
| 末端定年 | tip dating | 直接使用古老样本或化石的采样年龄参与定年 |

## G. 验证与报告

| 中文术语 | 英文检索词 | 白话解释 |
|---|---|---|
| 参数恢复 | parameter recovery | 从已知参数模拟数据，再看分析能否估计回来 |
| 校准 | calibration | 声称 70% 的事件长期是否约有 70% 为真 |
| 模型比较 | model comparison | 在候选模型之间评估相对支持 |
| 模型充分性 | model adequacy | 检查拟合模型是否能产生像观察数据的数据 |
| 后验预测检查 | posterior predictive check | 从后验生成数据，与观察摘要比较 |
| 参数自举 | parametric bootstrap | 从拟合参数模拟重复数据，重跑流程评估统计量 |
| 模型平均 | model averaging | 按模型权重汇总参数或祖先结果 |
| 敏感性分析 | sensitivity analysis | 改变合理假设，检查结论依赖哪些选择 |
| 外部金标准 | external golden/reference | 用独立软件或已知结果锁定实现语义的对照 |
| 交叉固定重算 | cross-evaluation | 在甲软件最优参数处让乙重算似然，反之亦然 |
| 可复现性 | reproducibility | 同一输入、版本、配置与种子能得到可核查结果 |
| 可重复性 | repeatability | 独立重复研究或测量能否得到相符证据 |
| 条件主张 | conditional claim | 明确写出结论依赖的树、数据、模型和先验 |
| 假阳性 | false positive | 真实没有目标效应，却被分析判作有支持 |
| 假精确 | false precision | 输出数字很窄或小数很多，但遗漏了重要不确定性 |

## 一个必须记住的四句翻译

- **range as state：** 把一个物种同时占据哪些区域，作为谱系此刻的类别。
- **state-dependent diversification：** 不同类别的谱系可以有不同的分叉和整条谱系灭绝速率。
- **integrating over ghost lineages：** 不逐条画出未观察支系，而把所有可能未观察历史的概率加总进似然。
- **joint inference：** 树、时间、性状或取样中的多个未知层一起估计，并传播它们的相互依赖。
