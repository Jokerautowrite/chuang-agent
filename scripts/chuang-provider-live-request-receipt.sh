#!/usr/bin/env bash
set -euo pipefail

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
FORMAT="text"
CONFIG_PATH=""
REQUEST_INPUT="${CHUANG_PROVIDER_LIVE_INPUT:-provider live receipt probe: reply with ok only}"
REQUEST_ID="${CHUANG_PROVIDER_REQUEST_ID:-provider-live-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"

usage() {
  cat <<'EOF'
usage: scripts/chuang-provider-live-request-receipt.sh [--json] [--config PATH] [--input TEXT] [--request-id ID]

Execute one bounded provider live request and emit a structured receipt.

Boundaries:
  connects_real_provider=true
  prints_secret_values=false
  request_path_must_be=/v1/responses
  readiness_only_status_is_not_enough=true
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
      CONFIG_PATH="$2"
      shift
      ;;
    --config=*)
      CONFIG_PATH="${1#--config=}"
      ;;
    --input)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --input" >&2
        exit 2
      fi
      REQUEST_INPUT="$2"
      shift
      ;;
    --input=*)
      REQUEST_INPUT="${1#--input=}"
      ;;
    --request-id)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --request-id" >&2
        exit 2
      fi
      REQUEST_ID="$2"
      shift
      ;;
    --request-id=*)
      REQUEST_ID="${1#--request-id=}"
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

if [ -z "$CONFIG_PATH" ]; then
  CONFIG_PATH="$ROOT/config.toml"
fi

cd "$ROOT"

if [ -f "$PROVIDER_ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$PROVIDER_ENV_FILE"
  set +a
fi

set +e
run_output="$(cargo run --quiet -- run --config "$CONFIG_PATH" --input "$REQUEST_INPUT" 2>&1)"
run_status=$?
set -e

RUN_OUTPUT="$run_output" \
RUN_STATUS="$run_status" \
FORMAT="$FORMAT" \
REQUEST_ID="$REQUEST_ID" \
REQUEST_INPUT="$REQUEST_INPUT" \
python3 - <<'PY'
import datetime
import json
import os
import re
import sys
import urllib.parse

FORMAT = os.environ["FORMAT"]
RUN_OUTPUT = os.environ.get("RUN_OUTPUT", "")
RUN_STATUS = int(os.environ.get("RUN_STATUS", "1"))
REQUEST_ID = os.environ["REQUEST_ID"]
REQUEST_INPUT = os.environ.get("REQUEST_INPUT", "")


def sanitize_api_key_state(raw):
    text = str(raw or "").strip().lower()
    if not text or text == "<missing>" or text == "none":
        return "<missing>"
    return "<set>"


def parse_lines(raw):
    parsed = {}
    for line in raw.splitlines():
        if ": " not in line:
            continue
        key, value = line.split(": ", 1)
        key = key.strip()
        if not key:
            continue
        parsed[key] = value.strip()
    return parsed


def parse_trace(trace):
    data = {}
    for match in re.finditer(r"\b([a-z_]+)=([^ ]+)", trace):
        data[match.group(1)] = match.group(2)
    return data


fields = parse_lines(RUN_OUTPUT)
trace = fields.get("trace", "")
trace_fields = parse_trace(trace)

provider_transport = trace_fields.get("transport", "")
provider_kind = "openai_compatible" if provider_transport == "openai-compatible" else "unknown"
provider_id = fields.get("provider") or trace_fields.get("provider") or "unknown"
model_name = fields.get("model_name") or trace_fields.get("model") or "unknown"
transport_mode = fields.get("transport_mode") or trace_fields.get("transport_mode") or "unknown"

request_url = fields.get("request_url") or trace_fields.get("request_url")
request_method = fields.get("request_method", "POST")
request_path = None
if request_url:
    try:
        request_path = urllib.parse.urlparse(request_url).path or None
    except ValueError:
        request_path = None

status_code_text = fields.get("status_code") or trace_fields.get("status_code")
status_code = None
if status_code_text:
    try:
        status_code = int(status_code_text)
    except ValueError:
        status_code = None

provider_response_ok = (fields.get("provider_response_ok") or "").lower()
provider_fallback_used = (fields.get("provider_fallback_used") or "false").lower()
provider_failure_reason = fields.get("provider_failure_reason")
provider_failure_category = fields.get("provider_failure_category")
runtime_report_id = fields.get("runtime_report", "<missing>")
api_key_state = sanitize_api_key_state(trace_fields.get("api_key"))
response_body = fields.get("body", "")
response_summary = f"chars={len(response_body)} redacted=true"

blocked_reason = None
if RUN_STATUS != 0:
    blocked_reason = "cli_run_failed"
elif provider_fallback_used == "true":
    blocked_reason = "provider_fallback_used"
elif not request_url:
    blocked_reason = "request_url_missing"
elif request_path != "/v1/responses":
    blocked_reason = f"request_path_not_responses:{request_path or '<missing>'}"
elif provider_response_ok == "false":
    blocked_reason = provider_failure_reason or "provider_response_not_ok"
elif status_code is not None and not (200 <= status_code <= 299):
    blocked_reason = f"provider_http_status_non_2xx:{status_code}"

ok = blocked_reason is None

result = {
    "schema_version": 1,
    "tested_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    "request_id": REQUEST_ID,
    "ok": ok,
    "status": "verified" if ok else "blocked",
    "readonly": False,
    "connects_real_provider": True,
    "does_not_call_provider": False,
    "prints_secret_values": False,
    "provider_kind": provider_kind,
    "provider_id": provider_id,
    "model_name": model_name,
    "transport_mode": transport_mode,
    "api_key_state": api_key_state,
    "request_method": request_method,
    "request_url": request_url or "<missing>",
    "request_path": request_path or "<missing>",
    "status_code": status_code if status_code is not None else "<missing>",
    "provider_response_ok": provider_response_ok if provider_response_ok else "<missing>",
    "provider_fallback_used": provider_fallback_used,
    "provider_failure_reason": provider_failure_reason or "<none>",
    "provider_failure_category": provider_failure_category or "<none>",
    "runtime_report_id": runtime_report_id,
    "response_summary": response_summary,
    "request_input_chars": len(REQUEST_INPUT),
    "blocked_reason": blocked_reason or "<none>",
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(f"provider_live_request_receipt: ok={str(ok).lower()} status={result['status']}")
    print(f"request_id: {result['request_id']}")
    print(f"provider: {provider_id}")
    print(f"model_name: {model_name}")
    print(f"transport_mode: {transport_mode}")
    print(f"api_key_state: {api_key_state}")
    print(f"request_method: {request_method}")
    print(f"request_url: {result['request_url']}")
    print(f"request_path: {result['request_path']}")
    print(f"status_code: {result['status_code']}")
    print(f"provider_response_ok: {result['provider_response_ok']}")
    print(f"runtime_report_id: {runtime_report_id}")
    print(f"response_summary: {response_summary}")
    if blocked_reason:
        print(f"blocked_reason: {blocked_reason}")
    print("connects_real_provider: true")
    print("does_not_call_provider: false")
    print("prints_secret_values: false")

sys.exit(0 if ok else 1)
PY
