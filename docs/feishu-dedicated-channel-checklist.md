# Dedicated Feishu Channel Checklist

This checklist is for the future Chuang Feishu bot. It must stay separate from Codex and Hermes.

## Hard Rules

- Use a new Feishu app/bot id for Chuang.
- Do not reuse Codex `codex-feishu-bot.service`.
- Do not reuse Hermes gateway service, credentials, session, or message queue.
- Do not write Feishu secrets into repo files.
- Store secrets in an external env file or service manager secret store.
- Keep provider credentials separate from Feishu credentials. The current bridge reads `CHUANG_PROVIDER_ENV_FILE` and expects that external file to define `CODEX_PPTOKEN_API_KEY=<set>`.
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
node scripts/chuang-feishu-live-preflight.js \
  --env-file /home/user/.codex-im/chuang-feishu-bridge.env \
  --json
cargo run --quiet -- doctor --config config.toml
CHUANG_PROVIDER_ENV_FILE=/home/user/.config/chuang-agent/provider.env \
  scripts/chuang-app-server-health.sh
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
node scripts/chuang-feishu-command-smoke.js
```

The live preflight is a local-only readiness gate. It checks the Chuang-scoped
Feishu env file, workspace/config, `channel feishu-check`, app-server diagnostic
health, bridge command smoke, provider env file presence, and session state path
access. It does not connect to Feishu, send messages, write the session store,
modify services, or print secret values. For a fixture-backed contract test, run:

```bash
node scripts/chuang-feishu-live-preflight-smoke.js
```

Expected live preflight evidence:

- Top-level `evidence.operation_mode=local_readonly_preflight`.
- Top-level `evidence.live_feishu_connection_attempted=false`.
- Top-level `evidence.live_feishu_message_send_attempted=false`.
- Top-level `evidence.session_store_write_attempted=false`.
- Top-level `evidence.service_modify_attempted=false`.
- `env_source_isolation` shows no Codex/Hermes Feishu credential names in the Chuang env file and reports inherited forbidden env names only as `<set_ignored>` or `<unset>`.
- `provider_env_file` reports provider secret variable states as `<set>/<missing>` only, and must not contain `CHUANG_FEISHU_*` credential names.
- `session_store_access.method=fs_access_only_no_write`; if a state file exists, the preflight may read only shape metadata such as version and binding count.

Expected root status:

- provider: `openai_compatible`
- subagent: `queued_external`
- actuator: `command`
- control plane: `command`
- placeholder warnings: `none`
- Feishu connection mode: `websocket`

Expected `feishu-check` fields:

- `env_file_is_chuang_scoped=true`
- `env_file_scope_warnings=[]`
- `workspace_root_exists=true`
- `workspace_config_exists=true`
- `connection_mode_ok=true`
- `has_legacy_names=false`
- `legacy_var_names=[]`

## Bot-Side Requirements

- Ignore non-text inbound interactive messages or convert them explicitly before calling Chuang.
- Preserve Feishu message id as `message_id`.
- Preserve Feishu sender id as `sender_id`.
- Use a stable per-chat or per-topic thread id.
- Forward plain text inbound content to Chuang; outbound replies may render as interactive cards and must fall back to text if card send fails.
- Return errors as short operational messages without secrets.

## Local Templates

```text
ops/systemd/chuang-feishu-bridge.env.example
ops/systemd/chuang-feishu-bridge.service.example
scripts/chuang-feishu-bridge.sh
```

The bridge script is a long-connection runtime. It validates the Chuang-only env file, opens the Feishu websocket client, and forwards plain text to Chuang `app-server`.

The bridge also sources `CHUANG_PROVIDER_ENV_FILE` before it starts the Node process. Keep this provider env outside the repository, with file mode `600`, and store only variable assignments such as `CODEX_PPTOKEN_API_KEY=<set>`. Do not put provider keys into the Feishu env template or `config.toml`.

The bridge handles `/new`, `/session`, `/health` and `/help` locally as bridge commands. `/new` starts a fresh app-server thread and binds the current Feishu chat to it; `/session` reports that binding; `/health` reports local bridge/app-server/provider-env status with secret values shown only as `<set>` or `<missing>`; `/help` lists the local commands. App-server failures during `/new` or normal `turn/start` are returned to Feishu as sanitized operational messages. These commands are not forwarded as user tasks to the Agent runtime.

## First Live Test

1. Send a text message to the new Chuang bot.
2. Confirm the response does not contain `fake-responder`.
3. Send `/session` and confirm the reported thread matches the active chat binding.
4. Send `/health` and confirm it shows bridge/app-server/provider-env status using only `<set>/<missing>` secret states.
5. Confirm Chuang writes session memory for the thread.
6. Confirm Codex Feishu and Hermes Feishu still respond on their own channels.
7. Confirm no service file for Codex or Hermes changed.
