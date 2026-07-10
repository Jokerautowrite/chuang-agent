#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" != "--json" ]]; then
  printf 'usage: %s --json\n' "$0" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADAPTER_REL="scripts/chuang-real-actuator-adapter.py"
ALLOWLIST_REL="config/actuator-allowlist.example.json"
ADAPTER="$REPO_ROOT/$ADAPTER_REL"
ALLOWLIST="$REPO_ROOT/$ALLOWLIST_REL"
ACTION="open_app"
APP_NAME="Chrome"
REQUIRED_ENV="CHUANG_REAL_ACTUATOR_ENABLE"
AUDIT_LABEL="actuator.operation.live"
GOVERNANCE_ACTION_KIND="LocalDesktopInteraction"
GOVERNANCE_DECISION="allowed"
GOVERNANCE_REASON="profile=full_local_workspace action=local desktop interaction permission=AllowWithAudit"

REQUEST_JSON='{"action":"open_app","open_app":{"app_name":"Chrome"}}'

ADAPTER_RESPONSE="$(
  printf '%s' "$REQUEST_JSON" \
    | env -u CHUANG_REAL_ACTUATOR_ENABLE "$ADAPTER" --json --allowlist "$ALLOWLIST"
)"

export ADAPTER_RESPONSE
export ADAPTER_REL
export ALLOWLIST_REL
export ACTION
export APP_NAME
export REQUIRED_ENV
export AUDIT_LABEL
export GOVERNANCE_ACTION_KIND
export GOVERNANCE_DECISION
export GOVERNANCE_REASON

python3 - <<'PY'
import json
import os
import re
import time


def message_bool(message, key, default=False):
    match = re.search(rf"(?:^| ){re.escape(key)}=(true|false)(?: |$)", message)
    if not match:
        return default
    return match.group(1) == "true"


response = json.loads(os.environ["ADAPTER_RESPONSE"])
message = str(response.get("message") or "")
app_handle = response.get("app_handle") or {}

receipt = {
    "schema_version": 1,
    "receipt_kind": "desktop_action_rehearsal_receipt",
    "tested_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "action": os.environ["ACTION"],
    "app_name": os.environ["APP_NAME"],
    "uses_actuator_adapter": True,
    "uses_allowlist": True,
    "adapter_path": os.environ["ADAPTER_REL"],
    "allowlist_path": os.environ["ALLOWLIST_REL"],
    "audit_label": os.environ["AUDIT_LABEL"],
    "required_env": os.environ["REQUIRED_ENV"],
    "live_gate_env_state": "<missing>",
    "dry_run": message_bool(message, "dry_run", default=True),
    "real_execution": message_bool(message, "real_execution", default=False),
    "performs_desktop_action": False,
    "adapter_response": {
        "allowed": message_bool(message, "allowed", default=False),
        "action": os.environ["ACTION"],
        "app_handle_uri": app_handle.get("handle_id"),
        "message": message,
    },
    "governance": {
        "action_kind": os.environ["GOVERNANCE_ACTION_KIND"],
        "decision": os.environ["GOVERNANCE_DECISION"],
        "reason": os.environ["GOVERNANCE_REASON"],
    },
    "boundaries": {
        "uses_actuator_adapter": True,
        "uses_allowlist": True,
        "requires_live_gate_for_real_execution": True,
        "live_gate_closed_for_rehearsal": True,
        "performs_desktop_action": False,
        "connects_real_provider": False,
        "connects_real_feishu": False,
        "modifies_repo": False,
        "deletes_files": False,
    },
    "can_mark_real_live_ready": False,
    "global_real_live_ready": False,
}

print(json.dumps(receipt, ensure_ascii=False, sort_keys=True))
PY
