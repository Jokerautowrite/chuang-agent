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
  "workspace_root": "/home/user/projects/chuang-agent",
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
    "workspaceRoot": "/home/user/projects/chuang-agent",
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

## Feishu Rule

Chuang must use a new dedicated Feishu bot and channel id. Do not reuse Codex or Hermes Feishu bridges, credentials, sessions, or services.

## Local Simulation

Before wiring a real channel, run:

```bash
cargo run -- channel feishu-check \
  --env-file /home/user/projects/chuang-agent/ops/systemd/chuang-feishu-bridge.env \
  --json

cargo run -- channel simulate \
  --workspace-root /home/user/projects/chuang-agent \
  --message-id test-msg-1 \
  --sender-id test-user \
  --thread-id test-thread \
  --text "还在吗？" \
  --json
```

This reads the workspace `config.toml`, runs one local turn, writes session memory for the supplied thread id, and returns a `ChannelOutboundMessage`. It does not connect to Feishu.
