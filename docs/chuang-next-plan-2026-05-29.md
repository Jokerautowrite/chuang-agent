# Chuang Next Plan - 2026-05-29

## Current Position

Chuang is in a local-gate-ready state, not a real-live-ready state.

- Full local Rust tests pass with `cargo test -q`.
- `status --json` reports project readiness as `ready` and release readiness as `second_test_version_ready`.
- `third_test_candidate.real_live_ready=false`; real external receipts are still required.
- The working tree is dirty and must be split by topic before more feature work.

## Worktree Buckets

1. `provider/responses-api`
   - Scope: `src/provider_openai_compatible.rs` and OpenAI-compatible provider tests.
   - Meaning: OpenAI-compatible requests now target `/v1/responses` while keeping legacy parsing fallback.

2. `subagent/evolver dry-run`
   - Scope: `cli_subagent`, `skill_evolver`, `slot_registry`, `subagent_report`, `subagent_spawner`, related tests.
   - Meaning: subagent reports can carry skill proposals; dry-run evolution is visible but still governed.

3. `status/readiness live boundary`
   - Scope: `kernel_status`, `cli_status_tests`, browser/knowledge readiness assertions.
   - Meaning: status can distinguish local-ready, preflight-ready, adapter-missing, and live-ready boundaries more precisely.

4. `feishu bridge summary`
   - Scope: `scripts/chuang-feishu-bridge.js`, `scripts/chuang-feishu-turn-summary-smoke.js`.
   - Meaning: Feishu output/summary surface has small updates, still within Chuang-scoped bridge boundaries.

5. `non-mainline ops artifacts`
   - Scope: Sub2 docs, generated images, `config.toml.bak-20260526-034332-cliproxy`.
   - Meaning: keep them untouched for now; do not delete or mix into Chuang core commits.

## 推进顺序

### Phase 0 - Freeze And Verify

- Run `git diff --check`.
- Run `scripts/chuang-third-test-smoke.sh` only after the working tree is clean; the script intentionally refuses dirty-tree execution.
- Reconfirm `status --json` still reports local gates cleanly.

### Phase 1 - Split The Dirty Tree

- Separate Chuang core changes from Sub2/ops artifacts.
- Keep Sub2 artifacts out of Chuang core commits unless explicitly requested.
- Prefer small commits by bucket: provider, subagent/evolver, status/readiness, Feishu summary.

### Phase 2 - Provider Live Receipt

- Use configured `cliproxy-local` provider only through approved Chuang commands.
- Send one minimal, non-sensitive live request.
- Record request path, response status, response parsing path, and no secret output.

### Phase 3 - Single Worker Rehearsal

- Enable or simulate one bounded live worker path.
- Prove spawn -> report -> collect -> admission refs without bypassing governance.
- Keep recursive spawning disabled unless separately approved.

### Phase 4 - Channel And Browser Receipts

- Verify Chuang Feishu live send/receive without reusing Hermes or Codex bridge credentials.
- Verify browser read/action boundary separately: read-only CDP observation first, action receipt later.

### Phase 5 - External Knowledge Read-Only Adapter

- Wire wiki/GBrain as audited read-only adapters only after endpoint/token env checks.
- Return provenance and no-write evidence in status/readiness.
- Do not write core memory automatically from external knowledge hits.

## Non-Goals For Next Slice

- Do not build autonomous deletion, cleanup, uninstall, reset, payment, or account-action flows.
- Do not claim real live readiness from local smoke tests.
- Do not mix Sub2 operational notes into Chuang core architecture commits.
- Do not enable automatic skill solidification until dry-run proposals and review receipts are stable.
