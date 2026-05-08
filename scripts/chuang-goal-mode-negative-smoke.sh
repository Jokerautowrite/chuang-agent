#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
smoke_name="${CHUANG_GOAL_MODE_NEGATIVE_SMOKE_NAME:-goal_mode_negative}"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${smoke_name}-smoke-$$"
goal_root="$work_dir/goal-runs"
queue_root="$work_dir/subagent-queue"
goal_id="${CHUANG_GOAL_MODE_NEGATIVE_SMOKE_GOAL_ID:-goal-mode-negative-smoke}"
checkpoint_id="${CHUANG_GOAL_MODE_NEGATIVE_SMOKE_CHECKPOINT_ID:-goal-mode-negative-smoke-checkpoint}"
goal_bin="${CHUANG_GOAL_MODE_NEGATIVE_SMOKE_BIN:-}"

mkdir -p "$work_dir"

goal_cmd() {
  if [ -n "$goal_bin" ]; then
    "$goal_bin" "$@"
  else
    cargo run --quiet -- "$@"
  fi
}

cd "$root_dir"

printf '%s\n' "[goal-mode-negative] plan"
goal_cmd goal plan \
  --root "$goal_root" \
  --goal-id "$goal_id" \
  --objective "goal-mode negative smoke validates not-ready from-collect rejection" \
  --scope "goal-main=src/goal_dispatch.rs" \
  --scope "goal-tests=tests/goal_dispatch_tests.rs" \
  --worker "goal-worker-1|goal-main|tighten dispatch bridge" \
  --worker "goal-worker-2|goal-tests|stabilize queue metadata" \
  --validation "cargo test -q --test cli_goal_tests" \
  --validation "cargo test -q --test goal_dispatch_tests" \
  --max-subtasks 2 \
  --json >/dev/null

printf '%s\n' "[goal-mode-negative] dispatch"
goal_cmd goal dispatch \
  --root "$goal_root" \
  --goal-id "$goal_id" \
  --subagent-queue-root "$queue_root" \
  --json >/dev/null

printf '%s\n' "[goal-mode-negative] partial-step"
step_output="$(
  goal_cmd goal step \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --subagent-queue-root "$queue_root" \
    --runner fake \
    --max-runs 1 \
    --max-concurrency 2 \
    --json
)"
printf '%s' "$step_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_id"] == "goal-mode-negative-smoke"
assert data["manifest"]["dispatch_count"] == 2
assert data["run_loop"]["ran_count"] == 1
assert data["collection"]["ready_to_checkpoint"] is False
assert len(data["collection"]["missing_run_ids"]) == 1
assert data["collection"].get("checkpoint_suggestion") is None
assert data["checkpoint_recorded"] is False
'

printf '%s\n' "[goal-mode-negative] collect-not-ready"
collect_output="$(
  goal_cmd goal collect \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --subagent-queue-root "$queue_root" \
    --json
)"
printf '%s' "$collect_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_id"] == "goal-mode-negative-smoke"
assert data["available_report_count"] == 1
assert len(data["missing_run_ids"]) == 1
assert data["blocked_report_run_ids"] == []
assert data["blocked_report_reasons"] == []
assert data["ready_to_checkpoint"] is False
assert data.get("checkpoint_suggestion") is None
missing_run_ids = " | ".join(data["missing_run_ids"])
print(
    f"goal_mode_negative_collect_ready_to_checkpoint=false "
    f"missing_run_ids={missing_run_ids}"
)
'

printf '%s\n' "[goal-mode-negative] checkpoint-from-collect-rejects"
set +e
checkpoint_output="$(
  goal_cmd goal checkpoint \
    --from-collect \
    --subagent-queue-root "$queue_root" \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --checkpoint-id "$checkpoint_id" \
    --json 2>&1
)"
checkpoint_status=$?
set -e
if [ "$checkpoint_status" -eq 0 ]; then
  printf '%s\n' "$checkpoint_output" >&2
  printf '%s\n' "[goal-mode-negative] expected checkpoint --from-collect to fail" >&2
  exit 1
fi
printf '%s' "$checkpoint_output" | python3 -c '
import sys

text = sys.stdin.read()
assert "goal_checkpoint_invalid: collect.ready_to_checkpoint" in text
assert "missing_run_ids=" in text
assert "report_run_ids=" in text
assert "blocked_report_run_ids=none" in text
assert "blocked_report_reasons=none" in text
print(text.strip())
'

printf '%s\n' "[goal-mode-negative] show-no-checkpoint"
show_output="$(
  goal_cmd goal show \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --json
)"
printf '%s' "$show_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_spec"]["goal_id"] == "goal-mode-negative-smoke"
assert len(data["checkpoint_log"]) == 0
assert data["goal_run_diagnostics"]["checkpoint_log_complete"] is False
'

printf 'goal_mode_negative_smoke_ok work_dir=%s goal_root=%s queue_root=%s checkpoint_id=%s\n' \
  "$work_dir" "$goal_root" "$queue_root" "$checkpoint_id"
