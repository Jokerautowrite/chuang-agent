#!/usr/bin/env bash
set -euo pipefail

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
FORMAT="text"
STATUS_ARGS=()
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"

usage() {
  cat <<'EOF'
usage: scripts/chuang-provider-readiness-check.sh [--json] [--config PATH]

Readonly provider readiness preflight.

Boundaries:
  reads status --json only
  connects_real_provider=false
  prints_secret_values=false
  api_key_state is reduced to <set>/<missing>
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    --config)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --config" >&2
        exit 2
      fi
      STATUS_ARGS+=("--config" "$2")
      shift
      ;;
    --config=*)
      STATUS_ARGS+=("--config" "${1#--config=}")
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

cd "$ROOT"

if [ -f "$PROVIDER_ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$PROVIDER_ENV_FILE"
  set +a
fi

status_json="$(cargo run --quiet -- status "${STATUS_ARGS[@]}" --json)"

STATUS_JSON="$status_json" FORMAT="$FORMAT" python3 - <<'PY'
import json
import os
import sys

FORMAT = os.environ["FORMAT"]


def sanitized_api_key_state(raw):
    text = str(raw or "").strip()
    if not text or text == "none" or "<missing" in text:
        return "<missing>"
    return "<set>"


def timeout_label(value):
    return "none" if value is None else str(value)


try:
    status = json.loads(os.environ["STATUS_JSON"])
except json.JSONDecodeError as exc:
    print(f"provider_readiness_check_error: invalid status --json output: {exc}", file=sys.stderr)
    sys.exit(2)

readiness = status.get("provider_readiness") or {}
config = status.get("config") or {}

provider_kind = readiness.get("provider_kind") or config.get("provider_kind") or "unknown"
transport = readiness.get("transport") or "unknown"
request_timeout_ms = readiness.get("request_timeout_ms")
api_key_state = sanitized_api_key_state(readiness.get("api_key_state"))
overall_state = readiness.get("overall_state") or "unknown"
next_action = readiness.get("next_action") or "inspect status --json provider_readiness"
current = readiness.get("current") or "provider readiness unavailable in status --json"
placeholder_warning_count = int(readiness.get("placeholder_warning_count") or 0)

blocked = (
    api_key_state == "<missing>"
    or overall_state in {"partial", "blocked"}
    or placeholder_warning_count > 0
)
blocked_reason = None
if api_key_state == "<missing>":
    blocked_reason = "provider_api_key_env_missing"
elif overall_state in {"partial", "blocked"}:
    blocked_reason = f"provider_readiness_{overall_state}"
elif placeholder_warning_count > 0:
    blocked_reason = "provider_placeholder_warnings_present"

result = {
    "schema_version": 1,
    "readonly": True,
    "source_status_surface": "cargo run --quiet -- status --json",
    "connects_real_provider": False,
    "prints_secret_values": False,
    "ok": not blocked,
    "provider_kind": provider_kind,
    "transport": transport,
    "request_timeout_ms": request_timeout_ms,
    "api_key_state": api_key_state,
    "overall_state": overall_state,
    "placeholder_warning_count": placeholder_warning_count,
    "current": current,
    "next_action": next_action,
    "blocked_reason": blocked_reason,
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(f"provider_readiness_check: ok={str(result['ok']).lower()} state={overall_state}")
    print(f"source_status_surface: {result['source_status_surface']}")
    print(f"provider_kind: {provider_kind}")
    print(f"transport: {transport}")
    print(f"request_timeout_ms: {timeout_label(request_timeout_ms)}")
    print(f"api_key_state: {api_key_state}")
    print("connects_real_provider: false")
    print("prints_secret_values: false")
    print(f"current: {current}")
    print(f"next_action: {next_action}")
    if blocked_reason:
        print(f"blocked_reason: {blocked_reason}")

sys.exit(1 if blocked else 0)
PY
