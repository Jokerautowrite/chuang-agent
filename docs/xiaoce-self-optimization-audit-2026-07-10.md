# 小策全面自审与优化报告

日期：2026-07-10

## 结论

本轮已完成小策运行态、长期习惯、飞书桥、Codex CLI、模型链路、插件、技能、日志和便携备份的全面审计，并落实高收益、可回滚的优化。

当前核心配置为 `gpt-5.6-terra`、`high`、`custom Responses`。真实模型请求与 app-server `model/list` 均已通过。

## 已完成

| 项目 | 处理 | 验证 |
| --- | --- | --- |
| Codex CLI | 从 `0.141.0` 升级到 `0.144.1`；旧版完整归档 | `codex --version`、真实 `pong`、app-server 初始化与模型列表通过 |
| 飞书默认强度 | `medium` 改为 `high` | 配置回读通过 |
| 飞书日志 | 新增 `quiet/normal/verbose`；默认 `normal` 屏蔽 delta、token usage、rate limit、item 生命周期和 routine 状态事件 | 新增与扩展测试通过 |
| 日志兜底 | 为飞书桥设置 `30s/200` 的 systemd 服务级日志限流 | unit 校验与配置回读通过 |
| CLI 预检 | CLI 健康时静默退出，只在修复或错误时写日志 | 连续运行前后日志大小与时间戳不变 |
| CardKit | 失败后转旧卡片，打开 5 分钟熔断，冷却后自动恢复 | 熔断与恢复测试通过 |
| 插件市场 | Linux 配置从失效 Wine 路径改为当前主机快照 | `codex plugin list` 恢复 |
| Cloudflare MCP | 未完成 OAuth 的旧插件暂时禁用 | 启动不再出现 `AuthorizationRequired`；浏览器与 Chrome 插件仍启用 |
| 周备份 | 从 `codex-feishu-bridge-current` 解析真实运行桥 | dry-run 确认抓取 `v0.2.4` |
| 恢复脚本 | 新机恢复时重建 `codex-feishu-bridge-current` 软链接 | staged restore 脚本回读通过 |
| 救援单元 | 将 `codex-feishu-rescue-bot.service` 纳入便携备份 | staged unit 存在 |
| 自检能力 | 新增 `xiaoce-self-audit` skill 和脱敏 doctor 脚本 | skill validator 与真实脚本执行通过 |

## 审计证据

- 过去 7 天飞书桥日志约 882,870 行。
- 其中 delta 类约 569,878 行，token usage 约 10,684 行，rate limit 约 10,628 行。
- 过去 7 天记录到 64 次 CardKit fallback。
- 最近 1 小时 31,911 行日志中，29,562 行为 delta 类，约占 92.6%。
- 当前 `journald` 占用约 1.6 GB，`~/.codex/log` 占用约 367 MB；历史文件本轮未删除。
- `codexpp-launch.log` 约 302.8 MB、`codex-wine.log` 约 79.1 MB，均自 2026-06-21 起未再写入。
- `codex-cli-preflight.log` 约 2.39 MB；健康预检现已零写盘，只有真实修复或错误会继续记录。
- 周备份 timer 已启用，下一次计划时间为 2026-07-13 10:02:10 CST。
- 最近一次私有备份提交为 `9c52513`，时间为 2026-07-10 00:40:28 CST。

## 日志加固实测

- 飞书桥于 2026-07-10 10:13:06 CST 完成重启，`active/running`，`NRestarts=0`。
- 启动验收窗口共 16 行，长连接成功 1 次，routine RPC、delta 和错误均为 0。
- 2026-07-10 13:18:44 CST 后的真实飞书回合再次统计：11 行，routine RPC、delta、token usage、rate limit 和错误均为 0。
- 当前默认模型继续为 `gpt-5.6-terra`，reasoning effort 为 `high`，provider 未切换。
- 自动验收报告：`/home/user/.codex/backups/xiaoce-log-hardening-20260710-100745/post-restart-verification.txt`。

## 用户习惯固化

- 默认简洁中文，先给结论。
- 服务器、systemd、线上服务先确认真实运行态。
- 用户说“先查”“先告诉”“不要直接做”时保持只读。
- 不编造 URL、渠道、配置或执行结果。
- 明确区分配置完成、测试通过、服务重启、远端推送和线上部署。
- 删除、清理、卸载、reset 前列出精确目标并等待确认。
- 有意义工作形成 concise handoff，但不把短期模型或临时状态写成永久规则。

## 未自动处理

1. Codex doctor 发现 1 个 active rollout 与 3 个 archived rollout 未进入状态库索引。数据库完整、文件可读，当前 CLI 没有安全的一键修复命令，因此未改数据库。
2. Cloudflare MCP 需要 OAuth 才能恢复。当前为暂时禁用，不影响浏览器、Chrome、飞书或 5.6 模型链路。
3. `.codex` 当前约有 847.71 MB active rollout 文件。未做清理，因为删除必须先列目标并获得明确批准。
4. 本轮只完成便携备份 dry-run，没有提前触发私有仓库推送；下一次周定时备份会使用修复后的真实运行桥范围。
5. 历史 `journald` 与 `~/.codex/log` 未清理。若后续要回收空间，应先列出精确 journal 保留策略和旧文件目标，再单独取得删除批准。

## 回滚点

- 本轮文件级备份：`/home/user/.codex/backups/xiaoce-self-opt-20260710-093317`
- 日志加固备份：`/home/user/.codex/backups/xiaoce-log-hardening-20260710-100745`
- 旧 CLI 归档：`/home/user/.local/share/codex-cli/0.141.0`
- 新 CLI 隔离副本：`/home/user/.local/share/codex-cli/0.144.1`

## 后续验收

飞书桥后续升级或改配置后复查：

1. `codex-feishu-bot.service` 为 `active/running`。
2. 日志出现 `Feishu long connection started`。
3. app-server 版本为 `0.144.1`。
4. 新消息使用 `gpt-5.6-terra/high`。
5. normal 日志不再逐条记录 delta、token usage 和 rate limit。
6. normal 日志不再逐条记录 item 生命周期、turn diff 和 routine thread 状态。
7. systemd 回读 `LogRateLimitIntervalUSec=30s`、`LogRateLimitBurst=200`。
8. CardKit 失败时旧卡片仍能送达，5 分钟后自动恢复尝试。
