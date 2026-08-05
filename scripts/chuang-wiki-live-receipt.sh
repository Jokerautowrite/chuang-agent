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
  CHUANG_WIKI_LOCAL_ROOT
  CHUANG_WIKI_SOURCE_MODE   http|local (default: http, falls back to local only when http is not configured)
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
export CHUANG_WIKI_LOCAL_ROOT="${CHUANG_WIKI_LOCAL_ROOT:-/opt/agent-hub/data/brain/wiki}"
export CHUANG_WIKI_SOURCE_MODE="${CHUANG_WIKI_SOURCE_MODE:-http}"
export CHUANG_WIKI_QUERY="${CHUANG_WIKI_QUERY:-live receipt probe}"
export CHUANG_WIKI_LIMIT="${CHUANG_WIKI_LIMIT:-1}"
export CHUANG_WIKI_REQUEST_ID="${CHUANG_WIKI_REQUEST_ID:-wiki-live-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
export CHUANG_WIKI_TIMEOUT_SEC="${CHUANG_WIKI_TIMEOUT_SEC:-5}"

python3 - <<'PY'
import json
import os
import pathlib
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
local_root = str(os.environ.get("CHUANG_WIKI_LOCAL_ROOT", "")).strip()
source_mode = str(os.environ.get("CHUANG_WIKI_SOURCE_MODE", "http")).strip()
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
    "local_root_state": "<missing>",
    "source_mode": source_mode,
    "source_kind": "<unknown>",
    "evidence_path": "<missing>",
    "acceptance_status": "blocked",
    "global_real_live_ready": False,
    "request_sent": False,
    "http_status": None,
    "blockers": [],
}

def query_local_wiki(root: str, search_query: str, limit: int):
    root_path = pathlib.Path(root)
    if not root_path.is_dir():
        return None, f"local_wiki_root_missing:{root}"
    index_path = root_path / "index.md"
    if not index_path.is_file():
        return None, "local_wiki_index_missing"
    try:
        content = index_path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        return None, f"local_wiki_read_error:{type(exc).__name__}"
    title = ""
    for line in content.splitlines()[:20]:
        if line.startswith("title:"):
            title = line.split(":", 1)[1].strip().strip('"')
            break
    lines = [line.strip() for line in content.splitlines() if line.strip()]
    matches = [line for line in lines if search_query.lower() in line.lower()]
    return {
        "source": "wiki",
        "local_root": str(root_path),
        "index": str(index_path),
        "title": title,
        "total_lines": len(lines),
        "matched_lines": len(matches),
        "matches_preview": matches[:limit],
        "read_only": True,
    }, None


use_http = endpoint_state == "<set>" and token_state == "<set>"
use_local = source_mode == "local" or (not use_http)

if use_http and not use_local:
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
            result["source_kind"] = "http"
    except urllib.error.HTTPError as err:
        result["http_status"] = int(getattr(err, "code", 0) or 0)
        result["request_sent"] = True
        result["blockers"].append("wiki_http_error")
    except (urllib.error.URLError, TimeoutError, OSError):
        result["request_sent"] = False
        result["blockers"].append("wiki_endpoint_unreachable")
elif use_local:
    result["local_root_state"] = "<set>" if local_root else "<missing>"
    if not local_root:
        result["blockers"].append("missing_wiki_local_root")
    else:
        local_result, local_error = query_local_wiki(local_root, query, limit)
        if local_error:
            result["blockers"].append(local_error)
            result["request_sent"] = False
        else:
            result["local_evidence"] = local_result
            result["evidence_path"] = local_result["index"]
            result["request_sent"] = True
            result["source_kind"] = "local_filesystem"
else:  # pragma: no cover - defensive; use_local covers all non-http cases
    result["blockers"].append("missing_wiki_endpoint")
    result["blockers"].append("missing_wiki_token")

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
