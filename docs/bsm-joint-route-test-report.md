# 多区域 founder 条件路线概率测试报告

构建：0.1.7-dev.20260930，Windows x86_64 MSVC，2026-09-30。
本地开发交付，未发布到 GitHub；exe 和完整包的 SHA256 随包提供。

## 行为

新增 bsm-assign-sources --allocation-mode probabilities，输出注册格式
biogeo-bsm-source-attribution-v2。支持扩散和多区域 founder 的 source × target
条件概率，switching 输出确定的唯一区域对。原事件及历史不变。
v2 不使用 RNG，无需 seed。原默认 sample 模式保持 v1 格式和既有 RNG。

每个原事件一行，候选路线另表保存，按 sample/event_id 连接。
每个有来源分配的事件总质量为 1；多个候选行不能当作多次事件。
算法 effective-dmat-joint-conditional-v1。完整语义和调用见 bsm-joint-route-probabilities.md。

## 自动化检查

|检查|结果|
|---|---|
|cargo test --workspace|457 通过，0 失败，2 项既有测试默认忽略。|
|cargo clippy --workspace --all-targets -- -D warnings|通过。|
|release 模式 bsm_source_contract|8 通过。|
|release 模式 rasp_host_contract|5 通过。|
|release 模式 schema_contract|1 通过；覆盖 v1/v2、full/compact、单目录/分片。|

覆盖非均匀方向性权重、多区域目标、两个时期、单/三线程、full/compact、原生分片、
部分文件缺失后的续跑、空历史、switching、不适用事件、合法概率和与稳定事件连接。
核心覆盖全零矩阵、缺省单位矩阵、超大有限权重和无效范围。
既有来源抽样测试继续覆盖六 preset 与 include/exclude、直接祖先和拟合后的地理修饰。

深度检查测试交换两个候选概率并同步修改文件校验值：
普通检查确认仍是合法概率分布，deep 则依据拟合重新计算并拒绝错误权重。
新旧格式均保留输入只读和模型/历史指纹绑定。

## 最终示例与 R 对照

用 examples/bsm_joint_routes 的固定模型生成 512 份历史，seed=1234。
两个时期 AB -> CD 的有效权重分别为 1:2:6:1 和 6:1:1:2。

- 原始事件 7,540 条。
- 可分配事件 3,793 条，不适用事件 3,747 条。
- 候选区域对 6,030 行，分成 30 对事件/候选分片。
- 多区域 founder 82 个，均获得条件概率；v1 对相同 82 个事件仍保持原未支持标记。
- v2 未确定事件为 0。
- 与 R 双精度矩阵归一化逐行比较，最大绝对差 1.1102230246251565e-16。

对照环境 R 4.5.0、BioGeoBEARS 1.1.3、cladoRcpp 0.15.1。
R 仅使用本项目隔离库；Windows 进程内使用 UTF-8 locale 读取中文路径，未改动全局 R 环境。

另外直接调用 cladoRcpp::rcpp_calc_anclikes_sp_COOweights_faster，
对 AB -> (AB,CD) 分别保留四个区域对中的一个矩阵项。
在两种时期、两种共同范围大小权重下，核对单项贡献之和与整体原始权重。
共同大小权重 1 时，原始权重和及归一化概率完全一致；
共同大小权重 0.2 时，原始权重和最大差约 7.45e-9，
归一化概率最大差约 5.96e-9，符合该 C++ 实现使用 float 中间量的舍入表现。
Rust 未为此增加任何数值修正。

按条件概率在 R 中每时期抽取 200,000 次，各区域对频率距期望均不到 1.86 个标准差，
低于事先设定的 6 个标准差阈值。
这是基于原始 C++ 分裂贡献的额外条件抽样验证，
不是声称原版 BGB 来源后处理已具有多目标接口，也不要求跨语言同 seed 逐条一致。

## 旧版与线程一致性

冻结 exe：0.1.6-dev.20260930；新 exe：0.1.7-dev.20260930。

- 同一保存拟合、相同 seed 的 512 份历史：86 个数据/字典/分片文件 SHA256 完全一致。
- metadata.tsv 仅 engine_version 不同。
- 同一历史、来源 seed=29：旧 sample 模式 5 个输出文件完全一致。
- v2 使用 1 或 3 workers：全部 64 个文件 SHA256 完全一致。
- 集成测试还核对来源后处理前后的拟合及历史输入快照完全一致。

逐文件证据和 R 数值表位于开发包 validation/ 目录。
此处只验证 Windows PC；没有把这些结果称为 Linux/服务器测试。
