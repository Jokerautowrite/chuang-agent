#!/bin/sh
set -eu

command="${1:-}"
format="${2:-}"

if [ "$format" != "--json" ]; then
  printf '%s\n' "usage: sh scripts/chuang-control-adapter-example.sh list|apply --json" >&2
  exit 2
fi

if [ "$command" = "list" ]; then
  printf '%s\n' '[
    {
      "unit_id": "hermes-xiaochuang",
      "display_name": "小创",
      "kind": "agent",
      "status": "Running",
      "model_name": "gpt-5.4",
      "metadata": {
        "adapter": "example",
        "channel": "hermes",
        "safe_mode": "true"
      }
    },
    {
      "unit_id": "hermes-xiaocheng",
      "display_name": "小承",
      "kind": "agent",
      "status": "Running",
      "model_name": "gpt-5.4",
      "metadata": {
        "adapter": "example",
        "channel": "hermes",
        "safe_mode": "true"
      }
    },
    {
      "unit_id": "openclaw-xiaoyun",
      "display_name": "小云",
      "kind": "agent",
      "status": "Stopped",
      "model_name": "gpt-5.4",
      "metadata": {
        "adapter": "example",
        "channel": "openclaw",
        "safe_mode": "true"
      }
    },
    {
      "unit_id": "codex-xiaoce",
      "display_name": "小策",
      "kind": "agent",
      "status": "Running",
      "model_name": "gpt-5.5",
      "metadata": {
        "adapter": "example",
        "channel": "feishu",
        "safe_mode": "true"
      }
    },
    {
      "unit_id": "codex-feishu-bot.service",
      "display_name": "Codex Feishu Bridge",
      "kind": "service",
      "status": "Running",
      "model_name": null,
      "metadata": {
        "adapter": "example",
        "channel": "feishu",
        "safe_mode": "true"
      }
    },
    {
      "unit_id": "chuang-demo-agent",
      "display_name": "Chuang Demo Agent",
      "kind": "agent",
      "status": "Running",
      "model_name": "gpt-5.5",
      "metadata": {
        "adapter": "example",
        "safe_mode": "true"
      }
    },
    {
      "unit_id": "chuang-demo-service",
      "display_name": "Chuang Demo Service",
      "kind": "service",
      "status": "Stopped",
      "model_name": null,
      "metadata": {
        "adapter": "example",
        "safe_mode": "true"
      }
    }
  ]'
  exit 0
fi

if [ "$command" = "apply" ]; then
  python3 -c '
import json
import sys

request = json.load(sys.stdin)
unit_id = request.get("unit_id", "")
action = request.get("action", "")
model_name = request.get("model_name")

known_units = {
    "hermes-xiaochuang",
    "hermes-xiaocheng",
    "openclaw-xiaoyun",
    "codex-xiaoce",
    "codex-feishu-bot.service",
    "chuang-demo-agent",
    "chuang-demo-service",
}

if unit_id not in known_units:
    print(f"unknown unit_id: {unit_id}", file=sys.stderr)
    sys.exit(2)

next_status = "Running"
if action == "stop":
    next_status = "Stopped"
elif action in {"start", "restart", "change_model"}:
    next_status = "Running"
else:
    print(f"unsupported action: {action}", file=sys.stderr)
    sys.exit(2)

if action == "change_model" and not model_name:
    print("change_model requires model_name", file=sys.stderr)
    sys.exit(2)

receipt = {
    "unit_id": unit_id,
    "action": action,
    "previous_status": "Running",
    "next_status": next_status,
    "model_name": model_name if action == "change_model" else None,
    "message": "example adapter accepted request; no real service was changed",
}
print(json.dumps(receipt, ensure_ascii=False))
'
  exit 0
fi

printf 'unsupported command: %s\n' "$command" >&2
exit 2
