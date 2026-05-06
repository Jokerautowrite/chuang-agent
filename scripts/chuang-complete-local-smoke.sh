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
data = json.load(sys.stdin)
assert data["project_readiness"]["overall_state"] == "ready"
assert data["release_readiness"]["overall_state"] == "second_test_version_ready"
assert data["release_readiness"]["connects_real_external_services"] is False
assert data["release_readiness"]["verifies_real_external_services"] is False
assert data["release_readiness"]["uses_stub_or_local_fixtures"] is True
assert data["memory_readiness"]["overall_state"] == "ready"
assert data["channel_readiness"]["overall_state"] == "ready"
assert data["subagent_readiness"]["local_contract_ready"] is True
assert data["subagent_readiness"]["live_adapter_ready"] is False
for gate in data["live_adapter_gates"]["gates"]:
    assert gate["enabled"] is False
'

printf '%s\n' "[complete] doctor readiness"
doctor_output="$(cargo run --quiet -- doctor --config "$config_path" --json)"
printf '%s' "$doctor_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["ok"] is True
checks = {check["name"] for check in data["checks"]}
for name in [
    "config",
    "project_readiness",
    "release_readiness",
    "memory_readiness",
    "channel_readiness",
    "subagent_readiness",
    "live_adapter_preflight",
]:
    assert name in checks, name
assert data["status"]["release_readiness"]["connects_real_external_services"] is False
'

printf '%s\n' "[complete] app-server health diagnostic"
app_health_output="$(cargo run --quiet -- app-server health --workspace-root "$work_dir" --diagnostic --json)"
printf '%s' "$app_health_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["ok"] is True
assert data["diagnostic_mode"] is True
assert data["release_readiness"]["connects_real_external_services"] is False
assert data["release_readiness"]["verifies_real_external_services"] is False
assert data["release_readiness"]["uses_stub_or_local_fixtures"] is True
'

printf '%s\n' "[complete] console snapshot"
console_output="$(cargo run --quiet -- console snapshot --config "$config_path" --json)"
printf '%s' "$console_output" | python3 -c '
import json, sys
data = json.load(sys.stdin)
status = data["status"]
assert data["ok"] is True
assert status["project_readiness"]["overall_state"] == "ready"
assert status["release_readiness"]["overall_state"] == "second_test_version_ready"
assert status["release_readiness"]["connects_real_external_services"] is False
assert status["release_readiness"]["verifies_real_external_services"] is False
'

printf '%s\n' "[complete] feishu local command smokes"
node scripts/chuang-feishu-command-smoke.js >/dev/null
node scripts/chuang-feishu-session-smoke.js >/dev/null
node scripts/chuang-feishu-rich-message-smoke.js >/dev/null

printf 'complete_local_smoke_ok work_dir=%s watchdog_log_dir=%s\n' "$work_dir" "$watchdog_log_dir"
