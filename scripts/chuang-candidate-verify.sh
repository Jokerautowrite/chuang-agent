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

printf '%s\n' "[candidate-verify] live runner readiness view"
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
print("candidate_runtime_report_surface_artifacts=" + str(runtime_surface["artifact_count"]))
print("candidate_runtime_report_surface_observability_fields=" + str(runtime_surface["observability_field_count"]))
print("candidate_policy_tool_status_ga_tool_descriptors=" + str(policy_tool_status["ga_tool_descriptor_mapped_count"]) + "/" + str(policy_tool_status["tool_descriptor_count"]))
print("candidate_live_runner_readiness_view_state=" + str(rehearsal["state"]))
print("candidate_live_runner_readiness_view_ready_for_live=" + str(rehearsal["ready_for_live"]).lower())
print("candidate_live_runner_readiness_view_blocked_reason=" + str(rehearsal["blocked_reason"]))
'

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
assert real_live["gap_count"] == 7
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
assert data["service_evidence"]["subagent_live_rehearsal"]["gate_receipt_ref"] == "<fill_after_test>"
assert data["service_evidence"]["subagent_live_rehearsal"]["allowlist_receipt_ref"] == "<fill_after_test>"
assert data["service_evidence"]["subagent_live_rehearsal"]["capability_routing_ref"] == "<fill_after_test>"
assert data["service_evidence"]["subagent_live_rehearsal"]["report_admission_ref"] == "<fill_after_test>"
real_live = data["real_live_acceptance"]
assert real_live["complete"] is False
assert real_live["status"] == "not_verified"
assert real_live["gap_count"] == 7
assert real_live["cannot_mark_complete_from_template"] is True
assert real_live["requires_operator_evidence"] is True
for service in real_live["services"]:
    assert service["manual_live_required"] is True
    assert service["must_not_count_as_complete"] is True
assert real_live["services"][2]["required"] == [
    "single worker only",
    "gate receipt is explicit",
    "allowlist receipt is explicit",
    "capability routing receipt is explicit",
    "report admission receipt or blocked reason is explicit",
]
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
assert isinstance(data["interactive_state"], str) and data["interactive_state"]
assert isinstance(data["activity_hint"], str) and data["activity_hint"]
project_goal_run = data["project_goal_run"]
assert project_goal_run["goal_id"] in ("mainline-mvp", None)
assert isinstance(project_goal_run["checkpoint_count"], int)
assert isinstance(project_goal_run["checkpoint_log_complete"], bool)
print("candidate_goal_run_status_overall=" + str(data["overall_status"]))
print("candidate_goal_run_status_interactive_state=" + str(data["interactive_state"]))
print("candidate_goal_run_status_activity_hint=" + str(data["activity_hint"]))
print("candidate_project_goal_run_checkpoint_count=" + str(project_goal_run["checkpoint_count"]))
print("candidate_project_goal_run_last_checkpoint=" + str(project_goal_run["last_checkpoint_id"]))
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
