# MVP Readiness - 2026-05-02

## Status

Chuang Agent has a runnable MVP chain:

```text
input -> identity/memory -> context -> governance -> provider -> report -> memory
```

The current root `config.toml` uses:

- provider: `openai_compatible`
- transport: `native`
- model: `gpt-5.5`
- fallback: optional explicit `fallback_*` config, controlled by `fallback_on_retryable / fallback_status_codes / fallback_error_classes`
- subagent: `queued_external`
- actuator: `command` through `scripts/chuang-actuator-adapter-example.sh`
- control plane: `command` through `scripts/chuang-control-adapter-example.sh`
- governance: `static_rule` with `rules/core.md`
- identity bootstrap: `identity/SOUL.md`, `STORY.md`, `FIRST_WAKE.md`, `agents.toml`

The checked-in actuator/control adapters are formal command-backed protocol adapters, but the example scripts are intentionally safe fixtures. They return deterministic JSON and do not operate the real desktop or real system services.

As of 2026-05-03, the MVP readiness surface also expects:

- execution slot: `generic_agent_mvp`
- GA atomic tools: 9-tool manifest visible in `status` / `doctor`
- mapped executable MVP tools: `file_read`, `file_write`, `code_execute`
- auxiliary MVP tool: `list_dir`
- interface-only tools: desktop/browser style atomic tools remain adapter/plugin boundaries
- governance readiness: `status` / `doctor` expose `rules_loaded`, rule count/fingerprint, `tool_surface_governed=true`, and classification probes showing read-only allowed while dangerous shell/write require approval and secret-bearing shell stays draft-only
- provider timeout: `provider_request_timeout_ms` is visible in `status` / `config show` and can be overridden from the CLI without leaking secrets
- goal mode: lightweight `run --goal TEXT` context wrapper, not a new core slot and not a governance bypass
- GoalRun: local `goal plan/show/checkpoint` records for checkpoint-first continuation; it persists plans/checkpoints but does not execute tasks or dispatch workers
- session memory diagnostics: `session_id`, recall isolation/filter/hit count, and writeback record metadata in runtime provider meta
- channel goal input: `channel simulate --goal TEXT` can pass goal context through app-server input, but the real Feishu bridge is still a dedicated channel adapter concern
- channel preflight: `channel feishu-check` verifies Chuang-only env file scope, workspace presence, workspace `config.toml`, allowed connection mode, and legacy Feishu variable names without leaking secrets or connecting to Feishu; MVP smoke now covers the local preflight path
- console readiness summary: `console snapshot` text and JSON expose project/channel/subagent/external-AI readiness for a future desktop/service console
- subagent report admission: `subagent run-once/run-loop/report/collect` expose `ReportAdmission` so controller acceptance is separate from worker execution status
- plugin registry: checked as manifest/path readiness only; disabled plugins are not executed, and status/doctor readiness only treats enabled plugins as runtime failures
- project readiness: `status` / `doctor` expose a module-level rollup across main chain, identity, memory, context, governance, execution tools, reporting, channel, subagent, goal, plugins, and external AI. Current expected overall state is `ready`.
- release readiness: `status` / `doctor` now report `release_name=second_test_version` with `overall_state=second_test_version_ready`. The second-test surface is an acceptance/readiness gate, not a live integration claim: `connects_real_external_services=false`, `verifies_real_external_services=false`, `uses_stub_or_local_fixtures=true`, and `writes_repo_files=false`.
- channel readiness: `status` / `doctor` expose app-server, channel simulate, Chuang dedicated Feishu bridge, Codex/Hermes isolation, and rich-message boundary status. This is a local readiness surface, not a live Feishu connection check.
- subagent readiness: `status` / `doctor` expose dispatch queue, report collect, command runner, multi-worker orchestration, and external-AI downstream status. The current `queued_external` mode remains a protocol surface, not an autonomous executor.

## Current Acceptance Commands

```bash
cargo test -q
git diff --check
cargo run --quiet -- doctor --config config.toml
cargo run --quiet -- status --config config.toml --json
sh scripts/chuang-mvp-smoke.sh
sh scripts/chuang-second-test-smoke.sh
```

The second-test smoke wrapper sets `CHUANG_SMOKE_NAME=second_test` and reuses the same non-destructive smoke flow, so it produces `second_test_smoke_ok` while keeping the legacy MVP smoke entrypoint available. The smoke script uses a temporary directory and stub provider. It validates:

- status JSON, including execution slot, governance/rules readiness, governed atomic tool schemas, goal mode, plugin registry, project/channel/subagent readiness, second-test release readiness, and the expected `provider transport=stub` placeholder warning in the smoke config
- doctor JSON, including config, identity, slots, atomic tools, governance readiness, goal mode, project/channel/subagent readiness, second-test release readiness, actuator/control smoke, isolated runtime smoke, isolated subagent queue smoke, and plugin registry
- two session-memory turns
- runtime `--goal` injection without changing the original user input path
- GoalRun plan/checkpoint/show persistence for checkpoint-first continuation
- session memory diagnostic meta for isolated recall and writeback
- channel simulate with `--goal`
- queued subagent dispatch/command-runner/report/collect with capability matching and report admission metadata
- command control example list/apply
- experiment plan/show

It does not delete files and does not touch real services.

## Ready For

- Local CLI runtime testing.
- App-server protocol testing against workspace `config.toml`.
- App-server health checks and Chuang-only systemd service template review.
- New dedicated Feishu bot/channel adapter design.
- Channel message conversion through `src/channel_adapter.rs`.
- Plugin registry checks through `plugin check --registry plugins/registry.example.json`.
- Local channel simulation through `cargo run -- channel simulate ...`.
- Identity memory compaction through `memory identity show -> write-user/write-memory --approve-overwrite`.
- Command-control adapter integration testing with a real external script.
- Command-actuator adapter integration testing with a real external script.
- Real subagent runner experiments through the queued command runner.
- Worker capability declarations through `subagent run-loop --capability`.
- Dispatch capability requirements through `subagent dispatch --requires-capability`.
- Stale claim recovery for queued subagent workers using dispatch `idle_timeout_ms`.
- Goal-scoped local CLI/runtime testing through `run --goal TEXT`.
- GoalRun checkpoint recovery testing through `goal plan/show/checkpoint`.
- Readiness dashboards based on `status --json`, `doctor --json`, and `console snapshot --json`.

## Not Ready For

- Reusing any existing Codex or Hermes Feishu bridge.
- Real desktop/browser control through `actuator` until a reviewed command adapter is supplied.
- Real service start/stop/restart without a reviewed command adapter.
- Installing or starting the app-server service automatically.
- Real control adapter activation before `docs/real-control-adapter-safety-plan.md` is satisfied.
- Automatic cleanup of queues, claims, experiments, or memory files.
- Automatic identity-memory compression without explicit model/operator overwrite.
- Silent fallback to unconfigured providers.
- Parallel subagent worker execution above `--max-concurrency 1`.
- Treating plugin registry entries as installed/running integrations; they are readiness manifests until explicitly enabled and tested.
- Treating GA interface-only atomic tools as real desktop/browser control; they currently define ports and mappings only.
- Treating goal mode as an autonomous background executor; it currently records structured runtime context plus local plans/checkpoints only.
- Treating GoalRun readiness as task execution or worker dispatch; the readiness field `goal_run_executes=false` is intentional.
- Treating `project_readiness.overall_state=ready` as proof that every live adapter is connected. It only means the local module rollup is green; live Feishu or other external service verification remains separate.
- Treating `release_readiness.release_name=second_test_version` as proof that real external services are connected. The second-test fields explicitly say the opposite: live provider, Feishu, desktop/browser, wiki/GBrain, and Hermes connections are not verified by status/doctor/smoke.

## Next Build Steps

1. Keep the main-process tool surface stable: status/doctor/smoke must continue to prove GA atomic tool schemas, structured tool reports, and safe command adapters.
2. Harden memory/context readiness: keep session recall isolated and expose diagnostics in runtime/channel meta.
3. Add a dedicated Chuang channel adapter for a new Feishu bot, separate from Codex and Hermes, using the existing app-server/channel protocol.
4. Replace the safe actuator/control example scripts with reviewed real adapters only after the target allowlists are explicit.
5. Build real subagent runners against `docs/subagent-runner-protocol.md`, starting with single-worker queues and capability matching.
6. Keep expanding smoke coverage only with non-destructive checks.
