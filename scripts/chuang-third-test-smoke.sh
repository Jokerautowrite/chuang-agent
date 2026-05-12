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

printf '%s\n' "[third-test] live runner readiness view"
runner_view_json="$(bash scripts/chuang-live-runner-readiness-view.sh --json)"
printf '%s' "$runner_view_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
rehearsal = data["live_runner_rehearsal"]
runtime_surface = data["runtime_report_surface"]
policy_tool_status = data["policy_tool_status"]
assert data["readonly"] is True
assert data["connects_real_provider"] is False
assert data["connects_real_feishu"] is False
assert runtime_surface["ok"] is True
assert runtime_surface["artifact_count"] == 11
assert runtime_surface["observability_field_count"] == 26
assert policy_tool_status["active_permission_profile"] == "local_ga"
assert policy_tool_status["ga_tool_descriptor_mapped_count"] == 9
assert policy_tool_status["tool_descriptor_count"] == 12
file_write = next(item for item in policy_tool_status["ga_tool_descriptors"] if item["name"] == "file_write")
assert file_write["external_commit"] is False
assert file_write["requires_approval"] is False
assert "write" in file_write["risk_tags"]
assert "runtime_meta.tool_protocol_errors_json" in runtime_surface["artifact_locators"]
assert "runtime_response.trace" in runtime_surface["artifact_locators"]
assert "runtime_meta.goal_handoff_query_summary_json" in runtime_surface["artifact_locators"]
assert "runtime_meta.subagent_children_summary_json" in runtime_surface["artifact_locators"]
assert "runtime_meta.context_compaction_summary_json" in runtime_surface["artifact_locators"]
assert "runtime_event_tool_started_count" in runtime_surface["observability_fields"]
assert "runtime_event_tool_finished_count" in runtime_surface["observability_fields"]
assert "runtime_event_approval_requested_count" in runtime_surface["observability_fields"]
assert "runtime_event_approval_resolved_count" in runtime_surface["observability_fields"]
assert "runtime_event_elicitation_requested_count" in runtime_surface["observability_fields"]
assert "tool_protocol_error_count" in runtime_surface["observability_fields"]
assert "runtime_response_trace_chars" in runtime_surface["observability_fields"]
assert "tool_unified_execution_status" in runtime_surface["observability_fields"]
assert "tool_unified_execution_failure_count" in runtime_surface["observability_fields"]
assert "tool_unified_execution_failure_classes" in runtime_surface["observability_fields"]
assert "goal_handoff_parent_context_handoff_count" in runtime_surface["observability_fields"]
assert "goal_handoff_report_admission_ref_count" in runtime_surface["observability_fields"]
assert "goal_handoff_report_admission_refs" in runtime_surface["observability_fields"]
assert "goal_handoff_report_admission_reason_codes" in runtime_surface["observability_fields"]
assert "subagent_children_child_count" in runtime_surface["observability_fields"]
assert "subagent_children_accepted_report_count" in runtime_surface["observability_fields"]
assert "subagent_children_report_admission_ref_count" in runtime_surface["observability_fields"]
assert "subagent_children_report_admission_refs" in runtime_surface["observability_fields"]
assert "subagent_children_missing_report_count" in runtime_surface["observability_fields"]
assert "subagent_children_report_reason_codes" in runtime_surface["observability_fields"]
assert "context_compaction_summary_json" in runtime_surface["observability_fields"]
assert rehearsal["ready_for_live"] is False
assert rehearsal["starts_external_worker"] is False
assert rehearsal["capability_mismatch_blocks_live"] is True
assert rehearsal["blocked_reason"]
assert rehearsal["next_action"]
print("live_runner_readiness_view_runtime_report_surface_artifacts=" + str(runtime_surface["artifact_count"]))
print("live_runner_readiness_view_runtime_report_surface_observability_fields=" + str(runtime_surface["observability_field_count"]))
print("live_runner_readiness_view_policy_tool_status_ga_tool_descriptors=" + str(policy_tool_status["ga_tool_descriptor_mapped_count"]) + "/" + str(policy_tool_status["tool_descriptor_count"]))
print("live_runner_readiness_view_state=" + str(rehearsal["state"]))
print("live_runner_readiness_view_ready_for_live=" + str(rehearsal["ready_for_live"]).lower())
print("live_runner_readiness_view_blocked_reason=" + str(rehearsal["blocked_reason"]))
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
for service in data["real_live_acceptance"]["services"]:
    assert service["manual_live_required"] is True
    assert service["must_not_count_as_complete"] is True
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
assert isinstance(data["interactive_state"], str) and data["interactive_state"]
assert isinstance(data["activity_hint"], str) and data["activity_hint"]
project_goal_run = data["project_goal_run"]
assert project_goal_run["goal_id"] in ("mainline-mvp", None)
assert isinstance(project_goal_run["checkpoint_count"], int)
assert isinstance(project_goal_run["checkpoint_log_complete"], bool)
print("goal_run_status_overall=" + str(data["overall_status"]))
print("goal_run_status_interactive_state=" + str(data["interactive_state"]))
print("goal_run_status_activity_hint=" + str(data["activity_hint"]))
print("project_goal_run_checkpoint_count=" + str(project_goal_run["checkpoint_count"]))
print("project_goal_run_last_checkpoint=" + str(project_goal_run["last_checkpoint_id"]))
print("goal_run_status_ok=" + str(data["ok"]).lower())
'

printf '%s\n' "third_test_candidate_smoke_ok"
