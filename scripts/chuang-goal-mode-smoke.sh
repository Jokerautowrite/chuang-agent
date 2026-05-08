#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
smoke_name="${CHUANG_GOAL_MODE_SMOKE_NAME:-goal_mode}"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${smoke_name}-smoke-$$"
goal_root="$work_dir/goal-runs"
queue_root="$work_dir/subagent-queue"
goal_id="${CHUANG_GOAL_MODE_SMOKE_GOAL_ID:-goal-mode-smoke}"
checkpoint_id="${CHUANG_GOAL_MODE_SMOKE_CHECKPOINT_ID:-goal-mode-smoke-checkpoint}"
goal_bin="${CHUANG_GOAL_MODE_SMOKE_BIN:-}"

mkdir -p "$work_dir"

goal_cmd() {
  if [ -n "$goal_bin" ]; then
    "$goal_bin" "$@"
  else
    cargo run --quiet -- "$@"
  fi
}

cd "$root_dir"

printf '%s\n' "[goal-mode] plan"
plan_output="$(
  goal_cmd goal plan \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --objective "goal-mode smoke validates plan dispatch step collect checkpoint from collect" \
    --scope "goal-main=src/goal_dispatch.rs" \
    --scope "goal-tests=tests/goal_dispatch_tests.rs" \
    --worker "goal-worker-1|goal-main|tighten dispatch bridge" \
    --worker "goal-worker-2|goal-tests|stabilize queue metadata" \
    --validation "cargo test -q --test cli_goal_tests" \
    --validation "cargo test -q --test goal_dispatch_tests" \
    --max-subtasks 2 \
    --json
)"
printf '%s' "$plan_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_id"] == "goal-mode-smoke"
assert data["checkpoint_count"] == 0
assert data["path"].endswith("/goal-mode-smoke.json")
'

printf '%s\n' "[goal-mode] dispatch"
dispatch_output="$(
  goal_cmd goal dispatch \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --subagent-queue-root "$queue_root" \
    --json
)"
printf '%s' "$dispatch_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_id"] == "goal-mode-smoke"
assert data["dispatch_count"] == 2
assert data["dispatch_diagnostics"]["ready_to_dispatch"] is True
assert len(data["dispatches"]) == 2
assert data["dispatch_manifest_path"].endswith(".dispatch.json")
'

printf '%s\n' "[goal-mode] step"
step_output="$(
  goal_cmd goal step \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --subagent-queue-root "$queue_root" \
    --runner fake \
    --max-runs 2 \
    --max-concurrency 2 \
    --json
)"
printf '%s' "$step_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_id"] == "goal-mode-smoke"
assert data["manifest"]["dispatch_count"] == 2
assert data["run_loop"]["ran_count"] == 2
assert data["collection"]["ready_to_checkpoint"] is True
assert data["checkpoint_recorded"] is False
assert data["writes_progress_log"] is False
assert data["writes_handoff"] is False
'

printf '%s\n' "[goal-mode] collect"
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
assert data["goal_id"] == "goal-mode-smoke"
assert data["available_report_count"] == 2
assert data["missing_run_ids"] == []
assert data["ready_to_checkpoint"] is True
assert data["checkpoint_suggestion"]["summary"] == "checkpoint ready for goal_id=goal-mode-smoke workers=goal-worker-1 | goal-worker-2"
assert data["checkpoint_suggestion"]["completed_worker_ids"] == ["goal-worker-1", "goal-worker-2"]
'

printf '%s\n' "[goal-mode] checkpoint-from-collect"
checkpoint_output="$(
  goal_cmd goal checkpoint \
    --from-collect \
    --subagent-queue-root "$queue_root" \
    --root "$goal_root" \
    --goal-id "$goal_id" \
    --checkpoint-id "$checkpoint_id" \
    --json
)"
printf '%s' "$checkpoint_output" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
assert data["goal_id"] == "goal-mode-smoke"
assert data["checkpoint_count"] == 1
assert data["last_checkpoint_id"] == "goal-mode-smoke-checkpoint"
assert data["last_checkpoint_summary"] == "checkpoint ready for goal_id=goal-mode-smoke workers=goal-worker-1 | goal-worker-2"
assert data["checkpoint_writeback"]["manual_only"] is True
assert data["checkpoint_writeback"]["update_progress_log"] is True
assert data["checkpoint_writeback"]["update_handoff"] is True
assert data["checkpoint_writeback"]["commit_checkpoint"] is True
assert data["checkpoint_writeback"]["documentation_targets"] == ["docs/progress-log.md", "docs/handoff-current.md"]
'

printf '%s\n' "[goal-mode] show"
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
assert data["goal_spec"]["goal_id"] == "goal-mode-smoke"
assert len(data["checkpoint_log"]) == 1
assert data["checkpoint_log"][0]["checkpoint_id"] == "goal-mode-smoke-checkpoint"
assert data["goal_run_diagnostics"]["last_checkpoint_id"] == "goal-mode-smoke-checkpoint"
assert data["goal_run_diagnostics"]["checkpoint_log_complete"] is True
'

printf 'goal_mode_smoke_ok work_dir=%s goal_root=%s queue_root=%s checkpoint_id=%s\n' \
  "$work_dir" "$goal_root" "$queue_root" "$checkpoint_id"
