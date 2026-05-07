#!/usr/bin/env bash
set -euo pipefail

ROOT="/home/user/projects/chuang-agent"
CODEX_BIN="${CODEX_BIN:-/home/user/.local/bin/codex}"
DURATION_SECONDS="${DURATION_SECONDS:-21600}"
ITERATION_TIMEOUT_SECONDS="${ITERATION_TIMEOUT_SECONDS:-2100}"
RUN_ROOT="${RUN_ROOT:-/home/user/.codex/chuang-goal-runs}"
RUN_ID="${RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
LOG_DIR="$RUN_ROOT/$RUN_ID"
LAST_MESSAGE="$LOG_DIR/last-message.md"
PROMPT_FILE="$LOG_DIR/prompt.md"
SUMMARY_FILE="$LOG_DIR/summary.md"
JSONL_LOG="$LOG_DIR/events.jsonl"
PLAIN_LOG="$LOG_DIR/run.log"
STATUS_FILE="${STATUS_FILE:-$LOG_DIR/status.json}"
SLEEP_SECONDS="${SLEEP_SECONDS:-30}"
CHUANG_OVERNIGHT_DRY_RUN="${CHUANG_OVERNIGHT_DRY_RUN:-0}"
CHUANG_OVERNIGHT_MAX_ITERATIONS="${CHUANG_OVERNIGHT_MAX_ITERATIONS:-}"
DEADLINE=$(( $(date +%s) + DURATION_SECONDS ))
ITERATION=0
LAST_ITERATION_EXIT_STATUS=""

mkdir -p "$LOG_DIR"

write_status() {
  local run_status="$1"
  local next_action="$2"
  STATUS_FILE="$STATUS_FILE" \
  RUN_ID="$RUN_ID" \
  ITERATION="$ITERATION" \
  DEADLINE="$DEADLINE" \
  LAST_ITERATION_EXIT_STATUS="$LAST_ITERATION_EXIT_STATUS" \
  LAST_MESSAGE="$LAST_MESSAGE" \
  JSONL_LOG="$JSONL_LOG" \
  PLAIN_LOG="$PLAIN_LOG" \
  CHUANG_OVERNIGHT_DRY_RUN="$CHUANG_OVERNIGHT_DRY_RUN" \
  RUN_STATUS="$run_status" \
  NEXT_ACTION="$next_action" \
  python3 - <<'PY'
import datetime
import json
import os

last_status_raw = os.environ.get("LAST_ITERATION_EXIT_STATUS", "")
last_status = None
if last_status_raw:
    last_status = int(last_status_raw)

deadline_epoch = int(os.environ["DEADLINE"])
deadline_iso = datetime.datetime.fromtimestamp(
    deadline_epoch, datetime.timezone.utc
).astimezone().isoformat()

data = {
    "schema_version": 1,
    "generated_at": datetime.datetime.now(datetime.timezone.utc).astimezone().isoformat(),
    "run_id": os.environ["RUN_ID"],
    "iteration": int(os.environ["ITERATION"]),
    "deadline": deadline_epoch,
    "deadline_iso": deadline_iso,
    "last_iteration_exit_status": last_status,
    "last_message_file": os.environ["LAST_MESSAGE"],
    "jsonl_log": os.environ["JSONL_LOG"],
    "plain_log": os.environ["PLAIN_LOG"],
    "status": os.environ["RUN_STATUS"],
    "next_action": os.environ["NEXT_ACTION"],
    "dry_run": os.environ.get("CHUANG_OVERNIGHT_DRY_RUN") == "1",
    "boundaries": {
        "restarts_codex": False,
        "cleans_logs": False,
        "touches_services": False,
    },
}

with open(os.environ["STATUS_FILE"], "w", encoding="utf-8") as handle:
    json.dump(data, handle, ensure_ascii=False, indent=2)
    handle.write("\n")
PY
}

write_status "running" "start_first_iteration"

cat >"$SUMMARY_FILE" <<EOF
# Chuang Overnight Goal Run

- run_id: $RUN_ID
- root: $ROOT
- started_at: $(date -Is)
- duration_seconds: $DURATION_SECONDS
- deadline_epoch: $DEADLINE
- status: running

EOF

cd "$ROOT"

while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  if [ -n "$CHUANG_OVERNIGHT_MAX_ITERATIONS" ] && [ "$ITERATION" -ge "$CHUANG_OVERNIGHT_MAX_ITERATIONS" ]; then
    write_status "running" "max_iterations_reached_finish"
    break
  fi

  ITERATION=$((ITERATION + 1))
  NOW="$(date -Is)"
  REMAINING=$(( DEADLINE - $(date +%s) ))
  write_status "running" "prepare_iteration_prompt"

  cat >"$PROMPT_FILE" <<EOF
你是一个独立终端里的 Codex 长跑执行器，目标是在当前仓库连续推进 Chuang 项目到“第二测试版本可验收、可回归、可持续续接”。

当前运行信息：
- iteration: $ITERATION
- started_at: $NOW
- remaining_seconds_before_deadline: $REMAINING
- workspace: $ROOT

主目标：
1. 持续推进 Chuang 项目，不要停在分析。
2. 优先修复或补齐最影响交付的缺口：readiness、smoke、goal/run、subagent protocol、workspace adapter、tool/runtime contract、handoff/checkpoint。
3. 每轮都必须先读当前状态，再做小而实的实现/测试/文档同步。
4. 每轮结束前必须更新 docs/handoff-current.md 或 docs/progress-log.md 中至少一个，用于下一轮续接。
5. 尽量运行验证命令，优先 cargo test -q 和 sh scripts/chuang-mvp-smoke.sh；如果太慢或失败，记录原因和下一步。

边界：
- 不要删除文件，不要 rm，不要 cleanup，不要 reset，不要 purge。
- 不要泄露密钥；日志和报告里变量值必须打码。
- 不要触碰 Hermes 服务或 Codex 飞书桥，除非 Chuang 项目自身的测试明确需要且只读。
- 不要改无关项目。
- 工作树可能已经有用户或其他代理改动，必须先识别并与之兼容，不要回滚。
- 每轮尽量把改动控制在可解释的小批次。

建议流程：
1. 读 docs/handoff-current.md、docs/progress-log.md、git status --short。
2. 找一个最影响交付的缺口。
3. 实现。
4. 测试。
5. 更新 handoff/progress。
6. 最终回答写清楚本轮完成、验证结果、下一轮入口。

如果你认为已完成一个目标，不要停止整个长跑；继续寻找下一个 readiness/smoke/goal/subagent 缺口推进，直到外层脚本时间结束。
EOF

  {
    echo
    echo "===== iteration $ITERATION started $NOW remaining=${REMAINING}s ====="
  } >>"$PLAIN_LOG"

  if [ "$CHUANG_OVERNIGHT_DRY_RUN" = "1" ]; then
    LAST_ITERATION_EXIT_STATUS=0
    echo "iteration $ITERATION dry-run skipped codex exec" >>"$PLAIN_LOG"
    write_status "running" "dry_run_skip_codex_exec"
  else
    write_status "running" "invoke_codex_exec"
    if timeout --kill-after=30s "$ITERATION_TIMEOUT_SECONDS" "$CODEX_BIN" exec \
      --cd "$ROOT" \
      --dangerously-bypass-approvals-and-sandbox \
      --skip-git-repo-check \
      -c 'model_reasoning_effort="high"' \
      --json \
      --output-last-message "$LAST_MESSAGE" \
      - <"$PROMPT_FILE" >>"$JSONL_LOG" 2>>"$PLAIN_LOG"; then
      LAST_ITERATION_EXIT_STATUS=0
    else
      LAST_ITERATION_EXIT_STATUS=$?
      echo "iteration $ITERATION codex exec exited with status $LAST_ITERATION_EXIT_STATUS" >>"$PLAIN_LOG"
    fi
  fi

  {
    echo "===== iteration $ITERATION ended $(date -Is) exit ignored for continuation ====="
    if [ -s "$LAST_MESSAGE" ]; then
      echo
      echo "----- last message -----"
      tail -n 120 "$LAST_MESSAGE"
    fi
  } >>"$PLAIN_LOG"

  write_status "running" "sleep_before_next_iteration"
  sleep "$SLEEP_SECONDS"
done

write_status "finished" "operator_review_status_and_logs"

cat >>"$SUMMARY_FILE" <<EOF
- finished_at: $(date -Is)
- iterations: $ITERATION
- status: finished
- log_dir: $LOG_DIR
- jsonl_log: $JSONL_LOG
- plain_log: $PLAIN_LOG
- last_message: $LAST_MESSAGE
- status_file: $STATUS_FILE
EOF

echo "Chuang overnight goal run finished: $LOG_DIR" >>"$PLAIN_LOG"
