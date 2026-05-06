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
DEADLINE=$(( $(date +%s) + DURATION_SECONDS ))
ITERATION=0

mkdir -p "$LOG_DIR"

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
  ITERATION=$((ITERATION + 1))
  NOW="$(date -Is)"
  REMAINING=$(( DEADLINE - $(date +%s) ))

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

  timeout --kill-after=30s "$ITERATION_TIMEOUT_SECONDS" "$CODEX_BIN" exec \
    --cd "$ROOT" \
    --dangerously-bypass-approvals-and-sandbox \
    --skip-git-repo-check \
    -c 'model_reasoning_effort="high"' \
    --json \
    --output-last-message "$LAST_MESSAGE" \
    - <"$PROMPT_FILE" >>"$JSONL_LOG" 2>>"$PLAIN_LOG" || {
      status=$?
      echo "iteration $ITERATION codex exec exited with status $status" >>"$PLAIN_LOG"
    }

  {
    echo "===== iteration $ITERATION ended $(date -Is) exit ignored for continuation ====="
    if [ -s "$LAST_MESSAGE" ]; then
      echo
      echo "----- last message -----"
      tail -n 120 "$LAST_MESSAGE"
    fi
  } >>"$PLAIN_LOG"

  sleep 30
done

cat >>"$SUMMARY_FILE" <<EOF
- finished_at: $(date -Is)
- iterations: $ITERATION
- status: finished
- log_dir: $LOG_DIR
- jsonl_log: $JSONL_LOG
- plain_log: $PLAIN_LOG
- last_message: $LAST_MESSAGE
EOF

echo "Chuang overnight goal run finished: $LOG_DIR" >>"$PLAIN_LOG"
