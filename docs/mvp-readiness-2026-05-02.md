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
- goal mode: lightweight `run --goal TEXT` context wrapper, not a new core slot and not a governance bypass
- session memory diagnostics: `session_id`, recall isolation/filter/hit count, and writeback record metadata in runtime provider meta
- channel goal input: `channel simulate --goal TEXT` can pass goal context through app-server input, but the real Feishu bridge is still a dedicated channel adapter concern
- plugin registry: checked as manifest/path readiness only; disabled plugins are not executed, and status/doctor readiness only treats enabled plugins as runtime failures

## Current Acceptance Commands

```bash
cargo test -q
git diff --check
cargo run --quiet -- doctor --config config.toml
cargo run --quiet -- status --config config.toml --json
sh scripts/chuang-mvp-smoke.sh
```

The smoke script uses a temporary directory and stub provider. It validates:

- status JSON, including execution slot, atomic tool schemas, goal mode, plugin registry, and the expected `provider transport=stub` placeholder warning in the smoke config
- doctor JSON, including config, identity, slots, atomic tools, goal mode, actuator/control smoke, isolated runtime smoke, isolated subagent queue smoke, and plugin registry
- two session-memory turns
- runtime `--goal` injection without changing the original user input path
- session memory diagnostic meta for isolated recall and writeback
- channel simulate with `--goal`
- queued subagent dispatch/command-runner/collect with capability matching
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
- Treating goal mode as an autonomous background executor; it is currently just structured runtime context.

## Next Build Steps

1. Keep the main-process tool surface stable: status/doctor/smoke must continue to prove GA atomic tool schemas, structured tool reports, and safe command adapters.
2. Harden memory/context readiness: keep session recall isolated and expose diagnostics in runtime/channel meta.
3. Add a dedicated Chuang channel adapter for a new Feishu bot, separate from Codex and Hermes, using the existing app-server/channel protocol.
4. Replace the safe actuator/control example scripts with reviewed real adapters only after the target allowlists are explicit.
5. Build real subagent runners against `docs/subagent-runner-protocol.md`, starting with single-worker queues and capability matching.
6. Keep expanding smoke coverage only with non-destructive checks.
