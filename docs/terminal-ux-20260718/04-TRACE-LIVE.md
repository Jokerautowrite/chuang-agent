# 第二刀 · /trace 驱动 live 投影 · 2026-07-18

## 行为

| 模式 | 命令 | 进行中（工作进展） | 回合结束 | 状态栏 |
|------|------|-------------------|----------|--------|
| 默认 | `/notrace` 或启动 | 工具/步骤人话；**隐藏**每轮「判断下一步」；成功 cap=14 | 无技术汇总 | 无「详细」 |
| 详细 | `/trace` | 另显示模型轮次、协议提示、答复就绪；成功 cap=40 | 有 `── 技术细节 ──` | 状态带 `· 详细` |

## 实现点
- `DisplayProjectionOptions::repl_trace()` — `src/display_projector.rs`
- `repl_display_projector(show_trace)` / `format_progress_event(line, show_trace)` / `poll_progress_events(..., show_trace)` — `src/main.rs`
- `/help`、`/trace`、`/notrace` 文案已同步

## 注意
- 中途 `/trace` 只影响**之后**新刷出的进展行；已打印的行不回放。
- `/quiet` 只关 `/verbose`，不关 `/trace`。

## 测试
- display_projector：`repl_default_*` + `repl_trace_*`
- main：`repl_progress_event_formats_*` 覆盖 model 默认隐藏 / trace 显示
