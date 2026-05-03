# Real Control Adapter Safety Plan

The checked-in command adapter is only a protocol example. A real adapter must be reviewed before it can start, stop, restart, or reconfigure local services or agents.

## Required Shape

The real adapter should still implement `docs/control-command-protocol.md`:

```text
list --json  -> JSON array of ManagedUnit
apply --json -> JSON receipt from stdin request
```

It should be a separate script or binary, referenced by config:

```toml
control = "command"
program = "/absolute/path/to/chuang-real-control-adapter"
list_args = "list --json"
apply_args = "apply --json"
control_timeout_ms = 30000
```

## Mandatory Allowlist

The adapter must use an explicit allowlist. It must not accept arbitrary systemd unit names, arbitrary process names, shell fragments, or paths from the model.

Initial candidate allowlist:

- Chuang dedicated Feishu bot service, once created.
- Chuang dedicated worker service, once created.
- Chuang local app-server service, once created.

Codex and Hermes services must not be included unless 老爸 explicitly asks for cross-agent control.

## Apply Rules

- `list` is read-only and can run without approval.
- `start`, `stop`, `restart`, and `change_model` are apply actions and must go through Chuang governance plus explicit surface approval.
- `change_model` can only edit the allowlisted Chuang service config or env file.
- `apply` must verify that receipt `unit_id`, `action`, and `model_name` match the request.
- The adapter must never execute shell text from stdin.
- The adapter must never delete files, logs, queues, reports, claims, memories, or credentials.

## First Real Integration Steps

1. Create a Chuang-only service allowlist file.
2. Implement `list --json` first.
3. Point `config.toml` to the real adapter only after `list --json` works.
4. Run `cargo run --quiet -- doctor --config config.toml`.
5. Implement `apply --json` for exactly one low-risk action.
6. Add CLI regression tests with a fake fixture, not the real service.
7. Only then run one approved live apply manually.

## Checked-In Allowlist Scaffold

The repository now includes a dry-run real adapter scaffold:

```bash
scripts/chuang-real-control-adapter.py list --json --allowlist config/control-allowlist.example.json
scripts/chuang-real-control-adapter.py apply --json --allowlist config/control-allowlist.example.json
```

Live execution is disabled by default. The adapter only runs allowlisted command arrays when `CHUANG_REAL_CONTROL_ENABLE=1` is set. Status commands are also dry by default and only run when `CHUANG_REAL_CONTROL_STATUS_ENABLE=1` is set.

The example allowlist includes only Chuang-owned service names. It does not include Codex or Hermes services.

## Explicit Non-Goals

- No direct control of Hermes.
- No direct control of Codex Feishu bridge.
- No broad `systemctl` passthrough.
- No automatic cleanup.
- No hidden restart on config load.
