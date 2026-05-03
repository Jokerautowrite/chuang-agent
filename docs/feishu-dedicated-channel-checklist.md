# Dedicated Feishu Channel Checklist

This checklist is for the future Chuang Feishu bot. It must stay separate from Codex and Hermes.

## Hard Rules

- Use a new Feishu app/bot id for Chuang.
- Do not reuse Codex `codex-feishu-bot.service`.
- Do not reuse Hermes gateway service, credentials, session, or message queue.
- Do not write Feishu secrets into repo files.
- Store secrets in an external env file or service manager secret store.
- Bind the channel to `/home/user/projects/chuang-agent` explicitly.
- For the current long-connection adapter, only `CHUANG_FEISHU_APP_ID` and `CHUANG_FEISHU_APP_SECRET` are mandatory; `CHUANG_FEISHU_BOT_ID` and `CHUANG_FEISHU_VERIFICATION_TOKEN` stay optional unless you later switch to a webhook-style adapter.

## Minimal Adapter Shape

```text
Feishu event
  -> verify/ack in plugin
  -> ChannelInboundMessage
  -> app-server turn/start
  -> app-server message event
  -> ChannelOutboundMessage
  -> Feishu reply
```

The core helper is `src/channel_adapter.rs`. It only converts messages; it does not know Feishu credentials.

## Preflight

Before enabling the bot:

```bash
cargo run --quiet -- doctor --config config.toml
cargo run --quiet -- channel feishu-check \
  --env-file /home/user/.codex-im/chuang-feishu-bridge.env \
  --json
cargo run --quiet -- channel simulate \
  --workspace-root /home/user/projects/chuang-agent \
  --message-id preflight-msg \
  --sender-id preflight-user \
  --thread-id preflight-thread \
  --text "还在吗？" \
  --json
sh scripts/chuang-mvp-smoke.sh
```

Expected root status:

- provider: `openai_compatible`
- subagent: `queued_external`
- actuator: `command`
- control plane: `command`
- placeholder warnings: `none`
- Feishu connection mode: `websocket`

## Bot-Side Requirements

- Ignore non-text interactive messages or convert them explicitly before calling Chuang.
- Preserve Feishu message id as `message_id`.
- Preserve Feishu sender id as `sender_id`.
- Use a stable per-chat or per-topic thread id.
- Forward only plain text to Chuang until rich-message support is explicitly added.
- Return errors as short operational messages without secrets.

## Local Templates

```text
ops/systemd/chuang-feishu-bridge.env.example
ops/systemd/chuang-feishu-bridge.service.example
scripts/chuang-feishu-bridge.sh
```

The bridge script is a long-connection runtime. It validates the Chuang-only env file, opens the Feishu websocket client, and forwards plain text to Chuang `app-server`.

## First Live Test

1. Send a text message to the new Chuang bot.
2. Confirm the response does not contain `fake-responder`.
3. Confirm Chuang writes session memory for the thread.
4. Confirm Codex Feishu and Hermes Feishu still respond on their own channels.
5. Confirm no service file for Codex or Hermes changed.
