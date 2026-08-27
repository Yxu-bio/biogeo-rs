# 多数据集批任务

## 定位

`dataset-batch` 面向新版 RASP 的多树、多数据集任务调度。每个数据集拥有独立的树、末端观测、
状态空间、时期/距离/面积修饰和优化设置，并在自己的目录中调用现有 `model-batch`。

层次必须保持明确：

- `model-batch`：一个数据集内拟合多个模型，计算 AIC/AICc/Akaike weight 和模型平均
  祖先范围；
- `dataset-batch`：运行多个彼此独立的 `model-batch`，汇总成功、失败和恢复状态；
- 不同数据集、不同树或不同末端观测之间绝不共同计算 Akaike weight。

该实现复用同一 `model-optimize`、`biogeo-analysis-result-v2` 和模型比较代码，没有第二套
似然或优化器。

## 数据集 Manifest

```text
biogeo-dataset-batch-manifest-v1
dataset_id<TAB>models<TAB>config
StudyA<TAB>models.tsv<TAB>study-a-config.tsv
StudyB<TAB>models.tsv<TAB>study-b-config.tsv
```

- `dataset_id` 在任务内按大小写不敏感方式唯一，并满足 Windows/Linux 可移植目录名规则。
- `models` 指向现有 `biogeo-model-batch-manifest-v1`；不同数据集可以共享或分别使用模型表。
- `config` 指向该数据集的 `biogeo-model-batch-config-v1`。
- 两类路径都相对于数据集 manifest 所在目录解析；初始化时记录规范路径、文件字节指纹和
  结果子目录。

## 数据集配置

配置使用一行一个 CLI 选项的版本化表：

```text
biogeo-model-batch-config-v1
option<TAB>value
--tree<TAB>tree.nex
--tree-name<TAB>selected
--ranges<TAB>ranges.tsv
--max-range-size<TAB>4
--include-null-range<TAB>true
--max-iterations<TAB>1000
```

路径选项相对于配置文件所在目录解析。支持当前通用模型批量入口的完整输入组合：

- 树和观测：`--tree`、`--tree-name`、`--ranges`、`--detections`、`--controls`、
  `--use-detection-model`、`--use-ambiguities`；
- 状态空间：`--min-branch-length`、`--max-range-size`、`--include-null-range`、`--root-prior`；
- 修饰输入：`--dispersal-multipliers`、`--dispersal-strata`、`--distance-matrix`、
  `--environment-distance-matrix`、`--extirpation-multipliers`、`--area-sizes`；
- 优化：`--initial-step`、`--tolerance`、`--max-iterations` 和可重复的
  `--additional-start`。

三个布尔开关必须显式写 `true` 或 `false`。除 `--additional-start` 外，选项不可重复。
`--manifest`、`--output-dir`、`--resume`、`--parameters` 和结果目录由调度层管理，配置中禁止
出现。配置最终仍交给正式 CLI 参数解析器，因此 detection/ranges 冲突、缺少输入、数值边界
和模型依赖不会在批量层被放宽。

## 运行与恢复

```powershell
biogeo-cli --error-format tsv dataset-batch `
  --manifest datasets.tsv `
  --output-dir dataset-results
```

任务按 manifest 顺序执行，避免当前 PC 同时启动多个高内存优化器。某个数据集失败后仍继续
后续数据集；进程最后返回退出码 `2`，机器错误指向该次不可变 attempt 文件。补齐暂缺输入或
排除外部故障后，以完全相同的 manifest 和配置追加：

```powershell
--resume
```

恢复严格校验 manifest、模型表和配置文件字节身份。已经完成的数据集会进入其内部
`model-batch --resume` 校验路径，缺失模型才重新优化；缺失的数据集目录从头初始化。修改
任何已冻结配置后必须使用新输出目录，不能借旧结果续跑。

## 结果目录

```text
dataset-results/
  run.tsv
  source-manifest.tsv
  jobs.tsv
  complete.tsv
  attempts/
    attempt-000001.tsv
    attempt-000002.tsv
  datasets/
    StudyA/
      comparison.tsv
      model-averaged-ancestral-ranges.tsv
      complete.tsv
      attempts/
      models/
    StudyB/
      ...
```

每次调用写一个不可覆盖的 `biogeo-dataset-batch-attempt-v2`，逐数据集记录
`complete/failed/cancelled/not_started`、结果路径、比较表、稳定错误分类和经过 TSV 百分号
编码的错误信息。嵌套 `model-batch` 同样写 `biogeo-model-batch-attempt-v2`，因此 RASP 可以
从数据集一直追到具体失败或取消的模型。已有 v1 attempt 仍保持不可变。

只有所有数据集都完成后才发布根 `complete.tsv`。每个数据集的 `comparison.tsv` 和
`model-averaged-ancestral-ranges.tsv` 只包含该数据集自己的模型，样本数、权重和 posterior
不会跨目录归一化。

## 当前边界

- 当前顺序执行数据集和模型，尚未加入跨进程并发调度。
- attempt 是耐久汇总；实时状态由 `biogeo-cli-progress-v1` 提供。
- 通用优化和两级 batch 已协作式响应 `Ctrl+C`；生物地理随机历史另有检查点和暂停能力。
- 各单模型 v2 分析结果已自包含；dataset-batch 的调度清单和 attempt 仍是目录层协议，
  整体搬迁后应先校验每个已完成分析结果。
