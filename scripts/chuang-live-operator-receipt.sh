#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-operator-receipt.sh [--json]

Readonly receipt template for a manual Chuang live test.

Environment overrides:
  CHUANG_LIVE_OPERATOR      operator name to record
  CHUANG_AGENT_ROOT         Chuang repo root
  CHUANG_LIVE_ENV_FILE      env file path to record
  CHUANG_LIVE_OPERATOR_ENV_FILE
                            same as CHUANG_LIVE_ENV_FILE, lower priority
  CHUANG_FEISHU_ENV_FILE    same as CHUANG_LIVE_ENV_FILE, lower priority
  CHUANG_LIVE_REQUEST_ID    operator/live test request id to record

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

ROOT="${CHUANG_AGENT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
ENV_FILE="${CHUANG_LIVE_ENV_FILE:-${CHUANG_LIVE_OPERATOR_ENV_FILE:-${CHUANG_FEISHU_ENV_FILE:-$HOME/.codex-im/chuang-feishu-bridge.env}}}"
OPERATOR="${CHUANG_LIVE_OPERATOR:-${USER:-<operator>}}"
REQUEST_ID="${CHUANG_LIVE_REQUEST_ID:-<fill_request_id>}"

export FORMAT ROOT ENV_FILE OPERATOR REQUEST_ID

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone

FORMAT = os.environ["FORMAT"]
ROOT = os.environ["ROOT"]
ENV_FILE = os.environ["ENV_FILE"]
OPERATOR = os.environ["OPERATOR"]
REQUEST_ID = os.environ["REQUEST_ID"]

BOUNDARIES = {
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

SERVICE_RECEIPTS = [
    {
        "id": "feishu",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "health_transcript_ref": "<fill_after_test>",
            "session_transcript_ref": "<fill_after_test>",
            "tools_or_capabilities_transcript_ref": "<fill_after_test>",
            "normal_message_transcript_ref": "<fill_after_test>",
            "runtime_report_id": "<fill_after_test>",
        },
        "required": [
            "/health transcript with redacted secret states",
            "/session transcript with active chat/thread binding",
            "/tools or /capabilities boundary transcript",
            "normal non-secret text reply with runtime report id when applicable",
        ],
    },
    {
        "id": "provider",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "provider_kind": "<fill_after_test>",
            "transport": "<fill_after_test>",
            "api_key_state": "<set|missing>",
            "provider_live_request_receipt_ref": "<fill_after_test>",
            "runtime_report_id": "<fill_after_test>",
            "does_not_call_provider": True,
            "does_not_read_provider_readiness": True,
        },
        "required": [
            "provider transport is not stub/fake",
            "api_key_state is recorded only as <set>/<missing>",
            "provider live request receipt ref or runtime report id exists",
            "no fake-responder fallback",
        ],
    },
    {
        "id": "subagent_live_rehearsal",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "dispatch_id": "<fill_after_test>",
            "worker_id": "<fill_after_test>",
            "gate_receipt_ref": "<fill_after_test>",
            "allowlist_receipt_ref": "<fill_after_test>",
            "capability_routing_ref": "<fill_after_test>",
            "report_admission_ref": "<fill_after_test>",
        },
        "required": [
            "single worker only",
            "gate receipt is explicit",
            "allowlist receipt is explicit",
            "capability routing receipt is explicit",
            "report admission receipt or blocked reason is explicit",
        ],
    },
    {
        "id": "desktop",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "audit_label": "<fill_after_test>",
            "action_receipt_ref": "<fill_after_test>",
            "governance_receipt_ref": "<fill_after_test>",
            "real_execution": "<true|false|not_attempted>",
        },
        "required": [
            "exact desktop action approved by governance",
            "audit label and action receipt exist",
            "real_execution=true only in an external audited receipt",
        ],
    },
    {
        "id": "browser",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "adapter_manifest_ref": "<fill_after_test>",
            "session_scope_ref": "<fill_after_test>",
            "browser_snapshot_or_transcript_ref": "<fill_after_test>",
            "report_admission_ref": "<fill_after_test>",
        },
        "required": [
            "audited adapter manifest exists",
            "browser/session scope is explicit",
            "URL/title/DOM or transcript evidence is referenced",
        ],
    },
    {
        "id": "wiki",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "source_contract_ref": "<fill_after_test>",
            "query_receipt_ref": "<fill_after_test>",
            "provenance_ref": "<fill_after_test>",
            "writes_core_memory": False,
        },
        "required": [
            "read-only source contract is explicit",
            "retrieval provenance is visible",
            "no automatic core-memory write",
        ],
    },
    {
        "id": "gbrain",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "source_contract_ref": "<fill_after_test>",
            "query_receipt_ref": "<fill_after_test>",
            "provenance_ref": "<fill_after_test>",
            "writes_core_memory": False,
        },
        "required": [
            "read-only source contract is explicit",
            "retrieval provenance is visible",
            "no automatic core-memory write",
        ],
    },
]

SERVICE_EVIDENCE = {service["id"]: service["evidence"] for service in SERVICE_RECEIPTS}
REAL_LIVE_ACCEPTANCE = {
    "complete": False,
    "status": "not_verified",
    "gap_count": len(SERVICE_RECEIPTS),
    "cannot_mark_complete_from_template": True,
    "requires_operator_evidence": True,
    "services": [
        {
            "id": service["id"],
            "completion_state": "not_verified",
            "manual_live_required": True,
            "must_not_count_as_complete": True,
            "required": service["required"],
        }
        for service in SERVICE_RECEIPTS
    ],
}

result = {
    "schema_version": 1,
    "tested_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
    "request_id": REQUEST_ID,
    "operator": OPERATOR,
    "env_file": ENV_FILE,
    "workspace_root": ROOT,
    "approval_scope": "<fill_exact_live_scope>",
    "rollback_condition": "<fill_abort_or_rollback_condition>",
    "acceptance_status": "not_verified",
    "can_mark_real_live_ready": False,
    "cannot_mark_complete_without_operator_evidence": True,
    "preflight_status": "<fill_after_test>",
    "health_status": "<fill_after_test>",
    "new_thread_status": "<fill_after_test>",
    "session_status": "<fill_after_test>",
    "runtime_report_id": "<fill_after_test>",
    "provider_status": "<fill_after_test>",
    "readonly_boundaries": BOUNDARIES,
    "service_evidence": SERVICE_EVIDENCE,
    "service_receipts": SERVICE_RECEIPTS,
    "real_live_acceptance": REAL_LIVE_ACCEPTANCE,
    "codex_hermes_isolation": "<keep_codex_and_hermes_separate>",
    "notes": [],
    "blockers": [],
    "boundaries": BOUNDARIES,
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2))
else:
    print(f"tested_at={result['tested_at']}")
    print(f"request_id={result['request_id']}")
    print(f"operator={result['operator']}")
    print(f"env_file={result['env_file']}")
    print(f"workspace_root={result['workspace_root']}")
    print(f"approval_scope={result['approval_scope']}")
    print(f"rollback_condition={result['rollback_condition']}")
    print(f"acceptance_status={result['acceptance_status']}")
    print(f"can_mark_real_live_ready={str(result['can_mark_real_live_ready']).lower()}")
    print(
        "cannot_mark_complete_without_operator_evidence="
        f"{str(result['cannot_mark_complete_without_operator_evidence']).lower()}"
    )
    print(f"preflight_status={result['preflight_status']}")
    print(f"health_status={result['health_status']}")
    print(f"new_thread_status={result['new_thread_status']}")
    print(f"session_status={result['session_status']}")
    print(f"runtime_report_id={result['runtime_report_id']}")
    print(f"provider_status={result['provider_status']}")
    print(f"codex_hermes_isolation={result['codex_hermes_isolation']}")
    print("service_receipts=feishu,provider,subagent_live_rehearsal,desktop,browser,wiki,gbrain")
    for service in SERVICE_RECEIPTS:
        print(f"service_receipt id={service['id']} status={service['status']}")
    print("notes=[]")
    print("blockers=[]")
    for key, value in BOUNDARIES.items():
        print(f"boundaries.{key}={str(value).lower()}")
PY
