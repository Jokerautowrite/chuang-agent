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

## Service Boundary

The systemd user service starts `scripts/chuang-app-server-service.sh`, which loads the existing
provider and live capability environment, creates `${XDG_RUNTIME_DIR}/chuang-agent`, and runs the
socket daemon at `${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock`. It does not use a FIFO, create an
HTTP listener, or own any Feishu connection strategy.

The service's `CHUANG_AGENT_WORKSPACE_ROOT` is the configuration root for provider, identity, rules,
and durable Chuang data. Each `turn/start.workspaceRoot` remains the caller's governed tool
workspace. These are deliberately separate: one canonical Chuang service can operate on the
caller's directory without treating that directory as a second runtime configuration.
