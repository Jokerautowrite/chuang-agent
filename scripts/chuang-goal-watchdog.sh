#!/usr/bin/env bash
set -euo pipefail

ROOT="/home/user/projects/chuang-agent"
SESSION="${SESSION:-chuang-goal}"
INTERVAL_SECONDS="${INTERVAL_SECONDS:-1800}"
LOG_DIR="${LOG_DIR:-/home/user/.codex/chuang-goal-interactive}"
LOG_FILE="$LOG_DIR/watchdog.log"
PANE_FILE="$LOG_DIR/last-pane.txt"

mkdir -p "$LOG_DIR"

record_once() {
  local now
  now="$(date -Is)"
  {
    echo "===== watchdog $now ====="
    if tmux has-session -t "$SESSION" 2>/dev/null; then
      echo "tmux_session: present"
      tmux list-panes -t "$SESSION" -F 'pane=#{pane_id} active=#{pane_active} pid=#{pane_pid} current_command=#{pane_current_command}'
      tmux capture-pane -pt "$SESSION" -S -120 >"$PANE_FILE" || true
      local pane_bytes
      pane_bytes="$(wc -c <"$PANE_FILE" 2>/dev/null || echo 0)"
      echo "last_pane_file: $PANE_FILE bytes=$pane_bytes"
      tail -n 40 "$PANE_FILE" | sed 's/^/pane_tail: /'
    else
      echo "tmux_session: missing"
      echo "ALERT: goal tmux session is not running"
    fi
    echo "codex_processes:"
    ps -eo pid,ppid,etime,cmd | rg 'codex --no-alt-screen|codex/codex|/home/user/.local/bin/codex' | rg -v 'rg ' || true
    echo "git_status:"
    git -C "$ROOT" status --short | sed -n '1,120p'
    echo
  } >>"$LOG_FILE"
}

record_once

while true; do
  sleep "$INTERVAL_SECONDS"
  record_once
done
