# Third Test Candidate Quick Entry

更新时间：2026-07-10

这一页只回答四件事：现在哪些已经达到 `local_completion=100%`，哪些必须人工 live check，怎么跑第三测试版候选，和 global real-live 前最后的硬门槛是什么。

## 一句话结论

第三测试版候选不是“所有 live adapter 全开”，而是本地完成后的独立 external acceptance 层：主控收集 provider/env、operator receipt、single worker rehearsal 等现场证据，然后再回到本地 `final verify` 绿。

当前 canonical 结论：项目本地合同已完成，`local_completion=100%`；默认 live 状态仍为 `local_ready_live_pending`。2026-05 的 Feishu 可联系记录只作为历史线索，不代替当前 canonical receipt。第三测试候选的本地 gates 已收口到 `final verify`、live-readonly preflight、live-gaps、candidate verify、operator checklist/receipt 模板结构、provider readiness 只读检查，以及 runtime/report 状态面复验。下一步不是重开本地核心，而是按需收集 Feishu/provider/single worker rehearsal/desktop/browser/wiki/GBrain 七项真实 live receipt。

本地合同新增覆盖包括：checkpoint runtime refs 可恢复，早期 v1 payload 缺少新增字段时仍兼容；session archive 与 searchable summary 使用同一 SQLite 事务，而 remember workflow 在 archive 已提交后若后续 identity / experience / queued dispatch 失败，会显式返回不可盲重试的 `partial_success`；queued spawn/steer 先持久化后提交内存态，恢复标准 `queued-run-N` 后从最大编号继续，非标准 run id 不推进计数器。上述能力不改变 global real-live 仍 pending 的边界。

当前固定状态词：

- `ga_local_mapped_only`：GA 9 tools 已在本地命令面和诊断面映射，但只代表 slot/route 可见。
- `desktop_browser_live_gated`：真实桌面和浏览器动作仍需要 live gate、allowlist、治理和 operator receipt；当前不能写成 desktop/browser live ready。
- `desktop_browser_read_only_observation_ready`：`observe` / `screenshot` 已可作为只读证据回执使用，回执会标明 `read_only=true` 和 `live_gate_required=false`；这不代表 click/input 已经 live ready。
- `live_worker_available=false`：当前 subagent preflight/rehearsal 不启动、不附着真实 worker。
- `real_external_acceptance_pending`：Feishu/provider/single worker rehearsal/desktop/browser/wiki/GBrain 的真实外部验收仍 pending，不能由本地 smoke、`<set>` 或 `/tools` 代替。

## 现在的分层

### local-ready

- `sh scripts/chuang-final-verify.sh`
- `sh scripts/chuang-live-readonly-preflight.sh`
- `bash scripts/chuang-live-gaps-check.sh`
- `sh scripts/chuang-candidate-verify.sh` -> `chuang_candidate_verify_ok`
- `sh scripts/chuang-third-test-smoke.sh` -> `third_test_candidate_smoke_ok`
- `cargo test -q`
- `scripts/chuang-provider-readiness-check.sh`
- `cargo run --quiet -- channel feishu-check --env-file $HOME/.codex-im/chuang-feishu-bridge.env --json`
- `node scripts/chuang-feishu-command-smoke.js`

这些都属于本地可复验门禁，不要求真实 Feishu、不读 secret、不控制服务。`chuang-live-gaps-check.sh` 会输出三段矩阵：`local_contract=ready`、`preflight=ready_but_no_start`、`real_live=pending`，用于防止把本地合同或 ready-but-no-start 预检误写成真实 live。`chuang-candidate-verify.sh` 会把 live-gaps、operator checklist 只读摘要、operator receipt 模板结构断言、goal run status 只读摘要和 provider readiness check 纳入候选门禁；provider readiness check 只读取 `status --json` 的 `provider_readiness`，输出 `<set>/<missing>`，不连接真实 provider。runtime/report 状态面当前固定 `runtime_report_surface=11/26`，runtime event ledger、context compaction、goal/subagent admission refs、tool protocol errors 和 unified execution 摘要都已进入 readiness/status/channel/app-server/health/wrapper 复验面。

本地 gate completion 的含义到这里为止：脚本、模板、诊断面和只读证据链可复验。它不能替代 `service_evidence` / `service_receipts` / `real_live_acceptance.services` 七项真实 evidence，也不能把 `can_mark_real_live_ready=false` 的模板默认值改写成 acceptance 结论。

### 必须人工 live check

- Chuang 专用 Feishu 在 2026-05-16 有历史可联系记录；当前仍需新的可审计 canonical receipt（request_id + transcript ref）。
- 在 Chuang 专用 Feishu 会话发 `/tools`，确认可见能力包含 `/new`、`/session`、`/health`、`/receipt`、`/live-check`、普通文本和图片 OCR 边界。
- Chuang provider env/readiness 对齐，输出只允许 `变量名=<set>`；真实 provider acceptance 还需要单独 live request receipt 或 runtime report id，不能由 `<set>` 代替。
- desktop/browser 的只读观察回执单独跑一轮，确认 `observe` / `screenshot` 回执里有 `read_only=true` 和 `live_gate_required=false`，但不把它写成 click/input live ready。
- 生成 live operator receipt，保留 request_id、operator、时间、允许范围、回退条件、service evidence ref 和结果摘要；`service_evidence` / `service_receipts` / `real_live_acceptance.services` 必须按 Feishu、provider、single worker rehearsal、desktop、browser、wiki、GBrain 七项 1:1 对齐。
- 单个 worker live rehearsal 通过 gate + allowlist + capability routing + report admission。
- live rehearsal 之后再次跑 `sh scripts/chuang-final-verify.sh`，确认本地合同没坏。

## Global Real-Live 前唯一硬门槛

唯一硬门槛不是“再多跑几个 smoke”，而是把上面的人工 live 链闭环做实：

1. Chuang 专用 Feishu live 通道拿到真实 receipt。
2. provider/env readiness 对齐完成，secret 只显示为 `<set>`，并拿到 provider live request receipt 或 runtime report id。
3. operator receipt 完整，七项 service evidence / receipt / acceptance services 同序同名且都有现场结论。
4. 单个 worker live rehearsal 完整。
5. rehearsal 后 `final verify` 仍然通过。

这条链没有闭环前，不能标记 `global_real_live_ready`；但不影响已经通过本地合同和门禁证明的 `local_completion=100%`。

## 建议执行顺序

1. 先跑本地门禁：

```bash
sh scripts/chuang-final-verify.sh
sh scripts/chuang-live-readonly-preflight.sh
bash scripts/chuang-live-gaps-check.sh
sh scripts/chuang-candidate-verify.sh
scripts/chuang-provider-readiness-check.sh
```

2. 再跑只读 operator 预检与回执模板：

```bash
bash scripts/chuang-live-operator-checklist.sh --json
scripts/chuang-live-operator-receipt.sh --json
```

3. 按 runbook 做人工 live check：

```text
/live-check
/tools
/health
/new
/session
```

已联系上的 Feishu 会话优先用 `/tools`、`/health` 和 `/session` 留证据；需要新上下文时再发 `/new`，避免把“能联系上”误判成还缺本地绑定。

4. 只允许单个 worker live rehearsal，不扩成 runner 池。
5. rehearsal 后再跑一次 `sh scripts/chuang-final-verify.sh`。
6. 最后做 `git diff --check -- docs/third-test-candidate.md docs/acceptance-next-matrix.md docs/live-operator-test-runbook.md`。

## 边界

- 不碰 Codex Feishu。
- 不碰 Hermes。
- 不打印 token 或 secret。
- 不做 deletion、cleanup、reset、purge、uninstall。
- 不把 local-ready smoke、provider readiness check、GA 本地映射或 `/tools` 可见能力解释成 100%。
- 不把 `desktop_browser_live_gated`、`live_worker_available=false` 或 `real_external_acceptance_pending` 写成真实 live 完成。

## 快速参考

- [Acceptance Next Matrix](./acceptance-next-matrix.md)
- [Live Operator Test Runbook](./live-operator-test-runbook.md)
