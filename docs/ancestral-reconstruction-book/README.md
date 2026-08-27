# 看不见的祖先，怎样成为可检验的推断

## 祖先状态、性状演化、历史生物地理与谱系生成模型

**中文研究型教材，版本 1.1**  
**资料检索截止：2026-08-10**  
**面向读者：系统发育、生物地理、宏演化研究者，以及本项目的算法开发者**

## 直接阅读

- [单文件网页版](dist/book/book.html)：带侧栏目录、公式、响应式布局和打印样式；无需联网。
- [PDF 版](dist/book/ancestral-reconstruction-book.pdf)：标准 A4 固定版式，适合 WPS、打印和批注。
- [EPUB 电子书](dist/book/book.epub)：适合电子阅读器和移动端。
- [合并 Markdown](dist/book/book.md)：适合全文检索、批注和版本比较。
- [独立数学小册子](dist/math-booklet/math-booklet.html)：从概率、树上剪枝到 DEC、SSE 与模拟训练，另有 [PDF](dist/math-booklet/ancestral-reconstruction-math-booklet.pdf)、[EPUB](dist/math-booklet/math-booklet.epub) 和 [Markdown](dist/math-booklet/math-booklet.md)。
- 运行 `powershell -ExecutionPolicy Bypass -File scripts/build_book.ps1` 可重建整书，运行 `powershell -ExecutionPolicy Bypass -File scripts/build_math_booklet.ps1` 可重建小册子，运行 `powershell -ExecutionPolicy Bypass -File scripts/build_pdfs.ps1` 可重建两份 PDF。

## 目录结构

- `src/book/`：整书分章源稿。
- `src/math-booklet/`：数学小册子的前言、导读和元数据。
- `assets/`：网页版与 EPUB 样式、Pandoc 过滤器。
- `scripts/`：所有构建脚本。
- `dist/book/`：整书 Markdown、HTML、EPUB 和 PDF 成品。
- `dist/math-booklet/`：数学小册子 Markdown、HTML、EPUB 和 PDF 成品；WPS/Word 版可运行 `scripts/build_wps_docx.ps1` 生成。

这部书围绕一句话展开：

> 祖先重建结果不是“祖先就是这样”，而是“在这些数据、这棵树、这套演化与取样模型成立时，哪些祖先状态和历史仍然说得通”。

全书不按软件菜单罗列功能，而按研究问题组织。每种方法都回答六件事：

1. 真实观测是什么？
2. 隐藏对象是什么？
3. 哪些量被固定、估计或求和消去？
4. 似然或后验是怎样算出来的？
5. 软件输出能支持什么结论？
6. 哪个反例最容易让结论失效？

## 先读结论

- **祖先节点、整段历史和谱系生成过程是三个不同对象。** 节点饼图不能自动拼成一条可能发生的完整历史。
- **性状模型与树模型也是两个层次。** Mk、布朗运动和 DEC 通常在给定树上描述变化；SSE、化石化生灭过程和部分 BEAST/RevBayes 模型还描述树怎样产生并被采样。
- **BioGeoBEARS 中的 `e` 通常是区域局部消失。** `A+B -> A` 后谱系仍活着；这不等于 SSE 中整条谱系灭绝的 `mu`。
- **SSE 确实把不可见的灭绝侧枝纳入似然，但通常不逐条复原它们。** `E_i(t)` 是“无已采样后代概率”，不是一棵已经画出来的幽灵树。
- **BEAST X 不是 BioGeoBEARS 的高级版。** 它主要联合估计时间树、序列演化、群体历史和系统发育地理；DEC 类方法主要重建物种可同时占据多个地区的范围演化。
- **更大的模型不天然更真实。** 数据不足时，联合模型会扩大不确定性，也可能暴露不可辨识性；收敛和 AIC 第一名都不等于模型合格。
- **本项目并不“低级”。** 当前 Rust 引擎解决的是范围演化条件似然、后验和完整生物地理随机历史，而且已有严格外部验证。把 SSE/FBD 接进来会改变概率对象和计算内核，不是补一个参数。

## 目录

1. [前言：如何用费曼方法学这门课](src/book/00-preface.md)
2. [第一章：先认清我们究竟在重建什么](src/book/01-question-map.md)
3. [第二章：从祖先故事到概率生成模型](src/book/02-history.md)
4. [第三章：共同的概率发动机](src/book/03-probability-engine.md)
5. [数学插章：把核心公式真正看懂](src/book/03-math-companion.md)
6. [第四章：离散性状与随机性状历史](src/book/04-discrete-traits.md)
7. [第五章：连续性状、相关演化与比较方法](src/book/05-continuous-traits.md)
8. [第六章：祖先分布与历史生物地理](src/book/06-historical-biogeography.md)
9. [第七章：SSE、PhyBEARS 与幽灵谱系](src/book/07-sse-ghost-lineages.md)
10. [第八章：祖先序列、系统发育地理、时间树与化石](src/book/08-sequences-phylogeography-fossils.md)
11. [第九章：方法和软件全景图](src/book/09-software-atlas.md)
12. [第十章：从数据到结论的严谨工作流](src/book/10-workflow.md)
13. [第十一章：失败模式、争议与不可辨识性](src/book/11-failure-modes.md)
14. [第十二章：BGB Rust 的学术位置与演进路线](src/book/12-bgb-rust-roadmap.md)
15. [术语表](src/book/glossary.md)
16. [参考文献与公开资源](src/book/bibliography.md)

## 三条阅读路径

**只想搞懂 PhyBEARS。** 依次读第 1、3、数学插章、6、7、12 章。重点看 DEC 的局部消失、SSE 的整条谱系灭绝，以及 `D(t)`/`E(t)` 方程。

**准备做祖先性状分析。** 依次读第 1、3、数学插章、4、5、10、11 章。先确定要的是节点状态、完整历史、相关演化还是多样化关联，再选软件。

**准备开发 BGB Rust。** 依次读第 3、数学插章、6、7、9、12 章。第 12 章把现有固定树范围引擎与未来 SSE/FBD 内核分成独立里程碑。

## 证据规则

本书优先采用以下资料，按顺序降低权重：

1. 原始方法论文与证明；
2. 2024-2026 年的领域综述和方法复盘；
3. 软件官方文档、手册、源代码仓库与验证材料；
4. 权威开放教材和课程；
5. 二手网页仅用于发现线索，不作为关键结论的唯一证据。

软件版本和网页内容会变化；书中“当前”均指检索截止日。文献目录尽量给 DOI，软件尽量给官方主页或仓库。不能由公开资料核实的能力不会写成既成事实。

## 范围声明

“所有方法”在字面上不可能穷尽：祖先基因组、语言、肿瘤克隆、祖先重组图和生态位模型各自都有庞大文献。本书完整覆盖主干概率思想，并系统梳理表型性状、物种分布、性状依赖多样化、祖先序列、系统发育地理、时间树和化石整合；对旁支领域给出定位、代表方法和继续阅读入口。
