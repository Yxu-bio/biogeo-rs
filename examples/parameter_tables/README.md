# 参数表示例

本目录保存 `parameter-template` 为六个正式 preset 生成的
`biogeo-parameter-table-v1` 完整示例：

- `dec.tsv`
- `decj.tsv`
- `divalike.tsv`
- `divalikej.tsv`
- `bayarealike.tsv`
- `bayarealikej.tsv`

这些文件是可编辑起点，不是六套独立算法。修改 `mode`、`value`、边界或
`expression` 后，通过 `model-evaluate` 或 `model-optimize` 运行同一个似然引擎。

完整格式、命令和当前参数语义边界见 `docs/parameter-table.md`。
