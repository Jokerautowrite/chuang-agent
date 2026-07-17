# Chuang 终端显示链路摸底 · 2026-07-18

范围：只读调研 `display_projector` / `terminal_event` / REPL 外壳（`main.rs`）/ runtime 进度写入（`cli_runtime.rs`）。  
`cli_output.rs` 基本不在交互对话显示主链上（见 §1.5）。

---

## 1. 事件流总览

```text
用户输入（TTY REPL）
  │
  ├─ render_user_message_block          main.rs:1339-1362
  │     「你」+ 蓝竖线正文 + provider/model/cwd
  │
  └─ spawn_repl_turn                    main.rs:773-825
        后台线程 run_with_options
        progress_path = /tmp/chuang-repl-progress-{pid}-{nonce}.jsonl
        live_guidance_path = /tmp/chuang-repl-guidance-...
              │
              ▼
        cli_runtime::run_governed_turn  (progress_path: Some)
              │
              │  write_terminal_event   cli_runtime.rs:1574-1595
              │  每行 JSONL:
              │  {
              │    "schema_version": 2,
              │    "ts_ms": ...,
              │    "event": <TerminalEvent 标签枚举>
              │  }
              │
              │  典型序列（成功一轮）:
              │    TurnStarted
              │    StepStarted("准备上下文") → StepFinished(Ok)
              │    [可选自动] ToolStarted/Finished(locate 桌面观察)
              │    loop max_tool_rounds:
              │      GuidanceInjected? → ModelStarted → ModelFinished
              │      ProtocolError? | AnswerReady | ToolStarted → ToolFinished
              │    轮次耗尽时:
              │      StepStarted("整理最终答复") → StepFinished → AnswerReady
              │    /stop:
              │      TurnCancelled
              │
              ▼
        主线程 poll（200ms）
              │
              ├─ poll_progress_events   main.rs:471-518
              │     读 progress.jsonl 增量
              │     format_progress_event → DisplayEvent
              │     print_progress_display_line
              │
              └─ poll_running_turn / finish_running_turn
                    print_repl_result / print_repl_failure
                    最终「小创」答复块 + muted 元数据
                    show_trace → visible_trace_lines + audit
```

### 1.1 Runtime 事件源：`TerminalEvent`

定义：`src/terminal_event.rs:4-63`

| 变体 | 关键字段 | 谁写（cli_runtime） |
|------|----------|---------------------|
| `TurnStarted` | `input_preview`, `max_tool_rounds` | 回合入口 ~423 |
| `StepStarted` / `StepFinished` | `title`, `status`, `detail` | 准备上下文 ~430；最终答复 ~974 |
| `ModelStarted` / `ModelFinished` | `round`, `finish`, `chars` | 每轮模型 ~533 / ~549 |
| `ToolStarted` / `ToolFinished` | `tool`, `ok`, `decision`, **`activity_title`/`activity_detail`** | 工具前后 ~636 / ~679；自动 locate ~446 |
| `ProtocolError` | `round`, `code` | 协议纠偏 ~592 等 |
| `GuidanceInjected` | `round`, `chars` | 直播补充 ~525 |
| `TurnCancelled` | `stage` | `ensure_turn_not_cancelled` ~1137 |
| `AnswerReady` | `chars`, `truncated`, `snapshot_path` | 正常收口 ~609 / finalization ~1030 |

人话标签在 runtime 侧生成，不进 DisplayProjector：

- `human_tool_activity_title` / `human_tool_activity_detail`：`cli_runtime.rs:1348-1394`
- shell 用途归类（测试/构建/Git/搜索…）：`cli_runtime.rs:1396-1490`
- 完成明细：`human_tool_finished_detail`：`cli_runtime.rs:1506+`

### 1.2 投影：`DisplayProjector`

定义：`src/display_projector.rs`

| 类型 | 作用 |
|------|------|
| `DisplayEvent` | `kind` / `state` / `prominence` / `suppressible` / `message`（schema_version=1） |
| `DisplayEventKind` | `Progress` \| `Tool` \| `Warning` \| `Final` |
| `DisplayState` | `Running` \| `Succeeded` \| `Failed` \| `Blocked` |
| `DisplayProminence` | `Primary` \| `Secondary` \| `Alert` |
| `DisplayProjectionOptions` | 五档开关，控制「成功/模型/协议/Final」是否出屏 |

核心入口：`DisplayProjector::project`（`display_projector.rs:95-212`）

要点：

- **不改**底层 `TerminalEvent`；只投影为人话 `message`。
- 成功工具/成功步骤/模型进度/协议警告/Final ready **均可按 options 关闭**。
- 失败、阻断、取消 **始终可见**（`suppressible=false`，`Alert`）。
- 文案人话化：`humanize_step_title` / `tool_running_label` / `humanize_protocol_error`；`sanitize_label` 截断并剥控制字符与危险符号。

**库默认 options**（`display_projector.rs:73-81`）——偏「静默」：

```text
show_successful_tool_events: false
show_successful_step_events: false
show_model_progress: false
show_protocol_warnings: false
show_final_ready_event: false
```

**REPL 实际 options**（`main.rs:555-562`）——偏「满血活动流」：

```text
show_successful_tool_events: true
show_successful_step_events: true
show_model_progress: true
show_protocol_warnings: true
show_final_ready_event: false   // 最终靠答复块，不靠 "答复已准备完成"
```

这是**产品意图与库 Default 的第一处分歧点**（详见 §3）。

### 1.3 终端渲染：`main.rs` REPL

| 阶段 | 函数 | 行号 | 输出形态 |
|------|------|------|----------|
| 启动 | `print_repl_banner` / `render_repl_banner` | 1040 / 1308 | 大字 CHUANG + model/profile/cwd |
| 用户 | `render_user_message_block` | 1339 | 「你」+ 蓝 `│` 正文 |
| 进度轮询 | `poll_progress_events` | 471 | 首条前打「小创正在处理」；逐行 `·/✓/!` |
| 投影解析 | `format_progress_event` | 521 | 优先 `event`→`DisplayProjector`；否则 legacy `kind/details` |
| 进度行 | `render_progress_display_line` | 1365 | icon 按 kind/state 着色 |
| 完成 | `print_repl_result` → `render_assistant_completion_block` | 1131 / 1588 | 「小创」+ 可选 trace + **正文** + muted 耗时 |
| 失败 | `print_repl_failure` / `render_repl_failure_block` | 902 / 1541 | 人话错误 + 最近进展摘要 |
| 输入框 | `render_repl_prompt` | 1381 | 就绪/运行中/审批；状态栏 token |
| 审批 | `render_approval_prompt` | 1483 | `[1]/[2]/[3]` |

进度去重与限流（`main.rs:494-499`）：

- 连续相同 `message` 跳过。
- `visible_count >= REPL_ACTIVITY_VISIBLE_LIMIT`（**14**，`main.rs:61`）且 `suppressible` → 丢弃（成功工具/次要进度会被砍）。

### 1.4 控制面与旁路

| 路径 | 显示行为 |
|------|----------|
| 交互 TTY `repl` | 完整投影链（上表） |
| 非 TTY `printf \| cargo run -- repl` | **只** `result.response.body`；`--verbose` 才 `print_runtime_result`（`main.rs:189-194`）；**无** progress_path |
| `run` 子命令 | `cli_output::print_runtime_result`：model/body/trace/context 字段墙（`cli_output.rs:853-888`） |
| 飞书 / channel / app-server | 自有出口；不经 `DisplayProjector`（本调研不展开） |

### 1.5 `cli_output.rs` 与显示的关系

`cli_output.rs` **不是** REPL 活动流渲染器。与「对话显示」弱相关的只有：

- `print_runtime_result`：`run` 与 REPL `/verbose` 用；
- `print_status` / doctor 等：运维文本墙。

交互「你 / 小创正在处理 / 进展 / 小创」链路在 **`main.rs` + `display_projector.rs` + `cli_runtime` 写事件**。

---

## 2. 默认可见 vs `/trace` 可见

### 2.1 默认（`show_trace = false`，`main.rs:142`）

**可见**

1. 启动 banner（模型、profile、cwd、/help /stop /exit）。
2. 用户块：「你」+ 输入正文 + provider/model/cwd 微标。
3. 活动区标题：「小创正在处理」（首条进度时一次）。
4. 活动行（REPL projector 全开策略下），典型中文：
   - `正在理解你的要求`（TurnStarted）
   - `正在准备上下文` / `准备上下文已完成`
   - `正在判断下一步`（每轮 ModelStarted）
   - `正在{activity} · {detail}` / `{activity}已完成 · …`（工具）
   - 协议可恢复：`正在调整执行格式并继续` / `正在补全必要的实际检查`
   - 失败/阻断：`…失败…` / `…需要你的确认` / 取消「已安全结束」
   - 补充：`已接收新的补充要求`
   - 收口：`正在整理最终答复`（及成功/失败 step 文案）
5. 完成块：
   - 「小创」+ model_name
   - **最终答复正文**（超 2400 字截断 + `/tmp` 快照路径，`main.rs:1187-1204`）
   - muted 一行：`耗时 {ms}` + 可选 tool_status 人话（等待确认 / 执行后整理 / 未完整收口）
6. 审批面板（若 meta 含 pending）。
7. 状态栏：context token、↑↓、session total。

**默认隐藏**

- `AnswerReady` 投影（`show_final_ready_event: false`）
- `ModelFinished`（projector 恒为 `None`）
- 原始 tool 名 / 命令 / 密钥 / summary 原文（projector 用 activity 与 sanitize）
- `/trace` 专属行（context/model/runtime 三行 + audit）
- `print_runtime_result` 全量 meta（需 `/verbose` 或 `--verbose`）
- 超过 14 条且 `suppressible` 的后续成功工具行

### 2.2 `/trace` 开启后（`main.rs:1106-1108`）

**不改变** live 活动流投影策略（仍用同一 `repl_display_projector()`）。

**额外出现在完成块**（`print_repl_result` / `visible_trace_lines` / `render_completion_audit_line`）：

```text
trace context={engine} tokens={n} recall_hits={n} dropped={n}
trace model={name} finish={reason}
trace runtime elapsed={ms} tools={n} protocol_errors={n} status={status}
技术细节 最近进展=…  tools=…  protocol=…  report={id}
```

失败时额外一行：原始 error 压缩预览（`render_repl_failure_block`，`show_trace` 分支，`main.rs:1573-1579`）。

### 2.3 `/verbose` vs `/trace`

| 开关 | 作用 |
|------|------|
| `/trace` | 完成块加 compact 技术行；失败露 raw error 预览 |
| `/verbose` | 回合结束后再 dump 整份 `print_runtime_result`（model/body/trace/context…） |
| `/quiet` | 只关 verbose，**不**关 trace |
| `/notrace` | 只关 trace |

---

## 3. 现状「不清晰」点

按信息层级、进度、工具步骤、最终答复四维。

### 3.1 信息层级

1. **三套「默认」语义冲突**  
   - 库 `DisplayProjectionOptions::default`：成功工具/步骤/模型进度全关（早期「2–4 条进展」）。  
   - REPL `repl_display_projector`：几乎全开（满血工作台）。  
   - 文案/进度日志两代叙事并存（`docs/progress-log.md` 2026-07-11 两段）。  
   读代码的人会不知道「默认」指哪套。

2. **双路径解析**  
   `format_progress_event`（`main.rs:521-552`）优先 schema v2 `event`，否则 legacy `kind/details`。legacy 对成功 `tool_finished`、`protocol_error` 直接 `None`，与 typed 路径行为不一致，维护成本高。

3. **人话标签双源**  
   Runtime：`human_tool_activity_*`（细粒度 shell 意图）。  
   Projector：`tool_running_label` / `tool_subject`（粗工具名）。  
   正常路径依赖 runtime 的 activity_*；缺字段时落回 projector 词表，文案可能跳变。

4. **icon 语义扁平**  
   `render_progress_display_line`：仅 `· / ✓ / !`，无「阶段折叠」「当前焦点」，Primary/Secondary/Alert 只进数据不进布局层级。

### 3.2 进度

5. **过程过满，缺少「当前一步」焦点**  
   每轮 `正在判断下一步` + 每个工具 start/finish + 每步 start/finish，多轮任务快速滚过 14 行 cap，后半段成功工具被静默丢弃，用户误以为「没干活了」。

6. **「准备上下文」瞬时双行**  
   runtime 几乎连写 StepStarted/Finished（`cli_runtime.rs:430-443`），无真实耗时或内容摘要（detail 未进投影 message）。

7. **无进行中原地更新**  
   全是 append-only 行；长工具（cargo test）只有「正在…」没有 spinner/耗时/完成合并，完成后又多一行「…已完成」。

8. **`/trace` 不调节 live 进度粒度**  
   想「默认安静、trace 看工具」时，只能改代码里的 `DisplayProjectionOptions`，命令面无档位。

### 3.3 工具步骤

9. **成功完成行噪声大**  
   start：`正在X · detail`；finish：`X已完成 · X已完成` 类重复（finished_detail 常再写一遍 title）。

10. **失败信息过抽象**  
    Projector 失败句「…失败，正在保留现场信息」不带失败类/路径安全摘要；真实 summary 只在 meta/report，默认用户看不到「为什么挂」。

11. **`REPL_ACTIVITY_VISIBLE_LIMIT=14` 偏硬**  
    非 suppressible（失败/主进度）仍会打印，成功工具被裁后，列表时间序断裂。

### 3.4 最终答复

12. **答复与过程边界靠滚动**  
    过程在「小创正在处理」下，答复在另一「小创」头；中间无分隔规则或「以下为结论」锚点，长过程后答复易被淹没。

13. **Trace 插在 header 与正文之间**  
    `render_assistant_completion_block` 顺序：header → **trace 行** → audit → **answer** → metadata（`main.rs:1588-1607`）。开 trace 时技术行挡在答复前，违背「先结论后细节」。

14. **无流式正文**  
    仅回合结束后整包 `body`；用户在模型生成最终答复阶段可能长时间只看到旧进度行。

15. **非 TTY / `run` 体验断层**  
    管道与 `run` 没有「小创正在处理」，脚本友好但人读时像另一产品。

---

## 4. 最小改动切入点

按「动刀面从小到大」排序。

| 优先级 | 位置 | 改什么 | 风险 |
|--------|------|--------|------|
| **P0** | `main.rs` `repl_display_projector()` **555-562** | 收敛默认可见档：例如默认关成功 finish / 模型进度；`/trace` 时切换 options | 低；测 `repl_*` + display 契约 |
| **P0** | `main.rs` `format_progress_event` **521-552** | 去掉或收窄 legacy 分支；统一只走 `TerminalEvent`→projector | 低；保留一个兼容测试 |
| **P1** | `display_projector.rs` `project` / `project_tool_finished` **95-300** | 合并 start/finish 文案；成功 finish 默认 suppress；失败带安全 detail 槽 | 中；改 `tests/display_projector_tests.rs` |
| **P1** | `main.rs` `poll_progress_events` **471-518** | 同 tool 合并为 in-place 一行；limit 策略按 prominence | 中；TTY 行为变化 |
| **P1** | `main.rs` `render_assistant_completion_block` **1588-1607** | 固定：header → **answer** → metadata →（可选）trace/audit | 低；改 `repl_assistant_completion_*` 测 |
| **P2** | `main.rs` `/trace` 处理 **1106** + `poll` 持有 projector 状态 | 让 `show_trace` 驱动 `DisplayProjectionOptions`，而非只装饰完成块 | 中 |
| **P2** | `cli_runtime.rs` Step/Tool 事件 **430+ / 636+** | Step detail 进投影；减少瞬时空 step；finished_detail 去重 | 中；不动治理/工具语义 |
| **P3** | `terminal_event.rs` | 仅当需要新语义（如 `Phase`、流式 `AnswerDelta`）再扩枚举 | 高；牵动 schema v2 与回放 |

**刻意不先动**

- `cli_output::print_runtime_result`：运维/脚本面，别和对话 transcript 搅在一起。  
- Provider / tool 执行语义、治理、审批指纹。  
- 引入完整 TUI 框架（进度日志已划界：stdout transcript 优先）。

### 4.1 推荐「最小切片」顺序

1. 统一默认 options + 完成块顺序（只动 `main.rs` 显示，测现成 `repl_*`）。  
2. 调 `DisplayProjector` 默认与测试矩阵对齐「2–4 条主进展 + 失败必显」。  
3. 再做 poll 合并/限流与 `/trace` 联动 options。

---

## 5. 关键代码索引（路径 + 行号）

| 主题 | 路径:行 |
|------|---------|
| TerminalEvent 定义 | `src/terminal_event.rs:4-71` |
| DisplayEvent / Projector | `src/display_projector.rs:5-224` |
| 投影 options 默认 | `src/display_projector.rs:73-81` |
| 写 JSONL 事件 | `src/cli_runtime.rs:1574-1595` |
| 工具 activity 人话 | `src/cli_runtime.rs:1348-1520` |
| 回合事件发射主循环 | `src/cli_runtime.rs:423-710` 等 |
| REPL 入口 / show_trace 默认 | `src/main.rs:137-147` |
| 交互主循环 poll | `src/main.rs:243-264` |
| 进度投影与打印 | `src/main.rs:471-563`, `640-646`, `1365-1378` |
| 完成 / trace / audit | `src/main.rs:1131-1185`, `1272-1300`, `1588-1628` |
| 命令 /trace /verbose | `src/main.rs:1050-1118` |
| run 命令打印 | `src/main.rs:106-134` + `src/cli_output.rs:853-888` |
| 契约测试 | `tests/display_projector_tests.rs` |
| REPL 单元测 | `src/main.rs:1696+`（`repl_progress_*` / `repl_completion_*`） |

---

## 6. 五条可执行改动建议

1. **把 `repl_display_projector()` 的默认档改成「主进度 + 工具开始 + 失败/阻断」**  
   建议：`show_successful_tool_events=false`、`show_successful_step_events=false`、`show_model_progress=false`（或仅首轮）、`show_protocol_warnings=true`（可恢复当 Progress）、`show_final_ready_event=false`。与库 Default 对齐一半，并把「2–4 条进展」写回可测断言。

2. **让 `/trace` 切换同一 projector 的 options，而不仅是完成块装饰**  
   在 `repl_interactive_loop` 持有 `DisplayProjectionOptions`；`/trace` 打开成功 finish + 模型进度 + audit；`/notrace` 恢复安静档。改动集中在 `main.rs:203-311` 与 `handle_repl_command`。

3. **固定完成块信息顺序为：小创头 → 答复正文 → 耗时 metadata →（可选）技术细节**  
   改 `render_assistant_completion_block`（`main.rs:1588-1607`），同步更新 `repl_assistant_completion_has_answer_*` 测试。结论永远在技术行之上。

4. **在 `DisplayProjector::project_tool_finished` 去重 message，失败时允许短安全 detail**  
   成功 finish 默认 `None`；失败 message 用已 sanitize 的 `activity_detail` 或 failure 类标签，禁止回灌 raw summary/命令。扩展 `tests/display_projector_tests.rs` 的失败/脱敏用例。

5. **精简 `poll_progress_events`：同 message 去重保留；对 suppressible 做「最新 N 条」滑动窗，并对 Primary 失败永不丢**  
   替代硬截断 14 条导致时间线断裂；可选下一步再做「同一 tool start 被 finish 覆盖」的单行更新。仍保持 stdout transcript，不引入 TUI 依赖。

---

## 7. 结论（给设计阶段）

Chuang 终端显示已具备**完整可审计事件层**（`TerminalEvent` JSONL）与**独立人话投影层**（`DisplayEvent`），交互 REPL 在 `main.rs` 消费投影结果。  
主要问题不是「缺抽象」，而是：**默认可见策略前后摇摆、live 流过满且与 `/trace` 解耦、完成块层次（过程 / 结论 / 技术）未钉死**。  

下一设计文档（`01-DESIGN.md`）应优先规定：

- 默认可见事件表（逐 TerminalEvent kind）；
- `/trace` 与 `/verbose` 边界；
- 完成块固定骨架；
- 再映射到上表 P0/P1 函数。
