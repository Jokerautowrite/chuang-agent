#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
smoke_name="${CHUANG_SMOKE_NAME:-mvp}"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${smoke_name}-smoke-$$"
mkdir -p "$work_dir"

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
provider_id = "mvp-smoke-openai"
base_url = "https://api.example.com/v1"
model = "gpt-mvp-smoke"
api_key_env = "CHUANG_AGENT_SMOKE_API_KEY"
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

export CHUANG_AGENT_SMOKE_API_KEY="test-key"

cd "$root_dir"

printf '%s\n' "[smoke] status"
status_output="$(cargo run --quiet -- status --config "$config_path" --json)"
printf '%s' "$status_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["slots"]["execution"] == "generic_agent_mvp"
assert data["atomic_tools"]["ok"] is True
assert data["atomic_tools"]["manifest_schema_version"] == 1
assert data["atomic_tools"]["tool_action_schema_version"] == 1
assert data["atomic_tools"]["tool_report_schema_version"] == 6
assert data["atomic_tools"]["mapped_atomic_tool_names"] == ["file_read", "file_write", "code_execute"]
assert data["atomic_tools"]["interface_only_atomic_tool_names"] == ["mouse", "keyboard", "screenshot", "locate", "wait", "human_suspend"]
assert data["goal_mode"]["ok"] is True
assert data["goal_mode"]["cli_entrypoint"] == "run --goal TEXT"
assert data["goal_run"]["ok"] is True
assert data["goal_run"]["goal_id"] == "mainline-mvp"
assert isinstance(data["goal_run"]["plan_exists"], bool)
assert isinstance(data["goal_run"]["checkpoint_count"], int)
assert data["goal_run"]["path"].endswith("/context/goal-runs/mainline-mvp.json")
assert data["plugin_registry"]["available"] is True
assert data["plugin_registry"]["ok"] is True
assert data["local_contract_readiness"]["ok"] is True
assert data["local_contract_readiness"]["overall_state"] == "ready"
assert data["local_contract_readiness"]["contract_count"] == 4
assert data["local_contract_readiness"]["connects_real_external_services"] is False
assert data["local_contract_readiness"]["writes_core_memory"] is False
assert data["local_contract_readiness"]["executes_plugins"] is False
local_contracts = {item["name"]: item for item in data["local_contract_readiness"]["contracts"]}
assert local_contracts["knowledge_context_preview"]["read_only"] is True
assert local_contracts["skill_proposal_review"]["dry_run"] is True
assert local_contracts["plugin_registry_evidence"]["executes_plugins"] is False
assert local_contracts["external_knowledge_source_contracts"]["boundary"] == "adapter_contract_only"
assert data["project_readiness"]["ok"] is True
assert data["project_readiness"]["overall_state"] == "ready"
assert data["release_readiness"]["ok"] is True
assert data["release_readiness"]["release_name"] == "second_test_version"
assert data["release_readiness"]["overall_state"] == "second_test_version_ready"
assert data["release_readiness"]["readiness_scope"] == "readiness_and_smoke_acceptance_only_no_live_external_service_connection"
assert data["release_readiness"]["acceptance_count"] == 7
assert data["release_readiness"]["connects_real_external_services"] is False
assert data["release_readiness"]["verifies_real_external_services"] is False
assert data["release_readiness"]["uses_stub_or_local_fixtures"] is True
assert data["release_readiness"]["writes_repo_files"] is False
release_acceptance = {item["name"]: item for item in data["release_readiness"]["acceptance"]}
assert release_acceptance["status_json_readiness"]["state"] == "ready"
assert release_acceptance["doctor_json_gate"]["state"] == "ready"
assert release_acceptance["channel_preflight_only"]["state"] == "partial"
assert release_acceptance["channel_preflight_only"]["connects_real_service"] is False
assert release_acceptance["subagent_protocol_acceptance"]["state"] == "ready"
assert release_acceptance["real_external_services"]["state"] == "deferred"
assert release_acceptance["real_external_services"]["connects_real_service"] is False
module_states = {module["name"]: module["state"] for module in data["project_readiness"]["modules"]}
assert module_states["main_chain"] == "ready"
assert module_states["execution_tools"] == "ready"
assert module_states["channel"] == "ready"
assert module_states["external_ai"] == "ready"
assert module_states["subagent"] == "ready"
assert data["memory_readiness"]["ok"] is True
assert data["memory_readiness"]["overall_state"] == "ready"
memory_layers = {layer["name"]: layer["state"] for layer in data["memory_readiness"]["layers"]}
assert memory_layers["internal_identity"] == "ready"
assert memory_layers["history_session"] == "ready"
assert memory_layers["lim_long_term"] == "ready"
assert memory_layers["external_knowledge"] == "ready"
assert memory_layers["maintenance_loop"] == "ready"
assert data["channel_readiness"]["ok"] is True
assert data["channel_readiness"]["overall_state"] == "ready"
channel_layers = {layer["name"]: layer["state"] for layer in data["channel_readiness"]["layers"]}
assert channel_layers["app_server"] == "ready"
assert channel_layers["channel_simulate"] == "ready"
assert channel_layers["dedicated_feishu_bridge"] == "ready"
assert channel_layers["rich_messages"] == "ready"
assert data["subagent_readiness"]["ok"] is True
assert data["subagent_readiness"]["overall_state"] == "ready"
assert data["subagent_readiness"]["local_contract_ready"] is True
assert data["subagent_readiness"]["live_adapter_ready"] is False
subagent_layers = {layer["name"]: layer["state"] for layer in data["subagent_readiness"]["layers"]}
subagent_local = {layer["name"]: layer["local_contract_ready"] for layer in data["subagent_readiness"]["layers"]}
subagent_live = {layer["name"]: layer["live_adapter_ready"] for layer in data["subagent_readiness"]["layers"]}
assert subagent_layers["dispatch_queue"] == "ready"
assert subagent_layers["report_collect"] == "ready"
assert subagent_layers["command_runner"] == "ready"
assert subagent_layers["multi_worker"] == "ready"
assert subagent_layers["external_ai_downstream"] == "ready"
assert subagent_local["command_runner"] is True
assert subagent_local["multi_worker"] is True
assert subagent_live["command_runner"] is False
assert subagent_live["external_ai_downstream"] is False
assert data["external_ai_readiness"]["ok"] is True
assert data["external_ai_readiness"]["overall_state"] == "ready"
external_ai_layers = {layer["name"]: layer["state"] for layer in data["external_ai_readiness"]["layers"]}
assert external_ai_layers["genesis_actuator"] == "ready"
assert external_ai_layers["browser_worker_frozen"] == "ready"
assert external_ai_layers["dispatch_sop"] == "ready"
assert external_ai_layers["unified_identity_engine"] == "ready"
assert data["config"]["provider_request_timeout_ms"] is None
present = {
    "soul": data["kernel"]["identity_soul_exists"],
    "story": data["kernel"]["identity_story_exists"],
    "first_wake": data["kernel"]["identity_first_wake_exists"],
    "agents": data["kernel"]["identity_agents_registry_exists"],
}
assert present["soul"] is True
assert present["story"] is True
assert present["first_wake"] is True
assert present["agents"] is True
warnings = data["config"]["placeholder_warnings"]
assert len(warnings) == 1
assert "provider transport=stub" in warnings[0]
'

printf '%s\n' "[smoke] doctor"
doctor_output="$(cargo run --quiet -- doctor --config "$config_path" --json)"
printf '%s' "$doctor_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["ok"] is True
checks = {check["name"] for check in data["checks"]}
for name in [
    "config",
    "identity_memory",
    "memory_readiness",
    "channel_readiness",
    "subagent_readiness",
    "external_ai_readiness",
    "local_contract_readiness",
    "slots",
    "atomic_tools",
    "goal_mode",
    "goal_run_readiness",
    "project_readiness",
    "release_readiness",
    "actuator_smoke",
    "control_plane_smoke",
    "runtime_smoke",
    "subagent_queue_smoke",
    "plugin_registry",
]:
    assert name in checks, name
status = data["status"]
assert status["goal_run"]["ok"] is True
assert status["goal_run"]["goal_id"] == "mainline-mvp"
assert isinstance(status["goal_run"]["checkpoint_count"], int)
assert status["atomic_tools"]["mapped_atomic_tool_names"] == ["file_read", "file_write", "code_execute"]
assert status["atomic_tools"]["interface_only_atomic_tool_names"] == ["mouse", "keyboard", "screenshot", "locate", "wait", "human_suspend"]
assert status["project_readiness"]["ok"] is True
assert status["project_readiness"]["overall_state"] == "ready"
assert status["local_contract_readiness"]["ok"] is True
assert status["local_contract_readiness"]["overall_state"] == "ready"
assert status["local_contract_readiness"]["connects_real_external_services"] is False
assert status["local_contract_readiness"]["writes_core_memory"] is False
assert status["local_contract_readiness"]["executes_plugins"] is False
assert status["release_readiness"]["ok"] is True
assert status["release_readiness"]["overall_state"] == "second_test_version_ready"
assert status["release_readiness"]["connects_real_external_services"] is False
assert status["release_readiness"]["verifies_real_external_services"] is False
assert status["release_readiness"]["acceptance_count"] == 7
assert status["memory_readiness"]["ok"] is True
assert status["memory_readiness"]["overall_state"] == "ready"
assert status["channel_readiness"]["ok"] is True
assert status["channel_readiness"]["overall_state"] == "ready"
assert status["subagent_readiness"]["ok"] is True
assert status["subagent_readiness"]["overall_state"] == "ready"
assert status["external_ai_readiness"]["ok"] is True
assert status["external_ai_readiness"]["overall_state"] == "ready"
assert status["kernel"]["identity_soul_exists"] is True
'

printf '%s\n' "[smoke] run with session memory"
first_run_output="$(cargo run --quiet -- run --config "$config_path" --input "mvp smoke first turn" --session-id smoke-session --remember-session --goal "mvp readiness smoke")"
printf '%s' "$first_run_output" | python3 -c '
import sys
text = sys.stdin.read()
assert "goal_context_injected: true" in text
assert "session_memory_write_requested: true" in text
assert "session_memory_record_id:" in text
'
second_run_output="$(cargo run --quiet -- run --config "$config_path" --input "mvp smoke second turn" --session-id smoke-session --remember-session)"
printf '%s' "$second_run_output" | python3 -c '
import sys
text = sys.stdin.read()
assert "session_id: smoke-session" in text
assert "session_memory_recall_isolated: true" in text
assert "session_memory_recall_filter:" in text
assert "session_memory_write_requested: true" in text
'

printf '%s\n' "[smoke] goal run checkpoints"
goal_plan_output="$(cargo run --quiet -- goal plan --root "$work_dir/goal-runs" --goal-id smoke-goal --objective "mvp smoke checkpoint-first continuation" --json)"
printf '%s' "$goal_plan_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["goal_id"] == "smoke-goal"
assert data["checkpoint_count"] == 0
assert data["path"].endswith("/smoke-goal.json")
'
goal_checkpoint_output="$(cargo run --quiet -- goal checkpoint --root "$work_dir/goal-runs" --goal-id smoke-goal --summary "mvp smoke checkpoint recorded" --completed-worker-id main-process --validation-note "mvp smoke checkpoint validation noted" --json)"
printf '%s' "$goal_checkpoint_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["goal_id"] == "smoke-goal"
assert data["checkpoint_count"] == 1
'
goal_show_output="$(cargo run --quiet -- goal show --root "$work_dir/goal-runs" --goal-id smoke-goal --json)"
printf '%s' "$goal_show_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["goal_spec"]["goal_id"] == "smoke-goal"
assert data["goal_spec"]["objective"] == "mvp smoke checkpoint-first continuation"
assert data["integration_policy"]["main_process_owns_integration"] is True
assert data["integration_policy"]["workers_may_commit"] is False
assert len(data["checkpoint_log"]) == 1
assert data["checkpoint_log"][0]["summary"] == "mvp smoke checkpoint recorded"
assert data["checkpoint_log"][0]["created_at"].endswith("Z")
assert data["checkpoint_log"][0]["completed_worker_ids"] == ["main-process"]
assert data["checkpoint_log"][0]["validation_notes"] == ["mvp smoke checkpoint validation noted"]
assert data["goal_run_diagnostics"]["checkpoint_log_complete"] is True
assert data["goal_run_diagnostics"]["last_checkpoint_summary"] == "mvp smoke checkpoint recorded"
'

printf '%s\n' "[smoke] identity memory compact flow"
cargo run --quiet -- memory identity append --config "$config_path" --id smoke-memory-1 --content "mvp smoke identity memory" --json >/dev/null
cargo run --quiet -- memory identity write-memory --config "$config_path" --content "## smoke-compact\nmvp smoke identity memory\n" --approve-overwrite --json >/dev/null
cargo run --quiet -- memory identity show --config "$config_path" --json >/dev/null

printf '%s\n' "[smoke] memory maintenance dry-run"
maintenance_output="$(cargo run --quiet -- memory maintenance report --config "$config_path" --query "mvp smoke" --session-id smoke-session --json)"
printf '%s' "$maintenance_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["dry_run"] is True
assert data["writes_automatically"] is False
assert data["session_id"] == "smoke-session"
assert data["identity_health"]["experiences_file"] == "experiences.md"
assert isinstance(data["lim_candidate_count"], int)
'

printf '%s\n' "[smoke] memory maintenance apply"
cargo run --quiet -- run --config "$config_path" --input "mvp smoke maintenance apply" --session-id smoke-maintenance --remember-session >/dev/null
maintenance_apply_output="$(cargo run --quiet -- memory maintenance apply --config "$config_path" --query "mvp smoke maintenance apply" --session-id smoke-maintenance --approve-writeback --json)"
printf '%s' "$maintenance_apply_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["dry_run"] is False
assert data["approved_writeback"] is True
assert len(data["applied_candidate_ids"]) >= 1
assert len(data["skipped_candidate_ids"]) == 0
'

printf '%s\n' "[smoke] memory knowledge contract"
knowledge_output="$(cargo run --quiet -- memory knowledge status --json)"
printf '%s' "$knowledge_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["adapter"] == "external_knowledge"
assert data["dry_run"] is True
assert data["read_only"] is True
assert data["connects_real_service"] is False
assert data["writes_automatically"] is False
assert data["runtime_retrieval_wired"] is False
sources = {source["name"]: source["state"] for source in data["sources"]}
assert sources["wiki"] == "documented_only"
assert sources["gbrain"] == "documented_only"
'

printf '%s\n' "[smoke] memory knowledge local search"
knowledge_root="$work_dir/knowledge"
mkdir -p "$knowledge_root/wiki"
cat > "$knowledge_root/wiki/local.md" <<EOF
mvp smoke provenance knowledge hit
EOF
knowledge_search_output="$(cargo run --quiet -- memory knowledge search --root "$knowledge_root" --query "provenance" --json)"
printf '%s' "$knowledge_search_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["adapter"] == "local_external_knowledge"
assert data["dry_run"] is True
assert data["read_only"] is True
assert data["connects_real_service"] is False
assert data["writes_automatically"] is False
assert data["runtime_retrieval_wired"] is False
assert data["hit_count"] == 1
assert data["hits"][0]["source"] == "local_file"
assert data["hits"][0]["path"] == "wiki/local.md"
'

printf '%s\n' "[smoke] memory knowledge context preview"
knowledge_preview_output="$(cargo run --quiet -- memory knowledge preview-context --root "$knowledge_root" --query "provenance" --json)"
printf '%s' "$knowledge_preview_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["adapter"] == "local_external_knowledge"
assert data["read_only"] is True
assert data["connects_real_service"] is False
assert data["writes_automatically"] is False
assert data["runtime_injection_applied"] is False
assert data["runtime_retrieval_wired"] is False
assert data["segment_count"] == 1
assert data["segments"][0]["path"] == "wiki/local.md"
'

printf '%s\n' "[smoke] memory knowledge source contract"
knowledge_contract_output="$(cargo run --quiet -- memory knowledge source-contract --source wiki --json)"
printf '%s' "$knowledge_contract_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["source"] == "wiki"
assert data["read_only"] is True
assert data["live_adapter_configured"] is False
assert data["connects_real_service"] is False
assert data["writes_automatically"] is False
assert data["runtime_retrieval_wired"] is False
assert data["boundary"]["requires_provenance"] is True
assert data["boundary"]["writes_core_memory"] is False
'

printf '%s\n' "[smoke] runtime knowledge context opt-in"
knowledge_runtime_output="$(cargo run --quiet -- run --config "$config_path" --input "use knowledge context" --enable-knowledge-context-preview --knowledge-context-root "$knowledge_root" --knowledge-context-query "provenance")"
printf '%s' "$knowledge_runtime_output" | python3 -c '
import sys
data = sys.stdin.read()
assert "knowledge_context_preview_enabled: true" in data
assert "knowledge_context_injected: true" in data
assert "knowledge_context_connects_real_service: false" in data
assert "knowledge_context_runtime_retrieval_wired: false" in data
'

printf '%s\n' "[smoke] channel simulate"
channel_output="$(cargo run --quiet -- channel simulate --workspace-root "$work_dir" --message-id smoke-msg-1 --sender-id smoke-user --thread-id smoke-channel-thread --text "mvp smoke channel" --goal "mvp channel goal" --json)"
printf '%s' "$channel_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
meta = data["provider_meta"]
assert meta["goal_context_injected"] == "true"
assert meta["goal_objective"] == "mvp channel goal"
'

printf '%s\n' "[smoke] feishu preflight"
feishu_env_path="$work_dir/chuang-feishu.env"
cat > "$feishu_env_path" <<EOF
CHUANG_AGENT_WORKSPACE_ROOT=$work_dir
CHUANG_FEISHU_APP_ID=cli_a_smoke
CHUANG_FEISHU_APP_SECRET=smoke-secret-value
CHUANG_FEISHU_CONNECTION_MODE=websocket
EOF
feishu_check_output="$(cargo run --quiet -- channel feishu-check --env-file "$feishu_env_path" --json)"
printf '%s' "$feishu_check_output" | python3 -c '
import json, sys
raw = sys.stdin.read()
assert "smoke-secret-value" not in raw
data = json.loads(raw)
assert data["ok"] is True
assert data["env_file_is_chuang_scoped"] is True
assert data["env_file_scope_warnings"] == []
assert data["workspace_root_exists"] is True
assert data["workspace_config_exists"] is True
assert data["connection_mode"] == "websocket"
assert data["connection_mode_ok"] is True
assert data["has_legacy_names"] is False
assert data["legacy_var_names"] == []
assert data["required_vars"]["CHUANG_FEISHU_APP_SECRET"] == "<set>"
'

printf '%s\n' "[smoke] feishu bridge commands"
node scripts/chuang-feishu-command-smoke.js >/dev/null

printf '%s\n' "[smoke] feishu turn summary"
node scripts/chuang-feishu-turn-summary-smoke.js >/dev/null

printf '%s\n' "[smoke] feishu session store"
node scripts/chuang-feishu-session-smoke.js >/dev/null

printf '%s\n' "[smoke] feishu rich message renderer"
node scripts/chuang-feishu-rich-message-smoke.js >/dev/null

printf '%s\n' "[smoke] external AI dispatch contract"
external_ai_output="$(cargo run --quiet -- external-ai dispatch --platform kimi --task "mvp smoke review" --context "bounded smoke context" --dry-run --json)"
printf '%s' "$external_ai_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["adapter"] == "unified_identity_engine"
assert data["dry_run"] is True
assert data["connects_real_service"] is False
assert data["writes_memory"] is False
assert data["request"]["platform"] == "kimi"
assert data["result"]["quality"] == "acceptable"
assert data["result"]["audit_id"].startswith("external-ai-kimi-")
'

printf '%s\n' "[smoke] app-server health"
cargo run --quiet -- app-server health --workspace-root "$work_dir" --json >/dev/null

printf '%s\n' "[smoke] repl launcher"
bash -n scripts/launch-chuang-agent-repl.sh
printf 'exit\n' | CHUANG_REPL_STUB=1 scripts/launch-chuang-agent-repl.sh >/dev/null
provider_env_path="$work_dir/provider.env"
printf '%s\n' 'CODEX_PPTOKEN_API_KEY=test-key' >"$provider_env_path"
env -u CODEX_PPTOKEN_API_KEY CHUANG_PROVIDER_ENV_FILE="$provider_env_path" scripts/chuang-app-server-health.sh >/dev/null
printf 'exit\n' | env -u CODEX_PPTOKEN_API_KEY CHUANG_PROVIDER_ENV_FILE="$provider_env_path" scripts/launch-chuang-agent-repl.sh >/dev/null

printf '%s\n' "[smoke] console snapshot"
cargo run --quiet -- console snapshot --config "$config_path" --json >/dev/null

printf '%s\n' "[smoke] plugin registry"
cargo run --quiet -- plugin check --registry "$root_dir/plugins/registry.example.json" --json >/dev/null

printf '%s\n' "[smoke] skill proposal dry-run"
skill_output="$(cargo run --quiet -- skill propose --event-id smoke-event --task-id smoke-task --summary "smoke skill proposal" --json)"
printf '%s' "$skill_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["dry_run"] is True
assert data["writes_skills"] is False
assert data["requires_approval"] is True
assert data["proposal_count"] == 1
assert data["boundary"]["writes_skill_files"] is False
assert data["boundary"]["solidifies_skill"] is False
'

printf '%s\n' "[smoke] subagent queue"
dispatch_output="$(cargo run --quiet -- subagent dispatch --config "$config_path" --task "mvp smoke subagent" --requires-capability smoke --json)"
run_id="$(printf '%s' "$dispatch_output" | python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])')"
cargo run --quiet -- subagent run-once --config "$config_path" --runner command --runner-command sh --runner-arg "$root_dir/scripts/chuang-subagent-runner-example.sh" --approve-exec --capability smoke --json >/dev/null
cargo run --quiet -- subagent collect --config "$config_path" --run-id "$run_id" --json >/dev/null
parallel_dispatch_one="$(cargo run --quiet -- subagent dispatch --config "$config_path" --task "mvp smoke parallel subagent one" --requires-capability smoke --json)"
parallel_dispatch_two="$(cargo run --quiet -- subagent dispatch --config "$config_path" --task "mvp smoke parallel subagent two" --requires-capability smoke --json)"
parallel_run_one="$(printf '%s' "$parallel_dispatch_one" | python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])')"
parallel_run_two="$(printf '%s' "$parallel_dispatch_two" | python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])')"
cargo run --quiet -- subagent run-loop --config "$config_path" --runner command --runner-command sh --runner-arg "$root_dir/scripts/chuang-subagent-runner-example.sh" --approve-exec --capability smoke --max-runs 2 --max-concurrency 2 --json >/dev/null
cargo run --quiet -- subagent collect --config "$config_path" --run-id "$parallel_run_one" --json >/dev/null
cargo run --quiet -- subagent collect --config "$config_path" --run-id "$parallel_run_two" --json >/dev/null

printf '%s\n' "[smoke] command control example"
cargo run --quiet -- control list --config "$config_path" --json >/dev/null
cargo run --quiet -- control apply --config "$config_path" --unit chuang-demo-agent --action change-model --model gpt-5.4 --reason "mvp smoke" --approve --json >/dev/null

printf '%s\n' "[smoke] experiment readonly flow"
experiment_output="$(cargo run --quiet -- experiment plan --root "$work_dir/experiments" --goal "mvp smoke experiment" --success "plan can be shown" --json)"
experiment_id="$(printf '%s' "$experiment_output" | python3 -c 'import json,sys; print(json.load(sys.stdin)["experiment_id"])')"
cargo run --quiet -- experiment show --root "$work_dir/experiments" --experiment-id "$experiment_id" --json >/dev/null

printf '%s_smoke_ok work_dir=%s\n' "$smoke_name" "$work_dir"
