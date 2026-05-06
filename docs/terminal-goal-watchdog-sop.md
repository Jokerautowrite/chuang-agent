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

- 项目根：`/home/user/projects/chuang-agent`
- tmux session：`chuang-goal`
- 日志目录：`/home/user/.codex/chuang-goal-interactive`
- 目标文件：`/home/user/.codex/chuang-goal-interactive/goal.txt`

脚本会：

1. 创建或复用 `chuang-goal` tmux session。
2. 在项目根启动 `/home/user/.local/bin/codex --no-alt-screen`。
3. 把预设 goal 发进终端。
4. attach 到该 session，方便操作者直接观察和接管。

如果 session 已存在，脚本只 attach，不重复启动第二个 worker。

### 批处理长跑 worker

```bash
./scripts/run-chuang-goal-overnight.sh
```

默认行为：

- 项目根：`/home/user/projects/chuang-agent`
- Codex bin：`/home/user/.local/bin/codex`
- 运行根目录：`/home/user/.codex/chuang-goal-runs`
- 默认总时长：`21600` 秒
- 单轮 timeout：`2100` 秒

每次 run 会生成独立日志目录，关键文件包括：

- `prompt.md`：本轮投给 Codex exec 的完整任务。
- `last-message.md`：Codex 本轮最后回复。
- `summary.md`：run 基本信息和结束状态。
- `events.jsonl`：Codex exec JSONL 输出。
- `run.log`：外层循环和错误摘要。

这个入口适合无人值守批处理，但它仍只是本地终端脚本，不是 Chuang runtime 内部调度器。

### watchdog

```bash
./scripts/chuang-goal-watchdog.sh
```

默认行为：

- 观察 tmux session：`chuang-goal`
- 间隔：`1800` 秒
- 日志目录：`/home/user/.codex/chuang-goal-interactive`
- watchdog 日志：`/home/user/.codex/chuang-goal-interactive/watchdog.log`
- 最近 pane 截图：`/home/user/.codex/chuang-goal-interactive/last-pane.txt`

每轮记录：

- tmux session 是否存在。
- pane 列表、当前命令和最近 120 行 pane 内容。
- 当前 Codex 相关进程。
- 仓库 `git status --short`。

watchdog 的职责是让终端进度一直可见。它不应该自动修复、自动提交、自动清理或自动重启 worker。

## 查看进度

交互式 worker：

```bash
tmux attach -t chuang-goal
tail -n 120 /home/user/.codex/chuang-goal-interactive/watchdog.log
tail -n 120 /home/user/.codex/chuang-goal-interactive/last-pane.txt
git status --short
```

批处理 worker：

```bash
ls -1 /home/user/.codex/chuang-goal-runs
tail -n 160 /home/user/.codex/chuang-goal-runs/<run-id>/run.log
tail -n 160 /home/user/.codex/chuang-goal-runs/<run-id>/last-message.md
```

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

后续如果要产品化，应该先补一层只读状态摘要，而不是直接让飞书下发执行命令。最小下一步可以是把 watchdog 最近状态整理成结构化 report，仍保持只读。

## 安全规则

- 不自动删除、清理、reset、purge、卸载或重建任何目标。
- 不触碰 Hermes。
- 不修改 Codex Feishu bridge 或 Chuang Feishu bridge，除非任务明确要求。
- 不把 secret 写进日志、文档、提交或聊天回复。
- 不把模型满载、worker 卡住、测试失败误报成第二测试版失败；先看具体日志和验证命令。
- 不把 `GoalRun`、subagent queue、Feishu channel 和终端 worker 混成同一个能力。
