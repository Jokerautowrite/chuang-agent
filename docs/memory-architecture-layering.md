# Memory Architecture Layering

更新时间：2026-05-04

## 结论

Chuang 的长期记忆方案不能简化成三层。老爸已明确：小创当前记忆是融合 Karpathy 式极简规则、Hermes 分层记忆、GBrain 外脑知识库后形成的组合系统。

正确理解是：

```text
内部记忆 + 历史会话 + LIM 长期沉淀 + 外脑知识库 + 自动维护闭环
```

核心口令：

```text
先分层，再记录；先检索，再下结论；骨架要小，外脑要大，LIM 负责沉淀。
```

参考源：

```text
/home/user/.hermes/memories/memory-management-guide.md
```

## 五层分工

### 1. 内部记忆

随身携带的核心记忆，决定“我是谁、老爸是谁、该怎么做事”。

目标态包括：

```text
MEMORY.md        骨架事实 / 高频规则 / 环境约束
USER.md          用户画像 / 协作偏好 / 联系人
experiences.md   踩坑经验 / 操作经验 / 修正规则
STORY.md         灵魂故事 / 关系连续性 / 为什么存在
```

Chuang 当前已实现：

```text
identity/SOUL.md
identity/STORY.md
identity/FIRST_WAKE.md
identity/agents.toml
data/hermes-memory/USER.md
data/hermes-memory/MEMORY.md
data/hermes-memory/experiences.md 入口 / config / status / doctor 可诊断
```

待补：

```text
experiences.md 从 session/LIM 自动抽取经验
内部记忆健康检查的内容质量规则
内部记忆流转规则
```

当前 `experiences.md` 的 MVP 边界：

```text
已做：默认文件 contract、只读打开、status/config show 路径、doctor 存在性检查、memory identity show 展示
已做：append_experience admission / provenance 入口，run --remember-experience 显式沉淀 runtime_turn
未做：默认自动写经验、注入 runtime prompt、从 LIM/session 自动抽取经验
```

### 2. 历史会话层

用于找回过去具体说过什么、做过什么、怎么修好的。

目标态：

```text
session_search
```

Chuang 当前已实现：

```text
session_id / thread recall
app-server 使用 thread id 写会话记忆
session recall isolation diagnostics
memory session search --query TEXT [--session-id ID]
```

待补：

```text
把 session_search 挂成主进程可调用工具
会话摘要与原文归档的边界
```

### 3. LIM 长期沉淀层

LIM 负责把对话中的长期有效事实，从 session 中提取出来，沉淀成可复用记忆对象。它不是当前会话必须实时注入的骨架。

Hermes 当前参考：

```text
Honcho / peer cards / honcho-export
memory_extractor.py
自动记忆提取 cron
```

Chuang 迁移策略：

```text
先做轻量 extractor + provenance
不要第一步硬搬完整 Honcho/GBrain/PGLite 内核
```

Chuang 当前已实现：

```text
memory lim extract --query TEXT [--session-id ID]：只读 dry-run 候选，输出 provenance，不自动写回
```

### 4. 外脑知识库层

用于承载大块资料、SOP、研究文档、项目知识。外脑不是提示词记忆，必须通过检索和导入进入当前任务。

Hermes 当前参考：

```text
Obsidian wiki: /home/user/.hermes/wiki
GBrain: 文件索引 + embedding + 语义召回
```

Chuang 迁移策略：

```text
wiki 作为主文件层
GBrain 作为索引与召回层
日常入库默认 no-embed
embedding 后续批量补
```

### 5. 自动维护闭环

目标不是只“能恢复”，而是能持续维护记忆质量。

Hermes 当前参考：

```text
memory_extractor.py
memory_health.py
capability-evolver/evolver.py
memory_self_maintain.py
maintenance reports
cron: memory-self-maintain
```

Chuang 迁移策略：

```text
先做 dry-run 维护报告
再做人工确认写回
最后才考虑自动写回
```

## 迁移顺序

1. 身份与内部记忆：`SOUL / STORY / FIRST_WAKE / agents.toml / USER / MEMORY / experiences`
2. 历史会话召回：`session_id / thread recall / session_search`
3. LIM 长期沉淀：轻量 extractor、来源、置信度、去重
4. 外脑知识库：wiki 主文件层、GBrain 索引召回层
5. 自动维护闭环：health、decay、evolver、extractor dry-run、maintenance report

## 禁止误解

- 不能再把 Chuang 记忆说成只有身份、热记忆、会话三层。
- 不能把 wiki/GBrain 当成可选附属；它是外脑知识库层。
- 不能把 LIM 和 MEMORY.md 混为一谈；LIM 负责沉淀，MEMORY.md 只保留骨架。
- 不能为了“长期记忆强大”把所有内容塞进 prompt。
- 不能第一步硬搬完整旧 GBrain/PGLite 内核。
