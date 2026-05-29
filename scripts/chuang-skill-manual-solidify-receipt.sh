#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-skill-manual-solidify-receipt.sh [--json]

Manual-only dry-run receipt for skill proposal -> manual solidify path.

Environment overrides:
  CHUANG_AGENT_ROOT
  CHUANG_SKILL_MANUAL_SOLIDIFY_REQUEST_ID
  CHUANG_SKILL_PROPOSAL_ID
  CHUANG_SKILL_PROPOSAL_REF
  CHUANG_SKILL_JUDGE_RECEIPT_REF
  CHUANG_SKILL_APPROVE_RECEIPT_REF
  CHUANG_SKILL_OPERATOR_DECISION_REF
  CHUANG_SKILL_PROPOSED_PATH
  CHUANG_SKILLS_ROOT

Manual boundaries:
  dry_run=true
  writes_automatically=false
  requires_human_approval=true
  writes_long_term_skills=false
  modifies_real_skill_directory=false
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

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
REQUEST_ID="${CHUANG_SKILL_MANUAL_SOLIDIFY_REQUEST_ID:-skill-manual-solidify-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$}"
PROPOSAL_ID="${CHUANG_SKILL_PROPOSAL_ID:-<fill_proposal_id>}"
PROPOSAL_REF="${CHUANG_SKILL_PROPOSAL_REF:-<fill_proposal_ref>}"
JUDGE_REF="${CHUANG_SKILL_JUDGE_RECEIPT_REF:-<fill_judge_receipt_ref>}"
APPROVE_REF="${CHUANG_SKILL_APPROVE_RECEIPT_REF:-<fill_approve_receipt_ref>}"
DECISION_REF="${CHUANG_SKILL_OPERATOR_DECISION_REF:-<fill_operator_decision_ref>}"
PROPOSED_PATH_OVERRIDE="${CHUANG_SKILL_PROPOSED_PATH:-}"
SKILLS_ROOT="${CHUANG_SKILLS_ROOT:-data/skills}"

export FORMAT ROOT REQUEST_ID PROPOSAL_ID PROPOSAL_REF JUDGE_REF APPROVE_REF DECISION_REF PROPOSED_PATH_OVERRIDE SKILLS_ROOT

python3 - <<'PY'
import json
import os
import re
from datetime import datetime, timezone

FORMAT = os.environ["FORMAT"]
ROOT = os.environ["ROOT"]
REQUEST_ID = os.environ["REQUEST_ID"]
PROPOSAL_ID = os.environ["PROPOSAL_ID"]
PROPOSAL_REF = os.environ["PROPOSAL_REF"]
JUDGE_REF = os.environ["JUDGE_REF"]
APPROVE_REF = os.environ["APPROVE_REF"]
DECISION_REF = os.environ["DECISION_REF"]
PROPOSED_PATH_OVERRIDE = os.environ.get("PROPOSED_PATH_OVERRIDE", "").strip()
SKILLS_ROOT = os.environ["SKILLS_ROOT"]


def normalize_skill_id(value: str) -> str:
    lowered = (value or "").strip().lower()
    normalized = re.sub(r"[^a-z0-9]+", "_", lowered).strip("_")
    return normalized or "manual_skill_candidate"


skill_id = normalize_skill_id(PROPOSAL_ID)
proposed_path = PROPOSED_PATH_OVERRIDE or f"{SKILLS_ROOT.rstrip('/')}/{skill_id}.md"

evidence_refs = {
    "proposal_ref": PROPOSAL_REF,
    "judge_receipt_ref": JUDGE_REF,
    "approve_receipt_ref": APPROVE_REF,
    "operator_decision_ref": DECISION_REF,
}

manual_confirmation_checklist = [
    {
        "id": "proposal_provenance_confirmed",
        "description": "proposal carries traceable provenance and scope",
        "status": "pending_human_confirmation",
        "required_evidence_refs": ["proposal_ref"],
    },
    {
        "id": "judge_receipt_confirmed",
        "description": "skill judge receipt passes threshold before any write",
        "status": "pending_human_confirmation",
        "required_evidence_refs": ["judge_receipt_ref"],
    },
    {
        "id": "approval_receipt_confirmed",
        "description": "manual approval receipt exists and matches proposal",
        "status": "pending_human_confirmation",
        "required_evidence_refs": ["approve_receipt_ref", "operator_decision_ref"],
    },
    {
        "id": "manual_write_step_confirmed",
        "description": "human performs explicit skill solidify write step",
        "status": "pending_human_confirmation",
        "required_evidence_refs": ["operator_decision_ref"],
    },
]

result = {
    "schema_version": 1,
    "receipt_kind": "skill_manual_solidify_dry_run_receipt",
    "tested_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
    "request_id": REQUEST_ID,
    "workspace_root": ROOT,
    "skills_root": SKILLS_ROOT,
    "proposal_id": PROPOSAL_ID,
    "proposed_skill_id": skill_id,
    "proposed_path": proposed_path,
    "mode": "manual_dry_run",
    "dry_run": True,
    "manual_only": True,
    "writes_automatically": False,
    "requires_human_approval": True,
    "writes_skill_files": False,
    "writes_long_term_skills": False,
    "modifies_real_skill_directory": False,
    "acceptance_status": "pending_human_approval",
    "blockers": [
        "manual_confirmation_required",
        "manual_write_step_not_executed",
    ],
    "manual_confirmation_checklist": manual_confirmation_checklist,
    "evidence_refs": evidence_refs,
    "boundary": {
        "reads_existing_skills": False,
        "solidifies_skill": False,
        "upserts_canonical_skill": False,
        "connects_llm": False,
        "connects_external_service": False,
    },
    "can_mark_real_live_ready": False,
    "global_real_live_ready": False,
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
else:
    print("skill_manual_solidify_receipt: mode=manual_dry_run")
    print("writes_automatically=false requires_human_approval=true")
    print(f"proposal_id: {PROPOSAL_ID}")
    print(f"proposed_skill_id: {skill_id}")
    print(f"proposed_path: {proposed_path}")
    print(f"checklist_count: {len(manual_confirmation_checklist)}")
    print("global_real_live_ready: false")
PY
