# Terminal Goal Watchdog SOP

更新时间：2026-05-07

## 定位

这份文档记录已经跑通过的 Chuang 长任务值守方案：

```text
真实终端 Codex worker + watchdog 可视化日志
```

它不是 Chuang 内部子代理队列，也不是飞书命令调度层。当前可用形态是让真实终端里的 Codex 进程持续工作，watchdog 从旁边观察 tmux pane、Codex 进程和 git 状态，方便操作者随时接管、判断是否卡住、整理提交。

边界必须保持清楚：

- 终端 Codex 是当前工作执行体。
- watchdog 只观察和记录，不派活、不修改仓库。
- Chuang `GoalRun` / subagent queue 是协议和计划层，不代表当前已有真实外部 worker 自动干活。
- 飞书是交互入口，不等于必须做飞书命令层。
- 当前不需要补 systemd、timer、常驻服务或 tmux 包装层，除非后续明确要把值守变成后台产品能力。

## 已有入口

### 交互式终端 worker

```bash
./scripts/start-codex-goal-terminal.sh
```

默认行为：

- 项目根：`$CHUANG_AGENT_ROOT`
- tmux session：`chuang-goal`
- 日志目录：`$HOME/.codex/chuang-goal-interactive`
- 目标文件：`$HOME/.codex/chuang-goal-interactive/goal.txt`

脚本会：

1. 创建或复用 `chuang-goal` tmux session。
2. 在项目根启动 `$HOME/.local/bin/codex --no-alt-screen`。
3. 把预设 goal 发进终端。
4. attach 到该 session，方便操作者直接观察和接管。

如果 session 已存在，脚本只 attach，不重复启动第二个 worker。

### 批处理长跑 worker

```bash
./scripts/run-chuang-goal-overnight.sh
```

默认行为：

- 项目根：`$CHUANG_AGENT_ROOT`
- Codex bin：`$HOME/.local/bin/codex`
- 运行根目录：`$HOME/.codex/chuang-goal-runs`
- 默认总时长：`21600` 秒
- 单轮 timeout：`2100` 秒

每次 run 会生成独立日志目录，关键文件包括：

- `prompt.md`：本轮投给 Codex exec 的完整任务。
- `last-message.md`：Codex 本轮最后回复。
- `summary.md`：run 基本信息和结束状态。
- `status.json`：外层 runner 每轮写入的结构化心跳。
- `events.jsonl`：Codex exec JSONL 输出。
- `run.log`：外层循环和错误摘要。

`status.json` 用于主控快速判断批处理 worker 停在哪一步，不需要先翻完整日志。字段包括 `run_id`、`iteration`、`deadline`、`last_iteration_exit_status`、`last_message_file`、`jsonl_log`、`plain_log`、`status` 和 `next_action`。脚本只写状态，不自动重启 Codex、不清理旧日志、不触碰 systemd/timer。

可选 dry-run：

```bash
CHUANG_OVERNIGHT_DRY_RUN=1 CHUANG_OVERNIGHT_MAX_ITERATIONS=1 SLEEP_SECONDS=0 ./scripts/run-chuang-goal-overnight.sh
```

dry-run 只用于测试外层状态写入路径，不会调用真实 Codex；默认行为仍是正常执行 Codex。

这个入口适合无人值守批处理，但它仍只是本地终端脚本，不是 Chuang runtime 内部调度器。

### watchdog

```bash
./scripts/chuang-goal-watchdog.sh
```

默认行为：

- 观察 tmux session：`chuang-goal`
- 间隔：`1800` 秒
- 日志目录：`$HOME/.codex/chuang-goal-interactive`
- watchdog 日志：`$HOME/.codex/chuang-goal-interactive/watchdog.log`
- 最近 pane 截图：`$HOME/.codex/chuang-goal-interactive/last-pane.txt`
- 最新结构化状态：`$HOME/.codex/chuang-goal-interactive/latest-watchdog-report.json`
- 最新 pane/process/git 只读快照：`latest-panes.txt`、`latest-codex-processes.txt`、`latest-git-status.txt`

每轮记录：

- tmux session 是否存在。
- pane 列表、当前命令和最近 120 行 pane 内容。
- 当前 Codex 相关进程。
- 仓库 `git status --short`。
- JSON 状态摘要里的 `takeover.next_action`、tmux 是否存在、Codex 进程数量、git dirty 状态和只读边界。

watchdog 的职责是让终端进度一直可见。它不应该自动修复、自动提交、自动清理或自动重启 worker。

一次性状态检查可用：

```bash
./scripts/chuang-goal-watchdog.sh --once
```

或：

```bash
WATCHDOG_ONCE=1 ./scripts/chuang-goal-watchdog.sh
```

一次性模式只记录一轮状态后退出，适合主控接管前检查或未来只读控制台读取。它和循环模式一样只观察，不派活、不修改仓库、不重启 worker、不触碰服务。

## 查看进度

交互式 worker：

```bash
tmux attach -t chuang-goal
tail -n 120 $HOME/.codex/chuang-goal-interactive/watchdog.log
tail -n 120 $HOME/.codex/chuang-goal-interactive/last-pane.txt
jq . $HOME/.codex/chuang-goal-interactive/latest-watchdog-report.json
git status --short
```

批处理 worker：

```bash
ls -1 $HOME/.codex/chuang-goal-runs
jq . $HOME/.codex/chuang-goal-runs/<run-id>/status.json
tail -n 160 $HOME/.codex/chuang-goal-runs/<run-id>/run.log
tail -n 160 $HOME/.codex/chuang-goal-runs/<run-id>/last-message.md
```

先看 `status.json`：`status=running` 表示外层循环仍在跑或最后一次心跳停在运行态，`status=finished` 表示外层脚本已经到达截止时间或达到显式最大轮次；`next_action` 给出主控下一步应查看日志、等待下一轮、还是人工复核。`last_iteration_exit_status` 是上一轮 Codex exec 的退出码，`null` 表示尚未完成任何一轮。

统一只读状态入口：

```bash
./scripts/chuang-goal-run-status.sh
./scripts/chuang-goal-run-status.sh --json
```

这个入口只读取 watchdog JSON、overnight `status.json` 和最新 run 目录摘要，方便主控快速判断终端 worker 或 overnight runner 是否仍在推进。它不启动 worker、不重启、不修改仓库、不删除日志、不触碰服务；需要临时查看其他位置时，用 `CHUANG_GOAL_WATCHDOG_REPORT_FILE`、`CHUANG_GOAL_RUN_ROOT` 或 `CHUANG_GOAL_OVERNIGHT_STATUS_FILE` 覆盖读取路径。

查看时不要把日志里的 secret 值、完整私有配置或 token 贴回聊天。需要确认密钥状态时只说变量名和 `<set>`。

## 操作者收口流程

每次 worker 跑完一段后，由操作者或主 Codex 做收口：

1. 读 `git status --short`，确认有哪些改动。
2. 读相关 diff，区分可提交改动和无关改动。
3. 跑适合的验证，文档-only 至少跑 `git diff --check`。
4. 更新 `docs/handoff-current.md` 或 `docs/progress-log.md`。
5. 按逻辑拆小提交，提交说明写清楚本次行为和边界。
6. 回复下一阶段推进清单，让老爸挑选对齐后再继续。

这个收口权仍在主控侧。终端 worker 可以实现和测试，但不应绕过主控直接定义方向。

## 暂停和恢复

交互式 worker 暂时离开时，优先 detach：

```text
Ctrl-b d
```

恢复：

```bash
./scripts/start-codex-goal-terminal.sh
```

或：

```bash
tmux attach -t chuang-goal
```

需要真正停止 worker 时，应由操作者在 tmux 里看清当前状态后正常中断或退出当前进程。不要让 watchdog 或脚本自行做停止、清理、删除、reset。

## 什么时候再扩展

当前先不做：

- 飞书命令层：除非已经证明手动查看日志和接管不够用。
- systemd/timer 常驻：除非要稳定无人值守多天运行。
- 自动重启：除非有明确失败分类和防重复执行策略。
- Chuang 内部真实 worker adapter：除非本地协议、治理、报告和 live gate 都已经验收。

后续如果要产品化，应该继续沿着只读状态摘要推进，而不是直接让飞书下发执行命令。当前 `latest-watchdog-report.json` 已经提供最小接管面；下一步可以让桌面控制台或 Feishu 报告只读展示它，但仍不能让通道直接派活、重启或修改仓库。

## 安全规则

- 不自动删除、清理、reset、purge、卸载或重建任何目标。
- 不触碰 Hermes。
- 不修改 Codex Feishu bridge 或 Chuang Feishu bridge，除非任务明确要求。
- 不把 secret 写进日志、文档、提交或聊天回复。
- 不把模型满载、worker 卡住、测试失败误报成第二测试版失败；先看具体日志和验证命令。
- 不把 `GoalRun`、subagent queue、Feishu channel 和终端 worker 混成同一个能力。
