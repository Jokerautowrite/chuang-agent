# App Server Service

Chuang app-server keeps its JSON-lines stdin/stdout mode for compatibility:

```bash
chuang-agent app-server
```

The terminal canonical transport is a persistent Unix socket:

```bash
chuang-agent app-server daemon --socket "${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock"
```

The app-server does not contain Feishu credentials and does not reuse Codex or Hermes bridges.

The daemon creates a `0600` socket. An occupied reachable socket fails as already running. A stale
socket is preserved by renaming it to a `.stale-<timestamp>-<pid>-*` sibling before binding; the
daemon never removes socket paths.

Each active turn gets a private directory under
`${XDG_RUNTIME_DIR}/chuang-agent/live-turns`. The runtime root and per-turn directory are `0700`;
`guidance.txt` and `progress.jsonl` are atomically created with `create_new` and mode `0600`.
If `XDG_RUNTIME_DIR` is unavailable, the daemon still creates a private `0700` per-turn directory
under the system temporary directory instead of using shared flat control files.

## Terminal Client

```bash
chuang-agent app-server probe --socket "${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock" --json
chuang-agent app-server ask \
  --socket "${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock" \
  --workspace-root "$PWD" \
  --text "检查当前目录的 git 状态" \
  --json
```

`ask` consumes app-server event lines and returns final assistant text with thread and turn metadata.
It never falls back to a direct local runtime when the socket is unavailable.

Repository `scripts/chuang` routes `chuang ask` and free-form one-shot tasks through this socket by
default. Set `CHUANG_APP_SERVER_MODE=local` only when an explicit legacy direct-runtime invocation is
needed.

## Live Turn Control

The daemon handles each Unix client on its own thread and keeps active-turn control metadata behind
short-lived state locks. Provider and tool execution never run while holding the shared state lock.

An active socket turn emits:

```text
turn/started
turn/progress
item/agentMessage/delta
item/completed
turn/completed
```

`turn/guidance` queues text for the runtime's existing safe-point guidance reader.
`turn/interrupt` queues `[chuang-control] stop` and returns
`effectiveAt=next_safe_point`; it does not claim an already-blocking provider or shell operation was
immediately cancelled. Concurrent clients may send these controls while the original
`turn/start` connection remains open.

Guidance and interrupt writes share a per-turn file mutex. Active-turn validation and the write
also occur inside one app-server state-lock interval, so the operation has one linearization point:
it either writes and returns a queued response, or returns `turn_not_active` without writing.

## SQLite Thread/Turn Snapshots

The socket daemon loads the workspace runtime configuration at startup and uses its normalized
`db_path` for the app-server snapshot store. The store is SQLite-backed and keeps one transactional
snapshot of daemon thread state, including sequence floors, thread identity/workspace/display
metadata, and turn records.

Persisted turn fields are limited to user text, assistant text, model name, status, timestamps, and
the allowlisted metadata `pending_approval_id`, `pending_approval_path`, and
`app_server_interruption_reason`. Runtime `tool_trace`, `tool_surface`, `tool_calls_json`, provider
credentials, and other runtime secrets are never written to the snapshot. On restore, those
runtime-only fields remain empty or absent.

When the daemon starts after a process restart, any persisted `active` turn is changed to
`interrupted` with reason `daemon_restarted_before_turn_completion`. The daemon does not replay the
provider request. Completed history remains resumable and can be injected into a later turn under
the existing history-admission rules.

The compatibility JSON-lines stdio mode remains in-memory and does not open the daemon snapshot
store. The restart recovery and sensitive-field boundaries are covered by focused unit and black-box
tests.

The snapshot store is owned by the one canonical app-server service for a configured `db_path`.
Running multiple manually launched daemons against the same database is outside the supported
service topology.

The installed user service was accepted on 2026-07-18: after one completed turn, the service was
restarted and the old thread id continued successfully. The second turn reported
`recent_conversation_history_injected=true`, history item count `2`, and history turn count `1`.
The service remained `active/running` with `NRestarts=0`, probe succeeded, the socket stayed `0600`,
and the SQLite snapshot contained one thread/two turns with no forbidden runtime keys.

The 2026-07-18 persistence and service validation also confirmed:

- SQLite uses one OS advisory exclusive lock per database, and a single daemon owns the lock for
  the configured database.
- `server/status` is a read-only aggregate operation.
- `status` and `doctor` read real persistence state through the canonical socket; if that read
  fails, the persistence state is reported only as `unavailable`.
- A second daemon pointed at the same database returns `app_server_db_locked`.
- The SQLite lock and canonical socket both use permission mode `0600`.
- The service preserves persistence across a service restart.

## Health Check

```bash
cargo run -- app-server health --workspace-root $CHUANG_AGENT_ROOT --json
```

The health command only loads and validates the workspace runtime config. It does not start a conversation and does not call the provider.

The repository also includes:

```bash
scripts/chuang-app-server-health.sh
```

It reads:

```text
CHUANG_AGENT_ROOT
CHUANG_AGENT_WORKSPACE_ROOT
```

## Service Template

Templates live under:

```text
ops/systemd/chuang-agent-app-server.service.example
ops/systemd/chuang-agent-app-server.env.example
```

They are examples only. They are not installed automatically.

Before installing a real user service:

- copy the env example outside git or to an ignored local file;
- put provider secrets only in the env file or system secret store;
- keep the service Chuang-only;
- do not point it at Codex or Hermes Feishu bridge files;
- run the health command manually.

## Service Boundary

The systemd user service starts `scripts/chuang-app-server-service.sh`, which loads the existing
provider and live capability environment, creates `${XDG_RUNTIME_DIR}/chuang-agent`, and runs the
socket daemon at `${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock`. It does not use a FIFO, create an
HTTP listener, or own any Feishu connection strategy.

The service's `CHUANG_AGENT_WORKSPACE_ROOT` is the configuration root for provider, identity, rules,
and durable Chuang data. Each `turn/start.workspaceRoot` remains the caller's governed tool
workspace. These are deliberately separate: one canonical Chuang service can operate on the
caller's directory without treating that directory as a second runtime configuration.
