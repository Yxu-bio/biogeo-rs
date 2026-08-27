# 维护工具

本目录保存不会进入 `biogeo-cli` 构建或运行过程的维护工具。

`make-ponerinae-subset.py` 从完整 Ponerinae 输入确定性生成公开的 32-tip、7-area fixture。
它显式使用新版 RASP 提供的内置 vendor 版 ETE3，并在输出中记录输入 SHA-256、选择规则和类群
列表。正常测试直接读取已经生成并提交的 fixture，不需要 Python 或 ETE3。
