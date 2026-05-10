# Acceptance Next Matrix

更新时间：2026-05-10

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

今晚候选验收另有 dirty-tree friendly 入口：`sh scripts/chuang-candidate-verify.sh`。它串联 complete-local smoke、live runner rehearsal smoke、`live-gaps` 矩阵、operator checklist 只读摘要、operator receipt 模板结构断言、goal run status 只读摘要和 provider readiness check；provider readiness check 只读取 `status --json` 的 `provider_readiness`，输出 `<set>/<missing>` 状态，不连接真实 provider、不打印 secret。缺 provider env 时按候选现场状态报告 blocker，但不会伪装成本地合同失败。

第三测试版候选不是“所有 live adapter 全开”，而是 100% 前最后一跳：用最小真实链路证明老爸可以通过 Chuang 专用 Feishu live 通道发起请求，主控能拿到 provider/env 状态、operator receipt、单个子代理 live rehearsal 证据，并最终回到本地 verify 绿。真实 runner 池、桌面 mutation、服务控制、wiki/GBrain live 仍后置，不纳入第三测试版必须项。

当前 acceptance 口径必须区分“已 mapped/已 preflight”和“已 live”。GA 9 tools 已 mapped 只代表工具槽位、命令面和能力边界可见；真实 desktop/browser live 仍缺证据。live subagent worker 仍需要 audited adapter、config 和 gate 三件套后才能启用；三大 live gates 默认关闭，分别覆盖 provider live、subagent live runner、desktop/browser actuator live action。Feishu、provider、single subagent rehearsal、desktop、browser、wiki、GBrain 都需要各自的真实 live receipt，不能由本地 readiness 或 `<set>` 状态代替。

状态面单一入口：`status --json` 的 `live_readiness` 固定复述这些词，验收矩阵只引用同一组词。`ready` / `local-ready` 只表示本地合同、smoke、诊断面或只读预检已通过，不表示真实 live receipt 已完成。

固定状态词：

| 状态词 | 当前值 | 含义 | 不能误报成 |
| --- | --- | --- | --- |
| `ga_local_mapped_only` | true | GA 9 tools 已完成本地 slot、route、命令面和诊断面映射 | 真实 desktop/browser live 已验收 |
| `desktop_browser_live_gated` | true | 真实桌面/浏览器动作仍在 live gate、allowlist、治理和 receipt 之后 | actuator live action ready |
| `browser_worker_frozen` | true | 旧 BrowserWorker 线冻结且不在主执行路径 | browser automation ready 或已恢复 |
| `live_worker_available` | false | 当前 subagent preflight/rehearsal 不启动、不附着真实 worker | runner 池可用或 live worker 已上线 |
| `provider_live_request_verified_by_status` | false | `status --json` 只报告 provider 配置/readiness，不发真实 provider 请求 | provider live 已验收 |
| `real_external_acceptance_pending` | true | Feishu/provider/single subagent rehearsal/desktop/browser/wiki/GBrain 真实外部验收仍需人工 receipt | 第三测试 100% 完成 |
| `ready/local-ready` | local only | 本地合同、smoke、诊断面或只读 preflight 通过 | live-ready、external acceptance 完成 |

## 第三测试版候选 Acceptance

| 项目 | 判定 | 验收方式 | 100% 前是否必须人工验证 | 边界 |
| --- | --- | --- | --- | --- |
| live/readiness 状态面 | `local_ready_live_pending` | `cargo run --quiet -- status --json` -> `live_readiness.overall_state=local_ready_live_pending`，并固定 `mapped_does_not_mean_live=true / gated_does_not_mean_ready=true / frozen_does_not_mean_ready=true / ready_does_not_mean_live=true` | 否，自动复验即可 | 状态面只收口术语；不连接真实服务、不启动 worker、不把 local-ready 当 live-ready |
| third-test candidate wrapper | ready | `sh scripts/chuang-third-test-smoke.sh` -> `third_test_candidate_smoke_ok` | 否，自动复验即可 | 只串本地门禁和只读摘要；operator env blocked 可见但不让本地合同失败 |
| candidate verify wrapper | ready | `sh scripts/chuang-candidate-verify.sh` -> `chuang_candidate_verify_ok`，或明确报告 provider non-live block | 否，自动复验即可 | dirty-tree friendly；覆盖 live-gaps、operator checklist 和 goal run status 只读摘要；不连接真实 provider/Feishu；provider env 缺失只作为候选现场 blocker |
| live-gaps matrix | ready | `bash scripts/chuang-live-gaps-check.sh` -> `marker=live_gaps_check_ok`；`--json` 输出 `local_contract=ready / preflight=ready_but_no_start / real_live=pending` | 否，自动复验即可 | 只读 `status --json` 和 `subagent live-preflight --json`；不启 live gate、不启动 worker、不连接真实服务，provider 只显示 `<set>/<missing>` |
| final verify 本地门禁 | ready | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 否，自动复验即可 | 证明本地合同闭环，不证明 live Feishu 或真实 runner |
| live-readiness 只读预检 | local-preflight-ready | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | 否，自动复验即可 | 只读预检，不连接真实 Feishu、不读 secret、不控制服务；不等于 live-ready |
| Feishu `/tools` 可见能力 | ready | 在 Chuang 专用 Feishu 会话发送 `/tools` 或 `/capabilities`，可见 `/new`、`/session`、`/health`、`/receipt`、`/live-check`、普通文本和图片 OCR 边界 | 否，已作为 bridge 命令面可复验；live 侧可继续截图/receipt 留证 | 只展示当前能力与边界，不执行本地检查、不修改服务、不打印 secret |
| operator receipt template | ready | `scripts/chuang-live-operator-receipt.sh --json` 输出 `request_id`、`approval_scope`、`rollback_condition`、`readonly_boundaries`、`service_evidence`、`service_receipts` 和 `real_live_acceptance` | 否，自动复验模板结构即可 | 只生成模板，不连接服务；`can_mark_real_live_ready=false`，不能代替人工 evidence |
| GA 9 tools mapped | `ga_local_mapped_only` | `/tools` / `/capabilities` 和本地诊断面显示 GA 9 工具映射、scope 和边界 | 否，自动和人工查看均可 | 只证明 mapped/routed；真实 desktop/browser live 仍需单独 receipt 和 action allowlist |
| desktop/browser live gate | `desktop_browser_live_gated` | `desktop_browser_live_gated=true`，等待 action allowlist、治理审批和 operator receipt | 是 | 当前不是 desktop/browser live ready，不允许由 mapped tools 或 dry-run adapter 代替 |
| BrowserWorker old path | `browser_worker_frozen` | `status --json` -> `live_readiness.browser_worker_frozen=true` | 否，自动复验即可 | 冻结是排除边界，不是 browser automation live-ready |
| 人工 Feishu live check | candidate | 老爸用 Chuang 专用 Feishu 通道发 `/health`、`/session` 和一条普通测试消息，确认 app-server/session/channel 有真实 receipt | 是 | 只用 Chuang 专用 bot 和 env；不碰 Codex Feishu、不碰 Hermes、不打印 token |
| provider env 对齐 | readiness-only | `scripts/chuang-provider-readiness-check.sh` 读取 `status --json`，并在存在时自动吸收标准 `CHUANG_PROVIDER_ENV_FILE`；人工确认 Chuang provider env 变量存在且配置名一致；输出只允许 `<set>/<missing>` | 是 | 不连接真实 provider；不在聊天、日志、文档或 patch 中泄露 secret；无 fallback 时必须显式报错；`provider_live_request_verified_by_status=false` |
| live operator receipt | candidate | 人工执行 live cutover checklist，保存 request_id、operator、时间、允许范围、回退条件、service evidence ref 和结果摘要 | 是 | receipt 只记录审计元数据，不记录凭证、验证码或私密正文；模板本身不能标记 real live ready |
| single subagent live rehearsal | candidate | 在 live gate + allowlist 下只跑一个子代理 rehearsal，确认 report/proposal 被主控接收 | 是 | 单 worker、bounded、可停止；子代理不能直接写核心记忆，不能扩大成 runner 池 |
| final verify after live rehearsal | candidate | live rehearsal 后再次运行 `sh scripts/chuang-final-verify.sh` 和本文档 diff check | 是 | live 尝试不能破坏本地合同；失败时先停在诊断，不做 cleanup/reset |

## 100% 前必须人工验证

这些项目是从第三测试版候选走向 100% 前的真实证据，不应由本地 smoke 冒充：

1. Chuang 专用 Feishu live 通道真实收发一次，并拿到可审计 receipt。
2. Chuang provider env 与运行配置对齐，所有 secret 只显示为 `<set>`。
3. live operator receipt 完整记录审批范围、执行人、时间、request_id、结果、回退条件和 Feishu/provider/subagent/desktop/browser/wiki/GBrain 的 evidence ref。
4. 单个子代理 live rehearsal 通过 gate、allowlist、capability routing 和 report admission。
5. live rehearsal 后本地 final verify 仍通过。

## 仍然后置

| 后置能力 | 后置原因 | 最早进入条件 | 验收边界 |
| --- | --- | --- | --- |
| 真实 runner 池 | 多 worker 并发会放大外部进程、登录态和任务副作用 | 单 subagent live rehearsal 有 receipt，stop/timeout/report admission 都可审计 | 先单 worker，再 bounded 并发；不把 rehearsal 结果解释成 runner 池 ready |
| 桌面 mutation | 涉及真实 UI、登录态、验证码和不可逆操作 | observe-only、action allowlist、验证码规则和人工 receipt 稳定 | observe 可先行；点击/提交/修改类动作后置 |
| 服务控制 apply | 涉及 start/stop/restart/change_model 等服务扰动 | Chuang-only allowlist、dry-run receipt、人工审批范围明确 | 不允许任意 systemd；不含 Codex Feishu 或 Hermes |
| wiki/GBrain live | 涉及外部知识库权限、检索质量和写入策略 | 本地 knowledge search provenance/evidence 稳定，live 只读账号和审计面确认 | 先只读检索；不自动写外脑 |

## 当前证据状态

| 证据面 | 最新状态 | 已验证命令 | 第三测试版含义 |
| --- | --- | --- | --- |
| live/readiness status surface | `local_ready_live_pending` | `cargo run --quiet -- status --json` -> `live_readiness` | 状态面固定区分 mapped/gated/frozen/ready/live，不把 local-ready 冒充 live-ready |
| final verify | 已完成 | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 本地门禁可作为 live 前后对照 |
| live-readiness preflight | local-preflight-ready | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | live 前只读排查入口已收口，但不是 live-ready |
| live-gaps matrix | 已完成 | `bash scripts/chuang-live-gaps-check.sh` -> `marker=live_gaps_check_ok` | 明确区分本地合同 ready、preflight ready-but-no-start、real live pending；不连接真实服务、不启动 worker |
| candidate verify | 已完成 | `sh scripts/chuang-candidate-verify.sh`，并包含 `scripts/chuang-live-gaps-check.sh`、operator checklist 只读摘要、operator receipt 模板结构断言、goal run status 只读摘要和 `scripts/chuang-provider-readiness-check.sh` | 本地候选门禁已经覆盖 live gaps、operator/goal 只读摘要、receipt 模板结构和 provider readiness 只读状态；缺 env 会显式报告 blocker |
| Feishu evidence | 已完成 | `node --check scripts/chuang-feishu-live-preflight.js && node scripts/chuang-feishu-live-preflight-smoke.js && node scripts/chuang-feishu-command-smoke.js` | 本地命令和诊断链已可复验；`/tools` 已列出当前可见能力与边界 |
| Feishu live contact | 已完成 | 老爸在 Chuang 专用 Feishu 会话中确认已联系上；本地 `chuang-feishu-bot.service` active | 说明 bridge 已挂上；后续重点是 `/health`、`/session`、普通任务 runtime report 和 receipt 证据 |
| provider evidence | 已完成 | `cargo test -q --test slot_registry_tests --test runtime_report_tests`；`scripts/chuang-provider-readiness-check.sh` | fallback/capacity/retryable 诊断合同已存在；provider readiness check 已进入候选门禁；live 前仍要人工确认 env 对齐 |
| subagent evidence | 已完成 | `cargo test -q --test cli_subagent_live_preflight_tests` | gate/allowlist/capability/report admission rehearsal 已有；下一步只允许单 worker live rehearsal |
| GA 9 tools evidence | 已完成但非 live | `/tools` / `/capabilities` 和本地诊断面可见 mapped 工具 | `ga_local_mapped_only=true`；映射完成不等于真实 desktop/browser live；缺真实桌面/browser action receipt |
| desktop/browser live evidence | pending | `desktop_browser_live_gated=true`，待 action allowlist、governance receipt 和 operator receipt | 真实桌面/浏览器验收仍 pending；不能由 mapped 工具或本地 dry-run 代替 |
| BrowserWorker evidence | frozen | `status --json` -> `live_readiness.browser_worker_frozen=true` | 冻结旧路径，不表示 browser live-ready |
| wiki/GBrain evidence | 后置缺 live | 待补真实只读账号、provenance/evidence 和 operator receipt | 本地知识检索口径不能代替 wiki/GBrain live 接通证据 |
| console/watchdog evidence | 已完成 | `cargo test -q --test cli_console_tests` 和 `./scripts/chuang-goal-watchdog.sh --once` | 长跑状态有只读入口；不派活、不重启、不提交 |
| memory evidence | 已完成 | `cargo test -q --test memory_maintenance_cli_tests` | 写回仍需 `--approve-writeback`；live rehearsal 不得让子代理直写核心记忆 |

## 第三测试版执行顺序

1. 先看 [第三测试版候选一页入口](./third-test-candidate.md)。
2. 在干净工作树上复跑 `sh scripts/chuang-third-test-smoke.sh`，确认本地候选 wrapper 输出 `third_test_candidate_smoke_ok`。
3. 复跑 `sh scripts/chuang-candidate-verify.sh`，确认 complete-local、live runner rehearsal、live-gaps matrix、operator checklist 只读摘要、goal run status 只读摘要和 provider readiness check 口径一致。
4. 复跑 `sh scripts/chuang-final-verify.sh`，确认本地门禁绿。
5. 复跑 `sh scripts/chuang-live-readonly-preflight.sh`，确认 live 只读预检绿。
6. 复跑 `cargo run --quiet -- status --json`，确认 `live_readiness.overall_state=local_ready_live_pending`，且四个防混字段为 true。
7. 人工确认 provider env 对齐，只报告变量名和 `<set>`。
8. 在 Chuang 专用 Feishu 会话发 `/tools`，确认可见能力与边界符合本文档。
9. 人工执行 Chuang 专用 Feishu live check，优先发 `/health`、`/session` 和一条普通任务，采集 request/session/channel/runtime report receipt。
10. 人工执行 single subagent live rehearsal，采集 gate/allowlist/report receipt。
11. 复跑 `sh scripts/chuang-final-verify.sh`，确认 live rehearsal 未破坏本地合同。
12. 跑 `git diff --check -- docs/acceptance-next-matrix.md`，确认本文档格式干净。

## 非目标

- 不启用真实 runner 池。
- 不做桌面 mutation。
- 不做真实服务控制 apply。
- 不接入 wiki/GBrain live 写入。
- 不修改 Codex Feishu 或 Hermes。
- 不删除、cleanup、reset、purge、uninstall 任何目标。
- 不把本地 readiness 证据表述成 live 100% 完成。
- 不把 `ga_local_mapped_only`、`desktop_browser_live_gated`、`browser_worker_frozen`、`live_worker_available=false`、`provider_live_request_verified_by_status=false` 或 `real_external_acceptance_pending` 改写成真实 live 完成。
