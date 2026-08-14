#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-feishu-live-receipt.sh [--json]

Readonly Feishu live receipt evidence collector.

Environment overrides:
  CHUANG_AGENT_ROOT
  CHUANG_FEISHU_LIVE_RECEIPT_OPERATOR
  CHUANG_FEISHU_LIVE_RECEIPT_REQUEST_ID
  CHUANG_FEISHU_EVENT_LOG_FILE
  CHUANG_FEISHU_EVENT_LOOKBACK_SECONDS
  CHUANG_FEISHU_STATE_FILE
  CHUANG_FEISHU_RECEIPT_SKIP_PREFLIGHT=1

Readonly boundaries:
  readonly=true
  connects_real_feishu=false
  observed_live_feishu_events=<derived_from_existing_log_only>
  sends_feishu_messages=false
  connects_real_provider=false
  starts_workers=false
  dispatches_tasks=false
  performs_desktop_actions=false
  performs_browser_actions=false
  connects_real_wiki=false
  connects_real_gbrain=false
  reads_secret_values=false
  prints_secret_values=false
  starts_services=false
  stops_services=false
  touches_services=false
  modifies_repo=false
  deletes_files=false
  reuses_codex_or_hermes_credentials=false
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

ROOT="${CHUANG_AGENT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
OPERATOR="${CHUANG_FEISHU_LIVE_RECEIPT_OPERATOR:-<operator>}"
REQUEST_ID="${CHUANG_FEISHU_LIVE_RECEIPT_REQUEST_ID:-feishu-live-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
EVENT_LOG_FILE="${CHUANG_FEISHU_EVENT_LOG_FILE:-/tmp/chuang-feishu-bridge-events.log}"
LOOKBACK_SECONDS="${CHUANG_FEISHU_EVENT_LOOKBACK_SECONDS:-86400}"
STATE_FILE="${CHUANG_FEISHU_STATE_FILE:-$ROOT/context/feishu-session-state.json}"

PREFLIGHT_STATUS="<skipped>"
PREFLIGHT_STDOUT=""

if [ "${CHUANG_FEISHU_RECEIPT_SKIP_PREFLIGHT:-0}" != "1" ]; then
  set +e
  PREFLIGHT_STDOUT="$(node "$ROOT/scripts/chuang-feishu-live-preflight.js" --json 2>&1)"
  PREFLIGHT_EXIT=$?
  set -e
  if [ "$PREFLIGHT_EXIT" -eq 0 ]; then
    PREFLIGHT_STATUS="ok"
  else
    PREFLIGHT_STATUS="failed"
  fi
fi

export FORMAT ROOT OPERATOR REQUEST_ID EVENT_LOG_FILE LOOKBACK_SECONDS STATE_FILE
export PREFLIGHT_STATUS PREFLIGHT_STDOUT

python3 - <<'PY'
import json
import os
from collections import Counter
from datetime import datetime, timedelta, timezone

format_name = os.environ["FORMAT"]
root = os.environ["ROOT"]
operator = os.environ["OPERATOR"]
request_id = os.environ["REQUEST_ID"]
event_log_file = os.environ["EVENT_LOG_FILE"]
state_file = os.environ["STATE_FILE"]
preflight_status = os.environ.get("PREFLIGHT_STATUS", "<skipped>")
preflight_stdout = os.environ.get("PREFLIGHT_STDOUT", "")

try:
    lookback_seconds = max(1, int(str(os.environ.get("LOOKBACK_SECONDS", "86400"))))
except ValueError:
    lookback_seconds = 86400

def env_state(name: str) -> str:
    return "<set>" if str(os.environ.get(name, "")).strip() else "<missing>"


def parse_time(value: str):
    text = str(value or "").strip()
    if not text:
        return None
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        return None


def summarize_session_state(path: str):
    if not os.path.exists(path):
        return {
            "state_file": path,
            "state_file_state": "<missing>",
            "parse_status": "missing",
            "version": None,
            "binding_count": 0,
            "has_workspace_roots": False,
        }
    try:
        with open(path, "r", encoding="utf-8") as handle:
            parsed = json.load(handle)
        bindings = parsed.get("bindings") if isinstance(parsed, dict) else {}
        binding_count = len(bindings) if isinstance(bindings, dict) else 0
        has_workspace_roots = False
        if isinstance(bindings, dict):
            for binding in bindings.values():
                if isinstance(binding, dict) and str(binding.get("workspaceRoot", "")).strip():
                    has_workspace_roots = True
                    break
        return {
            "state_file": path,
            "state_file_state": "<set>",
            "parse_status": "ok",
            "version": parsed.get("version") if isinstance(parsed, dict) else None,
            "binding_count": binding_count,
            "has_workspace_roots": has_workspace_roots,
        }
    except Exception as exc:  # noqa: BLE001
        return {
            "state_file": path,
            "state_file_state": "<set>",
            "parse_status": f"parse_error:{type(exc).__name__}",
            "version": None,
            "binding_count": 0,
            "has_workspace_roots": False,
        }


def parse_preflight(status: str, stdout: str):
    if status == "<skipped>":
        return {
            "status": "skipped",
            "ok": None,
            "summary": "skipped_by_env_override",
            "check_status_counts": {},
        }
    try:
        parsed = json.loads(stdout)
        checks = parsed.get("checks") if isinstance(parsed, dict) else []
        counts = Counter()
        if isinstance(checks, list):
            for check in checks:
                if isinstance(check, dict):
                    counts[str(check.get("status", "unknown"))] += 1
        return {
            "status": status,
            "ok": bool(parsed.get("ok")) if isinstance(parsed, dict) and "ok" in parsed else None,
            "summary": str(parsed.get("status", "unknown")) if isinstance(parsed, dict) else "unknown",
            "check_status_counts": dict(counts),
        }
    except Exception:  # noqa: BLE001
        return {
            "status": status,
            "ok": None,
            "summary": "unparseable_preflight_json",
            "check_status_counts": {},
        }


now = datetime.now(timezone.utc)
threshold = now - timedelta(seconds=lookback_seconds)

all_events = []
recent_events = []
recent_refs = []
blockers = []
notes = []

if not os.path.exists(event_log_file):
    blockers.append("missing_bridge_event_log")
else:
    try:
        with open(event_log_file, "r", encoding="utf-8") as handle:
            for line_no, raw in enumerate(handle, start=1):
                line = raw.strip()
                if not line:
                    continue
                try:
                    entry = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if not isinstance(entry, dict):
                    continue
                kind = str(entry.get("kind", "")).strip()
                at = str(entry.get("at", "")).strip()
                if not kind:
                    continue
                event = {"line": line_no, "kind": kind, "at": at}
                all_events.append(event)
                parsed_at = parse_time(at)
                if parsed_at and parsed_at.tzinfo is None:
                    parsed_at = parsed_at.replace(tzinfo=timezone.utc)
                if parsed_at and parsed_at >= threshold:
                    recent_events.append(event)
        if not all_events:
            blockers.append("missing_bridge_event_log")
        if all_events and not recent_events:
            blockers.append("missing_recent_inbound_outbound_pair")
    except Exception as exc:  # noqa: BLE001
        blockers.append(f"bridge_event_log_read_error:{type(exc).__name__}")

recent_kinds = Counter(event["kind"] for event in recent_events)
recent_inbound_count = recent_kinds.get("inbound", 0)
recent_outbound_count = (
    recent_kinds.get("outbound", 0)
    + recent_kinds.get("command", 0)
    + recent_kinds.get("outbound_format", 0)
)

if "missing_bridge_event_log" not in blockers:
    if recent_inbound_count < 1 or recent_outbound_count < 1:
        blockers.append("missing_recent_inbound_outbound_pair")

for event in recent_events[-8:]:
    recent_refs.append(
        {
            "ref": f"event_log:line:{event['line']}",
            "kind": event["kind"],
            "at": event["at"],
        }
    )

observed_live_events = len(all_events) > 0
acceptance_status = "verified" if not blockers else "blocked"

if acceptance_status == "verified":
    notes.append("feishu_channel_evidence_only")
else:
    notes.append("feishu_channel_evidence_blocked")

tracked_env_names = [
    "CHUANG_FEISHU_APP_ID",
    "CHUANG_FEISHU_APP_SECRET",
    "CHUANG_FEISHU_BOT_ID",
    "CHUANG_FEISHU_VERIFICATION_TOKEN",
    "CHUANG_FEISHU_ENCRYPT_KEY",
    "CHUANG_FEISHU_CONNECTION_MODE",
    "CHUANG_FEISHU_EVENT_LOG_FILE",
    "CHUANG_FEISHU_STATE_FILE",
    "CHUANG_FEISHU_ENV_FILE",
]

env_states = {name: env_state(name) for name in tracked_env_names}

readonly_boundaries = {
    "readonly": True,
    "connects_real_feishu": False,
    "observed_live_feishu_events": observed_live_events,
    "sends_feishu_messages": False,
    "connects_real_provider": False,
    "starts_workers": False,
    "dispatches_tasks": False,
    "performs_desktop_actions": False,
    "performs_browser_actions": False,
    "connects_real_wiki": False,
    "connects_real_gbrain": False,
    "reads_secret_values": False,
    "prints_secret_values": False,
    "starts_services": False,
    "stops_services": False,
    "touches_services": False,
    "modifies_repo": False,
    "deletes_files": False,
    "reuses_codex_or_hermes_credentials": False,
}

result = {
    "schema_version": 1,
    "receipt_kind": "feishu_live_readonly_receipt",
    "tested_at": now.astimezone().isoformat(timespec="seconds"),
    "request_id": request_id,
    "operator": operator,
    "workspace_root": root,
    "readonly": True,
    "connects_real_feishu": False,
    "observed_live_feishu_events": observed_live_events,
    "acceptance_status": acceptance_status,
    "can_mark_real_live_ready": False,
    "global_real_live_ready": False,
    "cannot_mark_complete_without_operator_evidence": True,
    "readonly_boundaries": readonly_boundaries,
    "feishu_live_evidence": {
        "event_log_file": event_log_file,
        "event_log_file_state": "<set>" if os.path.exists(event_log_file) else "<missing>",
        "lookback_seconds": lookback_seconds,
        "event_counts": {
            "total": len(all_events),
            "recent": len(recent_events),
            "recent_inbound": recent_inbound_count,
            "recent_outbound_command_or_outbound_format": recent_outbound_count,
        },
        "recent_kind_counts": dict(recent_kinds),
        "recent_event_refs": recent_refs,
        "session_state": summarize_session_state(state_file),
        "preflight_summary": parse_preflight(preflight_status, preflight_stdout),
    },
    "env_var_states": env_states,
    "notes": notes,
    "blockers": sorted(set(blockers)),
}

if format_name == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(f"feishu_live_receipt: acceptance_status={acceptance_status}")
    print(f"request_id: {request_id}")
    print(f"operator: {operator}")
    print(f"workspace_root: {root}")
    print(f"connects_real_feishu: false")
    print(f"observed_live_feishu_events: {str(observed_live_events).lower()}")
    print(f"event_log_file_state: {result['feishu_live_evidence']['event_log_file_state']}")
    print(f"recent_inbound: {recent_inbound_count}")
    print(f"recent_outbound_command_or_outbound_format: {recent_outbound_count}")
    print(f"can_mark_real_live_ready: false")
    print(f"global_real_live_ready: false")
    if blockers:
        print(f"blockers: {','.join(sorted(set(blockers)))}")
PY
