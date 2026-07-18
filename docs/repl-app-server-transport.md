# REPL App-Server Transport

Interactive `chuang repl` uses the canonical Unix socket by default:

```text
${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock
```

`CHUANG_APP_SERVER_MODE=local` and `CHUANG_REPL_STUB=1` keep the direct local runtime path.
Socket failures are returned as `app_server_unavailable`; the REPL does not fall back locally.
The launchers preserve the caller's directory in `CHUANG_REPL_WORKSPACE_ROOT`, so socket turns,
local compatibility turns, terminal metadata, and approval handling use the same workspace even
though the launcher changes into the project root to load Chuang's config.

The socket daemon accepts clients concurrently. A `turn/start` connection receives
`turn/started`, live `turn/progress` notifications, the existing completion notifications, and the
final request/response result. Progress payloads preserve the runtime's existing JSONL terminal
event envelope, so Ratatui and legacy REPL reuse the same renderer.

While a socket turn is active, the REPL forwards local control input over separate connections:

- `!guidance` and ordinary mid-turn text call `turn/guidance`;
- `/stop` calls `turn/interrupt`;
- a second turn on the same thread is rejected with `thread_busy`;
- stale, mismatched, or completed turn controls return `turn_not_active`.

Successful guidance or interrupt responses mean the request was queued. The runtime remains
cooperative: guidance is applied and stop becomes effective at the next safe point. An already
blocking provider request or shell process is not claimed to stop immediately. `GuidanceInjected`
and `TurnCancelled` progress events are the authoritative applied/cancelled evidence. Control
delivery failures are surfaced as visible REPL warnings and never trigger a local fallback.

After reading the final response and before publishing the turn result to the UI, the REPL closes
its live-control gate and performs one final control drain. A control accepted by the UI is
therefore either forwarded or represented by a visible `live_control_warning`. Socket progress and
warning records share one writer lock, and both legacy and Ratatui cursors consume only
newline-terminated JSONL records; an incomplete tail is retained for the next read instead of being
skipped.

The compatibility stdio mode keeps its existing JSON-lines request/response behavior. Same-stream
live control is not promised there; the canonical terminal live-control path is the Unix socket
daemon.

The canonical socket daemon persists thread/turn state in the runtime configuration's SQLite
`db_path`. A caller must omit `threadId` to start a new thread; an unknown or stale id is rejected
instead of silently creating a replacement. After daemon restart, completed history remains
resumable and sequence floors prevent thread/turn id reuse. A persisted `active` turn becomes
`interrupted` and is not replayed.

Live responses may expose full runtime `providerMeta`, but the durable snapshot retains only
`pending_approval_id`, `pending_approval_path`, and `app_server_interruption_reason`. Tool traces,
tool surfaces, provider credentials, and other runtime-only metadata are not persisted. Approval
turns use `status=human_input_required`; cancelled, failed, interrupted, and `provider_error` turns
remain visible but are not injected into later model conversation history.
