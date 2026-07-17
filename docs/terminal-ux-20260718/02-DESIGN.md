# 终端呈现设计 · 综合 Grok + OpenCode → chuang

**日期**：2026-07-18  
**状态**：实现中

## 用户要回答的三句话
1. 我问了什么？  
2. 它现在在干什么？  
3. 结论是什么？

## 纵向主轴（stdout 时间线，不做全屏 TUI）

```text
┌ 你 ─────────────────────────────┐
│  用户输入（蓝竖线）               │
│  provider · model · cwd（dim）   │
├ 工作进展 ───────────────────────┤
│  ● 正在理解…                     │
│  ▸ 正在读文件 · path（运行中）     │
│  ✓ 读文件已完成（成功，可折叠）   │
│  ✗ 写文件失败（始终醒目）         │
├ 小创 ───────────────────────────┤
│  <最终答复正文，主注意力>          │
│  耗时…（dim）                     │
│  [/trace 才有：技术细节…]         │
└─────────────────────────────────┘
```

## 从参照搬什么

| 来源 | 采用 | 不采用 |
|------|------|--------|
| Grok | 块分层；过程可压；Final 独占注意力；工具目的优先 | 全屏 TUI、sticky header、侧栏 |
| OpenCode | Tool 状态机图标；失败醒目；verbose 闸门；人话 title | OpenTUI 全屏壳 |
| chuang 已有 | TerminalEvent→DisplayProjector；activity_title；中文 | 库 Default 与 REPL 冲突叙事 |

## 默认档（REPL）

| 项 | 默认 | 说明 |
|----|------|------|
| 成功工具 start/finish | 开 | 过程可见（OpenCode 风格） |
| 成功步骤 | 开 | 但 secondary |
| 模型「判断下一步」 | **关** | 减刷屏（Grok 折叠 thinking） |
| 协议可恢复提示 | 开 | 人话 |
| AnswerReady 投影 | 关 | 用最终答复块代替 |
| 成功行 cap | 14 | 超出折叠 + 一行提示 |
| /trace | 只影响完成块技术行 | live 投影不变（本轮） |

## 渲染规则

1. **图标**：● 主进度 / ▸ 工具运行 / ✓ 成功 / ✗ 失败 / ! 阻断  
2. **完成块顺序**：标题 → **答复** → 元数据 →（可选）trace/audit  
3. **过程不过度抢答**：成功 secondary 用 dim  
4. **折叠提示**：触达 cap 时一行 dim 说明  

## 改动文件

- `src/main.rs`：REPL 渲染主改  
- `src/display_projector.rs`：可选 `repl_default()` preset  
- `tests/display_projector_tests.rs` + main 内 unit tests  

## 非目标

- 不改 provider/runtime 语义  
- 不改飞书通道  
- 不做原地 spinner 刷新（append-only 保留；后续可加）
