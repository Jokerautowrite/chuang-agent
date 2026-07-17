# 第三刀 · 实时状态 HUD（模型 / 计时 / 阶段）· 2026-07-18

## 为谁
老爸要「该有的信息都看得见」：模型、倒计时、思考时间——对齐 Grok 体验，不堆诊断。

## 运行中底部实时行（约 200ms 刷新）

```text
⏱ 12.4s · 思考中 3.1s · gpt-5.5 · 剩余 17.6s · 当前 正在检查 Git · ctx 10%
```

| 字段 | 含义 |
|------|------|
| ⏱ 总时长 | 本回合从开始到现在 |
| 阶段 + 阶段时长 | 理解中 / 思考中 / 执行中 / 整理答复 |
| 模型名 | 当前配置/返回的 model |
| 剩余 | 若配置了 `provider.request_timeout_ms` 的倒计时 |
| 当前 | 最新人话进展 |
| ctx % | 上下文占用（有数据时） |

实现：原地改写最后一行（ANSI 上移清行），新进展先顶掉 HUD 再打印步骤，再重画 HUD。

## 工作进展区头
`工作进展 · {model} · 进行中|详细`

## 回合结束摘要
```text
耗时 12.4s  模型 gpt-5.5  思考 5.0s  执行 3.2s
```
思考/执行来自阶段累计（Model 轮 ≈ 思考，Tool 运行 ≈ 执行）。

## 文件
- `src/main.rs`：`LivePhase` / `ProgressCursor` / `render_live_hud_line` / `refresh_live_hud`

## 测试
`cargo test --bin chuang-agent` 全绿（含 `live_hud_line_*`、`format_short_duration_*`）
