#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

cd "$root_dir"

status_short="$(git status --short)"
if [ -n "$status_short" ]; then
    printf '%s\n' "[third-test] error: working tree must be clean before third test smoke" >&2
    printf '%s\n' "$status_short" >&2
    exit 2
fi

printf '%s\n' "[third-test] final verify"
sh scripts/chuang-final-verify.sh

printf '%s\n' "[third-test] live readonly preflight"
sh scripts/chuang-live-readonly-preflight.sh

printf '%s\n' "[third-test] live gaps matrix"
live_gaps_json="$(bash scripts/chuang-live-gaps-check.sh --json)"
printf '%s' "$live_gaps_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["ok"] is True
assert data["check_name"] == "live-gaps"
assert data["summary"] == "local_contract=ready preflight=ready_but_no_start real_live=pending"
boundaries = data["boundaries"]
assert boundaries["readonly"] is True
assert boundaries["connects_real_feishu"] is False
assert boundaries["connects_real_provider"] is False
assert boundaries["starts_external_worker"] is False
assert boundaries["enables_live_gate"] is False
assert boundaries["modifies_repo"] is False
assert boundaries["prints_secret_values"] is False
matrix = {item["name"]: item for item in data["matrix"]}
assert matrix["local_contract"]["state"] == "ready"
assert matrix["local_contract"]["live_worker_available"] is False
assert matrix["preflight_ready_but_no_start"]["state"] == "ready_but_no_start"
assert matrix["preflight_ready_but_no_start"]["ready_for_live"] is False
assert matrix["preflight_ready_but_no_start"]["starts_external_worker"] is False
assert matrix["preflight_ready_but_no_start"]["live_worker_available"] is False
assert matrix["real_live"]["state"] == "pending"
assert matrix["real_live"]["real_live_ready"] is False
assert matrix["real_live"]["connects_real_external_services"] is False
gap_ids = [item["id"] for item in data["gaps"]]
assert "live_worker_adapter_pending" in gap_ids
assert "live_runner_gate_disabled" in gap_ids
assert "manual_operator_live_receipt_missing" in gap_ids
assert "real_external_services_not_verified" in gap_ids
print("live_gaps_summary=" + data["summary"])
print("live_gaps_gap_count=" + str(len(data["gaps"])))
print("live_gaps_marker=" + data["marker"])
'

printf '%s\n' "[third-test] live operator checklist readonly summary"
operator_status=0
operator_json="$(bash scripts/chuang-live-operator-checklist.sh --json)" || operator_status=$?
if [ "$operator_status" -ne 0 ] && [ "$operator_status" -ne 1 ]; then
    printf '%s\n' "[third-test] error: live operator checklist failed unexpectedly with status $operator_status" >&2
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
assert real_live["gap_count"] == 7
assert real_live["cannot_mark_complete_from_readonly_checklist"] is True
assert real_live["operator_receipt_template"] == "scripts/chuang-live-operator-receipt.sh --json"
assert real_live["operator_receipt_template_can_mark_complete"] is False
assert real_live["required_receipt_service_ids"] == [
    "feishu",
    "provider",
    "subagent_live_rehearsal",
    "desktop",
    "browser",
    "wiki",
    "gbrain",
]
service_ids = [item["id"] for item in real_live["services"]]
assert service_ids == ["feishu", "provider", "subagent_live_rehearsal", "desktop", "browser", "wiki", "gbrain"]
print("live_operator_checklist_status=" + str(data["status"]))
print("live_operator_checklist_ok=" + str(data["ok"]).lower())
print("live_operator_checklist_blockers=" + str(len(data.get("blockers", []))))
print("live_operator_real_live_acceptance=" + str(real_live["status"]))
print("live_operator_real_live_gap_count=" + str(real_live["gap_count"]))
'

printf '%s\n' "[third-test] live operator receipt readonly template"
receipt_json="$(bash scripts/chuang-live-operator-receipt.sh --json)"
printf '%s' "$receipt_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["schema_version"] == 1
assert data["acceptance_status"] == "not_verified"
assert data["can_mark_real_live_ready"] is False
assert data["cannot_mark_complete_without_operator_evidence"] is True
boundaries = data["boundaries"]
assert boundaries["readonly"] is True
assert boundaries["connects_real_feishu"] is False
assert boundaries["sends_feishu_messages"] is False
assert boundaries["connects_real_provider"] is False
assert boundaries["starts_workers"] is False
assert boundaries["performs_desktop_actions"] is False
assert boundaries["performs_browser_actions"] is False
assert boundaries["connects_real_wiki"] is False
assert boundaries["connects_real_gbrain"] is False
assert boundaries["reads_secret_values"] is False
assert boundaries["prints_secret_values"] is False
assert boundaries["modifies_repo"] is False
assert boundaries["deletes_files"] is False
assert boundaries["reuses_codex_or_hermes_credentials"] is False
service_ids = [item["id"] for item in data["service_receipts"]]
assert service_ids == [
    "feishu",
    "provider",
    "subagent_live_rehearsal",
    "desktop",
    "browser",
    "wiki",
    "gbrain",
]
assert data["service_receipts"][0]["evidence"]["runtime_report_id"] == "<fill_after_test>"
assert data["service_receipts"][1]["evidence"]["api_key_state"] == "<set|missing>"
assert data["service_receipts"][2]["evidence"]["allowlist_receipt_ref"] == "<fill_after_test>"
assert data["service_receipts"][2]["evidence"]["capability_routing_ref"] == "<fill_after_test>"
assert data["service_receipts"][5]["evidence"]["writes_core_memory"] is False
assert data["service_evidence"]["subagent_live_rehearsal"]["gate_receipt_ref"] == "<fill_after_test>"
assert data["service_evidence"]["subagent_live_rehearsal"]["allowlist_receipt_ref"] == "<fill_after_test>"
assert data["service_evidence"]["subagent_live_rehearsal"]["capability_routing_ref"] == "<fill_after_test>"
assert data["service_evidence"]["subagent_live_rehearsal"]["report_admission_ref"] == "<fill_after_test>"
assert data["real_live_acceptance"]["services"][2]["required"] == [
    "single worker only",
    "gate receipt is explicit",
    "allowlist receipt is explicit",
    "capability routing receipt is explicit",
    "report admission receipt or blocked reason is explicit",
]
print("live_operator_receipt_acceptance_status=" + str(data["acceptance_status"]))
print("live_operator_receipt_service_count=" + str(len(data["service_receipts"])))
'

printf '%s\n' "[third-test] goal run status readonly summary"
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
print("goal_run_status_overall=" + str(data["overall_status"]))
print("goal_run_status_ok=" + str(data["ok"]).lower())
'

printf '%s\n' "third_test_candidate_smoke_ok"
