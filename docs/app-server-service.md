# App Server Service

Chuang app-server is a JSON-lines stdin/stdout protocol server. The dedicated Feishu plugin can either spawn it as a child process or supervise it through a Chuang-only service.

The app-server does not contain Feishu credentials and does not reuse Codex or Hermes bridges.

## Health Check

```bash
cargo run -- app-server health --workspace-root /home/user/projects/chuang-agent --json
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

## Current Boundary

The service template supervises the stdio app-server process and sends logs to journald. It does not create an HTTP or WebSocket listener. A channel plugin still needs to own the connection strategy.
