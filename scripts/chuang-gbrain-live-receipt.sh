#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-gbrain-live-receipt.sh [--json]

Readonly GBrain live receipt collector.

Environment overrides:
  CHUANG_GBRAIN_LIVE_ENDPOINT
  CHUANG_GBRAIN_LIVE_TOKEN
  CHUANG_GBRAIN_LOCAL_QUERY_CLI
  CHUANG_GBRAIN_READ_SOCKET
  CHUANG_GBRAIN_SOURCE_MODE   http|local (default: http, falls back to local only when http is not configured)
  CHUANG_GBRAIN_QUERY
  CHUANG_GBRAIN_LIMIT
  CHUANG_GBRAIN_REQUEST_ID
  CHUANG_GBRAIN_TIMEOUT_SEC

Readonly boundaries:
  source=gbrain
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
export CHUANG_GBRAIN_LIVE_ENDPOINT="${CHUANG_GBRAIN_LIVE_ENDPOINT:-}"
export CHUANG_GBRAIN_LIVE_TOKEN="${CHUANG_GBRAIN_LIVE_TOKEN:-}"
export CHUANG_GBRAIN_LOCAL_QUERY_CLI="${CHUANG_GBRAIN_LOCAL_QUERY_CLI:-/home/user/agent-hub/bin/agent-hub-brain-query}"
export CHUANG_GBRAIN_READ_SOCKET="${CHUANG_GBRAIN_READ_SOCKET:-/run/agent-hub/gbrain/read.sock}"
export CHUANG_GBRAIN_SOURCE_MODE="${CHUANG_GBRAIN_SOURCE_MODE:-http}"
export CHUANG_GBRAIN_QUERY="${CHUANG_GBRAIN_QUERY:-live receipt probe}"
export CHUANG_GBRAIN_LIMIT="${CHUANG_GBRAIN_LIMIT:-1}"
export CHUANG_GBRAIN_REQUEST_ID="${CHUANG_GBRAIN_REQUEST_ID:-gbrain-live-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
export CHUANG_GBRAIN_TIMEOUT_SEC="${CHUANG_GBRAIN_TIMEOUT_SEC:-5}"

python3 - <<'PY'
import json
import os
import shutil
import subprocess
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
endpoint = str(os.environ.get("CHUANG_GBRAIN_LIVE_ENDPOINT", "")).strip()
token = str(os.environ.get("CHUANG_GBRAIN_LIVE_TOKEN", "")).strip()
query_cli = str(os.environ.get("CHUANG_GBRAIN_LOCAL_QUERY_CLI", "")).strip()
read_socket = str(os.environ.get("CHUANG_GBRAIN_READ_SOCKET", "")).strip()
source_mode = str(os.environ.get("CHUANG_GBRAIN_SOURCE_MODE", "http")).strip()
query = str(os.environ.get("CHUANG_GBRAIN_QUERY", "live receipt probe")).strip() or "live receipt probe"
limit = parse_limit(os.environ.get("CHUANG_GBRAIN_LIMIT", "1"))
request_id = str(os.environ.get("CHUANG_GBRAIN_REQUEST_ID", "")).strip()
timeout_sec = parse_limit(os.environ.get("CHUANG_GBRAIN_TIMEOUT_SEC", "5"))

token_state = state_from_secret(token)
endpoint_state = state_from_secret(endpoint)

payload = {
    "source": "gbrain",
    "query": query,
    "limit": limit,
    "read_only": True,
}

result = {
    "schema_version": 1,
    "receipt_kind": "gbrain_live_readonly_receipt",
    "request_id": request_id,
    "source": "gbrain",
    "read_only": True,
    "writes_automatically": False,
    "token_state": token_state,
    "endpoint_state": endpoint_state,
    "read_socket_state": "<missing>",
    "source_mode": source_mode,
    "source_kind": "<unknown>",
    "query_receipt": "<missing>",
    "acceptance_status": "blocked",
    "global_real_live_ready": False,
    "request_sent": False,
    "http_status": None,
    "blockers": [],
}

def query_local_gbrain(cli_path: str, socket_path: str, search_query: str, limit: int):
    if not shutil.which(cli_path):
        return None, f"local_gbrain_cli_missing:{cli_path}"
    env = dict(os.environ)
    env["AGENT_HUB_BRAIN_READ_SOCKET"] = socket_path
    try:
        proc = subprocess.run(
            [cli_path, "semantic", search_query, str(limit)],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_sec,
            env=env,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return None, f"local_gbrain_query_error:{type(exc).__name__}"
    if proc.returncode != 0:
        return None, "local_gbrain_query_failed"
    try:
        parsed = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return None, "local_gbrain_query_unparseable"
    results = parsed.get("results") if isinstance(parsed, dict) else None
    if parsed.get("ok") is not True or not isinstance(results, list):
        return None, "local_gbrain_query_not_ok"
    return {
        "source": "gbrain",
        "socket": socket_path,
        "cli": cli_path,
        "mode": parsed.get("mode", "<missing>"),
        "result_count": len(results),
        "top_slug": results[0].get("slug") if results else "<missing>",
        "top_score": results[0].get("score") if results else None,
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
        result["blockers"].append("gbrain_http_error")
    except (urllib.error.URLError, TimeoutError, OSError):
        result["request_sent"] = False
        result["blockers"].append("gbrain_endpoint_unreachable")
elif use_local:
    result["read_socket_state"] = "<set>" if read_socket else "<missing>"
    if not query_cli or not read_socket:
        result["blockers"].append("missing_gbrain_local_query_path")
        result["request_sent"] = False
    else:
        local_result, local_error = query_local_gbrain(query_cli, read_socket, query, limit)
        if local_error:
            result["blockers"].append(local_error)
            result["request_sent"] = False
        else:
            result["local_evidence"] = local_result
            result["query_receipt"] = f"receipt://gbrain/{request_id}/local-semantic"
            result["request_sent"] = True
            result["source_kind"] = "local_unix_socket"
else:  # pragma: no cover - defensive
    result["blockers"].append("missing_gbrain_endpoint")
    result["blockers"].append("missing_gbrain_token")

if not result["blockers"] and result["request_sent"]:
    result["acceptance_status"] = "verified"
else:
    result["acceptance_status"] = "blocked"

if fmt == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print("gbrain_live_receipt: readonly=true")
    print(f"request_id: {result['request_id']}")
    print(f"receipt_kind: {result['receipt_kind']}")
    print("source: gbrain")
    print("read_only: true")
    print("writes_automatically: false")
    print(f"token_state: {result['token_state']}")
    print(f"endpoint_state: {result['endpoint_state']}")
    print(f"acceptance_status: {result['acceptance_status']}")
    print("global_real_live_ready: false")
    print(f"request_sent: {str(result['request_sent']).lower()}")
    print(f"http_status: {result['http_status']}")
    blockers = ",".join(result["blockers"]) if result["blockers"] else "<none>"
    print(f"blockers: {blockers}")
PY
