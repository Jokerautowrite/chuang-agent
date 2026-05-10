#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-runner-rehearsal-receipt.sh [--json]

Readonly single worker rehearsal receipt skeleton.
This template does not start a live gate, dispatch a worker, or touch services.
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

DISPATCH_ID="${CHUANG_LIVE_REHEARSAL_DISPATCH_ID:-<fill_after_test>}"
WORKER_ID="${CHUANG_LIVE_REHEARSAL_WORKER_ID:-<fill_after_test>}"
GATE_RECEIPT_REF="${CHUANG_LIVE_REHEARSAL_GATE_RECEIPT_REF:-<fill_after_test>}"
ALLOWLIST_RECEIPT_REF="${CHUANG_LIVE_REHEARSAL_ALLOWLIST_RECEIPT_REF:-<fill_after_test>}"
CAPABILITY_ROUTING_REF="${CHUANG_LIVE_REHEARSAL_CAPABILITY_ROUTING_REF:-<fill_after_test>}"
REPORT_ADMISSION_REF="${CHUANG_LIVE_REHEARSAL_REPORT_ADMISSION_REF:-<fill_after_test>}"

export FORMAT DISPATCH_ID WORKER_ID GATE_RECEIPT_REF ALLOWLIST_RECEIPT_REF CAPABILITY_ROUTING_REF REPORT_ADMISSION_REF

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone

FORMAT = os.environ["FORMAT"]
DISPATCH_ID = os.environ["DISPATCH_ID"]
WORKER_ID = os.environ["WORKER_ID"]
GATE_RECEIPT_REF = os.environ["GATE_RECEIPT_REF"]
ALLOWLIST_RECEIPT_REF = os.environ["ALLOWLIST_RECEIPT_REF"]
CAPABILITY_ROUTING_REF = os.environ["CAPABILITY_ROUTING_REF"]
REPORT_ADMISSION_REF = os.environ["REPORT_ADMISSION_REF"]

READONLY_BOUNDARIES = {
    "readonly": True,
    "connects_real_feishu": False,
    "sends_feishu_messages": False,
    "connects_real_provider": False,
    "starts_external_worker": False,
    "enables_live_gate": False,
    "starts_workers": False,
    "dispatches_tasks": False,
    "restarts_worker": False,
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

APPROVAL_AUDIT_PREREQUISITES = {
    "ok": False,
    "explicit_operator_approval_required": True,
    "governance_approval_required": True,
    "audit_receipt_required": True,
    "dispatch_evidence_required": True,
    "audit_label": "subagent.runner.single-worker-rehearsal.live",
    "prerequisites": [
        "operator approval for the exact single worker rehearsal dispatch",
        "governance approval for the exact gate, allowlist, capability routing, and report admission refs",
        "dispatch evidence must exist before runner pool readiness can be claimed",
    ],
    "reason": "read-only receipt skeleton only; it does not satisfy operator approval, governance approval, or dispatch evidence",
}

REAL_LIVE_ACCEPTANCE = {
    "complete": False,
    "status": "not_runner_pool_ready",
    "runner_pool_ready": False,
    "single_worker_rehearsal_is_runner_pool_ready": False,
    "cannot_mark_complete_from_template": True,
    "cannot_mark_runner_pool_ready_from_template": True,
    "requires_operator_evidence": True,
    "gap_count": 1,
    "reason": "single worker rehearsal is read-only and is not runner pool ready",
}

result = {
    "schema_version": 1,
    "tested_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
    "receipt_kind": "single_worker_rehearsal_live_receipt_skeleton",
    "dispatch_id": DISPATCH_ID,
    "worker_id": WORKER_ID,
    "gate_receipt_ref": GATE_RECEIPT_REF,
    "allowlist_receipt_ref": ALLOWLIST_RECEIPT_REF,
    "capability_routing_ref": CAPABILITY_ROUTING_REF,
    "report_admission_ref": REPORT_ADMISSION_REF,
    "readonly_boundaries": READONLY_BOUNDARIES,
    "approval_audit_prerequisites": APPROVAL_AUDIT_PREREQUISITES,
    "real_live_acceptance": REAL_LIVE_ACCEPTANCE,
    "notes": [],
    "blockers": [],
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(
        "single_worker_rehearsal_receipt_skeleton "
        f"dispatch_id={DISPATCH_ID} worker_id={WORKER_ID} "
        "starts_external_worker=false enables_live_gate=false runner_pool_ready=false"
    )
    print(
        "receipt_refs "
        f"gate={GATE_RECEIPT_REF} allowlist={ALLOWLIST_RECEIPT_REF} "
        f"capability_routing={CAPABILITY_ROUTING_REF} report_admission={REPORT_ADMISSION_REF}"
    )
    print(
        "approval_audit_prerequisites_ok=false "
        "real_live_acceptance=not_runner_pool_ready"
    )
PY
