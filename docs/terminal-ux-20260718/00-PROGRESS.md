# Terminal UX 改造进度 · 2026-07-18

## 目标
综合 Grok CLI + OpenCode 终端呈现优点，改造 chuang-agent 终端对话显示层。

## 阶段
| 阶段 | 状态 | 备注 |
|------|------|------|
| 第一刀 块分层/图标/答复优先 | done | 03-IMPLEMENTED.md |
| 第二刀 /trace 驱动 live 投影 | done | 见 04-TRACE-LIVE.md |
| 第三刀 实时 HUD 计时/模型/阶段 | done | 见 05-LIVE-HUD.md |
| 抛光底栏/banner/过程行 | done | 见 11-POLISH.md |
| 第四刀（可选）run 子命令字段墙 | pending | |

## 第二刀目标
- `/trace`：live 显示模型轮次进度 + 更高折叠上限 + 完成块技术行
- `/notrace`：恢复 repl_default（工具可见、模型轮次隐藏）
- 命令反馈文案说清楚 live 与完成块
- 测试覆盖 projector 档位 + format_progress 依赖 trace
