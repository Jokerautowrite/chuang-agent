# Chuang Next Plan - 2026-05-30

## Current State

Chuang is clean, pushed, and ready for the next implementation decision.

- Branch: `main`
- Remote: `origin/main`
- Current head: `09f5d57 docs(ops): archive sub2 operation artifacts`
- Git state: local `main` is synced with `origin/main`
- Latest known validation after repo整理:
  - `git diff --check` passed
  - `cargo test -q` passed
  - `sh scripts/chuang-third-test-smoke.sh` passed with `third_test_candidate_smoke_ok`

The project is still **local-gate-ready**, not **real-live-ready**.

`cargo run -q -- status --json` currently reports the configured provider as `openai_compatible` / `cliproxy-local`, model `gpt-5.5`, and `api_key_state=<set>`. This only proves readiness for a live-request attempt; status does not send a real provider request.

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
   - `subagent_live_worker.enabled=false` remains the default boundary.

5. Feishu surface
   - Feishu turn summary has readiness wording to avoid claiming live-ready from local gates.
   - Codex Feishu and Hermes must remain separate.

## Main Gaps

### Gap 1 - Provider Live Receipt

Goal: prove the configured `cliproxy-local` provider can make one minimal, non-sensitive `/v1/responses` request through the Chuang provider path.

Acceptance evidence:

- Request path is `/v1/responses`.
- Response is parsed by the Responses path.
- No API key, token, or raw secret is printed.
- Receipt records timestamp, provider kind, model, request path, status, and redacted response summary.
- Failure mode is structured and does not silently fall back to fake/stub.

Recommended next action:

```bash
cargo run -q -- status --json
sh scripts/chuang-provider-readiness-check.sh
```

Then run or add a deliberately bounded provider receipt command if the existing scripts still only do readiness checks.

### Gap 2 - Single Live Worker Rehearsal

Goal: prove one bounded live worker can go through spawn -> report -> collect -> admission refs without bypassing governance.

Acceptance evidence:

- Worker starts only after explicit configuration/approval.
- Required capability mismatch is rejected before worker start.
- A successful rehearsal produces a structured `SubagentReport`.
- `goal collect` shows accepted report refs.
- No recursive spawn unless separately approved.

Recommended next action:

```bash
sh scripts/chuang-live-runner-readiness-view.sh
```

Then implement the smallest missing command or fixture that turns readiness evidence into a real single-worker rehearsal receipt.

### Gap 3 - Feishu Live Receipt

Goal: prove Chuang's own Feishu channel can send/receive through the intended identity without borrowing Hermes or the Codex bridge's internal credentials.

Acceptance evidence:

- Chuang Feishu identity and route are explicit.
- Send/receive receipt is stored as evidence.
- No Feishu tokens are printed.
- Hermes remains untouched.

Recommended next action:

Read the existing Feishu receipt scripts and decide whether current live identity is ready. If not ready, document the missing env/config as a blocker instead of wiring around it.

### Gap 4 - Browser/Desktop Boundary

Goal: keep browser read and desktop action separate.

Acceptance evidence:

- Browser read receipt only records audited URL/title/DOM evidence.
- Desktop action receipt is separate and goes through actuator/governance.
- No action is inferred from read readiness.

Recommended next action:

First close CDP read-only receipt, then add one explicit low-risk desktop action rehearsal.

### Gap 5 - Wiki/GBrain Read-Only Adapter

Goal: connect external knowledge as audited read-only adapters.

Acceptance evidence:

- Endpoint and token env are detected without printing secrets.
- Adapter returns provenance.
- Reads do not write core memory automatically.
- Status says read-only live adapter is available only after real receipt.

Recommended next action:

Pick one source first, preferably wiki, and wire a read-only adapter behind the existing `knowledge_read` contract.

## Recommended Order

1. Provider live receipt
2. Single live worker rehearsal
3. Feishu live receipt
4. Browser read-only receipt
5. Desktop action rehearsal
6. Wiki/GBrain read-only adapter
7. Skill proposal review -> manual solidify path

Provider first is the cleanest next step because the code has already moved to `/v1/responses`, and one real provider receipt will quickly prove whether the runtime path is actually usable.

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

If the tree is clean and third-test still passes, continue with **Gap 1 - Provider Live Receipt**. If any command fails, stop and record the exact failing command and output in `docs/progress-log.md` before changing code.
