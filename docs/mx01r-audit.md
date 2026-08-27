# `mx01r` 语义审计

## 结论

在本项目冻结的 BioGeoBEARS `1.1.3` 源码提交
`7d2092f94a5d2b598807771379ef6c58a84b4fb3` 中，`mx01r` 是参数表中的占位行，当前
似然、祖先范围、节点分裂和分时期计算路径均不消费它。它不能作为自由参数优化，否则只会
增加一个完全平坦、不可识别的维度。

Rust 保留 `mx01r`，以便完整读写 BioGeoBEARS 的 23 行参数表，但当前只接受：

```text
mx01r=fixed(0.5)
```

这不是用 fixture 特判结果，也不是自行删减一个有效模型参数，而是复现当前 BioGeoBEARS
版本的可观察语义。非默认固定值和 `Free`/`Derived` 配置都会被公共参数入口拒绝。

## 源码证据

对隔离源码执行全仓库文本搜索后，`mx01r` 只出现在以下位置：

1. `BioGeoBEARS_classes_v1.R` 定义参数行：默认类型 `fixed`、初值 `0.5`、
   `note="no"`，描述为 root range-size probability。
2. `calc_uppass_probs_v1.R` 的四处已序列化示例对象中，作为 23 行参数表的名称、值和描述
   出现；这些位置没有读取它参与计算。
3. BioGeoBEARS 自带测试中的两份同类序列化对象。

实际生成 cladogenesis 权重的代码只读取 `maxent01s_param`、`maxent01v_param`、
`maxent01j_param` 和 `maxent01y_param`。非分时期和分时期统一模型入口都把
`probs_of_states_at_root=NULL` 传给 pruning；root 代码在该值为 `NULL` 时不乘任何
range-size prior。源码中没有从 `mx01r` 构造 `probs_of_states_at_root` 的路径。

## 运行时扰动

[`biogeobears-mx01r-audit.R`](../validation/biogeobears-mx01r-audit.R) 使用 BioGeoBEARS
原生运行对象和似然函数，把 `mx01r` 固定为 `0.0001`、`0.5` 和 `0.9999`。`0.5` 是
基线，脚本要求所有已提取量的逐元素最大绝对差严格等于 0，否则直接失败。

| 案例 | 审计量 | 三点结果 |
| --- | --- | --- |
| 5-area、8-tip、26-state 复杂静态 DEC | lnL、26 维根后验、完整 uppass/downpass、1295 个 split probability、765 项 cladogenesis 签名 | 全部最大绝对差 `0` |
| BioGeoBEARS 官方 Psychotria M4b 五时期案例 | lnL、五时期共 2155 项 cladogenesis 签名 | 全部最大绝对差 `0` |

冻结的逐案例结果见
[`biogeobears-mx01r-audit.tsv`](../validation/golden/biogeobears-mx01r-audit.tsv)。复现命令：

```powershell
& 'C:\Program Files\R\R-4.5.0\bin\x64\Rscript.exe' `
  validation/biogeobears-mx01r-audit.R
```

## 版本升级规则

升级 BioGeoBEARS 来源版本时必须重新做源码搜索和运行时扰动。若上游以后真正把 `mx01r`
接入 root prior，则应先明确归一化、null range、状态约束和分时期 root 的规则，再实现
Rust 语义并建立固定似然、根后验、优化及生物地理随机历史对照；不能沿用当前空操作结论。
