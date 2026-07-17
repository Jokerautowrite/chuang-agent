# OpenCode 终端呈现研究 · 供 chuang 借鉴

**日期**：2026-07-18  
**角色**：research 只读摸底（B-opencode）  
**本机版本线索**：日志会话 `version=1.17.4`；本地插件 `@opencode-ai/plugin@1.15.10` / SDK 同族；TUI 基于 **OpenTUI**（`@opentui/core` + `@opentui/solid`）。

---

## 1. 本机证据路径

| 路径 | 内容 |
|------|------|
| `~/.opencode/bin/opencode` | 本机二进制 |
| `~/.opencode/node_modules/@opencode-ai/plugin/dist/tui.d.ts` | TUI 插件 API：路由、sidebar slots、toast、theme、attention |
| `~/.opencode/node_modules/@opencode-ai/plugin/dist/tool.d.ts` | Tool 结果 `title` / `metadata` 契约 |
| `~/.opencode/node_modules/@opencode-ai/sdk/dist/v2/gen/types.gen.d.ts` | 会话消息 Part 模型、ToolState、SessionStatus、事件总线 |
| `~/.config/opencode/` | 配置与记忆；`node_modules/@opencode-ai/*` 同套 SDK |
| `~/.local/share/opencode/log/` | 运行日志：`loop step=N`、`session.status`、permission 评估 |
| `~/hermes-agent-upstream/skills/autonomous-ai-agents/opencode/SKILL.md` | 使用面：TUI keybinds、`run --format json`、`--thinking` |
| `~/agent-hub/adapters/opencode/` | 空壳，无 UI 源码 |
| 官方文档 | https://opencode.ai/docs/tui 、 `/docs/cli` |

**说明**：本机没有完整 OpenCode 应用源码树（TUI 实现在编译二进制 / 历史上 Go TUI + 现 OpenTUI Solid）。呈现结构以 **SDK Part 模型 + plugin TUI 类型 + 官方 TUI 文档 + issue 中的 title 渲染语义** 重建。

---

## 2. TUI / 会话显示相关结构

### 2.1 双通道：全屏 TUI vs 非交互 `run`

1. **交互 TUI**（默认 `opencode`）  
   - 全屏会话视图 + 底部 prompt + 可切换 sidebar  
   - 路由：`home` | `session(sessionID)`（`TuiRouteCurrent`）  
   - 渲染栈：OpenTUI `CliRenderer` + Solid JSX slots  
   - 与后端解耦：`opencode serve` / `web` + `attach`，同一会话事件流可被 TUI 消费

2. **非交互 `opencode run`**  
   - `--format default`：人类可读 formatted 流  
   - `--format json`：原始 JSON 事件（脚本/编排）  
   - `--thinking`：可选展示 reasoning 块  
   - 不依赖 pty（Hermes skill 明确区分）

chuang 当前更接近 **第 2 路（stdout 事件流）**，而不是第 1 路全屏 TUI。

### 2.2 消息 = Part 流（核心数据模型）

助手一轮输出被拆成可独立更新的 **Part**（`Part` union），经 `message.part.updated` / `message.part.delta` 推送：

| Part 类型 | 用户可见含义 | 关键字段 |
|-----------|--------------|----------|
| `text` | 正式答复正文 | 流式 delta |
| `reasoning` | 思考/推理块 | `text`；TUI 有 `thinkingOpacity`；`/thinking` 仅控制**显示** |
| `tool` | 工具调用卡片 | `tool` 名 + `state` 状态机 |
| `step-start` / `step-finish` | 模型 loop 一步边界 | finish 带 `reason`、`cost`、`tokens` |
| `file` | 附件/引用 | mime、filename、url、source |
| `patch` / `snapshot` | 文件变更摘要 | files、hash |
| `agent` | 当前 agent 片段 | name（build/plan 等） |
| `retry` | 重试 | attempt + error |
| `compaction` | 上下文压缩 | auto/overflow |
| `subtask` | 子任务 | prompt/description/agent |

**设计要点**：UI 不「打印一整坨 log」，而是 **按 part 类型分块渲染**；每个 part 有自己的生命周期。

### 2.3 Tool 状态机（用户最常盯的一块）

```
pending → running → completed
                  ↘ error
```

| 状态 | 关键展示字段 | 用户读到什么 |
|------|--------------|--------------|
| `pending` | `input` / `raw` | 已排队，尚未执行 |
| `running` | 可选 **`title`**、`metadata`、`time.start` | 「正在做 X」；title 可中途 `context.metadata({ title })` 更新 |
| `completed` | 必填 **`title`**、`output`、`metadata`、`time.end`；可 `compacted` | 一行人话标题 + 可展开结果 |
| `error` | `error`、时间戳 | 失败原因，不吞掉 |

Plugin 契约强化这一点：

- 执行中：`context.metadata({ title?, metadata? })` 刷新标题  
- 结果：`ToolResult = string | { title?, output, metadata?, attachments? }`

历史 issue（#1736）确认 TUI 的 **tool title 优先用人类 description**（如 bash 的 `description`：「Get current git status」），而不是原始 `command`——**默认可读，细节进 expand/details**。

### 2.4 Session 级状态（status line 语义）

`SessionStatus`：

- `idle` — 可输入  
- `busy` — 回合进行中  
- `retry` — 重试中：`attempt`、`message`、可选 action（provider/title/link）、`next` 时间

事件：`session.status`、`session.idle`、`session.error`、`session.diff`、`session.compacted`。

本机日志对应：`loop session.id=… step=N` → 用户侧应感到「第几步还在动」，而不是假死。

### 2.5 Sidebar / 布局 slots（全屏 TUI 专属）

`TuiHostSlotMap` 暴露可插拔槽位：

- `sidebar_title`：会话标题 + 可选 share_url  
- `sidebar_content` / `sidebar_footer`  
- `session_prompt` / `session_prompt_right`  
- `home_*` logo/prompt/footer  

Sidebar 数据源（`TuiState.session`）：

- **diff**：文件 + additions/deletions  
- **todo**：content + status（pending/in_progress/completed/cancelled）  
- **permission** / **question** 队列  
- **mcp** / **lsp** 连接状态  

### 2.6 交互提示层（非主对话流）

| 机制 | 作用 |
|------|------|
| **Toast** | `info/success/warning/error` + title/message/duration；SDK 事件 `tui.toast.show` |
| **Dialog** | Alert / Confirm / Prompt / Select（权限与选择） |
| **Question** | `header`（≤30 字）+ `question` + options（label 1–5 词 + description） |
| **Permission** | permission 名 + patterns + once/always；回复 once/always/reject |
| **Attention** | 音效/桌面通知：`question` / `permission` / `error` / `done` / `subagent_done`；可按 focused/blurred 触发 |
| **`/details`** | 切换工具执行细节显隐 |
| **`/thinking`** | 切换 reasoning 显示（不开关模型能力） |
| **Plan vs Build** | Tab 切换；右下角 mode 指示（文档明确） |
| **`!cmd`** | 用户 shell 作为 tool result 入会话 |
| **`@file`** | 模糊引用文件进上下文 |

### 2.7 主题与「安静的次要信息」

`TuiThemeCurrent` 区分：

- 主色 / muted text / panel 背景  
- **diff** 专用色（added/removed/context/hunk）  
- **markdown** 语法色  
- **`thinkingOpacity`**：思考块默认降对比，避免抢正文  

### 2.8 与 chuang 现状对照（便于迁）

| OpenCode | chuang 已有 | 差距 |
|----------|-------------|------|
| Part 流 + ToolState | `TerminalEvent` + `DisplayEvent` | 事件种类够，默认投影偏「藏成功工具」 |
| tool `title` 人话 | `activity_title` / `activity_detail` | 方向一致；需默认始终可见一行 |
| step-start/finish + tokens/cost | StepStarted/Finished | 缺 step 边界上的轻量成本/轮次提示 |
| SessionStatus busy/idle/retry | TurnStarted / 隐式 | 缺显式「忙/闲/重试」状态行 |
| `/details` 细节闸门 | `DisplayProjectionOptions` | 已有 suppressible；缺用户侧开关体验 |
| toast / attention | 无 | 可选；stdout 可用单行 WARN/DONE 代替 |
| 全屏 sidebar todo/diff | 无 | 重度 TUI，不宜硬搬 |

---

## 3. 呈现原则（10 条，带具体例子）

### P1 · 默认人话标题，原始细节可展开

- **例**：bash 显示「检查 Git 状态」或 description「Get current git status」，`git status -sb` 进 `/details` 或展开块。  
- **反例**：默认刷完整 argv 与 200 行 stdout。

### P2 · 工具必须有四态，且 running 可刷新标题

- **例**：`write` 从「准备写入…」→「正在写 src/foo.rs」→「已写入 src/foo.rs」；长操作不卡死在第一句。  
- OpenCode：`ToolState` + `metadata({ title })`。

### P3 · 一条工具一行主标题；结果是附属块

- **例**：  
  `✓ read  packages/api/auth.ts`  
  （折叠）`… 42 lines`  
- 用户扫屏只读标题流即可知道「做过什么」。

### P4 · 成功默认克制，失败/阻断默认醒目

- OpenCode：details 可关；error/permission 仍弹 dialog + attention。  
- chuang 已有 `suppressible` + 失败 `Alert`——**应坚持：成功 secondary，失败 primary**。

### P5 · 思考块与正文分层（视觉降权）

- **例**：reasoning 用低 opacity / 缩进 / 前缀 `thinking`；`/thinking` 默认关或默认淡。  
- 正文 `text` 永远是最高优先。

### P6 · 显式「系统在忙还是在等你」

- **例** status：`busy` 转圈 / `idle` 可输入 / `retry 2/5 · provider timeout · 3s`。  
- 日志已有 `step=N`；UI 应暴露等价信息，避免「没输出=死了」。

### P7 · 权限与提问打断主线，但不污染历史正文

- **例**：Question：`header: 数据库` + 完整 question + 短 label 选项。  
- Permission：突出 patterns，答复 once/always/reject。  
- 结束后 toast「已允许 read *.env」即可，不必把对话框再抄进 assistant 长文。

### P8 · 会话元信息靠边：标题、模型、agent、diff、todo

- **例** sidebar：`build · gpt-5.5 · +12/-3 · 3 files`；todo 勾选进度。  
- 主栏留给对话与工具时间线。

### P9 · 双模式输出：人读 formatted + 机读 json 事件

- **例**：`opencode run --format json` 给编排；默认 formatted 给人。  
- chuang 的 `TerminalEvent` JSON 已具备机读面；缺的是 **稳定、好看的人读投影默认开**。

### P10 · 细节闸门与模式指示要可见

- **例**：`/details` 开关工具展开；Tab 显示 Plan/Build 在角标。  
- 用户永远知道「我现在看的是简版还是详版」「能不能改文件」。

### P11 · 结束有收口信号

- **例**：attention `done`；或 `step-finish` 带 cost/tokens；session → idle。  
- 用户明确「这轮完了」，而不是输出戛然而止。

### P12 · 终端标题与分享是加分项，不是主路径

- OpenCode：`OPENCODE_DISABLE_TERMINAL_TITLE`、`/share`。  
- 借鉴优先级低；先做时间线清晰。

---

## 4. 适合迁到 chuang / 不适合迁

### 4.1 适合（stdout 日志式 / 轻量 TUI）

| 借鉴项 | 迁法建议 | 落点 |
|--------|----------|------|
| Tool 四态 + 人话 title | 默认打印 `… 正在{title}` / `✓ {title}` / `✗ {title}: {error}` | `display_projector` + REPL 格式化 |
| activity_title 优先于工具原名 | 已有 `human_tool_activity_title`；默认 **始终展示** Tool 事件（可调） | `DisplayProjectionOptions` 默认值 |
| Step / round 边界 | `—— step 3 · 模型思考 ——` 轻分隔 | `StepStarted`/`ModelStarted` |
| busy/idle/retry 一行状态 | 回合开始/结束打印状态行；retry 打印 attempt | 新 `TerminalEvent` 或复用 Turn* |
| 成功克制、失败醒目 | 保持 suppressible；失败不 suppress | 已有逻辑对齐 P4 |
| thinking 可选 | 有 reasoning 时默认折叠一行「思考中…」 | 若 provider 暴露 reasoning |
| details 闸门 | env/CLI 旗标 `--verbose-tools` 对标 `/details` | cli 参数 |
| 双格式 | 已有 JSON 事件；补「人话默认 + jsonl 可选」 | cli_output / repl |
| 结束收口 | `AnswerReady` 默认可显示简短「✓ 答复完成」 | `show_final_ready_event=true` 默认 |

### 4.2 不适合（重度全屏 TUI）

| 项 | 原因 |
|----|------|
| OpenTUI/Solid 全屏应用壳 | chuang 是 Rust CLI/REPL；引入 OpenTUI 栈成本高、与飞书/服务路径无关 |
| Sidebar todo/diff/LSP/MCP 面板 | 需持续布局刷新与鼠标/滚动；stdout 用「回合结束 diff 摘要」足够 |
| Dialog 栈 + leader key chord（Ctrl+X …） | 全屏 keymap 体系；chuang 用确认 prompt/行内选择即可 |
| Attention 音效/桌面通知包 | 可选后续；非终端清晰度主矛盾 |
| Theme 全套 syntax/markdown 渲染器 | 有成本；最多 ANSI 色 4～5 种状态 |
| 实时 patch 高亮双栏 diff | `diff_style auto/stacked` 是 TUI 能力；chuang 可打印 unified diff 摘要 |
| 插件 Slot 热插拔 UI | 过度设计 |

### 4.3 迁移优先级（给设计阶段）

1. **P0**：工具时间线默认可见（title 人话 + 四态）  
2. **P0**：busy/完成收口；失败/需确认高亮  
3. **P1**：step/round 轻分隔；verbose 闸门  
4. **P2**：thinking 折叠；回合 diff/todo 一行摘要  
5. **不做**：全屏 TUI 重写  

---

## 5. 对设计文档的直接输入（草稿句）

> chuang 终端 UX 对齐 OpenCode 的 **会话 Part 时间线语义**，而不是其 **OpenTUI 全屏壳**：  
> 默认「人话工具标题流 + 状态行 + 最终答复」；细节与原始命令进 verbose；  
> 成功可压、失败与权限不可压；机读继续走 `TerminalEvent` JSON。

---

## 6. 参考与局限

- 官方 TUI：`/details`、`/thinking`、Plan/Build、attention、slash 命令体系  
- 官方 CLI：`run --format default|json`、`--thinking`  
- SDK Part / ToolState / SessionStatus / Question / Permission  
- Plugin TUI slots / toast / theme.thinkingOpacity  
- 本机 log：`loop … step=N`、permission allow 轨迹  
- **局限**：未反汇编二进制像素级 UI；title 具体 ANSI 样式以类型与文档/issue 为准；issue #1736 反映「title=description」产品选择（可读优先）。

---

## 7. 三句摘要

1. OpenCode 的清晰度来自 **Part 化时间线 + Tool 四态人话 title + Session busy/idle/retry**，不是堆 log。  
2. 全屏 OpenTUI（sidebar/dialog/theme/attention）是增强层；chuang 应只借 **事件语义与默认可见性策略**。  
3. 最值得立刻迁的是：**默认展示工具标题流、失败醒目、回合收口、verbose 闸门**——与现有 `TerminalEvent`/`DisplayProjector` 同构。
