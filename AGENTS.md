# Chuang Agent Project Rules

This project builds a local agent operating system, not a chatbot.

Core thesis:

- Memory is the body; agent runtimes are shells.
- Codex contributes the Rust/event-loop backbone.
- Hermes contributes bounded memory and identity continuity.
- OpenClaw contributes isolated full-capability subagents.
- GenericAgent contributes desktop actuation and skill evolution.

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
- Core runtime must depend on traits, events, schemas, and commands; it must not depend on concrete backends.
- No silent fallback. If a configured backend is unavailable, return a structured error.
- Every pluggable slot needs a fake implementation before a real adapter is treated as stable.
- Every adapter must satisfy contract tests for its trait.
- Keep modules replaceable: provider, memory store, context engine, subagent spawner, actuator, governance, and evolver.
- Do not hardcode secrets, model names, service credentials, user IDs, or local-only paths in Rust code.
- Prefer small traits and explicit data structs over global state.
- Runtime events should be serializable, auditable, and replay-friendly.

## Risk Rules

- Governance is mandatory and cannot be disabled.
- Subagents must not write core memory directly; they may only produce reports or memory proposals.
- Actuators must not decide risk; they propose actions and execute only after governance allows them.
- External sends, public posts, payments, orders, account actions, network changes, and service-disrupting actions require explicit approval unless a narrower project rule says otherwise.
- Verification codes may be entered only when provided or approved by 老爸; never bypass platform verification.
- Deletion, cleanup, uninstall, reset, or destructive changes require explicit approval for exact targets.
- Keep secrets out of logs, reports, tests, fixtures, and chat output.

## Memory Rules

- No execution, no memory.
- Temporary state does not belong in core memory.
- Hot memory must stay bounded; over-budget writes should be rejected with current entries for model-directed compression.
- Important memory should have source, reason, and scope when possible.
- Preserve identity boundaries between 小创, 小承, 小云, and 小策.

## Progress Rules

- Update `docs/progress-log.md` after meaningful architecture or implementation changes.
- Tests are the source of truth for implementation status.
- BrowserWorker is an ability line; it must not take priority over memory, subagents, context, governance, and pluggability unless 老爸 explicitly redirects.

## Rust Implementation Rules

- Keep public structs and enums explicit and serializable when they cross module boundaries.
- Prefer deterministic behavior in core logic; put model judgment behind provider/evolver interfaces.
- Add focused tests for each new trait contract and each error path.
- Do not rewrite unrelated modules while implementing one slot.
