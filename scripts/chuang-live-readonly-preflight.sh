#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
preflight_name="${CHUANG_PREFLIGHT_NAME:-live_readiness}"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${preflight_name}-preflight-$$"
watchdog_log_dir="$work_dir/watchdog"
config_path="$work_dir/config.toml"
session_name="${CHUANG_WATCHDOG_SESSION:-chuang-goal}"

mkdir -p "$watchdog_log_dir"

# Keep this wrapper local-only even when the operator shell has live gates set.
unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

cd "$root_dir"

printf '%s\n' "[preflight] watchdog readonly once"
ROOT="$root_dir" \
SESSION="$session_name" \
LOG_DIR="$watchdog_log_dir" \
bash "$root_dir/scripts/chuang-goal-watchdog.sh" --once >/dev/null
python3 - "$watchdog_log_dir/latest-watchdog-report.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)

assert data["readonly"] is True
assert data["boundaries"]["dispatches_tasks"] is False
assert data["boundaries"]["modifies_repo"] is False
assert data["boundaries"]["restarts_worker"] is False
assert data["boundaries"]["touches_services"] is False
PY

printf '%s\n' "[preflight] local diagnostic config"
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
provider_id = "live-readiness-preflight-openai"
base_url = "https://api.example.com/v1"
model = "gpt-live-readiness-preflight"
api_key_env = "CHUANG_AGENT_LIVE_READINESS_PREFLIGHT_API_KEY"
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
export CHUANG_AGENT_LIVE_READINESS_PREFLIGHT_API_KEY="test-key"

run_quiet() {
  label="$1"
  shift
  printf '%s\n' "[preflight] $label"
  "$@" >/dev/null
}

run_quiet "status diagnostic" cargo run --quiet -- status --config "$config_path" --json
run_quiet "doctor diagnostic" cargo run --quiet -- doctor --config "$config_path" --json
run_quiet "app-server health diagnostic" cargo run --quiet -- app-server health --workspace-root "$work_dir" --diagnostic --json
run_quiet "console snapshot diagnostic" cargo run --quiet -- console snapshot --config "$config_path" --json

printf '%s\n' "[preflight] complete local smoke"
CHUANG_SMOKE_NAME=live_readiness sh "$root_dir/scripts/chuang-complete-local-smoke.sh"

printf 'live_readiness_preflight_ok work_dir=%s watchdog_log_dir=%s\n' "$work_dir" "$watchdog_log_dir"
