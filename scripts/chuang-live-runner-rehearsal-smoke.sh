#!/usr/bin/env bash
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
smoke_name="${CHUANG_LIVE_RUNNER_REHEARSAL_SMOKE_NAME:-live_runner_rehearsal}"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${smoke_name}-smoke-$$"
config_path="$work_dir/config.toml"
queue_root="$work_dir/subagent-queue"
runner_workspace="$work_dir/runner-workspace"
preflight_json="$work_dir/live-preflight.json"
dispatch_json="$work_dir/dispatch.json"
list_before_json="$work_dir/list-before-runner.json"
run_once_json="$work_dir/run-once.json"
report_json="$work_dir/report.json"
collect_json="$work_dir/collect.json"

mkdir -p "$work_dir" "$runner_workspace"

# Keep this smoke local-only even if the operator shell has live gates set.
unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

cd "$root_dir"

cat > "$config_path" <<EOF
db_path = "$work_dir/chuang-agent.db"
identity_memory_root = "$work_dir/identity-memory"
provider = "fake"
provider_id = "fake-runtime"
model = "stub-responder"
subagent = "queued_external"
subagent_queue_root = "$queue_root"
EOF

printf '%s\n' "[smoke] live preflight stays read-only"
cargo run --quiet -- subagent live-preflight \
  --runner-command scripts/chuang-codex-runner.py \
  --allow-runner-command scripts/chuang-codex-runner.py \
  --requires-capability rehearsal \
  --capability rehearsal \
  --json > "$preflight_json"
python3 - "$preflight_json" "$queue_root" <<'PY'
import json
import pathlib
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)["rehearsal"]

queue_root = pathlib.Path(sys.argv[2])
assert data["ok"] is True
assert data["ready_for_live"] is False
assert data["readonly"] is True
assert data["starts_external_worker"] is False
assert data["gate_enabled"] is False
assert data["runner_allowlist_ok"] is True
assert data["runner_allowlist"]["runner"] == "command"
assert data["runner_allowlist"]["runner_command"] == "scripts/chuang-codex-runner.py"
assert data["runner_allowlist"]["matched_runner_command"] == "scripts/chuang-codex-runner.py"
assert data["capability_routing_ok"] is True
assert data["capability_routing"]["required_capabilities"] == ["rehearsal"]
assert data["capability_routing"]["worker_capabilities"] == ["rehearsal"]
assert data["capability_routing"]["matched_capabilities"] == ["rehearsal"]
assert data["capability_routing"]["missing_capabilities"] == []
assert data["report_admission_ok"] is True
assert "run-once" in data["report_admission"]["covered_commands"]
assert "report" in data["report_admission"]["covered_commands"]
assert "collect" in data["report_admission"]["covered_commands"]
assert "report_validated" in data["report_admission"]["stable_reason_codes"]
assert data["approval_audit_prerequisites"]["explicit_operator_approval_required"] is True
assert data["approval_audit_prerequisites"]["governance_approval_required"] is True
assert not queue_root.exists()
PY

printf '%s\n' "[smoke] dispatch local rehearsal task"
cargo run --quiet -- subagent dispatch \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --task "safe live runner rehearsal using disabled codex runner" \
  --requires-capability rehearsal \
  --json > "$dispatch_json"

cargo run --quiet -- subagent list \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --json > "$list_before_json"
python3 - "$list_before_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)

assert data["dispatch_count"] == 1
assert data["report_count"] == 0
item = data["items"][0]
assert item["required_capabilities"] == ["rehearsal"]
assert item["is_claimed"] is False
assert item["has_report"] is False
PY

printf '%s\n' "[smoke] run disabled codex command runner with explicit approval"
CHUANG_CODEX_RUNNER_WORKSPACE="$runner_workspace" \
cargo run --quiet -- subagent run-once \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --runner command \
  --runner-command scripts/chuang-codex-runner.py \
  --approve-exec \
  --capability rehearsal \
  --json > "$run_once_json"
python3 - "$run_once_json" "$dispatch_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    run_once = json.load(handle)
with open(sys.argv[2], encoding="utf-8") as handle:
    dispatch = json.load(handle)

assert run_once["runner"] == "command"
assert run_once["worker_capabilities"] == ["rehearsal"]
assert run_once["ran"] is True
assert run_once["run_id"] == dispatch["run_id"]
assert run_once["report_path"]
assert run_once["report_admission"]["status"] == "Accepted"
assert run_once["report_admission"]["reason_code"] == "report_validated"
assert "codex runner disabled" in run_once["summary"]
PY

run_id="$(python3 - "$dispatch_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["run_id"])
PY
)"

printf '%s\n' "[smoke] report admission remains visible"
cargo run --quiet -- subagent report \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --run-id "$run_id" \
  --json > "$report_json"

cargo run --quiet -- subagent collect \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --run-id "$run_id" \
  --json > "$collect_json"

python3 - "$report_json" "$collect_json" <<'PY'
import json
import sys

for path in sys.argv[1:]:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
    assert (data.get("report_available") or data.get("available")) is True
    assert data["report_admission"]["status"] == "Accepted"
    assert data["report_admission"]["reason_code"] == "report_validated"
    report = data["report"]
    assert report["schema_version"] == "1.0"
    assert report["status"] == "Failed"
    assert report["exit_code"] == 2
    assert "codex runner disabled" in report["summary"]
    assert report["stderr_preview"] == "codex runner disabled by default"
    assert report["governance_decision"]["decision"] == "needs_approval"
    assert report["governance_decision"]["reason"] == "approved_by_cli_flag: --approve-exec"
PY

printf 'live_runner_rehearsal_smoke_ok work_dir=%s queue_root=%s run_id=%s\n' "$work_dir" "$queue_root" "$run_id"
