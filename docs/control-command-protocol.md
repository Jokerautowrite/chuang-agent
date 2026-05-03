# Command Control Plane Protocol

`CommandControlPlane` lets Chuang call an external executable for service/Agent control without putting systemd, desktop, Hermes, or OpenClaw details into the core.

## Config

```toml
control = "command"
program = "/path/to/chuang-control-adapter"
list_args = "list --json"
apply_args = "apply --json"
timeout_ms = 30000
```

`list_args` is called without stdin. `apply_args` receives one JSON object on stdin.
Arguments are split without a shell; token-leading single and double quotes can group values that contain spaces.
`timeout_ms` is optional and defaults to 30000. Chuang terminates only the command process it started when the timeout expires.

## List Output

The list command must print a JSON array:

```json
[
  {
    "unit_id": "codex-feishu-bot.service",
    "display_name": "Codex Feishu Bridge",
    "kind": "service",
    "status": "Running",
    "model_name": null,
    "metadata": {
      "channel": "systemd"
    }
  }
]
```

Supported `kind`: `service`, `agent`.

Supported `status`: `Running`, `Stopped`, `Failed`, `Unknown`.

## Apply Input

The apply command receives:

```json
{
  "unit_id": "codex-xiaoce",
  "action": "change_model",
  "reason": "user approved model switch",
  "model_name": "gpt-5.5"
}
```

Supported `action`: `start`, `stop`, `restart`, `change_model`.

`model_name` is only present for `change_model`.

## Apply Output

The apply command must print one JSON receipt:

```json
{
  "unit_id": "codex-xiaoce",
  "action": "change_model",
  "previous_status": "Running",
  "next_status": "Running",
  "model_name": "gpt-5.5",
  "message": "model switched"
}
```

Chuang still applies governance before calling `apply`. Dangerous changes require explicit approval at the CLI/control surface layer.

## Checked-In Example

The repo includes a safe example adapter that implements the protocol without touching real services:

```toml
control = "command"
program = "sh"
list_args = "./scripts/chuang-control-adapter-example.sh list --json"
apply_args = "./scripts/chuang-control-adapter-example.sh apply --json"
control_timeout_ms = 30000
```

Try it with:

```bash
cargo run -- control list --config config.example-control.toml --json
cargo run -- control apply --config config.example-control.toml \
  --unit chuang-demo-agent \
  --action change-model \
  --model gpt-5.4 \
  --reason "manual protocol smoke" \
  --approve
```

The example returns deterministic JSON and only says it accepted the request; it does not start, stop, restart, or reconfigure any real process.
