#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-browser-read-live-receipt.sh [--json]

Readonly browser_read live receipt evidence collector.

Environment overrides:
  CHUANG_AGENT_ROOT
  CHUANG_BROWSER_READ_RECEIPT_REQUEST_ID
  CHUANG_BROWSER_READ_RECEIPT_SKIP_STATUS=1
  CHUANG_BROWSER_READ_RECEIPT_STATUS_JSON
  CHUANG_CDP_PORT

Readonly boundaries:
  readonly=true
  desktop_read_is_separate=true
  browser_read_does_not_use_desktop_read=true
  performs_desktop_actions=false
  performs_browser_actions=false
  connects_real_provider=false
  connects_real_wiki=false
  connects_real_gbrain=false
  writes_core_memory=false
  prints_secret_values=false
  modifies_repo=false
  deletes_files=false
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
REQUEST_ID="${CHUANG_BROWSER_READ_RECEIPT_REQUEST_ID:-browser-read-live-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
SKIP_STATUS="${CHUANG_BROWSER_READ_RECEIPT_SKIP_STATUS:-0}"
CDP_PORT="${CHUANG_CDP_PORT:-}"

STATUS_JSON_RAW=""
STATUS_ERROR=""
STATUS_SOURCE="cargo run --quiet -- status --json"

if [ -n "${CHUANG_BROWSER_READ_RECEIPT_STATUS_JSON:-}" ]; then
  STATUS_JSON_RAW="${CHUANG_BROWSER_READ_RECEIPT_STATUS_JSON}"
  STATUS_SOURCE="CHUANG_BROWSER_READ_RECEIPT_STATUS_JSON"
elif [ "$SKIP_STATUS" != "1" ]; then
  cd "$ROOT"
  set +e
  STATUS_JSON_RAW="$(cargo run --quiet -- status --json 2>&1)"
  status_exit=$?
  set -e
  if [ "$status_exit" -ne 0 ]; then
    STATUS_ERROR="status_command_failed"
  fi
fi

export FORMAT ROOT REQUEST_ID SKIP_STATUS CDP_PORT STATUS_JSON_RAW STATUS_ERROR STATUS_SOURCE

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
SKIP_STATUS = os.environ.get("SKIP_STATUS", "0") == "1"
CDP_PORT_RAW = os.environ.get("CDP_PORT", "").strip()
if not CDP_PORT_RAW:
    state_dir = os.environ.get(
        "CHUANG_HEADLESS_STATE_DIR",
        os.path.join(
            os.environ.get("XDG_STATE_HOME", os.path.join(os.path.expanduser("~"), ".local", "state")),
            "chuang-agent",
            "headless-chrome",
        ),
    )
    port_path = os.path.join(state_dir, "cdp.port")
    try:
        with open(port_path, "r", encoding="utf-8") as handle:
            raw_port = handle.read().strip()
        if raw_port:
            CDP_PORT_RAW = raw_port
    except OSError:
        pass
STATUS_JSON_RAW = os.environ.get("STATUS_JSON_RAW", "")
STATUS_ERROR = os.environ.get("STATUS_ERROR", "")
STATUS_SOURCE = os.environ.get("STATUS_SOURCE", "cargo run --quiet -- status --json")


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


def parse_status(raw: str, had_error: str):
    if SKIP_STATUS:
        return {
            "state": "skipped",
            "available": None,
            "adapter_kind": "<unknown>",
            "adapter_state": "<unknown>",
            "adapter_reason_code": "status_skipped",
            "adapter_reason": "status surface skipped by CHUANG_BROWSER_READ_RECEIPT_SKIP_STATUS=1",
            "browser_read_boundary": "<unknown>",
            "browser_read_does_not_use_desktop_read": True,
            "capabilities": [],
            "current": "status surface skipped",
            "next_action": "unset CHUANG_BROWSER_READ_RECEIPT_SKIP_STATUS to inspect browser_read adapter status",
            "error": None,
        }

    if had_error:
        return {
            "state": "error",
            "available": None,
            "adapter_kind": "<unknown>",
            "adapter_state": "<unknown>",
            "adapter_reason_code": "status_command_failed",
            "adapter_reason": "status --json command failed",
            "browser_read_boundary": "<unknown>",
            "browser_read_does_not_use_desktop_read": True,
            "capabilities": [],
            "current": "status command failed",
            "next_action": "fix local status command before collecting browser_read live receipt",
            "error": "status_command_failed",
        }

    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError:
        return {
            "state": "error",
            "available": None,
            "adapter_kind": "<unknown>",
            "adapter_state": "<unknown>",
            "adapter_reason_code": "status_json_parse_failed",
            "adapter_reason": "status --json output could not be parsed",
            "browser_read_boundary": "<unknown>",
            "browser_read_does_not_use_desktop_read": True,
            "capabilities": [],
            "current": "status json parse failed",
            "next_action": "repair status --json output and rerun receipt",
            "error": "status_json_parse_failed",
        }

    browser = parsed.get("browser_readiness") if isinstance(parsed, dict) else None
    if not isinstance(browser, dict):
        return {
            "state": "error",
            "available": None,
            "adapter_kind": "<unknown>",
            "adapter_state": "<unknown>",
            "adapter_reason_code": "browser_readiness_missing",
            "adapter_reason": "browser_readiness is missing in status --json",
            "browser_read_boundary": "<unknown>",
            "browser_read_does_not_use_desktop_read": True,
            "capabilities": [],
            "current": "browser_readiness missing",
            "next_action": "update status surface to include browser_readiness",
            "error": "browser_readiness_missing",
        }

    return {
        "state": "ok",
        "available": bool(browser.get("browser_read_adapter_available")),
        "adapter_kind": str(browser.get("browser_read_adapter_kind") or "<missing>"),
        "adapter_state": str(browser.get("browser_read_state") or "<missing>"),
        "adapter_reason_code": str(browser.get("browser_read_reason_code") or "<missing>"),
        "adapter_reason": str(browser.get("browser_read_reason") or "<missing>"),
        "browser_read_boundary": str(browser.get("browser_read_boundary") or "<missing>"),
        "browser_read_does_not_use_desktop_read": bool(
            browser.get("browser_read_does_not_use_desktop_read", True)
        ),
        "capabilities": [str(item) for item in browser.get("browser_read_capabilities") or []],
        "current": str(browser.get("current") or "<missing>"),
        "next_action": str(browser.get("next_action") or "<missing>"),
        "error": None,
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
            "error": "cdp_metadata_unavailable",
        }

    if not isinstance(payload, list):
        return {
            "enabled": True,
            "port": port,
            "metadata_state": "unexpected_shape",
            "target_count": 0,
            "target_summaries": [],
            "error": "cdp_metadata_unavailable",
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


status = parse_status(STATUS_JSON_RAW, STATUS_ERROR)
cdp = fetch_cdp_metadata(CDP_PORT_RAW)

blockers = []
if status["error"]:
    blockers.append(status["error"])
if status["available"] is not True:
    blockers.append("browser_read_adapter_unavailable")
if cdp.get("error"):
    blockers.append(str(cdp["error"]))

acceptance_status = "verified" if not blockers else "blocked"

readonly_boundaries = {
    "readonly": True,
    "desktop_read_is_separate": True,
    "browser_read_does_not_use_desktop_read": True,
    "performs_desktop_actions": False,
    "performs_browser_actions": False,
    "connects_real_provider": False,
    "connects_real_wiki": False,
    "connects_real_gbrain": False,
    "writes_core_memory": False,
    "prints_secret_values": False,
    "modifies_repo": False,
    "deletes_files": False,
}

result = {
    "schema_version": 1,
    "receipt_kind": "browser_read_live_readonly_receipt",
    "tested_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    "request_id": REQUEST_ID,
    "workspace_root": ROOT,
    "readonly": True,
    "acceptance_status": acceptance_status,
    "can_mark_real_live_ready": False,
    "global_real_live_ready": False,
    "cannot_mark_complete_without_operator_evidence": True,
    "readonly_boundaries": readonly_boundaries,
    "browser_read_evidence": {
        "source_status_surface": STATUS_SOURCE,
        "status_collection": status["state"],
        "adapter_available": status["available"],
        "adapter_kind": status["adapter_kind"],
        "adapter_state": status["adapter_state"],
        "adapter_reason_code": status["adapter_reason_code"],
        "adapter_reason": status["adapter_reason"],
        "browser_read_boundary": status["browser_read_boundary"],
        "browser_read_does_not_use_desktop_read": status[
            "browser_read_does_not_use_desktop_read"
        ],
        "capabilities": status["capabilities"],
        "status_current": status["current"],
        "status_next_action": status["next_action"],
        "cdp_metadata": cdp,
    },
    "desktop_read_is_separate": True,
    "browser_read_does_not_use_desktop_read": True,
    "performs_desktop_actions": False,
    "performs_browser_actions": False,
    "blockers": sorted(set(blockers)),
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(f"browser_read_live_receipt: acceptance_status={acceptance_status}")
    print(f"request_id: {REQUEST_ID}")
    print(f"adapter_available: {str(status['available']).lower() if status['available'] is not None else '<unknown>'}")
    print(f"adapter_kind: {status['adapter_kind']}")
    print(f"adapter_state: {status['adapter_state']}")
    print(f"cdp_port: {cdp['port']}")
    print(f"cdp_metadata_state: {cdp['metadata_state']}")
    print(f"cdp_target_count: {cdp['target_count']}")
    print("can_mark_real_live_ready: false")
    print("global_real_live_ready: false")
    if blockers:
        print(f"blockers: {','.join(sorted(set(blockers)))}")
PY
