#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-provider-live-receipt.sh [--json]

Readonly template for a provider live request receipt.

Environment overrides:
  CHUANG_PROVIDER_KIND
  CHUANG_PROVIDER_TRANSPORT
  CHUANG_PROVIDER_API_KEY_STATE
  CHUANG_PROVIDER_REQUEST_ID
  CHUANG_PROVIDER_RUNTIME_REPORT_ID
  CHUANG_PROVIDER_LIVE_REQUEST_RECEIPT_REF
  CHUANG_PROVIDER_BLOCKED_REASON
  CHUANG_PROVIDER_NEXT_ACTION

Readonly boundaries:
  connects_real_provider=false
  prints_secret_values=false
  does_not_call_provider=true
  does_not_read_provider_readiness=true
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
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

export FORMAT
export CHUANG_PROVIDER_KIND="${CHUANG_PROVIDER_KIND:-<fill_after_test>}"
export CHUANG_PROVIDER_TRANSPORT="${CHUANG_PROVIDER_TRANSPORT:-<fill_after_test>}"
export CHUANG_PROVIDER_API_KEY_STATE="${CHUANG_PROVIDER_API_KEY_STATE:-<missing>}"
export CHUANG_PROVIDER_REQUEST_ID="${CHUANG_PROVIDER_REQUEST_ID:-<fill_after_test>}"
export CHUANG_PROVIDER_RUNTIME_REPORT_ID="${CHUANG_PROVIDER_RUNTIME_REPORT_ID:-<fill_after_test>}"
export CHUANG_PROVIDER_LIVE_REQUEST_RECEIPT_REF="${CHUANG_PROVIDER_LIVE_REQUEST_RECEIPT_REF:-<fill_after_test>}"
export CHUANG_PROVIDER_BLOCKED_REASON="${CHUANG_PROVIDER_BLOCKED_REASON:-<fill_after_test>}"
export CHUANG_PROVIDER_NEXT_ACTION="${CHUANG_PROVIDER_NEXT_ACTION:-<fill_after_test>}"

python3 - <<'PY'
import json
import os

FORMAT = os.environ["FORMAT"]


def sanitized_api_key_state(raw):
    text = str(raw or "").strip()
    if not text or text == "none" or "<missing" in text:
        return "<missing>"
    return "<set>"


result = {
    "schema_version": 1,
    "readonly": True,
    "connects_real_provider": False,
    "prints_secret_values": False,
    "provider_kind": os.environ["CHUANG_PROVIDER_KIND"].strip() or "<fill_after_test>",
    "transport": os.environ["CHUANG_PROVIDER_TRANSPORT"].strip() or "<fill_after_test>",
    "api_key_state": sanitized_api_key_state(os.environ["CHUANG_PROVIDER_API_KEY_STATE"]),
    "request_id": os.environ["CHUANG_PROVIDER_REQUEST_ID"].strip() or "<fill_after_test>",
    "runtime_report_id": os.environ["CHUANG_PROVIDER_RUNTIME_REPORT_ID"].strip() or "<fill_after_test>",
    "provider_live_request_receipt_ref": os.environ["CHUANG_PROVIDER_LIVE_REQUEST_RECEIPT_REF"].strip() or "<fill_after_test>",
    "blocked_reason": os.environ["CHUANG_PROVIDER_BLOCKED_REASON"].strip() or "<fill_after_test>",
    "next_action": os.environ["CHUANG_PROVIDER_NEXT_ACTION"].strip() or "<fill_after_test>",
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print("provider_live_receipt: readonly=true")
    print(f"provider_kind: {result['provider_kind']}")
    print(f"transport: {result['transport']}")
    print(f"api_key_state: {result['api_key_state']}")
    print(f"request_id: {result['request_id']}")
    print(f"runtime_report_id: {result['runtime_report_id']}")
    print(f"provider_live_request_receipt_ref: {result['provider_live_request_receipt_ref']}")
    print(f"blocked_reason: {result['blocked_reason']}")
    print(f"next_action: {result['next_action']}")
    print("connects_real_provider: false")
    print("prints_secret_values: false")
    print("does_not_call_provider: true")
    print("does_not_read_provider_readiness: true")
PY
