#!/bin/sh
set -eu

format="${1:-}"

if [ "$format" != "--json" ]; then
  printf '%s\n' "usage: sh scripts/chuang-actuator-adapter-example.sh --json" >&2
  exit 2
fi

python3 -c '
import json
import sys

request = json.load(sys.stdin)
action = request.get("action", "")

if action == "observe":
    target = request.get("observe_target")
    print(json.dumps({
        "observation": {
            "target": target,
            "summary": "example command actuator observation; no real desktop operation was performed",
            "evidence_ref": {"uri": "command-example://observation"}
        },
        "app_handle": None,
        "evidence_ref": None,
        "message": "observed"
    }, ensure_ascii=False))
    sys.exit(0)

if action == "open_app":
    app = (request.get("open_app") or {}).get("app_name", "")
    if not app:
        print("open_app requires app_name", file=sys.stderr)
        sys.exit(2)
    print(json.dumps({
        "observation": None,
        "app_handle": {
            "app_name": app,
            "handle_id": f"command-example://app/{app}"
        },
        "evidence_ref": None,
        "message": "accepted open_app without launching a real app"
    }, ensure_ascii=False))
    sys.exit(0)

if action in {"focus", "click", "input_text"}:
    print(json.dumps({
        "observation": None,
        "app_handle": None,
        "evidence_ref": None,
        "message": f"accepted {action} without performing a real desktop operation"
    }, ensure_ascii=False))
    sys.exit(0)

if action == "screenshot":
    print(json.dumps({
        "observation": None,
        "app_handle": None,
        "evidence_ref": {"uri": "command-example://screenshot"},
        "message": "returned example screenshot evidence"
    }, ensure_ascii=False))
    sys.exit(0)

print(f"unsupported actuator action: {action}", file=sys.stderr)
sys.exit(2)
'
