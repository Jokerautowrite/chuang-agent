#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
work_dir="${TMPDIR:-/tmp}/chuang-terminal-acceptance-$$"
workspace="$work_dir/workspace"
queue_root="$work_dir/subagent-queue"
goal_root="$work_dir/goal-runs"
memory_root="$work_dir/hermes-memory"
db_path="$work_dir/chuang-agent.db"
config_path="$work_dir/config.toml"
real_config_path="$work_dir/config-real.toml"
provider_env_file="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"

mkdir -p "$workspace" "$queue_root" "$goal_root" "$memory_root"
cd "$root_dir"

export CHUANG_TERMINAL_ACCEPTANCE_API_KEY="test-key"
export CHUANG_REAL_ACTUATOR_ENABLE="${CHUANG_REAL_ACTUATOR_ENABLE:-1}"
export CHUANG_REAL_CONTROL_ENABLE="${CHUANG_REAL_CONTROL_ENABLE:-1}"
export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"

if [ -f "$provider_env_file" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$provider_env_file"
  set +a
fi

cat > "$config_path" <<EOF
db_path = "$db_path"
recall_limit = 5
tool_max_rounds = 4
tool_shell_timeout_ms = 30000
identity_memory_root = "$memory_root"
identity_root = "$root_dir/identity"
soul_path = "$root_dir/identity/SOUL.md"
story_path = "$root_dir/identity/STORY.md"
first_wake_path = "$root_dir/identity/FIRST_WAKE.md"
agents_registry_path = "$root_dir/identity/agents.toml"
rules_root = "$root_dir/rules"
rules_core_path = "$root_dir/rules/core.md"

provider = "openai_compatible"
provider_id = "terminal-acceptance"
base_url = "https://api.example.com/v1"
model = "terminal-acceptance-stub"
api_key_env = "CHUANG_TERMINAL_ACCEPTANCE_API_KEY"
transport = "stub"
provider_timeout_ms = 30000

subagent = "queued_external"
subagent_queue_root = "$queue_root"

actuator = "command"
actuator_program = "$root_dir/scripts/chuang-real-actuator-adapter.py"
actuator_args = "--json --allowlist $root_dir/config/actuator-allowlist.example.json"
actuator_timeout_ms = 30000

control = "command"
program = "sh"
list_args = "$root_dir/scripts/chuang-control-adapter-example.sh list --json"
apply_args = "$root_dir/scripts/chuang-control-adapter-example.sh apply --json"
control_timeout_ms = 30000

context_engine = "deterministic_budget"
context_max_tokens = 272000
context_reserve_system_tokens = 32
context_min_working_tokens = 1
context_max_tool_results = 5
context_max_memory_segments = 5
EOF

if [ -n "${CHUANG_PROXY_API_KEY:-}" ]; then
cat > "$real_config_path" <<EOF
db_path = "$db_path"
recall_limit = 5
tool_max_rounds = 4
tool_shell_timeout_ms = 30000
identity_memory_root = "$memory_root"
identity_root = "$root_dir/identity"
soul_path = "$root_dir/identity/SOUL.md"
story_path = "$root_dir/identity/STORY.md"
first_wake_path = "$root_dir/identity/FIRST_WAKE.md"
agents_registry_path = "$root_dir/identity/agents.toml"
rules_root = "$root_dir/rules"
rules_core_path = "$root_dir/rules/core.md"

provider = "openai_compatible"
provider_id = "example-provider"
base_url = "https://example-provider.example/v1"
model = "gpt-5.5"
api_key_env = "CHUANG_PROXY_API_KEY"
transport = "native"
provider_timeout_ms = 120000

subagent = "queued_external"
subagent_queue_root = "$queue_root"

actuator = "command"
actuator_program = "$root_dir/scripts/chuang-real-actuator-adapter.py"
actuator_args = "--json --allowlist $root_dir/config/actuator-allowlist.example.json"
actuator_timeout_ms = 30000

control = "command"
program = "sh"
list_args = "$root_dir/scripts/chuang-control-adapter-example.sh list --json"
apply_args = "$root_dir/scripts/chuang-control-adapter-example.sh apply --json"
control_timeout_ms = 30000

context_engine = "deterministic_budget"
context_max_tokens = 272000
context_reserve_system_tokens = 32
context_min_working_tokens = 1
context_max_tool_results = 5
context_max_memory_segments = 5
EOF
else
  real_config_path=""
fi

run_chuang() {
  cargo run --quiet --manifest-path "$root_dir/Cargo.toml" -- "$@"
}

printf '%s\n' "[terminal] command entry"
command -v chuang >/dev/null
bash -n "$HOME/.local/bin/chuang"
bash -n "$root_dir/scripts/launch-chuang-agent-repl.sh"
chuang --help | grep -F 'chuang accept' >/dev/null

printf '%s\n' "[terminal] status gates"
status_json="$work_dir/status.json"
run_chuang status --config "$config_path" --json > "$status_json"
python3 - "$status_json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
assert data["project_readiness"]["overall_state"] == "ready"
assert data["provider_readiness"]["ok"] is True
assert data["provider_readiness"]["transport"] == "stub"
assert data["provider_readiness"]["api_key_state"] == "<set>"
assert data["atomic_tools"]["mapped_count"] == 9
assert data["atomic_tools"]["interface_only_count"] == 0
gates = data["live_adapter_gates"]
assert gates["enabled_count"] == 3
assert gates["disabled_count"] == 0
assert {gate["name"]: gate["enabled"] for gate in gates["gates"]} == {
    "subagent_runner": True,
    "control_apply": True,
    "actuator_operation": True,
}
PY

printf '%s\n' "[terminal] tool loop file and command"
if [ -z "$real_config_path" ]; then
  printf '%s\n' "[terminal] error: missing CHUANG_PROXY_API_KEY for real tool-loop acceptance" >&2
  printf '%s\n' "[terminal] provider_env_file=$provider_env_file" >&2
  exit 3
fi
tool_json="$work_dir/tool-run.json"
(
  cd "$workspace"
  run_chuang run \
    --config "$real_config_path" \
    --input '请在当前项目目录新建 notes/terminal.txt，内容写 terminal-ok，然后运行 cat notes/terminal.txt 验证。完成后回复结果。'
) > "$tool_json"
python3 - "$tool_json" "$workspace/notes/terminal.txt" <<'PY'
import json, sys
text = open(sys.argv[1], encoding="utf-8").read()
assert "body:" in text
assert open(sys.argv[2], encoding="utf-8").read().strip() == "terminal-ok"
assert "tool_calls_json:" in text
assert '"atomic_tool_name":"file_write"' in text or '"atomic_tool_name":"code_execute"' in text
PY

printf '%s\n' "[terminal] local denial correction"
denial_json="$work_dir/denial-run.json"
(
  cd "$workspace"
  run_chuang run \
    --config "$real_config_path" \
    --input '在当前项目目录新建文件夹 notes/local-denial-check。你必须用工具完成。'
) > "$denial_json"
test -d "$workspace/notes/local-denial-check"

printf '%s\n' "[terminal] chuang ask caller cwd"
caller_workspace="$work_dir/caller-workspace"
mkdir -p "$caller_workspace"
caller_json="$work_dir/caller-ask.txt"
(
  cd "$caller_workspace"
  chuang ask '在当前目录新建文件 notes/caller.txt，内容写 caller-ok，然后 cat notes/caller.txt 验证。' > "$caller_json"
)
test "$(cat "$caller_workspace/notes/caller.txt")" = "caller-ok"
grep -F "workspace_root\":\"$caller_workspace\"" "$caller_json" >/dev/null

printf '%s\n' "[terminal] memory session"
run_chuang run \
  --config "$config_path" \
  --session-id terminal-acceptance \
  --remember-session \
  --input '记住这条终端验收记忆：terminal-acceptance-memory-ok' \
  >/dev/null
memory_json="$work_dir/memory-search.json"
run_chuang memory session search \
  --config "$config_path" \
  --session-id terminal-acceptance \
  --query terminal-acceptance-memory-ok \
  --json > "$memory_json"
python3 - "$memory_json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
assert data["hit_count"] >= 1
assert any("terminal-acceptance-memory-ok" in hit.get("content", "") for hit in data["hits"])
PY

printf '%s\n' "[terminal] subagent dispatch run collect"
dispatch_json="$work_dir/subagent-dispatch.json"
run_chuang subagent dispatch \
  --config "$config_path" \
  --task "terminal acceptance subagent task" \
  --requires-capability terminal \
  --json > "$dispatch_json"
run_id="$(python3 - "$dispatch_json" <<'PY'
import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["run_id"])
PY
)"
run_chuang subagent run-once \
  --config "$config_path" \
  --runner command \
  --runner-command sh \
  --runner-arg "$root_dir/scripts/chuang-subagent-runner-example.sh" \
  --approve-exec \
  --capability terminal \
  --json > "$work_dir/subagent-run-once.json"
run_chuang subagent collect --config "$config_path" --run-id "$run_id" --json > "$work_dir/subagent-collect.json"
python3 - "$work_dir/subagent-collect.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
assert data["dispatch_available"] is True
assert data["report_available"] is True
assert data["report"]["status"] == "Success"
assert data["report_admission"]["status"] == "Accepted"
assert data["parent_context_handoff"]["accepted"] is True
PY

printf '%s\n' "[terminal] goal plan dispatch step collect"
goal_id="terminal-acceptance-goal"
run_chuang goal plan \
  --root "$goal_root" \
  --goal-id "$goal_id" \
  --objective "terminal acceptance goal" \
  --max-subtasks 1 \
  --json > "$work_dir/goal-plan.json"
run_chuang goal dispatch \
  --root "$goal_root" \
  --goal-id "$goal_id" \
  --subagent-queue-root "$queue_root" \
  --json > "$work_dir/goal-dispatch.json"
run_chuang goal step \
  --root "$goal_root" \
  --goal-id "$goal_id" \
  --subagent-queue-root "$queue_root" \
  --max-runs 1 \
  --max-concurrency 1 \
  --runner command \
  --runner-command sh \
  --runner-arg "$root_dir/scripts/chuang-subagent-runner-example.sh" \
  --approve-exec \
  --capability smoke \
  --json > "$work_dir/goal-step.json"
run_chuang goal collect \
  --root "$goal_root" \
  --goal-id "$goal_id" \
  --subagent-queue-root "$queue_root" \
  --json > "$work_dir/goal-collect.json"
python3 - "$work_dir/goal-collect.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
assert data["ready_to_checkpoint"] is True
assert data["available_report_count"] >= 1
assert data["missing_run_ids"] == []
assert data["blocked_report_run_ids"] == []
assert data["parent_context_handoffs"][0]["accepted"] is True
assert data["handoff_query_summary"]["report_admission_ref_count"] >= 1
assert data["handoff_query_summary"]["report_admission_reason_codes"]["report_validated"] >= 1
PY

printf '%s\n' "chuang_terminal_acceptance_ok"
printf '%s\n' "work_dir=$work_dir"
