# Chuang Agent Project Rules

This project builds a **local agent operating system**, not a chatbot.

## Core Thesis

- **Memory is the body; agent runtimes are shells.**
- Chuang is a **dispatcher / local agent OS**, not an all-purpose strongest worker.
  It does not need to beat Codex, Claude Code, or other agents at their primary
  games (coding UX, general chat/search). It orchestrates the strongest agents
  under governance, memory, and replaceable slots.
- When coding is the job, delegate to the current best coding agent — do not
  rebuild that product inside Chuang.
- Forbidden: death-marching on "be strongest at everything"; treating any single
  worker CLI/model as the irreplaceable body.
- Reference implementations: Codex contributes the Rust/event-loop backbone;
  Hermes contributes bounded memory and identity continuity; OpenClaw
  contributes isolated full-capability subagents; GenericAgent contributes
  desktop actuation and skill evolution.

See `docs/blueprint-v1.md` §0.1 and `docs/core-boundary.md` for the dispatcher
principle (调度台原则).

## Project Layout

```
src/       Rust runtime: kernel, slots, providers, governance, evolver
tests/     Contract tests (the source of truth for implementation status)
docs/      Blueprints, specs, and architecture notes
config/    Allowlist examples for real actuator/control adapters
scripts/   Entrypoints, adapters, smoke and receipt scripts
rules/     Governance constitution (rules/core.md)
identity/  Runtime identity files (git-ignored; use *.example templates)
data/      Runtime DB and queues (git-ignored)
```

## Required Reading

Before substantial work, read:

1. `docs/progress-log.md`
2. `docs/blueprint-v1.md`
3. `docs/pluggable-architecture-v1.md`
4. `docs/source-project-audit-v1.md`

For implementation details, also read the relevant spec:

- `docs/spec-v3.md`
- `docs/context-engine-design-v1.md`
- `docs/implementation-prep-v1.md`

## Engineering Rules

- Interface first, implementation second.
- Core runtime must depend on traits, events, schemas, and commands; it must not
  depend on concrete backends.
- No silent fallback. If a configured backend is unavailable, return a structured
  error with error kind, reason, and context.
- Every pluggable slot needs a fake implementation before a real adapter is
  treated as stable.
- Every adapter must satisfy contract tests for its trait.
- Keep modules replaceable: provider, memory store, context engine, subagent
  spawner, actuator, governance, and evolver.
- Do not hardcode secrets, model names, service credentials, user IDs, or
  local-only paths in Rust code.
- Prefer small traits and explicit data structs over global state.
- Runtime events should be serializable, auditable, and replay-friendly.

## Risk Rules

- Governance is mandatory and cannot be disabled.
- Subagents must not write core memory directly; they may only produce reports
  or memory proposals.
- Actuators must not decide risk; they propose actions and execute only after
  governance allows them.
- External sends, public posts, payments, orders, account actions, network
  changes, and service-disrupting actions require explicit approval unless a
  narrower project rule says otherwise.
- Deletion, cleanup, uninstall, reset, or destructive changes require explicit
  approval for exact targets. Never run autonomous `rm` on user data.
- Verification codes may be entered only when provided or approved by the user;
  never bypass platform verification.
- Keep secrets out of logs, reports, tests, fixtures, and chat output.

## Memory Rules

- No execution, no memory.
- Temporary state does not belong in core memory.
- Hot memory must stay bounded; over-budget writes should be rejected with
  current entries for model-directed compression.
- Important memory should have source, reason, and scope when possible.
- Preserve identity boundaries between agents in a multi-agent household.

## Progress Rules

- Update `docs/progress-log.md` after meaningful architecture or implementation
  changes.
- Tests are the source of truth for implementation status.
- Ability lines (browser, desktop, etc.) must not take priority over memory,
  subagents, context, governance, and pluggability unless the maintainer
  explicitly redirects.

## Rust Implementation Rules

- Keep public structs and enums explicit and serializable when they cross module
  boundaries.
- Prefer deterministic behavior in core logic; put model judgment behind
  provider/evolver interfaces.
- Add focused tests for each new trait contract and each error path.
- Do not rewrite unrelated modules while implementing one slot.

## Contributing

- Open an issue first for any design-level change; keep PRs small and focused on
  one slot.
- Every new backend/adapter must come with a fake implementation, a contract
  test, and a documented opt-in.
- Run `cargo test` and `cargo run -- doctor` before submitting.
- Do not commit generated runtime state, local config, identity files, or
  credentials — they are git-ignored by design.

## Security

- If you find a secret or credential committed in history, open a private issue
  immediately; do not post it publicly.
- Configuration uses `api_key_env` style references instead of inline secrets;
  keep it that way.
