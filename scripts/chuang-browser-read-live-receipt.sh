#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-browser-read-live-receipt.sh [--json]

Readonly browser_read live receipt template.

Environment overrides:
  CHUANG_BROWSER_READ_REQUEST_ID
  CHUANG_BROWSER_READ_ADAPTER_KIND
  CHUANG_BROWSER_READ_ADAPTER_STATE
  CHUANG_BROWSER_READ_ADAPTER_MANIFEST_REF
  CHUANG_BROWSER_READ_SESSION_SCOPE_REF
  CHUANG_BROWSER_READ_SNAPSHOT_REF
  CHUANG_BROWSER_READ_REPORT_ADMISSION_REF
  CHUANG_BROWSER_READ_RUNTIME_REPORT_ID
  CHUANG_BROWSER_READ_BLOCKED_REASON
  CHUANG_BROWSER_READ_NEXT_ACTION

Readonly boundaries:
  readonly=true
  desktop_read_is_separate=true
  browser_read_does_not_use_desktop_read=true
  performs_desktop_actions=false
  performs_browser_actions=false
  connects_real_browser=false
  connects_real_provider=false
  connects_real_wiki=false
  connects_real_gbrain=false
  writes_core_memory=false
  prints_secret_values=false
  modifies_repo=false
  deletes_files=false
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
export CHUANG_BROWSER_READ_REQUEST_ID="${CHUANG_BROWSER_READ_REQUEST_ID:-<fill_after_test>}"
export CHUANG_BROWSER_READ_ADAPTER_KIND="${CHUANG_BROWSER_READ_ADAPTER_KIND:-<fill_after_test>}"
export CHUANG_BROWSER_READ_ADAPTER_STATE="${CHUANG_BROWSER_READ_ADAPTER_STATE:-<fill_after_test>}"
export CHUANG_BROWSER_READ_ADAPTER_MANIFEST_REF="${CHUANG_BROWSER_READ_ADAPTER_MANIFEST_REF:-<fill_after_test>}"
export CHUANG_BROWSER_READ_SESSION_SCOPE_REF="${CHUANG_BROWSER_READ_SESSION_SCOPE_REF:-<fill_after_test>}"
export CHUANG_BROWSER_READ_SNAPSHOT_REF="${CHUANG_BROWSER_READ_SNAPSHOT_REF:-<fill_after_test>}"
export CHUANG_BROWSER_READ_REPORT_ADMISSION_REF="${CHUANG_BROWSER_READ_REPORT_ADMISSION_REF:-<fill_after_test>}"
export CHUANG_BROWSER_READ_RUNTIME_REPORT_ID="${CHUANG_BROWSER_READ_RUNTIME_REPORT_ID:-<fill_after_test>}"
export CHUANG_BROWSER_READ_BLOCKED_REASON="${CHUANG_BROWSER_READ_BLOCKED_REASON:-<fill_after_test>}"
export CHUANG_BROWSER_READ_NEXT_ACTION="${CHUANG_BROWSER_READ_NEXT_ACTION:-<fill_after_test>}"

python3 - <<'PY'
import json
import os

format_name = os.environ["FORMAT"]

result = {
    "schema_version": 1,
    "receipt_kind": "browser_read_live_readonly_receipt",
    "readonly": True,
    "can_mark_real_live_ready": False,
    "cannot_mark_complete_without_operator_evidence": True,
    "readonly_boundaries": {
        "readonly": True,
        "desktop_read_is_separate": True,
        "browser_read_does_not_use_desktop_read": True,
        "performs_desktop_actions": False,
        "performs_browser_actions": False,
        "connects_real_browser": False,
        "connects_real_provider": False,
        "connects_real_wiki": False,
        "connects_real_gbrain": False,
        "writes_core_memory": False,
        "prints_secret_values": False,
        "modifies_repo": False,
        "deletes_files": False,
    },
    "request_id": os.environ["CHUANG_BROWSER_READ_REQUEST_ID"].strip() or "<fill_after_test>",
    "adapter_kind": os.environ["CHUANG_BROWSER_READ_ADAPTER_KIND"].strip() or "<fill_after_test>",
    "adapter_state": os.environ["CHUANG_BROWSER_READ_ADAPTER_STATE"].strip() or "<fill_after_test>",
    "adapter_manifest_ref": os.environ["CHUANG_BROWSER_READ_ADAPTER_MANIFEST_REF"].strip() or "<fill_after_test>",
    "session_scope_ref": os.environ["CHUANG_BROWSER_READ_SESSION_SCOPE_REF"].strip() or "<fill_after_test>",
    "browser_snapshot_or_transcript_ref": os.environ["CHUANG_BROWSER_READ_SNAPSHOT_REF"].strip() or "<fill_after_test>",
    "report_admission_ref": os.environ["CHUANG_BROWSER_READ_REPORT_ADMISSION_REF"].strip() or "<fill_after_test>",
    "runtime_report_id": os.environ["CHUANG_BROWSER_READ_RUNTIME_REPORT_ID"].strip() or "<fill_after_test>",
    "blocked_reason": os.environ["CHUANG_BROWSER_READ_BLOCKED_REASON"].strip() or "<fill_after_test>",
    "next_action": os.environ["CHUANG_BROWSER_READ_NEXT_ACTION"].strip() or "<fill_after_test>",
}

if format_name == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print("browser_read_live_receipt: readonly=true")
    print(f"request_id: {result['request_id']}")
    print(f"adapter_kind: {result['adapter_kind']}")
    print(f"adapter_state: {result['adapter_state']}")
    print(f"adapter_manifest_ref: {result['adapter_manifest_ref']}")
    print(f"session_scope_ref: {result['session_scope_ref']}")
    print(f"browser_snapshot_or_transcript_ref: {result['browser_snapshot_or_transcript_ref']}")
    print(f"report_admission_ref: {result['report_admission_ref']}")
    print(f"runtime_report_id: {result['runtime_report_id']}")
    print(f"blocked_reason: {result['blocked_reason']}")
    print(f"next_action: {result['next_action']}")
    print("desktop_read_is_separate: true")
    print("browser_read_does_not_use_desktop_read: true")
    print("performs_browser_actions: false")
    print("writes_core_memory: false")
PY
