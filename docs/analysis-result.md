# 版本化分析结果与生物地理随机历史

## 目标

`biogeo-analysis-result-v2` 把一次通用 `model-evaluate` 或 `model-optimize` 的拟合点保存为
自包含、可审计、可严格重放的目录。它不是另一套似然或随机历史算法；`model-bsm` 重新构造原来的
`StateSpace + ModelConfig + PruningResult`，验证一致后交给现有的同一个 BSM sampler 和 writer。
旧 `biogeo-analysis-result-v1` 仍可读取和重放，但新分析只写 v2。

## 写出结果

固定评估和优化都可以追加 `--analysis-result-dir`：

```powershell
cargo run --release -q -p biogeo-cli -- model-optimize `
  --tree <tree.nwk> `
  --ranges <ranges.tsv> `
  --parameters <parameters.tsv> `
  --analysis-result-dir <fit-result> `
  --max-iterations 500
```

目标目录必须不存在。实现先在同一父目录完成暂存、输入读取和全部元数据写入，成功后再以
目录重命名发布；失败会清理暂存目录，不留下看似有效的半成品，也不会覆盖旧结果。

目录固定包含：

```text
fit-result/
  metadata.tsv
  inputs.tsv
  source-parameters.tsv
  resolved-parameters.tsv
  input-bundle/
    metadata.tsv
    files.tsv
    files/
      inputs/
      dependencies/
      provenance/
```

- `source-parameters.tsv`：用户提交的原始参数表，保留 `fixed/free/derived` 声明。
- `resolved-parameters.tsv`：把最终所有解析值冻结成 `fixed` 的可执行参数表。
- `inputs.tsv`：树、范围或 detection/control、时期表和各类静态修饰的包内相对路径、
  字节数、指纹及是否为重放必需输入。通过 `analysis-run` 执行时，原始统一请求还会以
  `analysis_request` 审计角色保存，但模型重放仍以冻结后的输入和参数为准。
- `input-bundle/`：版本化的 `biogeo-input-bundle-v1`。`files.tsv` 同时记录源文件和包内
  执行文件的字节数/指纹，所有清单路径都是使用 `/` 的相对路径。
- `metadata.tsv`：格式、lnL 十进制值和 IEEE 754 位值、状态空间、root prior、tip
  observation 模式、模型身份、输入文件名及优化诊断。

`status=complete` 表示结果目录和输入包已完整发布，不表示优化一定收敛。优化结果单独记录
`optimization_converged`、迭代数、评估数、起点数和收敛起点数；即使未收敛，用户仍可在
明确看到诊断的前提下重放该参数点，不会被软件伪装成收敛结果，也不会被静默禁止。

## 模型身份与重放门禁

模型指纹不依赖 Rust `Debug` 文本。核心的 `biogeo-model-identity-v1` 按固定顺序编码：

- `d/e/a/b` 的精确浮点位值；
- 静态 dispersal 和 extirpation 倍率；
- 每个时期的年龄边界、dispersal、extirpation、`areas_allowed` 和 adjacency；
- `y/s/v/j` 与 `mx01y/s/v/j`。

该字节规范有全配置固定哈希回归。加载结果时依次执行：

1. 校验结果目录内部文件、输入包清单和包聚合指纹；
2. 校验所有顶层输入、二级依赖和它们的包内路径边界；
3. 从 `resolved-parameters.tsv` 和原输入重建 tip likelihood、状态空间及模型；
4. 校验状态、区域、末端数和 `biogeo-model-identity-v1` 指纹；
5. 重新计算 lnL，并以严格浮点容差对照保存值；
6. 只有全部通过后才准备和抽取生物地理随机历史。

时期表会用结构化解析器识别 `matrix/distance_matrix/environment_distance_matrix/area_sizes`
及可选的 `areas_allowed/areas_adjacency`，收集它们引用的二级文件。包内执行副本的路径被
重写为相对路径；原时期表逐字节保存在 `provenance/`，不会因规范化丢失审计信息。
加载器拒绝绝对路径、`..` 越界、包外符号链接、未声明或未被引用的二级文件。
当前 FNV-1a 指纹用于可复现性和误操作诊断，不是抵御恶意篡改的密码学签名。

## 检查与迁移

只检查目录结构、内部文件和所有输入指纹：

```powershell
cargo run --release -q -p biogeo-cli -- analysis-result-inspect `
  --analysis-result <fit-result>
```

追加 `--replay` 会重建模型并重算 lnL。输入包也可独立检查：

```powershell
cargo run --release -q -p biogeo-cli -- input-bundle-inspect `
  --input-bundle <fit-result/input-bundle>
```

旧 v1 结果迁移为当前 v2：

```powershell
cargo run --release -q -p biogeo-cli -- analysis-result-migrate `
  --analysis-result <old-v1-result> `
  --output-dir <new-v2-result>
```

迁移先完整重放 v1，在同一父目录的临时目录打包且再次重放 v2，然后才以目录重命名
发布。目标已存在、源输入变化、二级依赖缺失或科学重放不一致时都不会发布目标。
当前 v2 作为源时会明确报告“无需迁移”，不会通过重复转换制造新身份。

## 运行生物地理随机历史

少量样本可保留在内存并输出兼容的八个表段：

```powershell
cargo run --release -q -p biogeo-cli -- model-bsm `
  --analysis-result <fit-result> `
  --bsm-samples 100 `
  --bsm-threads auto `
  --seed 1
```

大任务应使用现有流式目录、检查点和分片执行层：

```powershell
cargo run --release -q -p biogeo-cli -- model-bsm `
  --analysis-result <fit-result> `
  --bsm-samples 5000 `
  --bsm-output-dir <bsm-result> `
  --bsm-threads auto `
  --bsm-max-in-flight 32 `
  --bsm-checkpoint-samples 500 `
  --seed 1
```

`--bsm-resume`、事件预算、内存窗口预算、耗时上限、固定区间分片和交互暂停均复用固定
preset 命令的实现。每个样本仍由 master seed 与 sample index 派生随机流，所以 worker 数
变化不会改变第 n 条随机历史；自动回归已验证 1/4 worker 的八表逐字节一致。

当前重放入口覆盖 exact range、`0/1/?` ambiguous range 和 `mf/dp/fdp` detection 三种
tip observation 模式，也覆盖
静态与分时期的 `x/n/u/w`、extirpation 和状态约束。基础 DEC 结果生成的八类随机历史表已
与原固定 DEC 命令逐字节交叉一致；detection 与两时期原始修饰的联合案例也已通过重放。

`model-batch` 直接把每个优化模型发布成这种目录，再从目录中的冻结 lnL、优化诊断和原始
参数表计算模型比较。它不会从易变的终端文本抓取参数或 lnL。批量目录结构、恢复规则和
AIC/AICc 契约见 [`model-batch.md`](model-batch.md)；多数据集/多树分层调度见
[`dataset-batch.md`](dataset-batch.md)。

目录结构、metadata 键和表头的机器契约在 `schemas/registry.tsv` 中以
`biogeo-analysis-result-v2` 和 `biogeo-input-bundle-v1` 注册。真实 CLI 进程测试会生成结果、
执行重放和迁移，再按 schema 拒绝未发布的字段漂移；关系完整性仍以加载器和 `--replay` 为准。

## 当前边界

- v2 是文件系统级自包含目录，可整个复制或重命名；尚未定义分析结果专用 zip/tar 容器层和
  密码学签名。Windows 软件发布 ZIP 不改变这一点。
- v1 只读兼容仍依赖原机器绝对路径；移动前必须在源输入完整时迁移。
- 原始参数表的包内副本只用于审计，重放使用结果目录内部冻结的参数表。
- BSM 的 `biogeo-bsm-tsv-v1` 八表列没有改变；将来增加新的汇总列必须发布新格式。
- 结果格式只接受自身声明的版本和模型身份版本，不能把未知版本按“差不多兼容”加载。
