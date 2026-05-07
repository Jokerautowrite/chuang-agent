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

当前第二测试版已经是 readiness/smoke 合同 ready，并且最新最终本地验收已经通过 `sh scripts/chuang-final-verify.sh`，输出 `chuang_final_verify_ok`。live-readiness 只读预检主入口已经通过 `sh scripts/chuang-live-readonly-preflight.sh`，输出 `live_readiness_preflight_ok`。

下一阶段的“完整可用”应定义为：老爸可以通过终端或 Chuang 专用 Feishu 通道发起真实对话，主控能看到 worker 进度、provider/fallback 根因、Feishu 会话绑定、本地子代理 rehearsal、console/watchdog 状态和记忆写回状态；真实 live Feishu 长连接、真实外部 runner 池、真实桌面、真实服务控制、wiki/GBrain 和自动调度仍属于后置 live adapter。本文档只记录本地合同和下一阶段推进项，不声称真实 live Feishu 或真实 runner 已接通。

## Ready Matrix

| 能力 | 当前判定 | 验收命令 | 风险边界 |
| --- | --- | --- | --- |
| 第二测试版本地合同 | ready | `sh scripts/chuang-second-test-smoke.sh` | 不连接真实外部服务，不证明 live Feishu/桌面/wiki |
| 完整本地可用闭环 | ready | `sh scripts/chuang-complete-local-smoke.sh` | 串联本地 smoke、watchdog 只读快照和诊断读面，仍不连接真实 Feishu、不读真实 secret、不控制真实服务 |
| final verify 主入口 | ready | `sh scripts/chuang-final-verify.sh` | 要求干净工作树后串起 complete-local smoke 和 `git diff --check`；输出 `chuang_final_verify_ok` 只证明本地合同收口，不证明 live Feishu/真实 runner |
| live readonly preflight 主入口 | ready | `sh scripts/chuang-live-readonly-preflight.sh` | 串联 chmod/syntax check、provider fallback smoke、Feishu live preflight smoke、subagent live preflight、watchdog once、console snapshot 和 complete-local smoke；输出 `live_readiness_preflight_ok`；`scripts/chuang-live-readiness-preflight.sh` 仅作兼容别名/旧入口提示，仍不连接真实 Feishu、不读真实 secret、不控制真实服务 |
| 项目 readiness/doctor | ready | `cargo run --quiet -- status --config config.toml --json` 和 `cargo run --quiet -- doctor --config config.toml --json` | `project_readiness=ready` 只代表本地模块合同绿 |
| 主链 runtime/app-server/channel simulate | ready | `cargo run --quiet -- channel simulate --workspace-root . --message-id m --sender-id u --thread-id t --text "ping" --json` | channel simulate 不等于真实飞书在线 |
| GoalRun 计划/checkpoint | ready | `cargo run --quiet -- goal show --goal-id mainline-mvp --json` | 只是计划和续接记录，不执行任务 |
| 本地五层记忆 | ready | `cargo run --quiet -- memory maintenance report --query "验收" --session-id acceptance --json` | 不自动写 `MEMORY.md` 或 `experiences.md` |
| 记忆人工批准写回 | ready | `cargo test -q --test memory_maintenance_cli_tests` | 只能 `--approve-writeback` 追加 LIM 候选，decay 仍只 review |
| 外脑本地检索 evidence | ready | `cargo test -q --test memory_maintenance_cli_tests` | `memory knowledge search` evidence/provenance 仍来自本地文件行匹配，不代表 wiki/GBrain live 接入 |
| provider 满载/fallback 诊断 | ready | `cargo test -q --test slot_registry_tests --test runtime_report_tests` | fallback 必须显式配置，不能 silent fallback |
| 子代理本地队列和 bounded run-loop | ready | `cargo run --quiet -- subagent run-loop --runner command --runner-command sh --runner-arg scripts/chuang-subagent-runner-example.sh --approve-exec --max-runs 1 --max-concurrency 1 --json` | 这是本地命令 runner，不是 live 外部 worker 池 |
| 子代理 live-preflight evidence | ready | `cargo test -q --test cli_subagent_live_preflight_tests` | rehearsal 检查 gate、allowlist、capability routing 和 report admission；不 dispatch、不启动真实 runner、不写 report |
| 终端 Codex worker + watchdog | ready | `./scripts/chuang-goal-watchdog.sh --once` 和 `cargo run --quiet -- console snapshot --json` | watchdog/console 只读，不派活、不重启、不提交 |
| console watchdog freshness evidence | ready | `cargo test -q --test cli_console_tests` | console 只读展示 latest watchdog report 的 available/freshness/missing/invalid 状态，不创建或修复 report |
| live adapter gate | ready | `cargo test -q --test live_adapter_gate_tests --test cli_status_tests --test cli_doctor_tests` | env=1 只开 preflight，不绕过 allowlist/审批/审计 |
| command control/actuator 示例 | ready | `cargo test -q --test control_actuator_contract_tests` | 示例 adapter 不控制真实服务或桌面 |
| Chuang Feishu 本地命令 | ready | `node scripts/chuang-feishu-command-smoke.js` | 本地命令不等于真实飞书连接健康 |
| Feishu live-preflight evidence | ready | `node --check scripts/chuang-feishu-live-preflight.js && node scripts/chuang-feishu-live-preflight-smoke.js` | 只读检查 Chuang env/workspace/app-server/channel 边界和 evidence 链，不建立 websocket/webhook，不发送消息 |

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

## 最新证据状态

| 证据面 | 最新状态 | 已验证命令 | 下一阶段含义 |
| --- | --- | --- | --- |
| final verify | 已完成 | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 可作为第二测试版本地闭环主门禁；后续 live 工作先保持它稳定 |
| live-readiness preflight | 已完成 | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | 可作为启用真实 adapter 前的只读总排查入口；仍不是 live 接通证明 |
| Feishu evidence | 已完成 | `node --check scripts/chuang-feishu-live-preflight.js && node scripts/chuang-feishu-live-preflight-smoke.js && node scripts/chuang-feishu-command-smoke.js` | 本地命令、session/health 诊断、live preflight evidence 链已可复验；下一步才是人工 Chuang 专用 live 接入 |
| subagent evidence | 已完成 | `cargo test -q --test cli_subagent_live_preflight_tests` | live runner 启用前 gate/allowlist/capability/report admission rehearsal 已有；下一步先单 worker 人工启用，不直接开池 |
| console/watchdog evidence | 已完成 | `cargo test -q --test cli_console_tests` 和 `./scripts/chuang-goal-watchdog.sh --once` | console 能看到 watchdog 只读状态和 freshness；下一步补长跑 heartbeat/status 面 |
| memory evidence | 已完成 | `cargo test -q --test memory_maintenance_cli_tests` | LIM 批准写回 receipt、知识检索 provenance/evidence 已有；下一步补人工 UX 和定期 review，不自动写长期记忆 |

## 下一阶段小提交清单

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

8. 已完成：`feat(smoke): add live readonly preflight wrapper`
   文件范围：`scripts/chuang-live-readonly-preflight.sh`、README / readiness matrix 小段说明；`scripts/chuang-live-readiness-preflight.sh` 仅作兼容别名/旧入口提示。
   验证：`bash -n scripts/chuang-live-readonly-preflight.sh`、`sh scripts/chuang-live-readonly-preflight.sh`、`git diff --check`。
   边界：只读预检，不连接真实 Feishu，不读真实 secret，不控制服务。

9. 已完成：`docs: keep live preflight entry points aligned`
   文件范围：`README.md`、`docs/mvp-scope.md`、`docs/handoff-current.md`、`docs/progress-log.md`。
   验证：`git diff --check`。
   边界：只同步入口说明，不改 runtime。

10. 下一步：`feat(observability): add long-run heartbeat/status contract`
   文件范围建议：长跑状态读面、console/status 输出和对应 tests/docs。
   验证建议：`./scripts/chuang-goal-watchdog.sh --once`、`cargo run --quiet -- console snapshot --json`、相关 console/status 测试。
   边界：只报告 heartbeat、last_seen、worker_identity、current_goal、last_checkpoint、last_error、git_dirty 和 next_action；不派活、不重启、不清理、不提交。

11. 下一步：`feat(feishu): surface readonly long-run status command`
   文件范围建议：Chuang Feishu 本地命令和 command smoke。
   验证建议：`node scripts/chuang-feishu-command-smoke.js`、必要时新增 readonly status smoke。
   边界：只读展示 heartbeat/status 摘要；不连接真实 Feishu、不发送外部消息、不复用 Codex/Hermes env。

12. 下一步：`feat(subagent): rehearse single live runner readiness receipt`
   文件范围建议：subagent live-preflight / runner contract tests。
   验证建议：`cargo test -q --test cli_subagent_live_preflight_tests`。
   边界：仍是 rehearsal 和 receipt，不启动真实 runner；真实 runner 只允许后续人工显式 gate + 单 worker 试跑。

13. 下一步：`feat(memory): improve maintenance review UX`
   文件范围建议：memory maintenance CLI 输出和 tests/docs。
   验证建议：`cargo test -q --test memory_maintenance_cli_tests`。
   边界：继续要求 `--approve-writeback`；不做自动调度，不自动压缩或覆盖热记忆。

14. 进行中：`docs: define live cutover runbook`
   文件范围建议：`scripts/chuang-live-operator-checklist.sh`、`docs/live-operator-test-runbook.md` 和少量 Feishu checklist 链接。
   验证建议：`bash -n scripts/chuang-live-operator-checklist.sh`、fixture JSON 手动检查、`cargo test -q --test cli_smoke_tests live_operator_checklist_reports_redacted_manual_live_steps`、`git diff --check`。
   边界：只定义人工步骤、回滚条件和证据采集；不启用真实 live adapter，不改服务，不打印 secret。

## 建议并行拆分

- Worker A：provider/fallback 诊断，只碰 provider、slot_registry、runtime_report 和对应 tests/docs。
- Worker B：memory maintenance approval，只碰 cli_memory、memory tests、memory doc。
- Worker C：live adapter gate，只碰 live gate/status/doctor/output 和对应 tests/control safety doc。
- Worker D：Feishu 本地命令，只碰 `scripts/chuang-feishu-bridge-commands.js`、command smoke、channel docs。
- Worker E：验收矩阵和下一阶段排序，只碰本文件。
- Worker F：console 只读展示 terminal watchdog 状态，只碰 console、console tests 和本项文档。
- Worker G：长跑 heartbeat/status，只碰长跑状态读面、console/status 输出和对应 tests/docs。

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
