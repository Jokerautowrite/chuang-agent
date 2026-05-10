#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-feishu-live-receipt.sh [--json]

Readonly Feishu live receipt template.

Environment overrides:
  CHUANG_AGENT_ROOT                  Chuang repo root
  CHUANG_FEISHU_LIVE_RECEIPT_OPERATOR  operator name to record
  CHUANG_FEISHU_LIVE_RECEIPT_REQUEST_ID live receipt request id to record

Readonly boundaries:
  connects_real_feishu=false
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

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
OPERATOR="${CHUANG_FEISHU_LIVE_RECEIPT_OPERATOR:-<operator>}"
REQUEST_ID="${CHUANG_FEISHU_LIVE_RECEIPT_REQUEST_ID:-<fill_request_id>}"

export FORMAT ROOT OPERATOR REQUEST_ID

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone

format_name = os.environ["FORMAT"]
root = os.environ["ROOT"]
operator = os.environ["OPERATOR"]
request_id = os.environ["REQUEST_ID"]

readonly_boundaries = {
    "readonly": True,
    "connects_real_feishu": False,
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
    "tested_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
    "request_id": request_id,
    "operator": operator,
    "workspace_root": root,
    "acceptance_status": "not_verified",
    "can_mark_real_live_ready": False,
    "cannot_mark_complete_without_operator_evidence": True,
    "readonly": True,
    "readonly_boundaries": readonly_boundaries,
    "feishu_live_evidence": {
        "transcript_refs": {
            "health": "<fill_health_transcript_ref>",
            "session": "<fill_session_transcript_ref>",
            "tools": "<fill_tools_transcript_ref>",
        },
        "session_binding_refs": {
            "chat_binding_ref": "<fill_chat_binding_ref>",
            "thread_binding_ref": "<fill_thread_binding_ref>",
            "binding_state_ref": "<fill_binding_state_ref>",
        },
        "normal_message": {
            "transcript_ref": "<fill_normal_message_transcript_ref>",
            "runtime_report_id": "<fill_runtime_report_id>",
        },
        "secret_redaction_notes": [
            "<record_only_set_or_missing_values>",
        ],
        "codex_hermes_isolation": {
            "kept_separate": True,
            "notes": "<keep_codex_and_hermes_separate>",
        },
    },
    "notes": [],
    "blockers": [],
}

if format_name == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2))
else:
    print("Feishu live receipt template")
    print(f"request_id={result['request_id']}")
    print(f"operator={result['operator']}")
    print(f"workspace_root={result['workspace_root']}")
    print(f"runtime_report_id={result['feishu_live_evidence']['normal_message']['runtime_report_id']}")
    print("readonly_boundaries=connects_real_feishu=false sends_feishu_messages=false prints_secret_values=false modifies_repo=false")
PY
