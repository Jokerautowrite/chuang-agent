# Grok CLI（Grok Build TUI）终端呈现优点 · 调研

> 供 chuang-agent 终端 UX 改造借鉴。**原则层**，不抄实现/代码。  
> 日期：2026-07-18 · 代理：C-grok research（只读）

---

## 0. 资料来源与版本线索

| 来源 | 路径 / 说明 |
|------|-------------|
| 官方用户指南 | `~/.grok/docs/user-guide/`（01–22，本机已装） |
| 核心章节 | getting-started、keyboard-shortcuts、theming、agent-mode、sessions、subagents、headless、plan-mode、background-tasks、terminal-support |
| 二进制线索 | `~/.grok/downloads/grok-0.2.102-linux-x86_64`（及 0.2.101）；入口 `~/.local/bin/grok` → wrapper，真身常在 `~/.grok/bin/` |
| 会话形态 | 位置参数 `grok "prompt"` → 全屏 TUI（需 TTY）；`-p` / `--prompt-file` → headless |
| ACP 流 | `session/update` 区分 thought / tool / message chunk（agent-mode 文档） |

本笔记综合：**用户指南的可见交互设计** + **常见 Grok Build TUI 会话体验**（工具进度、状态条、块分层）。不声称反编译 UI 实现。

---

## 1. 用户可见的信息层级

Grok 把「一次回合」拆成可扫读的分层块，而不是把过程、工具、结论糊成同一段纯文本。

### 1.1 纵向主轴（Scrollback 时间线）

从上到下、从旧到新，典型顺序：

| 层级 | 用户看到什么 | 默认呈现 | 可交互 |
|------|--------------|----------|--------|
| **Input（用户输入）** | 本回合用户消息 | 独立块；滚动时可作为 **sticky header** 钉住 | 按「回合」跳转（Shift+Left/Right） |
| **Thinking / status** | 推理过程、回合内活动 | 可折叠 thinking 块；运行中 accent **动画**；截断模式只露前几行 | 展开/折叠；`Ctrl+E` 全局 thinking；可 pin 手动折叠 |
| **Tool（工具调用）** | 读/改/搜/shell/web/子代理… | 一行标题 + 状态 bullet；折叠后 **muted**；括号里次要数字 dim | 折叠；复制内容/元数据（如命令）；全屏查看 |
| **Result（工具结果）** | 输出摘要、diff、命令 stdout | shell：**头 2 行 + 尾 3 行** 截断；edit：**inline diff**，默认可展开 | 展开全文；Enter 全屏 |
| **Final（最终答复）** | Agent 自然语言 + markdown | 完整渲染 + 语法高亮；与 tool 块色系/accent 分离 | 按 response 跳转；raw markdown 切换 |
| **Task / plan 附属** | TODO 列表、plan 预览、子代理生命周期 | 侧栏或内联 badge（如 `2/5`）；子代理一行 lifecycle | Ctrl+T / Ctrl+B；Enter 钻入子会话 |

### 1.2 横向/浮层（不进主答案流）

| 区域 | 作用 |
|------|------|
| **Prompt 区（底栏）** | 输入、@ 附件、模式（Normal / Plan / Always-approve）、队列 |
| **Contextual shortcuts bar** | 随「焦点面板 / 是否 running / 选中块类型」变提示，减少记快捷键 |
| **Permission / plan 审批条** | 危险操作或 plan 完成时的 action bar（a/s/c/q 等），与答案分离 |
| **Tasks / Todos 侧栏** | 并行子代理与后台命令的 spinner、耗时、kill/inspect |
| **Queue pane** | 回合进行中的 follow-up：默认排队，显式「send now」才插入当前 turn |
| **Fullscreen viewer** | 单块深读，不污染主时间线 |

### 1.3 事件语义分层（ACP / streaming，可映射到展示）

文档中的 `sessionUpdate` 类型直接对应「该画什么」：

- `agent_thought_chunk` → Thinking 层（可压、可折）
- `tool_call` / `tool_call_update` → Tool 层（标题 + status + 结果更新）
- `agent_message_chunk` → Final 流式文本
- `plan` → 计划预览层

Headless `streaming-json` 也区分 `thought` / `text` / `end`，说明 **协议层已强制过程与结论分道**。

### 1.4 一回合的用户心智模型（简图）

```text
┌ sticky: 你刚才说了什么 ──────────────────────────┐
│  Thinking…（可折，默认浅）                          │
│  ◆ Read src/foo.rs          ✓  120 lines（dim）    │
│  ◆ Run cargo test           ▸  头尾摘要            │
│  ◆ Edit bar.rs              展开 diff              │
│  最终答复：人话 + markdown + 代码块高亮              │
└──────────────────────────────────────────────────┘
[ 提示条: Esc… | Ctrl+C 取消 | Ctrl+Enter 插入… ]  [composer]
```

**关键点**：用户始终能回答三句——「我问了什么？」「它在干什么？」「结论是什么？」——而不必从工具 dump 里捞答案。

---

## 2. 可迁移原则（10 条）

> 面向 chuang 的 display_projector / terminal_event / REPL，不绑定 Grok 组件名。

1. **块即语义，不是日志行**  
   用户输入、思考/状态、工具、结果、最终答复各自是独立「块」。块有标题、状态色/符号、可折叠边界。禁止把五层拼成一段无结构 stdout。

2. **默认折叠过程，默认展开结论**  
   Thinking / 成功的只读工具 / 长命令输出：默认 compact 或折叠；Final 与「需要审的 diff」默认可读。细节通过 expand / `/trace` / verbose 打开，而不是默认全开。

3. **运行中给「活着」的反馈，完成后给「结果」**  
   进行中：短状态 + 动画/spinner + 当前活动后缀（如 “Running: cargo test”）。结束后：同一块变色或追加 completed 行，避免再刷一整屏重复元数据。

4. **工具行：目的优先，载荷次之**  
   首行回答「做了什么 / 对谁」；路径、行数、匹配数放 dim 括号。完整命令、完整 stdout、原始 JSON 放折叠或 fullscreen，**默认人话摘要**。

5. **结果截断要有头有尾**  
   长 shell 输出保留开头 + 结尾（Grok execute 块：`first_lines` + `last_lines`）。用户要的是「起势 + 结局」，中间噪声默认藏。

6. **最终答复独占「Primary 注意力」**  
   Final 用更高 prominence（完整 markdown、代码高亮）。Progress/Tool 成功事件默认 Secondary / suppressible。chuang 现有 `DisplayProminence` + `suppressible` 与此同构，应坚持默认关掉成功工具刷屏。

7. **Sticky 用户意图**  
   长回合滚动时，用户消息仍可钉在视口上，避免「翻到工具海里忘了自己要啥」。轻量 REPL 可用「回合分隔 + 用户句摘要行」近似。

8. **状态条只说「现在你能做什么」**  
   底栏 hint 随状态变化（idle / running / 选中某块 / 审批中）。不把诊断堆在状态条；诊断进块或 `/status`。

9. **并行工作一行 lifecycle，深读再钻入**  
   子代理/后台任务：父时间线只留「running/completed + 活动后缀 + 耗时」；完整 transcript 在侧栏或二级视图。主对话不被子会话淹没。

10. **过程可取消、可插话，但不和「发新回合」混淆**  
    取消与「往当前 turn 插一句」与「排队下一句」三套语义分离。chuang 的 `!补充` / 安全点注入可对齐：默认排队或下一点注入；显式才打断。

11. **颜色与符号表达状态机，不表达日志级别废话**  
    running / success / error / thinking / user / tool 用稳定 accent 槽位。16 色退化仍可读（GrokNight 策略）。chuang 若暂时无真彩，至少用符号：`…` / `✓` / `✗` / `◆`。

12. **协议分层先于皮肤**  
    展示层消费「已分型事件」（Progress / Tool / Warning / Final），不在渲染时从混杂字符串猜类型。Grok ACP 与 chuang `TerminalEvent` → `DisplayEvent` 同一哲学：先投影，再画。

---

## 3. 与 chuang「少诊断、多人话」的兼容建议

### 3.1 现状对齐（简）

chuang 已有：

- `TerminalEvent` → `DisplayProjector` → `DisplayEventKind::{Progress, Tool, Warning, Final}`
- 默认 `DisplayProjectionOptions`：**成功工具/步骤/模型进度/协议警告/final-ready 元事件大多关闭**
- REPL 文案已声明：默认可读判断与结果；不 dump 隐藏思维链与完整密钥/命令；`/trace` `/verbose` 开关细节

这与 Grok「过程可查、默认不吵」一致，改造应 **加强结构与节奏**，而不是默认变吵。

### 3.2 兼容策略（推荐）

| 原则 | 怎么落到 chuang | 避免 |
|------|-----------------|------|
| 人话主通道 | Final 永远是完整自然语言；Progress 用短中文目的句（「正在查登录相关代码」） | 默认打印 `ToolStarted { name: grep, args: ... }` 式诊断 |
| 过程清楚但不诊断 | Tool 行：`读 xx` / `跑测试` + 状态；失败才抬到 Alert + 一句原因 | 成功也刷 stack / raw JSON |
| 进度清楚 | Turn 级一个 Primary running；步骤合并或折叠；并行任务一行 lifecycle | 每个 model chunk 一行「thinking…」 |
| 答复清楚 | Answer 前后可有轻量分隔；不把 AnswerReady 元事件当第二段废话（保持 `show_final_ready_event: false`） | Final 后再跟一串 `turn_finished ok` |
| 细节可开 | 对齐 `/trace`（技术步骤）与 `/verbose`（元数据）；可增「展开上一工具」而不改默认 | 把 Grok 全量 thinking 流默认打开（与 chuang 产品立场冲突） |
| Thinking 策略 | **不展示模型原始 CoT**（chuang 已定）；用 **人话 Progress** 替代 Grok Thinking 块的信息角色 | 为「像 Grok」而泄露 hidden reasoning |
| Diff / 命令 | 写文件：给人看的 unified diff 摘要或路径列表；命令：可复制元数据在 verbose 下 | 默认全文命令 + 全 stdout |
| 状态条 | 若有底栏：只放「进行中 / 可 /stop / 可补充」 | `/status` 那种 provider readiness 不要每回合刷 |

### 3.3 最小迁移包（实现时可对照）

不要求一次上全屏 TUI，优先：

1. **五层 kind 固定映射**（已有四层 + 建议显式 User 分隔或 sticky 摘要行）  
2. **Tool 两行协议**：标题行（目的）+ 可选折叠结果行（头尾截断）  
3. **Running 指示**：单行「活着」状态，结束即消失或改 ✓  
4. **默认 suppress 成功噪声**（保持 options 默认 false，只修文案与截断）  
5. **失败升格**：Failed/Blocked → Warning/Alert + 一句人话下一步  

全屏 fold、主题槽、子代理 frame 可二期。

### 3.4 明确不要从 Grok 照搬的

- 默认展开完整 thinking 流  
- 默认 always-approve / yolo 交互文化（chuang 有自己的审批与生产铁律）  
- 重度依赖 truecolor 动画才能读懂状态  
- 把 ACP 扩展方法或 pager.toml 整锅端进内核  

---

## 4. Grok 呈现优点速查（给设计稿）

| 优点 | 一句话 |
|------|--------|
| 双区布局 | Scrollback 历史 + 底栏 Prompt，焦点可切换 |
| Sticky 用户 prompt | 长工具链中不丢意图 |
| 块可折叠 + 手折 pin | 扫读与深读切换 |
| 工具 muted / dim 细节 | 成功过程不抢结论 |
| Shell 头尾截断 | 长输出仍有形状 |
| Edit inline diff | 改动可见、可审 |
| 情境快捷键条 | 降低记忆负担 |
| 子代理 lifecycle 一行 | 并行不炸主时间线 |
| 队列 vs 插入 | 不误打断当前 turn |
| 协议分型更新 | 展示层不猜字符串 |
| compact / minimal | 小屏与破终端可退 |
| Plan 审批与答案分离 | 决策 UI ≠ 聊天气泡 |

---

## 5. 对后续设计文档的输入（给 01-DESIGN / impl）

建议设计规格直接回答：

1. chuang REPL 的 **块类型表** 与 Grok 五层对照  
2. 默认可见矩阵（哪类 event → 默认 show/hide/collapse）  
3. Tool 标题人话模板（中文）与截断参数  
4. Progress 是否允许替代 thinking 的信息角色  
5. 与现有 `/trace` `/verbose` `/quiet` 的映射，避免第三套开关  

---

## 6. 参考路径（本机）

- `$HOME/.grok/docs/user-guide/01-getting-started.md` — Scrollback 内容清单  
- `$HOME/.grok/docs/user-guide/03-keyboard-shortcuts.md` — 折叠、取消、队列、hint bar  
- `$HOME/.grok/docs/user-guide/06-theming.md` — 块样式、截断、accent 槽  
- `$HOME/.grok/docs/user-guide/15-agent-mode.md` — sessionUpdate 分型  
- `$HOME/.grok/docs/user-guide/16-subagents.md` — 父时间线 lifecycle  
- `$CHUANG_AGENT_ROOT/src/display_projector.rs` — 已有投影与默认安静策略  

---

*本文件完成阶段 1 中 C-grok 摸底；不修改线上、不改 chuang 代码。*
