# 创 · 产品主色

**主基调：雷蛇绿（Razer Green）** — 产品特色，已定稿。

| Token | RGB | 用途 |
|-------|-----|------|
| BRAND | 68, 214, 44 | 输入框边框、提示符 `>`、thinking 标题 |
| BRAND_SOFT | 150, 235, 130 | 系统提示强调 |
| BRAND_DIM | 48, 120, 48 | 工具过程 |
| BRAND_MUTED | 90, 130, 90 | chip / 脚注 |
| USER_BG / USER_FG | 淡绿底 / 浅绿字 | 用户消息 |

- 源码唯一入口：`src/brand_theme.rs`
- 终端实现：`src/cli_repl_tui.rs`
- 以后 Figma 出终稿只改 `brand_theme.rs` 常量，禁止各处散落 RGB。
