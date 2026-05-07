#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

cd "$root_dir"

status_short="$(git status --short)"
if [ -n "$status_short" ]; then
    printf '%s\n' "[third-test] error: working tree must be clean before third test smoke" >&2
    printf '%s\n' "$status_short" >&2
    exit 2
fi

printf '%s\n' "[third-test] final verify"
sh scripts/chuang-final-verify.sh

printf '%s\n' "[third-test] live readonly preflight"
sh scripts/chuang-live-readonly-preflight.sh

printf '%s\n' "[third-test] live operator checklist readonly summary"
operator_status=0
operator_json="$(bash scripts/chuang-live-operator-checklist.sh --json)" || operator_status=$?
if [ "$operator_status" -ne 0 ] && [ "$operator_status" -ne 1 ]; then
    printf '%s\n' "[third-test] error: live operator checklist failed unexpectedly with status $operator_status" >&2
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
assert boundaries["starts_services"] is False
assert boundaries["modifies_repo"] is False
assert boundaries["prints_secret_values"] is False
print("live_operator_checklist_status=" + str(data["status"]))
print("live_operator_checklist_ok=" + str(data["ok"]).lower())
print("live_operator_checklist_blockers=" + str(len(data.get("blockers", []))))
'

printf '%s\n' "[third-test] goal run status readonly summary"
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
print("goal_run_status_overall=" + str(data["overall_status"]))
print("goal_run_status_ok=" + str(data["ok"]).lower())
'

printf '%s\n' "third_test_candidate_smoke_ok"
