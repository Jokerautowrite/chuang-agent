# chuang-agent

本地协作项目，用于沉淀小创 × 小承（DeepSeek）关于“创项目”的规格、方案、实验记录与实现计划。

## 当前目标
- 打通小创与 DeepSeek（小承）的稳定协作链路
- 产出 3 份最小规格草案：SubagentReport / MemoryAdmissionPolicy / ContextEngineLifecycle
- 后续由小创审稿、收敛并推进实现

## 当前状态
- 创建时间：2026-04-30 13:41:58 CST
- DeepSeek 当前通过可见 Chrome + X11 桌面输入协作
- `opencli deepseek status/read` 可用
- `opencli deepseek ask` 当前不可用，原因是发送前强制切模型失败
- 当前已将“致下一个窗口的我”长提示词发送到 DeepSeek 当前窗口
- 本地实现主进度以 `docs/progress-log.md` 为准，new 后先读它续上

## 目录约定
- `docs/`：规格草案、架构说明、评审结论
- `context/`：协作上下文、提示词、窗口接续材料
- `.hermes/plans/`：计划文档
