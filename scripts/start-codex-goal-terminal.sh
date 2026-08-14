#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SESSION="${SESSION:-chuang-goal}"
LOG_DIR="${LOG_DIR:-$HOME/.codex/chuang-goal-interactive}"
GOAL_FILE="$LOG_DIR/goal.txt"

mkdir -p "$LOG_DIR"

cat >"$GOAL_FILE" <<'EOF'
/goal 连续推进 Chuang 项目第二测试版本，优先 readiness、smoke、goal/run、subagent protocol、workspace adapter、tool/runtime contract。不要停在分析；能实现就实现。每完成一段必须运行合适验证，并更新 docs/handoff-current.md 或 docs/progress-log.md。禁止删除文件，禁止 rm/cleanup/reset/purge，禁止泄露密钥，禁止触碰 Hermes 或 Codex 飞书桥，除非只是只读检查。工作树里可能已有其他改动；必须兼容，不要回滚。
EOF

cd "$ROOT"

if tmux has-session -t "$SESSION" 2>/dev/null; then
  exec tmux attach -t "$SESSION"
fi

tmux new-session -d -s "$SESSION" -c "$ROOT" \
  "bash -lc 'exec ${CODEX_BIN:-$HOME/.local/bin/codex} --no-alt-screen'"

sleep 8
tmux send-keys -t "$SESSION" "$(cat "$GOAL_FILE")" C-m

exec tmux attach -t "$SESSION"
