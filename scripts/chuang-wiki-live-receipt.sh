#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-wiki-live-receipt.sh [--json]

Readonly wiki live receipt collector.

Environment overrides:
  CHUANG_WIKI_LIVE_ENDPOINT
  CHUANG_WIKI_LIVE_TOKEN
  CHUANG_WIKI_QUERY
  CHUANG_WIKI_LIMIT
  CHUANG_WIKI_REQUEST_ID
  CHUANG_WIKI_TIMEOUT_SEC

Readonly boundaries:
  source=wiki
  read_only=true
  writes_automatically=false
  global_real_live_ready=false
USAGE
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
export CHUANG_WIKI_LIVE_ENDPOINT="${CHUANG_WIKI_LIVE_ENDPOINT:-}"
export CHUANG_WIKI_LIVE_TOKEN="${CHUANG_WIKI_LIVE_TOKEN:-}"
export CHUANG_WIKI_QUERY="${CHUANG_WIKI_QUERY:-live receipt probe}"
export CHUANG_WIKI_LIMIT="${CHUANG_WIKI_LIMIT:-1}"
export CHUANG_WIKI_REQUEST_ID="${CHUANG_WIKI_REQUEST_ID:-wiki-live-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
export CHUANG_WIKI_TIMEOUT_SEC="${CHUANG_WIKI_TIMEOUT_SEC:-5}"

python3 - <<'PY'
import json
import os
import urllib.error
import urllib.request


def state_from_secret(value: str) -> str:
    return "<set>" if str(value or "").strip() else "<missing>"


def parse_limit(raw: str) -> int:
    try:
        value = int(str(raw).strip())
        return value if value > 0 else 1
    except (TypeError, ValueError):
        return 1


fmt = os.environ["FORMAT"]
endpoint = str(os.environ.get("CHUANG_WIKI_LIVE_ENDPOINT", "")).strip()
token = str(os.environ.get("CHUANG_WIKI_LIVE_TOKEN", "")).strip()
query = str(os.environ.get("CHUANG_WIKI_QUERY", "live receipt probe")).strip() or "live receipt probe"
limit = parse_limit(os.environ.get("CHUANG_WIKI_LIMIT", "1"))
request_id = str(os.environ.get("CHUANG_WIKI_REQUEST_ID", "")).strip()
timeout_sec = parse_limit(os.environ.get("CHUANG_WIKI_TIMEOUT_SEC", "5"))

token_state = state_from_secret(token)
endpoint_state = state_from_secret(endpoint)

payload = {
    "source": "wiki",
    "query": query,
    "limit": limit,
    "read_only": True,
}

result = {
    "schema_version": 1,
    "receipt_kind": "wiki_live_readonly_receipt",
    "request_id": request_id,
    "source": "wiki",
    "read_only": True,
    "writes_automatically": False,
    "token_state": token_state,
    "endpoint_state": endpoint_state,
    "acceptance_status": "blocked",
    "global_real_live_ready": False,
    "request_sent": False,
    "http_status": None,
    "blockers": [],
}

if endpoint_state == "<missing>":
    result["blockers"].append("missing_wiki_endpoint")
if token_state == "<missing>":
    result["blockers"].append("missing_wiki_token")

if not result["blockers"]:
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        endpoint,
        data=body,
        method="POST",
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {token}",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout_sec) as resp:
            result["http_status"] = int(getattr(resp, "status", 0) or 0)
            result["request_sent"] = True
    except urllib.error.HTTPError as err:
        result["http_status"] = int(getattr(err, "code", 0) or 0)
        result["request_sent"] = True
        result["blockers"].append("wiki_http_error")
    except (urllib.error.URLError, TimeoutError, OSError):
        result["request_sent"] = False
        result["blockers"].append("wiki_endpoint_unreachable")

if not result["blockers"] and result["request_sent"]:
    result["acceptance_status"] = "verified"
else:
    result["acceptance_status"] = "blocked"

if fmt == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print("wiki_live_receipt: readonly=true")
    print(f"request_id: {result['request_id']}")
    print(f"receipt_kind: {result['receipt_kind']}")
    print("source: wiki")
    print("read_only: true")
    print("writes_automatically: false")
    print(f"token_state: {result['token_state']}")
    print(f"acceptance_status: {result['acceptance_status']}")
    print("global_real_live_ready: false")
    print(f"request_sent: {str(result['request_sent']).lower()}")
    print(f"http_status: {result['http_status']}")
    blockers = ",".join(result["blockers"]) if result["blockers"] else "<none>"
    print(f"blockers: {blockers}")
PY
