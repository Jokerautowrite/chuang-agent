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

## Current Acceptance Commands

```bash
cargo test -q
git diff --check
cargo run --quiet -- doctor --config config.toml
sh scripts/chuang-mvp-smoke.sh
```

The smoke script uses a temporary directory and stub provider. It validates:

- status
- doctor
- two session-memory turns
- channel simulate
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

## Next Build Steps

1. Add a dedicated Chuang channel adapter for a new Feishu bot, separate from Codex and Hermes.
2. Keep the adapter thin: Feishu message -> app-server or runtime command -> response.
3. Replace the safe actuator/control example scripts with reviewed real adapters only after the target allowlists are explicit.
4. Build real subagent runners against `docs/subagent-runner-protocol.md`, starting with single-worker queues and capability matching.
5. Keep expanding smoke coverage only with non-destructive checks.
