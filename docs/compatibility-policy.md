# v0.1 接口兼容政策

本文定义 `biogeo-compatibility-policy-v1`。它适用于 `biogeo-cli` 与命令行脚本、新版 RASP 之间
已经进入 `schemas/registry.tsv` 的机器接口，不承诺 BioGeoBEARS R 函数逐函数兼容，也不参考旧版
RASP 的内部调用方式。

## 格式号

公开 artifact 使用完整格式号，例如 `biogeo-analysis-result-v2`。末尾 `vN` 是结构和语义主版本，
不是 CLI 软件版本。

- 同一格式号内不得删除、改名、重排、改变字段类型或改变字段语义；
- 当前 v0.1 采用严格 schema，同一格式号也不得静默增加未声明字段；
- 新增可选字段、改变目录布局或改变条件字段规则时发布新的 `vN` 格式和 schema；
- CLI 补丁版本可以修复计算错误、增加命令或增加新的独立格式，但不能改变既有格式的含义；
- artifact 自己的 `format` 字段优先于文件名、目录名、CLI 版本和字段相似度。

`engine-info` 中 `format_compatibility_policy=strict_versioned_schema`、
`unknown_format_policy=reject` 和 `unknown_field_policy=reject` 是上述规则的机器摘要。

## 读取与写出

读取器只接受代码明确实现且能力记录明确宣告的格式号。遇到未知格式号、未知字段、缺失必需字段、
未知目录条目或不满足条件字段的内容时必须报错，不能：

- 猜测它与某个旧格式相同；
- 忽略未知内容后继续计算；
- 根据扩展名选择近似解析器；
- 在原目录中原地修补或覆盖。

写出器只生成当前推荐格式。旧格式需要升级时使用显式迁移命令并写入新目录。例如
`analysis-result-migrate` 从 `biogeo-analysis-result-v1` 生成 `v2`，不会修改源结果。

## 弃用周期

v0.1 的最低弃用窗口为一个完整的 CLI 次版本：

1. 首次宣告弃用时，格式或命令仍可读取/运行，并进入 `engine-info` 的 `deprecated_formats` 或
   `deprecated_commands`；
2. 宣告弃用的整个次版本系列继续提供读取器或迁移路径；
3. 最早只能在下一个次版本删除，而且发布说明必须指出替代格式或命令；
4. 已归档科学结果优先保留只读加载或独立迁移工具，不依赖宿主悄悄改写。

当前明确弃用但仍受支持的格式是：

- `biogeo-analysis-result-v1`：只读并可迁移到 `v2`；
- `biogeo-bsm-tsv-v1` 和 `biogeo-bsm-sharded-tsv-v1`：兼容写出和读取，新任务推荐 compact v2。

当前没有弃用命令。`compatibility_commands` 表示受支持的低层入口，不等同于即将删除；新版 RASP
仍应优先使用版本化分析请求和高级工作流。

## 宿主行为

新版 RASP 启动时应按以下顺序处理兼容性：

1. 调用 `engine-info` 并识别 `biogeo-engine-capabilities-v1`；
2. 要求 `compatibility_policy_version=biogeo-compatibility-policy-v1`；
3. 对照同一发布目录的 `schemas/registry.tsv`；
4. 对每个输入和结果按其精确格式号选择 schema；
5. 对未知格式或未知字段停止导入并显示稳定机器错误，不进行模糊降级；
6. 只在用户或工作流明确要求时调用迁移工具，并保留原 artifact。

该政策不阻止 Rust 内部数据结构重构，也不要求帮助文本逐字稳定。稳定边界是格式号、schema、退出码、
机器错误/进度记录和已宣告命令的行为。

