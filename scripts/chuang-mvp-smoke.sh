#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
work_dir="${TMPDIR:-/tmp}/chuang-agent-mvp-smoke-$$"
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
cargo run --quiet -- status --config "$config_path" >/dev/null

printf '%s\n' "[smoke] doctor"
cargo run --quiet -- doctor --config "$config_path" >/dev/null

printf '%s\n' "[smoke] run with session memory"
cargo run --quiet -- run --config "$config_path" --input "mvp smoke first turn" --session-id smoke-session --remember-session >/dev/null
cargo run --quiet -- run --config "$config_path" --input "mvp smoke second turn" --session-id smoke-session --remember-session >/dev/null

printf '%s\n' "[smoke] identity memory compact flow"
cargo run --quiet -- memory identity append --config "$config_path" --id smoke-memory-1 --content "mvp smoke identity memory" --json >/dev/null
cargo run --quiet -- memory identity write-memory --config "$config_path" --content "## smoke-compact\nmvp smoke identity memory\n" --approve-overwrite --json >/dev/null
cargo run --quiet -- memory identity show --config "$config_path" --json >/dev/null

printf '%s\n' "[smoke] channel simulate"
cargo run --quiet -- channel simulate --workspace-root "$work_dir" --message-id smoke-msg-1 --sender-id smoke-user --thread-id smoke-channel-thread --text "mvp smoke channel" --json >/dev/null

printf '%s\n' "[smoke] app-server health"
cargo run --quiet -- app-server health --workspace-root "$work_dir" --json >/dev/null

printf '%s\n' "[smoke] console snapshot"
cargo run --quiet -- console snapshot --config "$config_path" --json >/dev/null

printf '%s\n' "[smoke] plugin registry"
cargo run --quiet -- plugin check --registry "$root_dir/plugins/registry.example.json" --json >/dev/null

printf '%s\n' "[smoke] subagent queue"
dispatch_output="$(cargo run --quiet -- subagent dispatch --config "$config_path" --task "mvp smoke subagent" --requires-capability smoke --json)"
run_id="$(printf '%s' "$dispatch_output" | python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])')"
cargo run --quiet -- subagent run-once --config "$config_path" --runner command --runner-command sh --runner-arg "$root_dir/scripts/chuang-subagent-runner-example.sh" --approve-exec --capability smoke --json >/dev/null
cargo run --quiet -- subagent collect --config "$config_path" --run-id "$run_id" --json >/dev/null

printf '%s\n' "[smoke] command control example"
cargo run --quiet -- control list --config "$config_path" --json >/dev/null
cargo run --quiet -- control apply --config "$config_path" --unit chuang-demo-agent --action change-model --model gpt-5.4 --reason "mvp smoke" --approve --json >/dev/null

printf '%s\n' "[smoke] experiment readonly flow"
experiment_output="$(cargo run --quiet -- experiment plan --root "$work_dir/experiments" --goal "mvp smoke experiment" --success "plan can be shown" --json)"
experiment_id="$(printf '%s' "$experiment_output" | python3 -c 'import json,sys; print(json.load(sys.stdin)["experiment_id"])')"
cargo run --quiet -- experiment show --root "$work_dir/experiments" --experiment-id "$experiment_id" --json >/dev/null

printf 'mvp_smoke_ok work_dir=%s\n' "$work_dir"
