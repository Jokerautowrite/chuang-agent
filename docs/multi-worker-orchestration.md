# Multi-Worker Orchestration

更新时间：2026-05-09

## 目标

把多个子代理的计划、范围和验收拆清楚，并提供受控的本地多 worker 批处理入口。

## 当前边界

```text
GoalRun planning + durable queue + foreground bounded goal step + bounded local run-loop
```

`GoalRun` 继续只负责记录目标、worker plan、scope 和 checkpoint，不自动执行。

`goal step` 是多 worker 层面给 goal 使用的最小执行包装：它在前台运行，只针对一个显式 goal 的已派发 dispatch，按固定 `max-runs` / `max-concurrency` 调用底层 `subagent run-loop`。它的产物应是本次 step 的运行回执和 worker report/admission 证据；是否已经可 checkpoint 仍由后续 `goal collect` 判断。

本阶段验收链先固定为 `goal dispatch -> goal step -> goal collect -> goal checkpoint --from-collect`；`goal collect` 只产出 checkpoint suggestion，不直接写 checkpoint，且会把失败、缺失或身份不匹配的 report 留在阻断证据里。

2026-05-08 增量状态：这条 happy path 已经进入 `scripts/chuang-goal-mode-smoke.sh`，not-ready 负例已经进入 `scripts/chuang-goal-mode-negative-smoke.sh`，两者都纳入 `scripts/chuang-complete-local-smoke.sh` 的本地门禁。多 worker goal-mode 当前已锁住正向闭环和基础负例门禁，避免 not-ready 或坏 report 被误提升为 checkpoint。

运行层的最小并行入口是：

```bash
cargo run -- subagent run-loop --max-concurrency 2 --max-runs 2
```

`--max-concurrency` 支持 `1..8`。每个 worker 仍通过文件队列 claim dispatch，按 capability 匹配任务，执行后写标准 `SubagentReport`，并由主控生成 `ReportAdmission`。

## 约束

- GoalRun 不自动调度 worker；实际执行必须显式调用 `subagent run-loop` 或前台 bounded `goal step`。
- `goal step` 不自动调度新 worker；它只消费该 goal 已派发的 queued dispatch，并且必须有前台批次上限。
- `goal step` 不做 daemon、不后台常驻、不自动续跑下一批。
- `goal step` 不自动写 checkpoint、progress-log、handoff 或 core memory。
- `goal step` 不做 cleanup/delete/purge/reset/release-claim，不删除 dispatch、claim、release 或 report 文件。
- `goal step` 不连接 Feishu，不复用 Codex/Hermes bridge，不控制 Hermes 服务或记忆。
- `goal collect` 只读聚合 manifest 里的 queued reports；只有当 report 齐全、身份匹配且执行成功时，才会返回可用于 `goal checkpoint --from-collect` 的 suggestion。
- `live-runner-readiness-view` 只做只读状态视图，汇总 `status` / `doctor` / `console snapshot` / `app-server health` 里的 runner gate、blocked reason、capability mismatch 和 next action；它不运行 `subagent live-preflight`，不启动 worker，不产生命令级 preflight 证据，也不等于 live runner ready。
- `live-operator-receipt-collect` 只做本地只读回执模板/收集器，记录脱敏占位字段和 evidence refs；它不连接 Feishu/provider/runner，不替代 operator-approved real live evidence。
- scope 必须先定义，不能互相重叠。
- worker 之间只通过计划和报告协作，不共享临时状态。
- command runner 仍必须显式传 `--approve-exec`。
- live external worker pool 仍是后续 audited adapter 边界，本地 run-loop 不连接真实外部平台。
- 真实外部 worker runner 启用前先跑只读 `subagent live-preflight`，确认 live gate、runner allowlist、capability routing、ReportAdmission 证据和 forbidden capability rejection 都可见；该命令不启动真实 worker。

## Live Worker 当前缺口

当前 multi-worker 能证明的是本地队列、bounded `goal step`、`goal collect` 和 live-preflight-only 证据链，不是 live worker 池已经可用。真实 live subagent worker 还缺三层接入件：audited adapter 负责把外部 runner 纳入可审计边界，config 负责固定 runner command、scope、capability 和 stop/timeout，gate 负责默认关闭并要求显式审批。

`live-runner-readiness-view`、`subagent live-preflight` 和真实 live evidence path 的分工要分开看：readiness view 只汇总本地状态面；`subagent live-preflight` 才是命令级 preflight 证据；真实 live evidence 只能来自 operator-approved live run 之后的外部证据 ref、runtime report id 或 live request receipt。前两者都不启动真实 worker，也都不能被解释成 live runner ready。

`live-operator-receipt-collect` 只收口 Feishu/provider/subagent/desktop/browser/wiki/GBrain 七项 receipt 模板字段，保持 `can_mark_real_live_ready=false` 和 `real_live_acceptance.status=not_verified`。它可以指向真实证据 ref，但模板本身不是证据产生动作。

在这三层完成前，`live_runner_rehearsal_smoke_ok` 只能解释为 route/admission/governance 字段可见。它不能解释为真实 worker 已启动、真实桌面/browser 已执行、provider 已 live 调用，或 wiki/GBrain 已接通。

三大 live gates 必须继续默认关闭：

- `provider live`：只读 readiness 可以显示 `<set>/<missing>` 和 blocked reason，但不能发真实 provider 请求。
- `subagent live runner`：preflight 可以验证 allowlist、capability route、ReportAdmission 和 governance receipt，但 `starts_external_worker=false` 仍是默认预期。
- `desktop/browser actuator live action`：GA 9 tools 已 mapped 时只能说明工具槽位和路由可见；真实 desktop/browser live 需要单独 operator receipt、action allowlist 和 observe/apply 边界。

## 今晚可操作入口

这个入口用于从 Feishu `/tools` / `/capabilities` 看到“goal/subagent 本地能力可见”之后，回到本地终端收一份可审计证据。它不改 Feishu bridge、不启动真实服务、不启用真实 runner、不删除或清理文件。

最快本地证据：

```bash
sh scripts/chuang-goal-mode-smoke.sh
sh scripts/chuang-live-runner-rehearsal-smoke.sh
```

预期 marker：

```text
goal_mode_smoke_ok
live_runner_rehearsal_smoke_ok
```

第一条覆盖 `goal dispatch -> goal step -> goal collect` 的本地闭环，并继续证明 checkpoint 只能来自 `--from-collect`。第二条覆盖 live-preflight-only 边界：`starts_external_worker=false`、live gate disabled、runner allowlist/capability route 可见、`ReportAdmission=Accepted/report_validated` 可见、治理审批字段可见。

如果今晚先想看 runner readiness 视图，再决定要不要跑 preflight 或 smoke，先看 `live-runner-readiness-view`；它只读汇总 runner gate、blocked reason、capability mismatch 和 next action，不启动 worker，也不替你跑 `subagent live-preflight`。`subagent live-preflight` 则继续负责命令级预检，验证 gate、allowlist、capability routing、ReportAdmission 和 `starts_external_worker=false`。

如果今晚只想看派活到收集，不写 checkpoint，使用临时目录手工跑到 collect 即停：

```bash
GOAL_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/chuang-tonight-goal-runs.XXXXXX")"
QUEUE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/chuang-tonight-subagent-queue.XXXXXX")"
GOAL_ID="tonight-local-subagent-loop"

cargo run --quiet -- goal plan \
  --root "$GOAL_ROOT" \
  --goal-id "$GOAL_ID" \
  --objective "tonight local check for goal dispatch step collect" \
  --scope "tonight-docs=docs/multi-worker-orchestration.md" \
  --worker "tonight-worker-1|tonight-docs|verify local goal dispatch step collect evidence" \
  --validation "sh scripts/chuang-goal-mode-smoke.sh" \
  --max-subtasks 1 \
  --json

cargo run --quiet -- goal dispatch \
  --root "$GOAL_ROOT" \
  --goal-id "$GOAL_ID" \
  --subagent-queue-root "$QUEUE_ROOT" \
  --json

cargo run --quiet -- goal step \
  --root "$GOAL_ROOT" \
  --goal-id "$GOAL_ID" \
  --subagent-queue-root "$QUEUE_ROOT" \
  --runner fake \
  --max-runs 1 \
  --max-concurrency 1 \
  --json

cargo run --quiet -- goal collect \
  --root "$GOAL_ROOT" \
  --goal-id "$GOAL_ID" \
  --subagent-queue-root "$QUEUE_ROOT" \
  --json
```

collect 输出里只看这几项：

- `ready_to_checkpoint=true` 才代表 dispatch/step/report 收齐；今晚如果不需要写 checkpoint，可以停在这里。
- `missing_run_ids=[]`、`blocked_report_run_ids=[]`、`blocked_report_reasons=[]` 才能继续 checkpoint。
- 任一 blocked 字段非空时，不手工补 checkpoint；把 exact reason 贴回对应 worker。

live-preflight-only 单独证据可以只跑：

```bash
cargo run --quiet -- subagent live-preflight \
  --runner-command scripts/chuang-codex-runner.py \
  --allow-runner-command scripts/chuang-codex-runner.py \
  --requires-capability rehearsal \
  --capability rehearsal \
  --json
```

预期边界是 `ready_for_live=false` 且 `starts_external_worker=false`。这表示 preflight 证据可见，不表示真实 runner 已经启动或允许启动。

如果要看 status / doctor / console / app-server health 对 live runner 的只读状态面，应把它作为 `live-runner-readiness-view` 使用；它只读展示 blocked/next-action，不把状态面提升成 preflight 或 real live evidence。

如果要收集 operator receipt 模板，可以单独跑：

```bash
sh scripts/chuang-live-operator-receipt.sh --json
```

这个 collector 只输出脱敏模板和七项 evidence ref 槽位，不会启动 worker、不会接真实外部服务，也不会把 blocked/not_verified 证据改写成 ready。

## 下一步

1. 把 GoalRun 的 worker plan 继续用作唯一计划入口。
2. 让 `goal step` 只成为 goal-scoped foreground batch wrapper，避免演变成后台 scheduler。
3. 继续收紧真实 runner 的 allowlist、身份校验和 capability routing。
4. 后续再把 live worker pool 接成 audited adapter，而不是塞进核心主链。

2026-05-08 已锁定的负例门禁：

1. `goal checkpoint --from-collect` 遇到 not-ready collect receipt 时必须拒绝写 checkpoint。
2. failed report / identity-mismatched report 必须保留为 blocked evidence，不能让 `ready_to_checkpoint=true`。
3. `goal step` 必须继续由 manifest allowlist 和显式 `max-runs` / `max-concurrency` 约束，不能执行非本 goal run id。

## 2026-05-08 可派活缺口清单

基于当前 `progress-log`、`handoff`、goal-mode 和本文件的状态，"能干活" 还缺最多的不是再跑通 goal happy path，而是真实 worker/live adapter 启用前的可审计任务包。当前本地队列、`goal step`、`goal collect`、正负 smoke 已有门禁；下一阶段最容易跑偏的是把真实 runner 接进来时缺少统一的 allowlist、capability route、审批和 report/admission 验收口径。

优先级排序：

1. 真实 worker 启用前边界：live gate、runner allowlist、dispatch `required_capabilities`、worker 自报 capability、`ReportAdmission` 和 governance receipt 必须端到端可见。
2. 状态面对齐：`status` / `doctor` / `console snapshot` / `app-server health` 对 live runner gate、blocked reason、capability mismatch 和 admission state 的文本/JSON 口径要一致。
3. 失败恢复：缺 report、failed report、identity mismatch、capability mismatch、malformed report 都必须保持 blocked evidence，不能提升为 checkpoint suggestion。
4. 操作员派活体验：每个 worker 任务必须提前写清楚写入范围、禁止事项、验收命令和预期 report 字段，避免 worker 自己猜权限。

### 下一阶段 worker 任务包模板

派给实现 worker 前，主控应把以下字段填完整：

```text
Worker ID:
Objective:
Allowed files:
Forbidden files/services:
Required capability:
Expected dispatch/runner mode:
Expected ReportAdmission state:
Expected governance receipt fields:
Acceptance commands:
Required negative case:
Final report must include:
```

字段约束：

- `Allowed files` 必须是互不重叠的文件或目录范围；没有明确范围就不派活。
- `Forbidden files/services` 必须写明不碰 Hermes、Feishu、secret、真实外部服务、删除/清理/reset/uninstall。
- `Required capability` 必须能映射到 dispatch `required_capabilities`，不能只写自然语言。
- `Expected dispatch/runner mode` 必须区分 fake/local queued runner、command runner rehearsal、live runner preflight、真实 live runner。
- `Expected ReportAdmission state` 必须写明成功路径和至少一个拒绝路径。
- `Acceptance commands` 优先用定向测试；改状态面时再补 `cargo test -q` 或 smoke。

### 6 Worker 派发清单

主控下一轮可以直接按 6 条并行线派活，但每条线必须先落到 `GoalRun` 的 scope/worker plan 里。最低要求：

```text
Goal ID:
Goal root: ./context/goal-runs
Queue root: ./context/subagent-queue
Max subtasks: 6
Worker count: 6
Worker scopes: 6 个互不重叠 scope
Global validation: cargo fmt --all --check; git diff --check; cargo test -q
Step budget: --max-runs 6 --max-concurrency 6
Checkpoint source: --from-collect only after collect ready
```

命令顺序固定为：

```text
goal plan      写入目标、scope、worker plan、validation plan
goal show      检查 worker scope / validation / governance 诊断
goal dispatch  把 6 个 worker plan 扇出成 queued dispatch 和 manifest
goal step      前台 bounded 执行最多 6 个 manifest run id
goal collect   只读收集 report，判断是否 ready_to_checkpoint
goal checkpoint --from-collect  只在 collect ready 时显式写 checkpoint
goal show      复查 checkpoint_log_complete 和最新验证证据
```

每个 worker 的任务卡至少包含：

```text
Worker ID:
Objective:
Allowed files:
Forbidden files/services: 不碰 Hermes/Feishu/secret/真实外部服务；不删除、不清理、不 reset、不卸载。
Required capability:
Expected dispatch/runner mode:
Expected ReportAdmission state:
Expected governance receipt fields:
Acceptance commands:
Required negative case:
Final report must include: changed files, evidence fields, tests run, blocked/remaining gaps.
```

主控验收字段：

- `goal_dispatch_ready=true`、`goal_dispatch_count=6`、`goal_dispatch_manifest_path` 存在。
- `goal_step_checkpoint_recorded=false`、`goal_step_writes_progress_log=false`、`goal_step_writes_handoff=false`。
- `goal_collect_ready_to_checkpoint=true`、`goal_collect_missing_run_ids=none`、`goal_collect_blocked_report_run_ids=none`、`goal_collect_blocked_report_reasons=none`。
- `goal_collect_report_run_ids` 必须覆盖 manifest 里的 6 个 run id；`goal_operability_collect_report_run_ids` 也必须能在 `goal show` 文本面看到。
- `goal_collect_checkpoint_completed_worker_ids` 必须覆盖 6 个 worker。
- `goal_collect_checkpoint_validation_notes` 必须能对应到每个 worker 的报告证据。
- `goal_operability_checkpoint_completed_worker_ids` 和 `goal_operability_checkpoint_validation_notes` 必须在 checkpoint-ready 状态下可见，不能只存在于一次性的 collect 输出里。
- checkpoint 后 `goal_checkpoint_source=collect`，最终 `goal_checkpoint_log_complete=true`。
- `subagent list` 文本面必须能看到 `queue_root`、`dispatch_count`、`report_count`、每个 `run_id` 的 `required_capabilities`、`is_claimed`、`is_claim_stale` 和 `has_report`，方便不看 JSON 时也能判断是否还缺 worker/report 证据。

Blocked evidence 的读取顺序：

1. 先看 `goal_collect_missing_run_ids`：有值说明 worker 未产出 report，继续跑 `goal step` 或让对应 worker 补报告。
2. 再看 `goal_collect_blocked_report_run_ids`：有值说明 report 已存在但被阻断。
3. 对照 `goal_collect_blocked_report_reasons`：常见原因是 report status 不是 success、report identity 和 manifest 不匹配、report admission 被拒绝。
4. 如果 blocked 字段非空，`goal checkpoint --from-collect` 拒绝写入是正确行为；主控只能回派修复，不能手工伪造 completed worker。

### Live Runner Preflight 派活 Runbook

这一节是下一阶段统一复制给 worker 的 runbook。它只覆盖真实 runner 启用前的只读 readiness view、命令级 preflight、capability mismatch 和 blocked evidence 验收；默认不启动真实 live runner，不连接外部平台，不接 Feishu/Hermes，不读取或输出 secret，不做删除、清理、reset 或卸载。

适用目标：

```text
Goal: harden live runner preflight before enabling a real worker pool
Purpose: prove live gate, runner allowlist, capability routing, governance receipt, ReportAdmission, status surfaces, and blocked evidence all stay auditable before any live worker can start
Default runner mode: live runner preflight / local tests only
Disallowed mode: real live runner start
```

6 线派活时，主控先把以下任务映射成 6 个互不重叠 `GoalRun` scope。文件范围按实际实现点填写；没有明确 allowed files 时不要派发。

```text
Worker ID: live-preflight-status
Objective: make status/doctor/console/app-server health show the same live gate, readiness reason, blocked reason, capability mismatch, and next action without running preflight.
Required capability: live_runner_readiness_view
Expected negative case: live gate closed or capability route missing still shows ready_for_live=false.

Worker ID: live-preflight-allowlist
Objective: lock runner command allowlist and disabled-by-default evidence before any real runner can start.
Required capability: live_runner_allowlist_audit
Expected negative case: unallowlisted runner path stays blocked and starts_external_worker=false.

Worker ID: live-preflight-capability-route
Objective: verify dispatch required_capabilities and worker --capability matching are visible and enforced.
Required capability: live_runner_capability_route
Expected negative case: capability mismatch exposes missing_capabilities and ready_for_live=false.

Worker ID: live-preflight-admission
Objective: keep ReportAdmission accepted/rejected state, reason_code, and upstream reason visible for reports from runner rehearsal.
Required capability: report_admission_audit
Expected negative case: malformed, identity-mismatched, or failed report stays rejected/blocked.

Worker ID: live-preflight-governance
Objective: surface governance receipt fields that prove runner start approval is separate from worker internal actions.
Required capability: governance_receipt_audit
Expected negative case: missing approval receipt or forbidden capability keeps live preflight not ready.

Worker ID: live-preflight-goal-collect
Objective: prove goal collect converts missing/failed/mismatched worker outputs into blocked evidence and never into checkpoint material.
Required capability: goal_collect_blocked_evidence
Expected negative case: goal checkpoint --from-collect refuses when any blocked evidence field is non-empty.
```

每个 worker 的任务卡必须按这个模板填写，不允许只写自然语言目标：

```text
Worker ID:
Objective:
Allowed files:
Forbidden files/services: no Hermes, no Feishu bridge, no secret output, no real external service, no real live runner start, no deletion/cleanup/reset/uninstall.
Required capability:
Expected dispatch required_capabilities:
Expected worker capabilities:
Expected dispatch/runner mode: live runner preflight / local tests only.
Expected live preflight fields:
Expected ReportAdmission state:
Expected governance receipt fields:
Expected blocked evidence fields:
Acceptance commands:
Required negative case:
Final report must include: changed files, evidence fields, tests run, negative case result, blocked/remaining gaps.
```

字段验收口径：

- `Required capability`、`Expected dispatch required_capabilities` 和 `Expected worker capabilities` 必须能一一对上；capability mismatch 是本阶段必须覆盖的负例，不是可选测试。
- `Expected readiness view fields` 至少覆盖 `live_runner_rehearsal` 的 `ready_for_live=false` 口径、`starts_external_worker=false` 口径、`capability_mismatch_blocks_live=true`、`blocked_reason` 和 `next_action`，但不得把它写成 preflight 已执行。
- `Expected live preflight fields` 至少覆盖 `ready_for_live=false`、`starts_external_worker=false`、`missing_capabilities`、runner allowlist 状态、live gate 状态、forbidden capability rejection 和 next action。
- `Expected ReportAdmission state` 至少覆盖一条 accepted 路径和一条 rejected 路径；rejected 路径必须能看到 `reason_code`，有上游协议原因时还要保留 `upstream_reason_code`。
- `Expected governance receipt fields` 至少覆盖 action id、decision、reason/source、approval boundary，并明确主控允许启动 runner 不等于 worker 内部动作自动获批。
- `Expected blocked evidence fields` 至少覆盖 `goal_collect_missing_run_ids`、`goal_collect_blocked_report_run_ids`、`goal_collect_blocked_report_reasons`、`goal_collect_ready_to_checkpoint=false`。
- `Expected read-only status fields` 至少覆盖 `goal_operability_collect_report_run_ids`、`goal_operability_checkpoint_completed_worker_ids`、`goal_operability_checkpoint_validation_notes`，以及 `subagent list` 的 claim/report/capability 字段。
- `Acceptance commands` 优先写定向测试和 `git diff --check`；跨状态面改动再补 `cargo test -q` 或 `sh scripts/chuang-complete-local-smoke.sh`。

主控验收 live preflight 时按这个顺序看，不要从 checkpoint 反推：

```text
1. live gate / runner allowlist: disabled-by-default or explicitly allowed state is visible.
2. dispatch capability route: dispatch required_capabilities is present and normalized.
3. worker capability route: worker --capability is present and mismatches are named in missing_capabilities.
4. preflight decision: ready_for_live=false for closed gate, missing route, mismatch, forbidden capability, or missing approval evidence.
5. start boundary: starts_external_worker=false for every preflight-only run.
6. admission boundary: ReportAdmission accepted/rejected state and reason codes are visible.
7. collect boundary: goal collect keeps missing/failed/mismatched reports in blocked evidence.
8. checkpoint boundary: goal checkpoint --from-collect only runs after ready_to_checkpoint=true and blocked evidence is empty.
```

Capability mismatch 的标准负例：

```text
Dispatch required_capabilities: browser_control
Worker capabilities: codex_runner
Expected result: ready_for_live=false
Expected evidence: missing_capabilities includes browser_control; starts_external_worker=false; no checkpoint suggestion; corresponding run id appears in blocked evidence if a report is present but not acceptable.
```

Blocked evidence 的复派规则：

```text
missing_run_ids non-empty: rerun bounded goal step or ask the worker to produce a report.
blocked_report_run_ids non-empty: inspect blocked_report_reasons, then return the exact reason to the worker.
blocked_report_reasons includes identity mismatch: fix report/manifest identity; do not rewrite checkpoint by hand.
blocked_report_reasons includes failed report: worker must report remediation or a scoped failure; main process does not mark it completed.
ReportAdmission rejected: fix protocol/report format first; do not parse stderr or free text as success.
ready_to_checkpoint=false: goal checkpoint --from-collect must fail and that failure is expected.
```

### 首个可派活低风险任务

建议先派一个 worker 只做只读状态面一致性，不接真实 runner：

```text
Worker ID: live-runner-readiness-view
Objective: 对齐 status/doctor/console/app-server health 的 live runner gate、blocked reason、capability mismatch 和 next action 文本/JSON 字段；不运行 preflight、不产生真实 live evidence。
Allowed files: src/kernel_status.rs, src/cli_output.rs, src/cli_status.rs, src/cli_doctor.rs, src/cli_console.rs, src/app_server.rs, tests/*status*/*doctor*/*console*/*app_server* 相关文件
Forbidden files/services: 不碰 Hermes/Feishu，不改 runner 执行逻辑，不启动真实 worker，不写 data/skills，不删除/清理任何文件。
Required capability: live_runner_readiness_view
Expected dispatch/runner mode: local tests only; no live runner start.
Expected ReportAdmission state: 不新增 admission 语义，只展示已有 blocked/ready reasons。
Acceptance commands: cargo fmt --all; cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests --test cli_console_tests --test app_server_tests
Required negative case: capability route 缺失或不匹配时，所有只读面都显示 not ready / blocked reason，不能显示 ready_for_live=true。
Final report must include: changed files, fields added/renamed, tests run, remaining live-runner gaps.
```

这个任务不连接外部服务、不扩大权限、不写长期记忆，是下一阶段把真实 worker 接入前最小的可派活验收面。
