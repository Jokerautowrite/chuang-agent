#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-cdp-readonly-session-receipt.sh [--json]

Controlled CDP readonly session receipt collector.
Checks only CHUANG_CDP_PORT and CDP /json metadata.
Does not open browser windows, click, type, or read DOM via websocket.

Environment overrides:
  CHUANG_AGENT_ROOT
  CHUANG_CDP_PORT
  CHUANG_CDP_READONLY_RECEIPT_REQUEST_ID
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

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
REQUEST_ID="${CHUANG_CDP_READONLY_RECEIPT_REQUEST_ID:-cdp-readonly-session-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
CDP_PORT="${CHUANG_CDP_PORT:-}"

export FORMAT ROOT REQUEST_ID CDP_PORT

python3 - <<'PY'
import datetime
import hashlib
import json
import os
import urllib.error
import urllib.parse
import urllib.request

FORMAT = os.environ["FORMAT"]
ROOT = os.environ["ROOT"]
REQUEST_ID = os.environ["REQUEST_ID"]
CDP_PORT_RAW = os.environ.get("CDP_PORT", "").strip()


def hash_ref(value: str) -> str:
    digest = hashlib.sha256(value.encode("utf-8", errors="ignore")).hexdigest()
    return f"sha256:{digest[:12]}"


def sanitize_url_ref(value: str):
    text = str(value or "").strip()
    if not text:
        return {
            "url_ref": "<missing>",
            "url_scheme": "<missing>",
            "url_host": "<missing>",
        }
    parsed = urllib.parse.urlsplit(text)
    host = parsed.netloc or "<missing>"
    scheme = parsed.scheme or "<missing>"
    canonical = f"{scheme}://{host}{parsed.path or ''}"
    return {
        "url_ref": hash_ref(canonical),
        "url_scheme": scheme,
        "url_host": host,
    }


def sanitize_title_ref(value: str):
    text = str(value or "")
    stripped = text.strip()
    if not stripped:
        return {
            "title_ref": "<missing>",
            "title_chars": 0,
        }
    return {
        "title_ref": hash_ref(stripped),
        "title_chars": len(stripped),
    }


def fetch_cdp_metadata(port_text: str):
    if not port_text:
        return {
            "enabled": False,
            "port": "<missing>",
            "metadata_state": "missing_cdp_port",
            "target_count": 0,
            "target_summaries": [],
            "error": "missing_chuang_cdp_port",
        }

    try:
        port = int(port_text)
    except ValueError:
        return {
            "enabled": False,
            "port": port_text,
            "metadata_state": "invalid_cdp_port",
            "target_count": 0,
            "target_summaries": [],
            "error": "invalid_chuang_cdp_port",
        }

    if port <= 0 or port > 65535:
        return {
            "enabled": False,
            "port": port_text,
            "metadata_state": "invalid_cdp_port",
            "target_count": 0,
            "target_summaries": [],
            "error": "invalid_chuang_cdp_port",
        }

    url = f"http://127.0.0.1:{port}/json"
    try:
        with urllib.request.urlopen(url, timeout=3) as response:
            body = response.read().decode("utf-8", errors="replace")
    except (urllib.error.URLError, TimeoutError, OSError):
        return {
            "enabled": True,
            "port": port,
            "metadata_state": "unavailable",
            "target_count": 0,
            "target_summaries": [],
            "error": "cdp_metadata_unavailable",
        }

    try:
        payload = json.loads(body)
    except json.JSONDecodeError:
        return {
            "enabled": True,
            "port": port,
            "metadata_state": "invalid_json",
            "target_count": 0,
            "target_summaries": [],
            "error": "cdp_metadata_invalid_json",
        }

    if not isinstance(payload, list):
        return {
            "enabled": True,
            "port": port,
            "metadata_state": "unexpected_shape",
            "target_count": 0,
            "target_summaries": [],
            "error": "cdp_metadata_unexpected_shape",
        }

    summaries = []
    for entry in payload[:10]:
        if not isinstance(entry, dict):
            continue
        kind = str(entry.get("type") or "<unknown>")
        target_id = str(entry.get("id") or "")
        url_info = sanitize_url_ref(entry.get("url") or "")
        title_info = sanitize_title_ref(entry.get("title") or "")
        summaries.append(
            {
                "target_type": kind,
                "target_ref": hash_ref(target_id) if target_id else "<missing>",
                **url_info,
                **title_info,
            }
        )

    return {
        "enabled": True,
        "port": port,
        "metadata_state": "ok",
        "target_count": len(payload),
        "target_summaries": summaries,
        "error": None,
    }


cdp = fetch_cdp_metadata(CDP_PORT_RAW)
blockers = []
if cdp.get("error"):
    blockers.append(str(cdp["error"]))

acceptance_status = "verified" if not blockers else "blocked"

readonly_boundaries = {
    "readonly": True,
    "checks_cdp_port_and_json_only": True,
    "reads_dom_websocket": False,
    "performs_desktop_actions": False,
    "performs_browser_actions": False,
    "global_real_live_ready": False,
    "writes_core_memory": False,
    "connects_real_provider": False,
    "connects_real_feishu": False,
    "modifies_repo": False,
    "deletes_files": False,
}

result = {
    "schema_version": 1,
    "receipt_kind": "controlled_cdp_readonly_session_receipt",
    "tested_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    "request_id": REQUEST_ID,
    "workspace_root": ROOT,
    "readonly": True,
    "acceptance_status": acceptance_status,
    "can_mark_real_live_ready": False,
    "global_real_live_ready": False,
    "performs_desktop_actions": False,
    "performs_browser_actions": False,
    "readonly_boundaries": readonly_boundaries,
    "cdp_metadata": cdp,
    "blockers": sorted(set(blockers)),
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(f"controlled_cdp_readonly_session_receipt: acceptance_status={acceptance_status}")
    print(f"request_id: {REQUEST_ID}")
    print(f"cdp_port: {cdp['port']}")
    print(f"cdp_metadata_state: {cdp['metadata_state']}")
    print(f"cdp_target_count: {cdp['target_count']}")
    print("performs_browser_actions: false")
    print("performs_desktop_actions: false")
    print("global_real_live_ready: false")
    if blockers:
        print(f"blockers: {','.join(sorted(set(blockers)))}")
PY
