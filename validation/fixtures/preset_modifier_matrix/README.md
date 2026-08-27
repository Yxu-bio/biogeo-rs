# 六 preset 修饰组合 fixture

该 fixture 用同一棵四区域、六末端树检查统一模型引擎的组合能力：

- 静态或两时期的 manual dispersal、地理距离、环境距离和 area size；
- 固定 `x/n/w/u` 对上述原始输入的组合；
- DEC、DEC+J、DIVALIKE、DIVALIKE+J、BAYAREALIKE、BAYAREALIKE+J 的正式
  `y/s/v/j` 联动规则；
- 非默认、事件专属 `mx01y/s/v/j` daughter-size 控制。

参数表只通过版本化公共入口读取。fixture 不参与核心分支判断，预期值由
`validation/preset-modifier-combination-matrix.tsv` 冻结。
