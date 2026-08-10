# 创 · 规范分层（Prompt Doctrine）· 2026-07-18

## 总原则

```text
仓库里可以厚；上下文里必须薄。
创是调度台：常驻卡片 + 按需技能 + 派工说明书 + 磁盘全文。
不抄 Codex 编码体验；写代码默认并行派工人。
Claude Code：抄 harness 纪律与分片组装，不 1:1 粘贴其 system prompt 全文。
```

## 资料来源（研究用，落地已改写）

| 源 | 用途 |
|----|------|
| [Piebald-AI/claude-code-system-prompts](https://github.com/Piebald-AI/claude-code-system-prompts) | Explore / Plan / Worker / Safety / Compact 分片结构与意图 |
| [shareAI-lab/learn-claude-code](https://github.com/shareAI-lab/learn-claude-code) | 运行时拼装、按需 skill、子代理干净上下文 |
| 本机 `~/.claude/CLAUDE.md` | Karpathy 行为纪律（最小改动、可验证目标） |
| 2026-03 npm sourcemap 泄露相关镜像 | 仅作架构背景；**不**把泄露原文当产品依赖 |

条文均为 **创自己的中文重写**，非 Anthropic 原文拷贝。

## 五片 → 创映射

| CC 公开分片意图 | 创落点 |
|-----------------|--------|
| Explore（只读快搜） | B `explore.md` + 派工 analyze |
| Plan（只读架构计划） | B `plan.md` |
| Worker fork（单指令、简报回报） | C `dispatch-worker-brief.txt` |
| Action safety + truthful reporting | A 常驻卡 + B `verify-before-claim` |
| Conversation summarization / compact | B `compact-handoff.md`（模板，宜落盘） |
| Act when ready / research before ask | A 常驻卡两句 |

## 四类清单

| 类 | 何时进主模型上下文 | 放哪 |
|----|-------------------|------|
| **A 常驻** | 每轮 | `doctrine-card.txt` + `skill-index.txt` |
| **B 按需** | 意图命中，最多 2 条 | `assets/norm/skills/*.md` |
| **C 仅派工** | 不进主会话 | `dispatch-worker-brief.txt` 包进 worker task |
| **D 仅磁盘** | 默认不进 | `docs/*`、完整架构 |

## 按需 skill 列表

| id | 触发（摘要） |
|----|----------------|
| explore | 在哪、定位、搜索代码、谁引用 |
| plan | 方案、怎么改、实施计划、架构 |
| coding-dispatch | 写代码、修 bug、重构、cargo test |
| surgical-diff | 最小改动、别重构无关 |
| think-before-act | 大改前想清楚、有歧义 |
| verify-before-claim | 验收、是否修好、完成了吗 |
| compact-handoff | 交接、总结会话、上下文满了 |
| readonly-triage | 排查、为什么挂、诊断 |

## 优先级（context_engine）

| segment | priority |
|---------|----------|
| system-core | 255 |
| capability primer | 254 |
| doctrine-card | 253 |
| skill-index | 252 |
| working user input | 220 |
| on-demand skill | 200 |

## 多子代理并行

- `spawn_subagent`：`task` 或 `tasks[]` + `max_concurrency`（默认 min(n,4)，上限 32，2026-08-11 从 8 放开）
- 每个 task 自动 wrap 工人简报（C）
- 主模型只收 admission 后摘要

## 老爸压缩包定理（2026-07-18）

| 定理 | 阶段 | 用法 |
|------|------|------|
| **奥卡姆剃刀** | 开发 | 如无必要勿增实体；防「按钮附赠登录防抖 i18n」 |
| **墨菲定律** | 验收 | 凡可能出错必测；禁快乐路径即全过 |
| **科斯定理** | 派工 | 交易成本 > 自干 → 不派；可并行/隔离才派 |
| **第一性原理** | 方案/修 bug | 从机制推最小充分解 |
| **对抗审查** | 复杂收尾 | 多 analyze 找茬；非默认每任务 |
| **禁止可选旁白** | 全程 | DO NOT send optional commentary |
| **Grill 澄清** | 仅歧义 | 一次一问；出口=目标+不做清单+验收，**不用 95%** |
| **矛盾分析** | 仅复杂取舍 | 主要矛盾/阶段/力量；**非语录、非常驻**（`contradiction-analysis.md`） |
| **闭环控制** | 仅跑偏/震荡/自进化 | 测量→偏差→再动作；积分防抖；**非原著、非常驻**（`closed-loop-control.md`） |
| **生成≠评审** | 仅复杂交付/质量环 | 生成器不自证过关；独立测量有预算；**非默认**（`gen-eval-separate.md`） |

常驻：闭环半句 +「模型只提议；挂了补 harness 别只会改 prompt」。  
研究：`docs/research-agent-cybernetics-notes.md`、`docs/research-git-scan-valuable-2026-07-18.md`。

## 禁止

- 把完整 docs/ 或 CC 万字 prompt 每轮灌进 system
- 把《工程控制论》/小乖式厚 AGENTS 整段灌进 always-on
- 在创内核死磕 Codex/Claude Code 编码手感
- 子代理直写 core memory / 递归乱派
- 用「95% 信心」当停问条件
- 简单任务默认对抗审查或默认 grill 或默认控制论 skill
