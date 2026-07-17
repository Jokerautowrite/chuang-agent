# 对话感修复 · 去掉流水账 · 2026-07-18

## 问题
默认刷「工作进展 1 理解 2 准备上下文 3 已完成」——像工单，不像聊天。

## 目标（对齐 Grok 感觉）
- **能快答** → 只见：你 → 小创答复（可加底部计时 HUD）
- **要干活** → 才出现工具过程行（读文件 / 执行…）
- **排障** → `/trace` 才露准备步骤、思考轮次、技术汇总

## 改动
- `DisplayProjectionOptions::show_lifecycle_steps`（默认关）
- `repl_default()`：关 lifecycle / 成功 step；开成功工具
- `repl_trace()`：全开 lifecycle + 思考「思考中…」
- 默认不再打印「工作进展」大标题；仅 `/trace` 时轻量「过程 · model」
- raw 事件仍驱动 HUD 阶段（思考中/执行中），不强制刷步骤行

## 验证
`cargo test --test display_projector_tests` + `--bin chuang-agent` 全绿  
新测：`conversational_default_hides_lifecycle_theater`
