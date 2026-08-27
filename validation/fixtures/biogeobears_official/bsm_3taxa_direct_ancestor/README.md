# BioGeoBEARS 官方派生直接祖先夹具

本夹具以 BioGeoBEARS 官方 `BSM_3taxa/M3areas_allowed_wFossilBranch` 的树和范围数据为基础。
`tree.nwk` 由 BioGeoBEARS 1.1.3 的 `add_hook()` 生成，调用参数为：

```r
add_hook(
  t = tr,
  tipname = "chimp",
  depthtime = 0.5,
  brlen_of_side_branch = 1e-7,
  newtipname = "fossil_hook"
)
```

`fossil_hook` 与所附着谱系使用相同范围 `C`。清单同时运行两个对照，二者均使用
BioGeoBEARS 默认阈值 `1e-6`：

- `tree.nwk` 的侧枝为 `1e-7`：按直接祖先语义处理，不发生节点分裂事件；
- `tree_threshold_equal.nwk` 的侧枝恰为 `1e-6`：源码使用严格 `<`，因此它仍是普通物种形成分支。

第二棵树同样由 `add_hook()` 生成，只把 `brlen_of_side_branch` 改为 `1e-6`。这样可以证明
结果差异来自公开的阈值语义，而不是为某个数值结果加入特判。
