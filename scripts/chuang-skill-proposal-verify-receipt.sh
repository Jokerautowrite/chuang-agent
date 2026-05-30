#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-skill-proposal-verify-receipt.sh [--json]

Read-only verify receipt for skill proposal JSON before manual solidify.

Environment:
  CHUANG_SKILL_PROPOSAL_FILE   optional proposal JSON file path

Read-only boundaries:
  read_only=true
  writes_automatically=false
  manual_approval_required=true
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

REQUEST_ID="skill-proposal-verify-receipt-$(date -u +%Y%m%dT%H%M%SZ)-$$"
PROPOSAL_FILE="${CHUANG_SKILL_PROPOSAL_FILE:-}"

export FORMAT REQUEST_ID PROPOSAL_FILE

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone

fmt = os.environ["FORMAT"]
request_id = os.environ["REQUEST_ID"]
proposal_file = os.environ.get("PROPOSAL_FILE", "").strip()

result = {
    "schema_version": 1,
    "receipt_kind": "skill_proposal_verify_receipt",
    "tested_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
    "request_id": request_id,
    "read_only": True,
    "writes_automatically": False,
    "manual_approval_required": True,
    "global_real_live_ready": False,
    "acceptance_status": "blocked",
    "blocker": "missing_skill_proposal_file",
    "blockers": ["missing_skill_proposal_file"],
}

if not proposal_file:
    pass
else:
    result["proposal_file"] = proposal_file
    try:
        with open(proposal_file, "r", encoding="utf-8") as f:
            payload = json.load(f)
    except FileNotFoundError:
        result["blocker"] = "skill_proposal_file_not_found"
        result["blockers"] = ["skill_proposal_file_not_found"]
    except json.JSONDecodeError:
        result["blocker"] = "invalid_skill_proposal_json"
        result["blockers"] = ["invalid_skill_proposal_json"]
    except Exception:
        result["blocker"] = "skill_proposal_read_failed"
        result["blockers"] = ["skill_proposal_read_failed"]
    else:
        blockers = []
        if not isinstance(payload, dict):
            blockers.append("skill_proposal_not_object")
            payload = {}

        proposal_id = payload.get("id")
        proposal_title = payload.get("title") or payload.get("name")
        description = payload.get("description")
        body = payload.get("body")
        summary = payload.get("summary")
        content_value = description or body or summary
        evidence_refs = payload.get("evidence_refs")

        if not isinstance(proposal_id, str) or not proposal_id.strip():
            blockers.append("missing_proposal_id")
        if not isinstance(proposal_title, str) or not proposal_title.strip():
            blockers.append("missing_proposal_title_or_name")
        if not isinstance(content_value, str) or not content_value.strip():
            blockers.append("missing_proposal_content")
        if "evidence_refs" in payload and not isinstance(evidence_refs, list):
            blockers.append("invalid_evidence_refs_type")

        result["proposal_summary"] = {
            "id": proposal_id if isinstance(proposal_id, str) else None,
            "title_or_name_chars": len(proposal_title.strip()) if isinstance(proposal_title, str) else 0,
            "content_field": (
                "description"
                if isinstance(description, str) and description.strip()
                else "body"
                if isinstance(body, str) and body.strip()
                else "summary"
                if isinstance(summary, str) and summary.strip()
                else None
            ),
            "content_chars": len(content_value.strip()) if isinstance(content_value, str) else 0,
            "evidence_refs_count": len(evidence_refs) if isinstance(evidence_refs, list) else None,
        }

        if blockers:
            result["acceptance_status"] = "blocked"
            result["blockers"] = blockers
            result["blocker"] = blockers[0]
        else:
            result["acceptance_status"] = "verified"
            result["blockers"] = []
            result["blocker"] = None

if fmt == "json":
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
else:
    print(f"receipt_kind={result['receipt_kind']}")
    print(f"acceptance_status={result['acceptance_status']}")
    print(f"read_only={str(result['read_only']).lower()}")
    print(f"writes_automatically={str(result['writes_automatically']).lower()}")
    print(f"manual_approval_required={str(result['manual_approval_required']).lower()}")
    print(f"global_real_live_ready={str(result['global_real_live_ready']).lower()}")
    blocker = result.get("blocker")
    if blocker:
        print(f"blocker={blocker}")
    summary = result.get("proposal_summary")
    if isinstance(summary, dict):
        print(f"proposal_id={summary.get('id')}")
        print(f"content_field={summary.get('content_field')}")
        print(f"content_chars={summary.get('content_chars')}")
        print(f"evidence_refs_count={summary.get('evidence_refs_count')}")
PY
