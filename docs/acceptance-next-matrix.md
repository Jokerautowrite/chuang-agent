# Acceptance Next Matrix

更新时间：2026-05-07

## 结论

从第二测试版到“完整可用”的最短路线不是立刻打开 live adapter，而是先把本地可用闭环收成稳定验收面：

```text
真实 provider 诊断可解释
+ 记忆维护可人工批准写回
+ 终端 worker 进程可见可接管
+ Feishu 本地命令和 app-server 会话可诊断
+ live adapter 全部默认关闭但审计面明确
```

当前第二测试版已经是 readiness/smoke 合同 ready。下一阶段的“完整可用”应定义为：老爸可以通过终端或 Chuang 专用 Feishu 通道发起真实对话，主控能看到 worker 进度、provider/fallback 根因、会话和记忆写回状态；真实桌面、真实服务控制、真实外部 worker、wiki/GBrain 和自动调度仍属于后置 live adapter。

## Ready Matrix

| 能力 | 当前判定 | 验收命令 | 风险边界 |
| --- | --- | --- | --- |
| 第二测试版本地合同 | ready | `sh scripts/chuang-second-test-smoke.sh` | 不连接真实外部服务，不证明 live Feishu/桌面/wiki |
| 完整本地可用闭环 | ready | `sh scripts/chuang-complete-local-smoke.sh` | 串联本地 smoke、watchdog 只读快照和诊断读面，仍不连接真实 Feishu、不读真实 secret、不控制真实服务 |
| live readiness 总入口 | ready | `sh scripts/chuang-live-readiness-preflight.sh` | 串联 provider fallback、Feishu live preflight、subagent live preflight、watchdog/console 和 complete-local smoke，仍全部只读 |
| 项目 readiness/doctor | ready | `cargo run --quiet -- status --config config.toml --json` 和 `cargo run --quiet -- doctor --config config.toml --json` | `project_readiness=ready` 只代表本地模块合同绿 |
| 主链 runtime/app-server/channel simulate | ready | `cargo run --quiet -- channel simulate --workspace-root . --message-id m --sender-id u --thread-id t --text "ping" --json` | channel simulate 不等于真实飞书在线 |
| GoalRun 计划/checkpoint | ready | `cargo run --quiet -- goal show --goal-id mainline-mvp --json` | 只是计划和续接记录，不执行任务 |
| 本地五层记忆 | ready | `cargo run --quiet -- memory maintenance report --query "验收" --session-id acceptance --json` | 不自动写 `MEMORY.md` 或 `experiences.md` |
| 记忆人工批准写回 | ready | `cargo test -q --test memory_maintenance_cli_tests` | 只能 `--approve-writeback` 追加 LIM 候选，decay 仍只 review |
| provider 满载/fallback 诊断 | ready | `cargo test -q --test slot_registry_tests --test runtime_report_tests` | fallback 必须显式配置，不能 silent fallback |
| 子代理本地队列和 bounded run-loop | ready | `cargo run --quiet -- subagent run-loop --runner command --runner-command sh --runner-arg scripts/chuang-subagent-runner-example.sh --approve-exec --max-runs 1 --max-concurrency 1 --json` | 这是本地命令 runner，不是 live 外部 worker 池 |
| 终端 Codex worker + watchdog | ready | `./scripts/chuang-goal-watchdog.sh --once` 和 `cargo run --quiet -- console snapshot --json` | watchdog/console 只读，不派活、不重启、不提交 |
| live adapter gate | ready | `cargo test -q --test live_adapter_gate_tests --test cli_status_tests --test cli_doctor_tests` | env=1 只开 preflight，不绕过 allowlist/审批/审计 |
| command control/actuator 示例 | ready | `cargo test -q --test control_actuator_contract_tests` | 示例 adapter 不控制真实服务或桌面 |
| Chuang Feishu 本地命令 | ready | `node scripts/chuang-feishu-command-smoke.js` | 本地命令不等于真实飞书连接健康 |

## Live Adapter 后置

| 后置能力 | 后置原因 | 最早进入条件 | 验收边界 |
| --- | --- | --- | --- |
| 真实 Feishu 长连接在线 | 涉及外部凭证和服务状态 | Chuang 专用 env、app-server health、command smoke 全绿 | 只用 Chuang 专用 bot，不碰 Codex/Hermes |
| 真实外部 worker 池 | 涉及外部进程、登录态和任务副作用 | runner allowlist、capability routing、report admission、live gate 证据齐 | 先单 worker，再 bounded 并发 |
| 真实服务控制 apply | 涉及 start/stop/restart/change_model | Chuang-only allowlist 和 receipt 校验完成 | 不允许任意 systemd，不含 Codex/Hermes |
| 真实桌面/浏览器 actuator | 涉及用户界面和登录态 | action allowlist、验证码规则、审计 receipt 完成 | observe 可先行，mutation 后置 |
| wiki/GBrain 外脑 live 接入 | 涉及外部知识库权限和写入策略 | 本地 knowledge search 和 provenance 注入稳定 | 先只读检索，不自动写外脑 |
| 自动记忆维护调度 | 涉及长期记忆污染 | dry-run/report/apply 证据足够且人工 UX 稳定 | 默认不自动写，先做建议 |
| systemd/timer 常驻 watchdog | 会改变运行形态和恢复策略 | 手动终端方案证明不够用 | 不自动清理、不自动重启、不自动提交 |

## 本轮已完成与下一步小提交

1. 已完成：`feat(provider): classify fallback failures`
   文件范围：`src/provider_openai_compatible.rs`、`src/slot_registry.rs`、`src/runtime_report.rs`、相关 tests、`docs/provider-fallback-diagnostics.md`。
   验证：`cargo test -q --test slot_registry_tests --test runtime_report_tests`。
   边界：无显式 fallback 配置时只暴露失败，不降级。

2. 已完成：`feat(memory): record maintenance approval receipts`
   文件范围：`src/cli_memory.rs`、`tests/memory_maintenance_cli_tests.rs`、`docs/memory-maintenance-loop.md`。
   验证：`cargo test -q --test memory_maintenance_cli_tests --test cli_identity_memory_tests`。
   边界：只写 `experiences.md`，必须 `--approve-writeback`。

3. 已完成：`feat(status): expose live adapter preflight audit`
   文件范围：`src/live_adapter_gate.rs`、`src/kernel_status.rs`、`src/cli_output.rs`、`src/cli_doctor.rs`、相关 tests。
   验证：`cargo test -q --test live_adapter_gate_tests --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests`。
   边界：不打开真实控制，只输出 gate/preflight/must_reject。

4. 已完成：`feat(feishu): add session health diagnostics`
   文件范围：`scripts/chuang-feishu-bridge-commands.js`、对应 command smoke。
   验证：`node scripts/chuang-feishu-command-smoke.js`。
   边界：只读诊断，不连接真实飞书，不打印 secrets。

5. 本文档提交：`docs: add acceptance next matrix`
   文件范围：`docs/acceptance-next-matrix.md`。
   验证：`git diff --check -- docs/acceptance-next-matrix.md`。
   边界：只给主控排程，不改变 runtime。

6. 已完成：`feat(console): surface readonly terminal watchdog state`
   文件范围建议：`src/cli_console.rs`、`tests/cli_console_tests.rs`，必要时加一个小 reader。
   验证：`./scripts/chuang-goal-watchdog.sh --once`、`cargo test -q --test cli_console_tests`。
   边界：只读 `latest-watchdog-report.json`，不派活、不重启。

7. 已完成：`test(smoke): add complete local acceptance wrapper`
   文件范围：新增 `scripts/chuang-complete-local-smoke.sh` 和 `tests/cli_smoke_tests.rs` 轻量 wrapper 合同测试。
   验证内容：second-test smoke、watchdog `--once`、Feishu command/session/rich smoke、status/doctor/app-server/console 诊断读面。
   边界：仍使用临时目录/stub/local fixtures，不连接 live services。

8. 已完成：`live readiness preflight wrapper`
   文件范围：`scripts/chuang-live-readiness-preflight.sh`、入口说明。
   验证：`bash -n scripts/chuang-live-readiness-preflight.sh`、`sh scripts/chuang-live-readiness-preflight.sh`。
   边界：仍全部只读、本地 fixture、本地 smoke。

9. 下一步：`docs: refresh readiness after complete local acceptance`
   文件范围：`docs/handoff-current.md`、`docs/progress-log.md`、必要时 `docs/mvp-scope.md`。
   验证：`git diff --check`。
   边界：最后串行更新共享文档，避免多个 worker 同时改。

## 建议并行拆分

- Worker A：provider/fallback 诊断，只碰 provider、slot_registry、runtime_report 和对应 tests/docs。
- Worker B：memory maintenance approval，只碰 cli_memory、memory tests、memory doc。
- Worker C：live adapter gate，只碰 live gate/status/doctor/output 和对应 tests/control safety doc。
- Worker D：Feishu 本地命令，只碰 `scripts/chuang-feishu-bridge-commands.js`、command smoke、channel docs。
- Worker E：验收矩阵，只碰本文件。
- Worker F：console 只读展示 terminal watchdog 状态，只碰 console、console tests 和本项文档。

## 避免冲突文件

这些文件当前属于高冲突面，除 owner 外先不要并行改：

- `docs/handoff-current.md`
- `docs/progress-log.md`
- `docs/mvp-scope.md`
- `README.md`
- `src/kernel_status.rs`
- `src/cli_output.rs`
- `src/cli_doctor.rs`
- `scripts/chuang-mvp-smoke.sh`
- `scripts/chuang-feishu-bridge-commands.js`

共享文档只在每批代码验证通过后由主控统一补，不要让每个 worker 都提前写。

## 推荐全量验收命令

按这个顺序跑，先快后慢：

```bash
cargo fmt --all --check
git diff --check

cargo test -q --test slot_registry_tests --test runtime_report_tests
cargo test -q --test memory_maintenance_cli_tests
cargo test -q --test live_adapter_gate_tests --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests
cargo test -q --test cli_smoke_tests

node scripts/chuang-feishu-command-smoke.js
node scripts/chuang-feishu-session-smoke.js
node scripts/chuang-feishu-rich-message-smoke.js
./scripts/chuang-goal-watchdog.sh --once

cargo run --quiet -- status --config config.toml --json
cargo run --quiet -- doctor --config config.toml --json
cargo run --quiet -- app-server health --workspace-root . --diagnostic --json
cargo run --quiet -- console snapshot --json

cargo test -q
sh scripts/chuang-second-test-smoke.sh
```

真实 provider 或真实 Feishu 只作为人工 live check，不放进默认全量验收。需要确认密钥时只报告变量名和 `<set>`。

## 主要瓶颈

1. 共享状态文件太集中：`kernel_status`、`cli_output`、`cli_doctor` 和 readiness tests 很容易互相踩，需要主控分批合。
2. “ready” 容易被误解为 live ready：当前 ready 多数是本地合同 ready，live adapter 必须继续后置。
3. provider 满载不是本地失败：必须把 retryable/capacity/fallback_used 暴露清楚，否则主控会误判第二测试版失败。
4. 终端 worker 和 Chuang subagent queue 容易混淆：当前真实干活的是终端 Codex worker，Chuang queue 仍是协议层。
5. 全量 `cargo test` 成本高：并行 worker 先跑专项测试，主控最后跑全量。
