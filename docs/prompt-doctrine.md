# 创 · 规范分层（Prompt Doctrine）· 2026-07-18

## 总原则

```text
仓库里可以厚；上下文里必须薄。
创是调度台：常驻卡片 + 按需技能 + 派工说明书 + 磁盘全文。
不抄 Codex 编码体验；写代码默认派工人（快的并行子代理优先）。
```

## 四类清单

| 类 | 何时进主模型上下文 | 放哪 | 示例 |
|----|-------------------|------|------|
| **A 常驻** | 每轮 | `assets/norm/doctrine-card.txt` + skill 索引 | 身份一句、调度台、红线、如何派活 |
| **B 按需 skill** | 用户意图命中时 | `assets/norm/skills/*.md` | 验证再声称、只读排查… |
| **C 仅派工** | 不进主会话；塞进子代理 task | `assets/norm/dispatch-worker-brief.txt` | 工人边界、交付格式 |
| **D 仅磁盘** | 默认不进；用 file_read | `docs/*`、完整架构 | blueprint、progress-log |

## 优先级（context_engine）

| segment id | priority | 裁剪时 |
|------------|----------|--------|
| system-core | 255 | 最后砍 |
| capability primer | 254 | 很晚砍 |
| doctrine-card | 253 | 很晚砍 |
| skill-index | 252 | 较晚砍 |
| on-demand skill | 200 | 可先于用户任务外资料砍 |
| working user input | 220 | 高 |

## 多子代理并行

- 工具 `spawn_subagent` 支持单任务，或 `tasks: ["…","…"]` 一次派多个。
- `max_concurrency` 默认 `min(任务数, 4)`，上限 8。
- 队列 `run-loop` 按并发跑工人；主模型只收各报告摘要（admission 后）。
- **不追求** 自研最强编码壳；追求 **快派、快收、可并行**。

## 禁止

- 把完整 docs/ 每轮灌进 system。
- 在创内核死磕 Codex/Claude Code 编码手感。
- 子代理直写 core memory。
