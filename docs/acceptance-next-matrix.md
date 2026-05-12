# Acceptance Next Matrix

更新时间：2026-05-13

快速入口：见 [第三测试版候选一页入口](./third-test-candidate.md) 和 [Live Operator Test Runbook](./live-operator-test-runbook.md)。

## 结论

当前基线已经达到第二测试版 `local-ready`：

```text
final verify 本地闭环通过
+ live-readiness 只读证据通过
+ Feishu / provider / subagent / console 诊断面可复验
+ long-run observability 有只读状态入口
+ Chuang 专用 Feishu bridge 已 active，老爸已确认可在 Feishu 联系上 Chuang
+ Feishu `/tools` 可见当前命令能力与边界
+ provider readiness check 已纳入本地候选门禁
```

第三测试版候选新增本地 wrapper：`sh scripts/chuang-third-test-smoke.sh`。它只串联 clean worktree gate、final verify、live-readiness 只读预检、`live-gaps` 矩阵、operator checklist 只读摘要和 goal run status 只读摘要；operator env blocked 只作为状态输出，不作为本地合同失败。该 wrapper 不连接真实 Feishu、不读 secret、不启动服务，最终 marker 为 `third_test_candidate_smoke_ok`。

今晚候选验收另有 dirty-tree friendly 入口：`sh scripts/chuang-candidate-verify.sh`。它串联 complete-local smoke、live runner rehearsal smoke、`live-gaps` 矩阵、operator checklist 只读摘要、operator receipt 模板结构断言、goal run status 只读摘要和 provider readiness check；provider readiness check 只读取 `status --json` 的 `provider_readiness`，输出 `<set>/<missing>` 状态，不连接真实 provider、不打印 secret。缺 provider env 时按候选现场状态报告 blocker，但不会伪装成本地合同失败；operator receipt 模板和 collector 只证明 JSON 结构可填报/可合并，不证明真实外部验收完成。

第三测试版候选不是“所有 live adapter 全开”，而是 100% 前最后一跳：用最小真实链路证明老爸可以通过 Chuang 专用 Feishu live 通道发起请求，主控能拿到 provider/env 状态、operator receipt、单 worker rehearsal 证据，并最终回到本地 verify 绿。真实 runner 池、桌面 mutation、服务控制、wiki/GBrain live 仍后置，不纳入第三测试版必须项。

当前 acceptance 口径必须区分“已 mapped/已 preflight”和“已 live”。GA 9 tools 已 mapped 只代表工具槽位、命令面和能力边界可见；真实 desktop/browser live 仍缺证据。live subagent worker 仍需要 audited adapter、config 和 gate 三件套后才能启用；三大 live gates 默认关闭，分别覆盖 provider live、subagent live runner、desktop/browser actuator live action。Feishu、provider、single worker rehearsal、desktop、browser、wiki、GBrain 都需要各自的真实 live receipt，不能由本地 readiness 或 `<set>` 状态代替。

`scripts/chuang-live-runner-readiness-view.sh --json` 只读汇总 `subagent live-preflight`、`status --json`、`doctor --json` 和 `app-server health --diagnostic --json` 的 runner gate、allowlist、capability route、admission 与 blocked reason；它帮助 operator 看见当前 runner readiness，但不启动 worker，不连接真实外部服务，也不等于 live runner ready。

`scripts/chuang-live-operator-receipt-collect.sh --json` 是本地只读 receipt collector：从 stdin/`--base-file` 读 base receipt，并用 0..n 个 `--overlay-file` 合并 partial evidence。它保留 Feishu、provider、single worker rehearsal、desktop、browser、wiki、GBrain 7 项 service receipt/evidence 口径，但固定 `can_mark_real_live_ready=false`，不能把 receipt 模板或 partial overlay 升级成真实外部验收完成。

状态面单一入口：`status --json` 的 `live_readiness` 固定复述这些词，验收矩阵只引用同一组词。`ready` / `local-ready` 只表示本地合同、smoke、诊断面或只读预检已通过，不表示真实 live receipt 已完成。

通道查询面已经同步引用同一份 workspace readiness：`channel simulate --json` 顶层返回 `live_readiness`，channel 文本面打印 `live_readiness_state` / `live_readiness_real_external_acceptance_pending` / `live_readiness_ready_does_not_mean_live`；app-server `turn/start` response 和 `turn/completed` event 顶层返回 `liveReadiness`；Feishu turn summary 只展示稳定短摘要 `live readiness local_ready_live_pending / 真实验收待完成 / ready不等于live`。这些字段不进入 `runtimeObservability`，避免把 workspace readiness 和单轮 runtime report 混淆。

固定状态词：

| 状态词 | 当前值 | 含义 | 不能误报成 |
| --- | --- | --- | --- |
| `ga_local_mapped_only` | true | GA 9 tools 已完成本地 slot、route、命令面和诊断面映射 | 真实 desktop/browser live 已验收 |
| `desktop_browser_live_gated` | true | 真实桌面/浏览器动作仍在 live gate、allowlist、治理和审计之后；普通打开/点击/输入不再要求额外人工审批 | actuator live action ready |
| `browser_worker_frozen` | true | 旧 BrowserWorker 线冻结且不在主执行路径 | browser automation ready 或已恢复 |
| `live_runner_readiness_view` | read-only-view | 本地脚本只读汇总 live-preflight/status/doctor/app-server health 的 runner readiness 和 blocked reason | live runner ready |
| `live_worker_available` | false | 当前 subagent preflight/rehearsal 不启动、不附着真实 worker | runner 池可用或 live worker 已上线 |
| `provider_live_request_verified_by_status` | false | `status --json` 只报告 provider 配置/readiness，不发真实 provider 请求 | provider live 已验收 |
| `real_external_acceptance_pending` | true | Feishu/provider/single worker rehearsal/desktop/browser/wiki/GBrain 真实外部验收仍需人工 receipt | 第三测试 100% 完成 |
| `ready/local-ready` | local only | 本地合同、smoke、诊断面或只读 preflight 通过 | live-ready、external acceptance 完成 |

## 第三测试版候选 Acceptance

| 项目 | 判定 | 验收方式 | 100% 前是否必须人工验证 | 边界 |
| --- | --- | --- | --- | --- |
| live/readiness 状态面 | `local_ready_live_pending` | `cargo run --quiet -- status --json` -> `live_readiness.overall_state=local_ready_live_pending`，并固定 `mapped_does_not_mean_live=true / gated_does_not_mean_ready=true / frozen_does_not_mean_ready=true / ready_does_not_mean_live=true` | 否，自动复验即可 | 状态面只收口术语；不连接真实服务、不启动 worker、不把 local-ready 当 live-ready |
| live/readiness 通道面 | `local_ready_live_pending` | `channel simulate --json` 顶层 `live_readiness`，app-server `turn/start` / `turn/completed` 顶层 `liveReadiness`，Feishu turn summary 短摘要；candidate/third-test 打印 live readiness 摘要 | 否，自动复验即可 | 通道面只复述 workspace readiness；不塞入 `runtimeObservability`，不连接真实 provider/Feishu，不把 channel 可见当 live-ready |
| live-runner-readiness-view | `read-only-view` | `scripts/chuang-live-runner-readiness-view.sh --json` 聚合 `subagent live-preflight` / `status --json` / `doctor --json` / `app-server health --diagnostic --json` | 否，自动复验即可 | 只读视图只汇总 runner gate、allowlist、capability route、admission 和 blocked reason；不启动 worker，不连接真实外部服务，不等于 live runner ready |
| third-test candidate wrapper | ready | `sh scripts/chuang-third-test-smoke.sh` -> `third_test_candidate_smoke_ok` | 否，自动复验即可 | 只串本地门禁和只读摘要；operator env blocked 可见但不让本地合同失败 |
| candidate verify wrapper | ready | `sh scripts/chuang-candidate-verify.sh` -> `chuang_candidate_verify_ok`，或明确报告 provider non-live block | 否，自动复验即可 | dirty-tree friendly；覆盖 live-gaps、operator checklist 和 goal run status 只读摘要；不连接真实 provider/Feishu；provider env 缺失只作为候选现场 blocker |
| live-gaps matrix | ready | `bash scripts/chuang-live-gaps-check.sh` -> `marker=live_gaps_check_ok`；`--json` 输出 `local_contract=ready / preflight=ready_but_no_start / real_live=pending` | 否，自动复验即可 | 只读 `status --json` 和 `subagent live-preflight --json`；不启 live gate、不启动 worker、不连接真实服务，provider 只显示 `<set>/<missing>` |
| final verify 本地门禁 | ready | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 否，自动复验即可 | 证明本地合同闭环，不证明 live Feishu 或真实 runner |
| live-readiness 只读预检 | local-preflight-ready | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | 否，自动复验即可 | 只读预检，不连接真实 Feishu、不读 secret、不控制服务；不等于 live-ready |
| live runner readiness view | local-readonly-view | `scripts/chuang-live-runner-readiness-view.sh --json` 输出 `schema_version`、只读边界、`live_runner_rehearsal.ready_for_live`、`starts_external_worker`、`capability_mismatch_blocks_live`、`blocked_reason`、`next_action` 和 source evidence refs | 否，自动复验即可 | 只读聚合 live-preflight / status / doctor / app-server health；不启动 worker，不接真实外部服务，不把 blocked 证据改写成 ready |
| Feishu `/tools` 可见能力 | ready | 在 Chuang 专用 Feishu 会话发送 `/tools` 或 `/capabilities`，可见 `/new`、`/session`、`/health`、`/receipt`、`/live-check`、普通文本和图片 OCR 边界 | 否，已作为 bridge 命令面可复验；live 侧可继续截图/receipt 留证 | 只展示当前能力与边界，不执行本地检查、不修改服务、不打印 secret |
| operator receipt template | ready | `scripts/chuang-live-operator-receipt.sh --json` 输出 `request_id`、`approval_scope`、`rollback_condition`、`readonly_boundaries`、`service_evidence`、`service_receipts` 和 `real_live_acceptance`；其中 `service_receipts` / `service_evidence` / `real_live_acceptance.services` 逐项对齐 Feishu、provider、single worker rehearsal、desktop、browser、wiki、GBrain 7 项 | 否，自动复验模板结构即可 | 只生成模板，不连接服务；`can_mark_real_live_ready=false`，不能代替人工 evidence |
| operator receipt collector | local-readonly-merge | `scripts/chuang-live-operator-receipt-collect.sh --json --base-file PATH --overlay-file PATH ...` 或 stdin base + overlay files，输出完整 receipt JSON | 否，自动复验合并/校验结构即可 | 只合并 base/overlay partial receipt；保留 7 项 service receipt/evidence 和 `real_live_acceptance`，固定 `can_mark_real_live_ready=false`，不能代替真实外部 acceptance |
| GA 9 tools mapped | `ga_local_mapped_only` | `/tools` / `/capabilities` 和本地诊断面显示 GA 9 工具映射、scope 和边界 | 否，自动和人工查看均可 | 只证明 mapped/routed；真实 desktop/browser live 仍需单独 receipt 和 action allowlist |
| desktop/browser live gate | `desktop_browser_live_gated` | `desktop_browser_live_gated=true`，等待 action allowlist、live gate 和审计回执；普通打开/点击/输入不要求额外人工审批 | 是 | 当前不是 desktop/browser live ready，不允许由 mapped tools 或 dry-run adapter 代替 |
| BrowserWorker old path | `browser_worker_frozen` | `status --json` -> `live_readiness.browser_worker_frozen=true` | 否，自动复验即可 | 冻结是排除边界，不是 browser automation live-ready |
| 人工 Feishu live check | candidate | 老爸用 Chuang 专用 Feishu 通道发 `/health`、`/session` 和一条普通测试消息，确认 app-server/session/channel 有真实 receipt | 是 | 只用 Chuang 专用 bot 和 env；不碰 Codex Feishu、不碰 Hermes、不打印 token |
| provider env 对齐 | readiness-only | `scripts/chuang-provider-readiness-check.sh` 读取 `status --json`，并在存在时自动吸收标准 `CHUANG_PROVIDER_ENV_FILE`；人工确认 Chuang provider env 变量存在且配置名一致；输出只允许 `<set>/<missing>` | 是 | 不连接真实 provider；不在聊天、日志、文档或 patch 中泄露 secret；无 fallback 时必须显式报错；`provider_live_request_verified_by_status=false` |
| live operator receipt | candidate | 人工执行 live cutover checklist，保存 request_id、operator、时间、允许范围、回退条件、service evidence ref 和结果摘要；collector 只能收口这些引用 | 是 | receipt 只记录审计元数据，不记录凭证、验证码或私密正文；模板/collector 本身不能标记 real live ready |
| single worker rehearsal | candidate | 在 live gate + allowlist 下只跑一个 worker rehearsal，确认 report/proposal 被主控接收；对应 receipt id 为 `subagent_live_rehearsal` | 是 | 单 worker、bounded、可停止；worker 不能直接写核心记忆，不能扩大成 runner 池 |
| final verify after live rehearsal | candidate | live rehearsal 后再次运行 `sh scripts/chuang-final-verify.sh` 和本文档 diff check | 是 | live 尝试不能破坏本地合同；失败时先停在诊断，不做 cleanup/reset |

## 100% 前必须人工验证

这些项目是从第三测试版候选走向 100% 前的真实证据，不应由本地 smoke 冒充：

1. Chuang 专用 Feishu live 通道真实收发一次，并拿到可审计 receipt。
2. Chuang provider env 与运行配置对齐，所有 secret 只显示为 `<set>`。
3. live operator receipt 完整记录审批范围、执行人、时间、request_id、结果、回退条件和 Feishu/provider/single worker rehearsal/desktop/browser/wiki/GBrain 的 evidence ref。
4. 单个 worker live rehearsal 通过 gate、allowlist、capability routing 和 report admission。
5. live rehearsal 后本地 final verify 仍通过。

## 仍然后置

| 后置能力 | 后置原因 | 最早进入条件 | 验收边界 |
| --- | --- | --- | --- |
| 真实 runner 池 | 多 worker 并发会放大外部进程、登录态和任务副作用 | 单 worker rehearsal 有 receipt，stop/timeout/report admission 都可审计 | 先单 worker，再 bounded 并发；不把 rehearsal 结果解释成 runner 池 ready |
| 桌面 mutation | 涉及真实 UI、登录态、验证码和不可逆操作 | action allowlist、live gate、验证码规则和审计稳定 | 普通打开/点击/输入直接执行；验证码、账号级提交和不可逆操作仍需询问 |
| 服务控制 apply | 涉及 start/stop/restart/change_model 等服务扰动 | Chuang-only allowlist、dry-run receipt、人工审批范围明确 | 不允许任意 systemd；不含 Codex Feishu 或 Hermes |
| wiki/GBrain live | 涉及外部知识库权限、检索质量和写入策略 | 本地 knowledge search provenance/evidence 稳定，live 只读账号和审计面确认 | 先只读检索；不自动写外脑 |

## 当前证据状态

| 证据面 | 最新状态 | 已验证命令 | 第三测试版含义 |
| --- | --- | --- | --- |
| live/readiness status surface | `local_ready_live_pending` | `cargo run --quiet -- status --json` -> `live_readiness` | 状态面固定区分 mapped/gated/frozen/ready/live，不把 local-ready 冒充 live-ready |
| live/readiness channel surface | `local_ready_live_pending` | `cargo test -q --test cli_channel_tests --test app_server_tests`；`node scripts/chuang-feishu-turn-summary-smoke.js`；`sh scripts/chuang-candidate-verify.sh` -> `chuang_candidate_verify_ok`；`sh scripts/chuang-third-test-smoke.sh` -> `third_test_candidate_smoke_ok` | channel/app-server/Feishu/candidate/third-test 都能看到同一份 readiness 边界；仍不表示真实外部验收完成 |
| final verify | 已完成 | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 本地门禁可作为 live 前后对照 |
| live-readiness preflight | local-preflight-ready | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | live 前只读排查入口已收口，但不是 live-ready |
| live-gaps matrix | 已完成 | `bash scripts/chuang-live-gaps-check.sh` -> `marker=live_gaps_check_ok` | 明确区分本地合同 ready、preflight ready-but-no-start、real live pending；不连接真实服务、不启动 worker |
| runtime/report surface | 已完成 | `cargo test -q --test live_runner_readiness_view_tests`；`cargo test -q --test app_server_tests --test cli_channel_tests --test runtime_report_tests --test kernel_status_tests`；`cargo test -q` | `runtime_report_surface=11/26`，runtime event ledger、context compaction、goal/subagent admission refs、tool protocol errors 和 unified execution 摘要均可在 readiness/status/channel/app-server/health/wrapper 面复验；GoalRun checkpoint count 到 159；不包含 secret/raw trace 外泄 |
| candidate verify | 已完成 | `sh scripts/chuang-candidate-verify.sh` -> `chuang_candidate_verify_ok`，并包含 complete-local、live runner rehearsal、live gaps、operator checklist/receipt、goal run status 和 provider readiness check | 本地候选门禁已经覆盖 live gaps、operator/goal 只读摘要、receipt 模板结构和 provider readiness 只读状态；receipt collector 属于本地收口工具，仍需人工 evidence；缺 env 会显式报告 blocker |
| Feishu evidence | 已完成 | `node --check scripts/chuang-feishu-live-preflight.js && node scripts/chuang-feishu-live-preflight-smoke.js && node scripts/chuang-feishu-command-smoke.js` | 本地命令和诊断链已可复验；`/tools` 已列出当前可见能力与边界 |
| Feishu live contact | 已推进 | 老爸在 Chuang 专用 Feishu 会话中确认 `/health`、`/session`、`/tools`：bridge=ready、app-server=running、session=`chuang-thread-1`、workspace=`/home/user/projects/chuang-agent`、Feishu/provider env 均为 `<set>`；普通文本 `哈喽` 成功返回 | bridge/session/tools 现场证据已拿到；普通文本已通过主链和 provider 返回，并生成 runtime report `report-turn-1` |
| provider evidence | live request verified | `scripts/chuang-provider-readiness-check.sh --json` -> `overall_state=ready`、`transport=native`、`api_key_state=<set>`；Feishu 普通文本 `哈喽` -> `gpt-5.5`、API 1 次、runtime report `report-turn-1` | readiness/env 对齐已通过，且 Feishu 主链已拿到一次真实 provider 响应；仍需把 provider receipt 纳入最终 operator receipt，不代表 desktop/browser/wiki/GBrain 已验收 |
| subagent evidence | 已完成 | `cargo test -q --test cli_subagent_live_preflight_tests`；`scripts/chuang-live-runner-readiness-view.sh --json`；`sh scripts/chuang-live-runner-rehearsal-smoke.sh` -> `live_runner_rehearsal_smoke_ok` | gate/allowlist/capability/report admission rehearsal 已有；readiness view 和 rehearsal 仍不是 runner 池 ready，下一步只允许 bounded single worker evidence |
| GA 9 tools evidence | 已完成但非 live | `/tools` / `/capabilities` 和本地诊断面可见 mapped 工具 | `ga_local_mapped_only=true`；映射完成不等于真实 desktop/browser live；缺真实桌面/browser action receipt |
| desktop/browser live evidence | desktop read-only evidence verified, browser live read pending | `desktop_browser_live_gated=true`；Kubuntu X11 `DISPLAY=:0` + `XAUTHORITY=/run/user/1000/.Xauthority` 下，`scripts/chuang-real-actuator-adapter.py` observe 读取 `current_window_title=飞书`，screenshot 生成 1920x1080 PNG evidence，均为 `read_only=true` / `live_gate_required=false` | 桌面只读 observation 已有 evidence；真实 open/click/input 需要 allowlist/live gate/governance/audit，但不需要额外人工审批；browser URL/title/DOM live read 仍缺 audited adapter，不能由桌面截图代替 |
| BrowserWorker evidence | frozen | `status --json` -> `live_readiness.browser_worker_frozen=true` | 冻结旧路径，不表示 browser live-ready |
| wiki/GBrain evidence | source-contract ready, live adapter missing | `memory knowledge source-contract --source wiki|gbrain --json` -> `read_only=true`、`connects_real_service=false`、`live_adapter_configured=false` | 本地 source-contract 不能代替 wiki/GBrain live 接通证据；仍需真实只读账号、provenance/evidence 和 operator receipt |
| console/watchdog evidence | 已完成 | `cargo test -q --test cli_console_tests` 和 `./scripts/chuang-goal-watchdog.sh --once` | 长跑状态有只读入口；不派活、不重启、不提交 |
| memory evidence | 已完成 | `cargo test -q --test memory_maintenance_cli_tests` | 写回仍需 `--approve-writeback`；live rehearsal 不得让子代理直写核心记忆 |

## 第三测试版执行顺序

1. 先看 [第三测试版候选一页入口](./third-test-candidate.md)。
2. 在干净工作树上复跑 `sh scripts/chuang-third-test-smoke.sh`，确认本地候选 wrapper 输出 `third_test_candidate_smoke_ok`。
3. 复跑 `sh scripts/chuang-candidate-verify.sh`，确认 complete-local、live runner rehearsal、live-gaps matrix、operator checklist 只读摘要、goal run status 只读摘要和 provider readiness check 口径一致。
4. 复跑 `sh scripts/chuang-final-verify.sh`，确认本地门禁绿。
5. 复跑 `sh scripts/chuang-live-readonly-preflight.sh`，确认 live 只读预检绿。
6. 复跑 `scripts/chuang-live-runner-readiness-view.sh --json`，确认本地只读视图保留 blocked reason，且 `ready_for_live=false` / `starts_external_worker=false`。
7. 复跑 `cargo run --quiet -- status --json`，确认 `live_readiness.overall_state=local_ready_live_pending`，且四个防混字段为 true。
8. 用 `scripts/chuang-live-operator-receipt-collect.sh --json` 对 base/overlay receipt 做本地收口检查，确认 `can_mark_real_live_ready=false` 和 7 项 service evidence 仍在。
9. 人工确认 provider env 对齐，只报告变量名和 `<set>`。
10. 在 Chuang 专用 Feishu 会话发 `/tools`，确认可见能力与边界符合本文档。
11. 人工执行 Chuang 专用 Feishu live check，优先发 `/health`、`/session` 和一条普通任务，采集 request/session/channel/runtime report receipt。
12. 人工执行 single worker rehearsal，采集 gate/allowlist/report receipt。
13. 复跑 `sh scripts/chuang-final-verify.sh`，确认 live rehearsal 未破坏本地合同。
14. 跑 `git diff --check -- docs/acceptance-next-matrix.md`，确认本文档格式干净。

## 非目标

- 不启用真实 runner 池。
- 不做桌面 mutation。
- 不做真实服务控制 apply。
- 不接入 wiki/GBrain live 写入。
- 不修改 Codex Feishu 或 Hermes。
- 不删除、cleanup、reset、purge、uninstall 任何目标。
- 不把本地 readiness 证据表述成 live 100% 完成。
- 不把 `ga_local_mapped_only`、`desktop_browser_live_gated`、`browser_worker_frozen`、`live_worker_available=false`、`provider_live_request_verified_by_status=false` 或 `real_external_acceptance_pending` 改写成真实 live 完成。
