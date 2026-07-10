#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
smoke_name="complete_local"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${smoke_name}-smoke-$$"
watchdog_log_dir="$work_dir/watchdog"
mkdir -p "$watchdog_log_dir"

# Keep this wrapper local-only even when the operator shell has live gates set.
unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

cd "$root_dir"

printf '%s\n' "[complete] second test smoke"
sh "$root_dir/scripts/chuang-second-test-smoke.sh"

printf '%s\n' "[complete] watchdog readonly once"
ROOT="$root_dir" \
SESSION="chuang-complete-local-smoke-missing-$$" \
LOG_DIR="$watchdog_log_dir" \
bash "$root_dir/scripts/chuang-goal-watchdog.sh" --once >/dev/null
python3 - "$watchdog_log_dir/latest-watchdog-report.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)

assert data["readonly"] is True
assert data["tmux_session_present"] is False
assert data["boundaries"]["dispatches_tasks"] is False
assert data["boundaries"]["modifies_repo"] is False
assert data["boundaries"]["restarts_worker"] is False
assert data["boundaries"]["touches_services"] is False
PY

printf '%s\n' "[complete] local diagnostic config"
config_path="$work_dir/config.toml"
cat > "$config_path" <<EOF
db_path = "$work_dir/chuang-agent.db"
recall_limit = 5
identity_memory_root = "$work_dir/hermes-memory"
identity_root = "$root_dir/identity"
soul_path = "$root_dir/identity/SOUL.md"
story_path = "$root_dir/identity/STORY.md"
first_wake_path = "$root_dir/identity/FIRST_WAKE.md"
agents_registry_path = "$root_dir/identity/agents.toml"
rules_root = "$root_dir/rules"
rules_core_path = "$root_dir/rules/core.md"

provider = "openai_compatible"
provider_id = "complete-local-openai"
base_url = "https://api.example.com/v1"
model = "gpt-complete-local-smoke"
api_key_env = "CHUANG_AGENT_COMPLETE_SMOKE_API_KEY"
transport = "stub"

subagent = "queued_external"
subagent_queue_root = "$work_dir/subagent-queue"

actuator = "command"
actuator_program = "sh"
actuator_args = "$root_dir/scripts/chuang-actuator-adapter-example.sh --json"
actuator_timeout_ms = 30000

control = "command"
program = "sh"
list_args = "$root_dir/scripts/chuang-control-adapter-example.sh list --json"
apply_args = "$root_dir/scripts/chuang-control-adapter-example.sh apply --json"
control_timeout_ms = 30000
EOF
export CHUANG_AGENT_COMPLETE_SMOKE_API_KEY="test-key"

printf '%s\n' "[complete] status readiness"
status_output="$(cargo run --quiet -- status --config "$config_path" --json)"
printf '%s' "$status_output" | python3 -c '
import json, sys
def assert_live_readiness(live_readiness):
    assert live_readiness["ok"] is True
    global_real_live_ready = live_readiness["overall_state"] == "global_real_live_ready"
    assert live_readiness["overall_state"] in ("local_ready_live_pending", "global_real_live_ready")
    assert live_readiness["ga_local_mapped_only"] is True
    assert live_readiness["desktop_browser_live_gated"] is True
    assert live_readiness["browser_worker_frozen"] is True
    assert live_readiness["live_worker_available"] is False
    assert live_readiness["real_external_acceptance_pending"] is (not global_real_live_ready)
    assert live_readiness["provider_live_request_verified_by_status"] is global_real_live_ready
    assert live_readiness["ready_does_not_mean_live"] is True
data = json.load(sys.stdin)
assert data["project_readiness"]["overall_state"] == "ready"
assert data["release_readiness"]["overall_state"] == "second_test_version_ready"
assert data["release_readiness"]["connects_real_external_services"] is False
assert data["release_readiness"]["verifies_real_external_services"] is False
assert data["release_readiness"]["uses_stub_or_local_fixtures"] is True
assert data["memory_readiness"]["overall_state"] == "ready"
assert data["channel_readiness"]["overall_state"] == "ready"
assert data["goal_run"]["ok"] is True
assert data["goal_run"]["goal_id"] == "mainline-mvp"
assert data["goal_run"]["plan_exists"] is True
assert_live_readiness(data["live_readiness"])
policy_tool_status = data["policy_tool_status"]
assert policy_tool_status["active_permission_profile"] == "full_local_workspace"
assert policy_tool_status["ga_tool_descriptor_mapped_count"] == 9
assert policy_tool_status["tool_descriptor_count"] == 12
file_write = next(item for item in policy_tool_status["ga_tool_descriptors"] if item["name"] == "file_write")
assert file_write["external_commit"] is False
assert file_write["requires_approval"] is False
assert "write" in file_write["risk_tags"]
runtime_report_surface = data["runtime_report_surface"]
assert runtime_report_surface["ok"] is True
assert runtime_report_surface["artifact_count"] == 11
assert runtime_report_surface["observability_field_count"] == 26
assert "runtime_meta.tool_protocol_errors_json" in runtime_report_surface["artifact_locators"]
assert "runtime_response.trace" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.goal_handoff_query_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.subagent_children_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.context_compaction_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_event_tool_started_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_tool_finished_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_requested_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_resolved_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_elicitation_requested_count" in runtime_report_surface["observability_fields"]
assert "tool_protocol_error_count" in runtime_report_surface["observability_fields"]
assert "runtime_response_trace_chars" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_status" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_count" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_classes" in runtime_report_surface["observability_fields"]
assert "goal_handoff_query_summary_json" in runtime_report_surface["observability_fields"]
assert "subagent_children_summary_json" in runtime_report_surface["observability_fields"]
assert "goal_handoff_parent_context_handoff_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_reason_codes" in runtime_report_surface["observability_fields"]
assert "subagent_children_child_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_accepted_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "subagent_children_missing_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_reason_codes" in runtime_report_surface["observability_fields"]
assert "context_compaction_summary_json" in runtime_report_surface["observability_fields"]
assert data["provider_readiness"]["ok"] is True
assert data["provider_readiness"]["provider_kind"] == "openai_compatible"
assert data["provider_readiness"]["provider_id"] == "complete-local-openai"
assert data["provider_readiness"]["model_name"] == "gpt-complete-local-smoke"
assert data["provider_readiness"]["transport"] == "stub"
assert data["provider_readiness"]["fallback_configured"] is False
assert data["provider_readiness"]["api_key_state"] == "<set>"
assert data["provider_readiness"]["placeholder_warning_count"] == 1
assert data["subagent_readiness"]["local_contract_ready"] is True
assert data["subagent_readiness"]["live_adapter_ready"] is False
assert data["subagent_readiness"]["live_worker_available"] is False
assert data["subagent_readiness"]["worker_runtime_state"] == "local_contract_only"
for gate in data["live_adapter_gates"]["gates"]:
    assert gate["enabled"] is False
'

printf '%s\n' "[complete] doctor readiness"
doctor_output="$(cargo run --quiet -- doctor --config "$config_path" --json)"
printf '%s' "$doctor_output" | python3 -c '
import json, sys
def assert_live_readiness(live_readiness):
    assert live_readiness["ok"] is True
    global_real_live_ready = live_readiness["overall_state"] == "global_real_live_ready"
    assert live_readiness["overall_state"] in ("local_ready_live_pending", "global_real_live_ready")
    assert live_readiness["ga_local_mapped_only"] is True
    assert live_readiness["desktop_browser_live_gated"] is True
    assert live_readiness["browser_worker_frozen"] is True
    assert live_readiness["live_worker_available"] is False
    assert live_readiness["real_external_acceptance_pending"] is (not global_real_live_ready)
    assert live_readiness["provider_live_request_verified_by_status"] is global_real_live_ready
    assert live_readiness["ready_does_not_mean_live"] is True
data = json.load(sys.stdin)
assert data["ok"] is True
checks_by_name = {check["name"]: check for check in data["checks"]}
checks = set(checks_by_name)
for name in [
    "config",
    "project_readiness",
    "release_readiness",
    "memory_readiness",
    "channel_readiness",
    "provider_readiness",
    "subagent_readiness",
    "live_adapter_preflight",
]:
    assert name in checks, name
assert data["status"]["release_readiness"]["connects_real_external_services"] is False
assert data["status"]["goal_run"]["ok"] is True
assert data["status"]["goal_run"]["goal_id"] == "mainline-mvp"
assert data["status"]["goal_run"]["plan_exists"] is True
assert_live_readiness(data["status"]["live_readiness"])
policy_tool_status = data["status"]["policy_tool_status"]
assert policy_tool_status["active_permission_profile"] == "full_local_workspace"
assert policy_tool_status["ga_tool_descriptor_mapped_count"] == 9
assert policy_tool_status["tool_descriptor_count"] == 12
file_write = next(item for item in policy_tool_status["ga_tool_descriptors"] if item["name"] == "file_write")
assert file_write["external_commit"] is False
assert file_write["requires_approval"] is False
assert "write" in file_write["risk_tags"]
runtime_report_surface = data["status"]["runtime_report_surface"]
assert runtime_report_surface["ok"] is True
assert runtime_report_surface["artifact_count"] == 11
assert runtime_report_surface["observability_field_count"] == 26
assert "runtime_meta.tool_protocol_errors_json" in runtime_report_surface["artifact_locators"]
assert "runtime_response.trace" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.goal_handoff_query_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.subagent_children_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.context_compaction_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_event_tool_started_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_tool_finished_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_requested_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_resolved_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_elicitation_requested_count" in runtime_report_surface["observability_fields"]
assert "tool_protocol_error_count" in runtime_report_surface["observability_fields"]
assert "runtime_response_trace_chars" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_status" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_count" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_classes" in runtime_report_surface["observability_fields"]
assert "goal_handoff_query_summary_json" in runtime_report_surface["observability_fields"]
assert "subagent_children_summary_json" in runtime_report_surface["observability_fields"]
assert "goal_handoff_parent_context_handoff_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_reason_codes" in runtime_report_surface["observability_fields"]
assert "subagent_children_child_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_accepted_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "subagent_children_missing_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_reason_codes" in runtime_report_surface["observability_fields"]
assert "context_compaction_summary_json" in runtime_report_surface["observability_fields"]
goal_run_readiness = checks_by_name["goal_run_readiness"]
assert goal_run_readiness["ok"] is True
assert "goal_id=mainline-mvp" in goal_run_readiness["detail"]
assert "plan_exists=true" in goal_run_readiness["detail"]
assert data["status"]["provider_readiness"]["transport"] == "stub"
assert data["status"]["provider_readiness"]["api_key_state"] == "<set>"
assert data["status"]["subagent_readiness"]["live_worker_available"] is False
assert data["status"]["subagent_readiness"]["worker_runtime_state"] == "local_contract_only"
'

printf '%s\n' "[complete] app-server health diagnostic"
app_health_output="$(cargo run --quiet -- app-server health --workspace-root "$work_dir" --diagnostic --json)"
printf '%s' "$app_health_output" | python3 -c '
import json, sys
def assert_live_readiness(live_readiness):
    assert live_readiness["ok"] is True
    global_real_live_ready = live_readiness["overall_state"] == "global_real_live_ready"
    assert live_readiness["overall_state"] in ("local_ready_live_pending", "global_real_live_ready")
    assert live_readiness["ga_local_mapped_only"] is True
    assert live_readiness["desktop_browser_live_gated"] is True
    assert live_readiness["browser_worker_frozen"] is True
    assert live_readiness["live_worker_available"] is False
    assert live_readiness["real_external_acceptance_pending"] is (not global_real_live_ready)
    assert live_readiness["provider_live_request_verified_by_status"] is global_real_live_ready
    assert live_readiness["ready_does_not_mean_live"] is True
data = json.load(sys.stdin)
assert data["ok"] is True
assert data["diagnostic_mode"] is True
assert data["release_readiness"]["connects_real_external_services"] is False
assert data["release_readiness"]["verifies_real_external_services"] is False
assert data["release_readiness"]["uses_stub_or_local_fixtures"] is True
assert data["goal_run"]["ok"] is True
assert data["goal_run"]["goal_id"] == "mainline-mvp"
assert data["goal_run"]["plan_exists"] is True
assert_live_readiness(data["live_readiness"])
policy_tool_status = data["policy_tool_status"]
assert policy_tool_status["active_permission_profile"] == "full_local_workspace"
assert policy_tool_status["ga_tool_descriptor_mapped_count"] == 9
assert policy_tool_status["tool_descriptor_count"] == 12
file_write = next(item for item in policy_tool_status["ga_tool_descriptors"] if item["name"] == "file_write")
assert file_write["external_commit"] is False
assert file_write["requires_approval"] is False
assert "write" in file_write["risk_tags"]
runtime_report_surface = data["runtime_report_surface"]
assert runtime_report_surface["ok"] is True
assert runtime_report_surface["artifact_count"] == 11
assert runtime_report_surface["observability_field_count"] == 26
assert "runtime_meta.tool_protocol_errors_json" in runtime_report_surface["artifact_locators"]
assert "runtime_response.trace" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.goal_handoff_query_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.subagent_children_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.context_compaction_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_event_tool_started_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_tool_finished_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_requested_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_resolved_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_elicitation_requested_count" in runtime_report_surface["observability_fields"]
assert "tool_protocol_error_count" in runtime_report_surface["observability_fields"]
assert "runtime_response_trace_chars" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_status" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_count" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_classes" in runtime_report_surface["observability_fields"]
assert "goal_handoff_query_summary_json" in runtime_report_surface["observability_fields"]
assert "subagent_children_summary_json" in runtime_report_surface["observability_fields"]
assert "goal_handoff_parent_context_handoff_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_reason_codes" in runtime_report_surface["observability_fields"]
assert "subagent_children_child_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_accepted_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "subagent_children_missing_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_reason_codes" in runtime_report_surface["observability_fields"]
assert "context_compaction_summary_json" in runtime_report_surface["observability_fields"]
assert data["provider_readiness"]["ok"] is True
assert data["provider_readiness"]["provider_kind"] == "openai_compatible"
assert data["provider_readiness"]["transport"] == "stub"
assert data["provider_readiness"]["api_key_state"] == "<set>"
assert data["provider_readiness"]["placeholder_warning_count"] == 1
assert data["subagent_readiness"]["local_contract_ready"] is True
assert data["subagent_readiness"]["live_adapter_ready"] is False
assert data["subagent_readiness"]["live_worker_available"] is False
assert data["subagent_readiness"]["worker_runtime_state"] == "local_contract_only"
'

printf '%s\n' "[complete] console snapshot"
console_output="$(cargo run --quiet -- console snapshot --config "$config_path" --json)"
printf '%s' "$console_output" | python3 -c '
import json, sys
def assert_live_readiness(live_readiness):
    assert live_readiness["ok"] is True
    global_real_live_ready = live_readiness["overall_state"] == "global_real_live_ready"
    assert live_readiness["overall_state"] in ("local_ready_live_pending", "global_real_live_ready")
    assert live_readiness["ga_local_mapped_only"] is True
    assert live_readiness["desktop_browser_live_gated"] is True
    assert live_readiness["browser_worker_frozen"] is True
    assert live_readiness["live_worker_available"] is False
    assert live_readiness["real_external_acceptance_pending"] is (not global_real_live_ready)
    assert live_readiness["provider_live_request_verified_by_status"] is global_real_live_ready
    assert live_readiness["ready_does_not_mean_live"] is True
data = json.load(sys.stdin)
status = data["status"]
assert data["ok"] is True
assert status["project_readiness"]["overall_state"] == "ready"
assert status["release_readiness"]["overall_state"] == "second_test_version_ready"
assert status["release_readiness"]["connects_real_external_services"] is False
assert status["release_readiness"]["verifies_real_external_services"] is False
assert status["goal_run"]["ok"] is True
assert status["goal_run"]["goal_id"] == "mainline-mvp"
assert status["goal_run"]["plan_exists"] is True
assert_live_readiness(status["live_readiness"])
policy_tool_status = status["policy_tool_status"]
assert policy_tool_status["active_permission_profile"] == "full_local_workspace"
assert policy_tool_status["ga_tool_descriptor_mapped_count"] == 9
assert policy_tool_status["tool_descriptor_count"] == 12
file_write = next(item for item in policy_tool_status["ga_tool_descriptors"] if item["name"] == "file_write")
assert file_write["external_commit"] is False
assert file_write["requires_approval"] is False
assert "write" in file_write["risk_tags"]
runtime_report_surface = status["runtime_report_surface"]
assert runtime_report_surface["ok"] is True
assert runtime_report_surface["artifact_count"] == 11
assert runtime_report_surface["observability_field_count"] == 26
assert "runtime_meta.tool_protocol_errors_json" in runtime_report_surface["artifact_locators"]
assert "runtime_response.trace" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.goal_handoff_query_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.subagent_children_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_meta.context_compaction_summary_json" in runtime_report_surface["artifact_locators"]
assert "runtime_event_tool_started_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_tool_finished_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_requested_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_approval_resolved_count" in runtime_report_surface["observability_fields"]
assert "runtime_event_elicitation_requested_count" in runtime_report_surface["observability_fields"]
assert "tool_protocol_error_count" in runtime_report_surface["observability_fields"]
assert "runtime_response_trace_chars" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_status" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_count" in runtime_report_surface["observability_fields"]
assert "tool_unified_execution_failure_classes" in runtime_report_surface["observability_fields"]
assert "goal_handoff_query_summary_json" in runtime_report_surface["observability_fields"]
assert "subagent_children_summary_json" in runtime_report_surface["observability_fields"]
assert "goal_handoff_parent_context_handoff_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "goal_handoff_report_admission_reason_codes" in runtime_report_surface["observability_fields"]
assert "subagent_children_child_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_accepted_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_ref_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_admission_refs" in runtime_report_surface["observability_fields"]
assert "subagent_children_missing_report_count" in runtime_report_surface["observability_fields"]
assert "subagent_children_report_reason_codes" in runtime_report_surface["observability_fields"]
assert status["provider_readiness"]["transport"] == "stub"
assert status["provider_readiness"]["api_key_state"] == "<set>"
assert status["subagent_readiness"]["live_worker_available"] is False
assert status["subagent_readiness"]["worker_runtime_state"] == "local_contract_only"
'

printf '%s\n' "[complete] goal mode smoke"
sh "$root_dir/scripts/chuang-goal-mode-smoke.sh"

printf '%s\n' "[complete] goal mode negative smoke"
sh "$root_dir/scripts/chuang-goal-mode-negative-smoke.sh"

printf '%s\n' "[complete] feishu local command smokes"
node scripts/chuang-feishu-command-smoke.js >/dev/null
node scripts/chuang-feishu-turn-summary-smoke.js >/dev/null
node scripts/chuang-feishu-image-smoke.js >/dev/null
node scripts/chuang-feishu-session-smoke.js >/dev/null
node scripts/chuang-feishu-rich-message-smoke.js >/dev/null

printf 'complete_local_smoke_ok work_dir=%s watchdog_log_dir=%s\n' "$work_dir" "$watchdog_log_dir"
