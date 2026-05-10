#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
provider_readiness_check="$root_dir/scripts/chuang-provider-readiness-check.sh"

cd "$root_dir"

# Candidate verification is intentionally dirty-tree friendly. The clean-tree
# release gate remains scripts/chuang-final-verify.sh.

# Keep this wrapper local-only even when the operator shell has live gates set.
unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

printf '%s\n' "[candidate-verify] complete local smoke"
sh scripts/chuang-complete-local-smoke.sh

printf '%s\n' "[candidate-verify] live runner rehearsal smoke"
bash scripts/chuang-live-runner-rehearsal-smoke.sh

printf '%s\n' "[candidate-verify] live gaps check"
bash scripts/chuang-live-gaps-check.sh

printf '%s\n' "[candidate-verify] live operator checklist readonly summary"
operator_status=0
operator_json="$(bash scripts/chuang-live-operator-checklist.sh --json)" || operator_status=$?
if [ "$operator_status" -ne 0 ] && [ "$operator_status" -ne 1 ]; then
    printf '%s\n' "[candidate-verify] live operator checklist failed unexpectedly with status $operator_status"
    exit "$operator_status"
fi
printf '%s' "$operator_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
boundaries = data["readonly_boundaries"]
assert boundaries["readonly"] is True
assert boundaries["connects_real_feishu"] is False
assert boundaries["sends_feishu_messages"] is False
assert boundaries["connects_real_provider"] is False
assert boundaries["performs_desktop_actions"] is False
assert boundaries["performs_browser_actions"] is False
assert boundaries["connects_real_wiki"] is False
assert boundaries["connects_real_gbrain"] is False
assert boundaries["starts_services"] is False
assert boundaries["modifies_repo"] is False
assert boundaries["prints_secret_values"] is False
real_live = data["real_live_acceptance"]
assert real_live["complete"] is False
assert real_live["status"] == "not_verified"
assert real_live["cannot_mark_complete_from_readonly_checklist"] is True
assert real_live["operator_receipt_template"] == "scripts/chuang-live-operator-receipt.sh --json"
assert real_live["operator_receipt_template_can_mark_complete"] is False
print("candidate_live_operator_checklist_status=" + str(data["status"]))
print("candidate_live_operator_real_live_acceptance=" + str(real_live["status"]))
'

printf '%s\n' "[candidate-verify] live operator receipt readonly template"
receipt_json="$(bash scripts/chuang-live-operator-receipt.sh --json)"
printf '%s' "$receipt_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
required = [
    "tested_at",
    "request_id",
    "operator",
    "env_file",
    "workspace_root",
    "approval_scope",
    "rollback_condition",
    "acceptance_status",
    "preflight_status",
    "health_status",
    "new_thread_status",
    "session_status",
    "runtime_report_id",
    "provider_status",
    "codex_hermes_isolation",
    "notes",
    "blockers",
    "boundaries",
    "readonly_boundaries",
    "service_evidence",
    "service_receipts",
    "real_live_acceptance",
]
for key in required:
    assert key in data, key
assert data["schema_version"] == 1
assert data["acceptance_status"] == "not_verified"
assert data["can_mark_real_live_ready"] is False
assert data["cannot_mark_complete_without_operator_evidence"] is True
for key in ["preflight_status", "health_status", "new_thread_status", "session_status", "runtime_report_id", "provider_status"]:
    assert data[key] == "<fill_after_test>"
boundaries = data["boundaries"]
assert boundaries == data["readonly_boundaries"]
for key in [
    "readonly",
]:
    assert boundaries[key] is True
for key in [
    "connects_real_feishu",
    "sends_feishu_messages",
    "connects_real_provider",
    "starts_workers",
    "dispatches_tasks",
    "performs_desktop_actions",
    "performs_browser_actions",
    "connects_real_wiki",
    "connects_real_gbrain",
    "reads_secret_values",
    "prints_secret_values",
    "starts_services",
    "stops_services",
    "touches_services",
    "modifies_repo",
    "deletes_files",
    "reuses_codex_or_hermes_credentials",
]:
    assert boundaries[key] is False, key
service_ids = [item["id"] for item in data["service_receipts"]]
assert service_ids == ["feishu", "provider", "subagent_live_rehearsal", "desktop", "browser", "wiki", "gbrain"]
assert sorted(data["service_evidence"].keys()) == sorted(service_ids)
real_live = data["real_live_acceptance"]
assert real_live["complete"] is False
assert real_live["status"] == "not_verified"
assert real_live["gap_count"] == 7
assert real_live["cannot_mark_complete_from_template"] is True
assert real_live["requires_operator_evidence"] is True
print("candidate_live_operator_receipt_status=" + str(data["acceptance_status"]))
print("candidate_live_operator_receipt_services=" + str(len(service_ids)))
'

printf '%s\n' "[candidate-verify] goal run status readonly summary"
goal_status_json="$(bash scripts/chuang-goal-run-status.sh --json)"
printf '%s' "$goal_status_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
boundaries = data["readonly_boundaries"]
assert boundaries["readonly"] is True
assert boundaries["dispatches_tasks"] is False
assert boundaries["starts_worker"] is False
assert boundaries["restarts_worker"] is False
assert boundaries["modifies_repo"] is False
assert boundaries["deletes_logs"] is False
assert boundaries["touches_services"] is False
print("candidate_goal_run_status_overall=" + str(data["overall_status"]))
print("candidate_goal_run_status_ok=" + str(data["ok"]).lower())
'

printf '%s\n' "[candidate-verify] provider readiness check"
if [ -f "$provider_readiness_check" ]; then
    if bash "$provider_readiness_check"; then
        printf '%s\n' "[candidate-verify] provider config/readiness preflight passed; connects_real_provider=false"
    else
        provider_status=$?
        if [ "$provider_status" -eq 1 ]; then
            printf '%s\n' "[candidate-verify] provider readiness check reported a non-live block; continuing candidate-only gate"
        else
            printf '%s\n' "[candidate-verify] provider readiness check failed unexpectedly with status $provider_status"
            exit "$provider_status"
        fi
    fi
else
    printf '%s\n' "[candidate-verify] provider readiness check script not found: scripts/chuang-provider-readiness-check.sh"
    printf '%s\n' "[candidate-verify] provider readiness remains covered by complete-local status/doctor/app-server stub checks; no real provider call is attempted"
fi

printf '%s\n' "chuang_candidate_verify_ok"
