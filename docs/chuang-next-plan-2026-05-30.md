# Chuang Next Plan - 2026-05-30

## Current State

Chuang has completed the first two real receipt slices, implemented the Feishu readonly collector, implemented the browser_read readonly collector, added the first wiki read-only HTTP adapter slice, and completed the desktop action rehearsal receipt. It is ready for the next implementation decision after commit.

- Branch: `main`
- Remote: `origin/main`
- Current head before local receipt slice: `09f5d57 docs(ops): archive sub2 operation artifacts`
- Git state before this slice: local `main` was synced with `origin/main`; after this slice it should be committed locally before any push.
- Latest known validation after repo整理:
  - `git diff --check` passed
  - `cargo test -q` passed
  - `sh scripts/chuang-third-test-smoke.sh` passed with `third_test_candidate_smoke_ok`
  - after the provider receipt slice, `cargo test -q` also passed, and the committed tree was rerun through `sh scripts/chuang-third-test-smoke.sh` with `third_test_candidate_smoke_ok`.
  - after the Gap 2 single-worker rehearsal slice, targeted receipt tests, mismatch regression, live runner smoke, full `cargo test -q`, `git diff --check`, and post-commit `sh scripts/chuang-third-test-smoke.sh` passed.
  - after the Gap 3 Feishu readonly receipt collector slice, `bash -n scripts/chuang-feishu-live-receipt.sh`, `bash scripts/chuang-feishu-live-receipt.sh --json`, `cargo test -q --test feishu_live_receipt_tests`, `node scripts/chuang-feishu-command-smoke.js`, full `cargo test -q`, and `git diff --check` passed.
  - after the Gap 4A browser_read readonly receipt collector slice, `bash -n scripts/chuang-browser-read-live-receipt.sh`, `bash scripts/chuang-browser-read-live-receipt.sh --json`, `cargo test -q --test browser_read_live_receipt_tests`, full `cargo test -q`, and `git diff --check` passed.
  - after the Gap 5A wiki read-only HTTP adapter slice, `rustfmt --edition 2021 src/knowledge_read.rs tests/knowledge_read_tests.rs`, `cargo test -q --test knowledge_read_tests`, and `git diff --check` passed.
  - after the Gap 4B desktop action rehearsal receipt slice, `bash -n scripts/chuang-desktop-action-rehearsal-receipt.sh`, `bash scripts/chuang-desktop-action-rehearsal-receipt.sh --json`, `rustfmt --edition 2021 tests/desktop_action_rehearsal_receipt_tests.rs`, `cargo test -q --test desktop_action_rehearsal_receipt_tests`, `git diff --check`, and full `cargo test -q` passed.

The project is still **local-gate-ready**, not **real-live-ready**.

`cargo run -q -- status --json` reports the configured provider as `openai_compatible` / `cliproxy-local`, model `gpt-5.5`, and `api_key_state=<set>`. Status alone only proves readiness, but this slice added and ran a real provider receipt command.

## What Is Already Solid

1. Local gate chain
   - MVP / complete-local / candidate / third-test smoke chain is in place.
   - Runtime report surface remains the main observability gate.
   - Readiness surfaces explicitly separate local readiness from real live acceptance.

2. Provider adapter
   - OpenAI-compatible provider has moved to `/v1/responses`.
   - Legacy chat-completions response parsing remains as fallback.
   - Request previews and metadata should continue to avoid printing secrets.

3. Subagent and evolution contract
   - `SubagentReport.skill_proposals` is now a surfaced contract field.
   - Dry-run skill proposals can be produced and reviewed.
   - Skill solidification is still not automatic.

4. Readiness boundaries
   - `browser_read` can distinguish unavailable vs live-ready CDP read state.
   - `knowledge_read` can distinguish local preview, preflight-ready, and adapter missing.
   - `ReadonlyHttpKnowledgeReadAdapter::new_wiki` now exists as the first audited read-only wiki adapter; it is not yet wired to a real endpoint receipt.
   - `subagent_live_worker.enabled=false` remains the default boundary.

5. Feishu surface
   - Feishu turn summary has readiness wording to avoid claiming live-ready from local gates.
   - Codex Feishu and Hermes must remain separate.

## Main Gaps

### Gap 1 - Provider Live Receipt - Completed 2026-05-30

Goal: prove the configured `cliproxy-local` provider can make one minimal, non-sensitive `/v1/responses` request through the Chuang provider path.

Acceptance evidence:

- Request path is `/v1/responses`.
- Response is parsed by the Responses path.
- No API key, token, or raw secret is printed.
- Receipt records timestamp, provider kind, model, request path, status, and redacted response summary.
- Failure mode is structured and does not silently fall back to fake/stub.

Current evidence:

```bash
bash scripts/chuang-provider-live-request-receipt.sh --json --input 'provider live receipt probe: reply with ok only'
```

Observed result: `ok=true`, `request_path=/v1/responses`, `status_code=200`, `provider_response_ok=true`, `provider_fallback_used=false`, `runtime_report_id=report-turn-1`, `api_key_state=<set>`, and `response_summary=chars=2 redacted=true`.

### Gap 2 - Single Live Worker Rehearsal - Completed 2026-05-30

Goal: prove one bounded live worker can go through spawn -> report -> collect -> admission refs without bypassing governance.

Acceptance evidence:

- Worker starts only after explicit configuration/approval.
- Required capability mismatch is rejected before worker start.
- A successful rehearsal produces a structured `SubagentReport`.
- `goal collect` shows accepted report refs.
- No recursive spawn unless separately approved.

Current evidence:

```bash
bash scripts/chuang-live-runner-rehearsal-receipt.sh --json
```

Observed result: `receipt_kind=single_worker_rehearsal_live_receipt`, `ran_count=1`, `max_runs=1`, `max_concurrency=1`, `report.status=Success`, `collect.admission_status=Accepted`, `collect.admission_reason_code=report_validated`, and `real_live_acceptance.global_real_live_ready=false`.

Safety regression remains in place:

```bash
cargo test -q --test cli_subagent_dispatch_tests cli_subagent_run_loop_rejects_capability_mismatch_before_worker_start
```

### Gap 3 - Feishu Live Receipt

Goal: prove Chuang's own Feishu channel can send/receive through the intended identity without borrowing Hermes or the Codex bridge's internal credentials.

Acceptance evidence:

- Chuang Feishu identity and route are explicit.
- Send/receive receipt is stored as evidence.
- No Feishu tokens are printed.
- Hermes remains untouched.

Current evidence:

```bash
bash scripts/chuang-feishu-live-receipt.sh --json
```

Observed current result: `acceptance_status=blocked`, blocker `missing_bridge_event_log`. The collector is implemented and tested, but current runtime evidence is still missing a recent Chuang Feishu inbound/outbound event pair. It records only sanitized event refs, counts, env var states, session state summary, and preflight summary.

Implemented behavior:

- If recent event log evidence has both `inbound` and `outbound`/`command`/`outbound_format`, the receipt reports `acceptance_status=verified`.
- If event log evidence is missing or does not contain an inbound/outbound pair, the receipt reports structured blockers.
- The receipt keeps `connects_real_feishu=false`, `can_mark_real_live_ready=false`, and `global_real_live_ready=false`; it is a readonly evidence collector, not an active sender.

Recommended next action:

Produce or locate a recent Chuang Feishu bridge event log containing one inbound message and one outbound/command formatting event, then rerun the receipt. Do not wire around this with Hermes, Codex Feishu credentials, or active sends unless explicitly approved.

### Gap 4 - Browser/Desktop Boundary - Completed For Code Receipts 2026-05-30

Goal: keep browser read and desktop action separate.

Acceptance evidence:

- Browser read receipt only records audited URL/title/DOM evidence.
- Desktop action receipt is separate and goes through actuator/governance.
- No action is inferred from read readiness.

Current browser_read evidence:

```bash
bash scripts/chuang-browser-read-live-receipt.sh --json
```

Observed current result: `acceptance_status=blocked`, blockers `browser_read_adapter_unavailable` and `missing_chuang_cdp_port`. The collector is implemented and tested, but current runtime evidence is still missing a configured reachable CDP port. If `CHUANG_CDP_PORT` is set, the collector reads only CDP `/json` metadata and emits sanitized refs, host/scheme, target counts, and title length.

Implemented behavior:

- Browser read receipt never performs browser actions, desktop actions, provider calls, wiki/GBrain calls, or core memory writes.
- It never reads DOM via WebSocket in the receipt path; DOM reading remains a separate adapter capability and should need its own evidence before being treated as live.
- It keeps `can_mark_real_live_ready=false` and `global_real_live_ready=false`.

Recommended next action:

Run a controlled Chrome/Chromium CDP readonly session if verified browser_read evidence is needed. Do not infer desktop action readiness from browser_read.

Current desktop action rehearsal evidence:

```bash
bash scripts/chuang-desktop-action-rehearsal-receipt.sh --json
```

Observed result: `receipt_kind=desktop_action_rehearsal_receipt`, `action=open_app`, `app_name=Chrome`, `uses_actuator_adapter=true`, `uses_allowlist=true`, `audit_label=actuator.operation.live`, `required_env=CHUANG_REAL_ACTUATOR_ENABLE`, `dry_run=true`, `real_execution=false`, `performs_desktop_action=false`, `governance.action_kind=LocalDesktopInteraction`, and `global_real_live_ready=false`.

Implemented behavior:

- Desktop action rehearsal uses the real command actuator adapter and allowlist.
- The script explicitly closes the live gate for the adapter child process with `env -u CHUANG_REAL_ACTUATOR_ENABLE`.
- It proves the actuator/governance/audit boundary, not real desktop execution.
- It does not connect to provider, Feishu, wiki, or GBrain, and does not modify the repo.

### Gap 5 - Wiki/GBrain Read-Only Adapter - Partially Completed 2026-05-30

Goal: connect external knowledge as audited read-only adapters.

Acceptance evidence:

- Endpoint and token env are detected without printing secrets.
- Adapter returns provenance.
- Reads do not write core memory automatically.
- Status says read-only live adapter is available only after real receipt.

Current evidence:

- `ReadonlyHttpKnowledgeReadAdapter::new_wiki` can issue a read-only HTTP `POST` to a wiki endpoint.
- Request body includes `source/query/limit/read_only=true`.
- Response `hits` or `results` are parsed into `KnowledgeReadHit` with provenance.
- Receipt redacts token and records `read_only=true` / `writes_automatically=false`.
- GBrain remains explicitly unavailable in this slice.
- Non-2xx errors are structured and do not echo response body/token.

Validation:

```bash
cargo test -q --test knowledge_read_tests
```

Recommended next action:

Either wire a real wiki endpoint/token env into a separate receipt script for live evidence, or keep Gap 5A as code-only and proceed to Gap 4B desktop action rehearsal. Do not claim real Wiki/GBrain live-ready until a real endpoint receipt exists.

## Recommended Order

1. Feishu live receipt evidence, if a recent Chuang event log becomes available
2. Controlled CDP browser_read evidence, if a CDP port is intentionally started
3. Wiki live receipt script or GBrain read-only adapter
4. Skill proposal review -> manual solidify path

Provider live receipt, single-worker rehearsal, Feishu readonly collector code, browser_read readonly collector code, desktop action rehearsal receipt, and wiki read-only adapter code are now done; keep them as regression surfaces, but do not spend the next slice there unless they fail or new live evidence is available.

## Do Not Do Yet

- Do not claim real live-ready from `status`, `doctor`, candidate, or third-test output.
- Do not enable recursive subagents.
- Do not auto-solidify skills.
- Do not wire wiki/GBrain writes into core memory.
- Do not touch Hermes unless explicitly requested.
- Do not delete, cleanup, reset, uninstall, or purge anything without exact approval.

## Handoff For Next Operator

Start here:

```bash
cd /home/user/projects/chuang-agent
git status --short --branch
cargo run -q -- status --json
sh scripts/chuang-third-test-smoke.sh
```

If the tree is clean and third-test still passes, continue with **Feishu live evidence**, **controlled CDP browser_read evidence**, or **Gap 5B Wiki/GBrain live receipt** unless 老爸 explicitly redirects. If any command fails, stop and record the exact failing command and output in `docs/progress-log.md` before changing code.
