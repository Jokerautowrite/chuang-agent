# Channel Adapter Protocol

External channels such as Feishu, WeChat, HTTP, or a desktop UI are plugins. They must not become part of the core runtime.

## Boundary

The adapter owns channel-specific details:

- bot credentials
- webhook verification
- message acknowledgment
- rich-card or markdown rendering
- retry behavior from the channel platform

Chuang core owns only:

- workspace root
- user text
- thread id
- app-server request/response events

## Minimal Flow

```text
channel message
  -> ChannelInboundMessage
  -> app-server turn/start JSON-RPC
  -> app-server events
  -> ChannelOutboundMessage
  -> channel reply
```

The current pure protocol helpers live in `src/channel_adapter.rs`.

## Inbound Shape

```json
{
  "channel": "feishu-dedicated-chuang",
  "message_id": "msg-1",
  "sender_id": "user-1",
  "workspace_root": "$CHUANG_AGENT_ROOT",
  "text": "还在吗？",
  "thread_id": "chuang-thread-1"
}
```

## App-Server Request

`app_server_turn_start_request()` converts the inbound message into:

```json
{
  "id": 1,
  "method": "turn/start",
  "params": {
    "threadId": "chuang-thread-1",
    "workspaceRoot": "$CHUANG_AGENT_ROOT",
    "text": "还在吗？",
    "channel": "feishu-dedicated-chuang",
    "channelMessageId": "msg-1",
    "senderId": "user-1"
  }
}
```

## Output

`outbound_from_app_server_event()` only converts app-server message events into outbound replies. Other lifecycle events are ignored by default.

For a batch of app-server events, use `outbounds_from_app_server_events()`. It prefers final `item/completed` messages over streaming `item/agentMessage/delta` messages, so a channel such as Feishu does not send duplicate partial and final replies by default.

`channel simulate` also exposes the current `runtime_report_id` and runtime observability snapshot in its JSON output. That keeps the local protocol aligned with the thin-bridge contract: the adapter can correlate a channel turn to a structured report without guessing from free-form text.

The repo-local Feishu bridge renders assistant replies as Feishu interactive cards through `scripts/chuang-feishu-client-adapter.js`. If rich-card send fails, the bridge falls back to a plain text payload. The card includes the thread id and runtime report id when app-server returns them, so a live channel reply can be correlated to the structured runtime report. The local contract is covered by `node scripts/chuang-feishu-rich-message-smoke.js`; it does not connect to Feishu.

The repo-local Feishu bridge also has local bridge commands in `scripts/chuang-feishu-bridge-commands.js`. These commands are answered by the bridge and are not forwarded as user tasks to the Agent runtime. The local contract is covered by `node scripts/chuang-feishu-command-smoke.js`; it does not connect to Feishu.

- `/new` starts a fresh app-server thread and binds the current Feishu chat to it.
- `/session` shows the current Feishu chat to Chuang thread binding.
- `/health` or `/status` shows local bridge, app-server process, workspace, Chuang Feishu env state, and provider env presence without printing secret values.
- `/help` lists bridge commands.

If `thread/start` or `turn/start` fails, the bridge sends a short sanitized error card back to the Feishu chat instead of only logging locally. The error tells the operator which stage failed and suggests `/health` or `/new`; secret-like tokens are redacted before rendering. Rich cards include the Feishu message id alongside model, thread id, and runtime report id, so local logs, channel messages, and runtime reports can be correlated.

The repo-local Feishu bridge sources `CHUANG_PROVIDER_ENV_FILE` before starting its Node runtime, so `app-server` child processes inherit provider variables such as `CODEX_PPTOKEN_API_KEY=<set>`. This provider env must stay outside the repository and separate from Feishu app credentials.

The dedicated Feishu live preflight lives in `scripts/chuang-feishu-live-preflight.js`.
It is an executable, read-only gate for the bridge startup path: it validates the
Chuang env path through `channel feishu-check`, checks workspace/config and
app-server diagnostic health, runs the local bridge command smoke, confirms the
session state path can be accessed without writing it, and reports provider env
file presence with `<set>/<missing>` states only. It must remain local-only:
no Feishu websocket/webhook connection, no outbound message, no service change,
no session store write, and no secret value in stdout/stderr.

## Feishu Rule

Chuang must use a new dedicated Feishu bot and channel id. Do not reuse Codex or Hermes Feishu bridges, credentials, sessions, or services.

## Local Simulation

Before wiring a real channel, run:

```bash
cargo run -- channel feishu-check \
  --env-file $CHUANG_AGENT_ROOT/ops/systemd/chuang-feishu-bridge.env \
  --json

cargo run -- channel simulate \
  --workspace-root $CHUANG_AGENT_ROOT \
  --message-id test-msg-1 \
  --sender-id test-user \
  --thread-id test-thread \
  --text "还在吗？" \
  --json
```

This reads the workspace `config.toml`, runs one local turn, writes session memory for the supplied thread id, and returns a `ChannelOutboundMessage`. It does not connect to Feishu.
