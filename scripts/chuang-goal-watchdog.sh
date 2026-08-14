#!/usr/bin/env bash
set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
SESSION="${SESSION:-chuang-goal}"
INTERVAL_SECONDS="${INTERVAL_SECONDS:-1800}"
LOG_DIR="${LOG_DIR:-$HOME/.codex/chuang-goal-interactive}"
LOG_FILE="$LOG_DIR/watchdog.log"
PANE_FILE="$LOG_DIR/last-pane.txt"
PANE_LIST_FILE="$LOG_DIR/latest-panes.txt"
PROCESS_FILE="$LOG_DIR/latest-codex-processes.txt"
GIT_STATUS_FILE="$LOG_DIR/latest-git-status.txt"
REPORT_FILE="${REPORT_FILE:-$LOG_DIR/latest-watchdog-report.json}"
ONCE="${WATCHDOG_ONCE:-0}"

if [[ "${1:-}" == "--once" ]]; then
  ONCE=1
fi

mkdir -p "$LOG_DIR"

line_count() {
  wc -l <"$1" 2>/dev/null | tr -d ' '
}

write_json_report() {
  local now="$1"
  local tmux_present="$2"
  local pane_bytes="$3"
  local codex_process_count="$4"
  local git_dirty="$5"
  local next_action="$6"

  if ! command -v jq >/dev/null 2>&1; then
    echo "json_report_error: jq_missing report_file=$REPORT_FILE"
    return 0
  fi

  jq -n \
    --argjson schema_version 1 \
    --arg generated_at "$now" \
    --arg project_root "$ROOT" \
    --arg session "$SESSION" \
    --arg log_file "$LOG_FILE" \
    --arg pane_file "$PANE_FILE" \
    --arg pane_list_file "$PANE_LIST_FILE" \
    --arg process_file "$PROCESS_FILE" \
    --arg git_status_file "$GIT_STATUS_FILE" \
    --arg report_file "$REPORT_FILE" \
    --arg next_action "$next_action" \
    --argjson tmux_session_present "$tmux_present" \
    --argjson pane_bytes "$pane_bytes" \
    --argjson codex_process_count "$codex_process_count" \
    --argjson git_dirty "$git_dirty" \
    --rawfile pane_list "$PANE_LIST_FILE" \
    --rawfile codex_processes "$PROCESS_FILE" \
    --rawfile git_status_short "$GIT_STATUS_FILE" \
    '
    def non_empty_lines($value):
      $value | split("\n") | map(select(length > 0));

    {
      schema_version: $schema_version,
      generated_at: $generated_at,
      readonly: true,
      project_root: $project_root,
      session: $session,
      tmux_session_present: $tmux_session_present,
      pane: {
        file: $pane_file,
        list_file: $pane_list_file,
        bytes: $pane_bytes,
        panes: non_empty_lines($pane_list)
      },
      codex_processes: {
        file: $process_file,
        count: $codex_process_count,
        processes: non_empty_lines($codex_processes)
      },
      git: {
        status_file: $git_status_file,
        dirty: $git_dirty,
        status_short: non_empty_lines($git_status_short)
      },
      takeover: {
        next_action: $next_action,
        attach_command: ("tmux attach -t " + $session),
        review_command: ("git -C " + $project_root + " status --short")
      },
      boundaries: {
        dispatches_tasks: false,
        modifies_repo: false,
        restarts_worker: false,
        touches_services: false
      },
      log_file: $log_file,
      report_file: $report_file
    }
    ' >"$REPORT_FILE"
  echo "json_report_file: $REPORT_FILE"
}

record_once() {
  local now
  now="$(date -Is)"
  {
    echo "===== watchdog $now ====="
    : >"$PANE_LIST_FILE"
    : >"$PROCESS_FILE"
    : >"$GIT_STATUS_FILE"

    local tmux_present=false
    local pane_bytes=0
    if tmux has-session -t "$SESSION" 2>/dev/null; then
      tmux_present=true
      echo "tmux_session: present"
      tmux list-panes -t "$SESSION" -F 'pane=#{pane_id} active=#{pane_active} pid=#{pane_pid} current_command=#{pane_current_command}' | tee "$PANE_LIST_FILE"
      tmux capture-pane -pt "$SESSION" -S -120 >"$PANE_FILE" || true
      pane_bytes="$(wc -c <"$PANE_FILE" 2>/dev/null || echo 0)"
      echo "last_pane_file: $PANE_FILE bytes=$pane_bytes"
      tail -n 40 "$PANE_FILE" | sed 's/^/pane_tail: /'
    else
      echo "tmux_session: missing"
      echo "ALERT: goal tmux session is not running"
    fi
    echo "codex_processes:"
    ps -eo pid,ppid,etime,cmd | rg 'codex --no-alt-screen|codex/codex|/.local/bin/codex' | rg -v 'rg ' >"$PROCESS_FILE" || true
    cat "$PROCESS_FILE"
    echo "git_status:"
    git -C "$ROOT" status --short | sed -n '1,120p' >"$GIT_STATUS_FILE"
    cat "$GIT_STATUS_FILE"

    local codex_process_count
    local git_dirty=false
    local next_action
    codex_process_count="$(line_count "$PROCESS_FILE")"
    if [[ -s "$GIT_STATUS_FILE" ]]; then
      git_dirty=true
    fi
    if [[ "$tmux_present" != "true" ]]; then
      next_action="start_or_attach_worker_after_operator_review"
    elif [[ "$git_dirty" == "true" ]]; then
      next_action="review_git_status_and_diff"
    elif [[ "$codex_process_count" == "0" ]]; then
      next_action="inspect_tmux_pane_for_worker_state"
    else
      next_action="monitor_or_attach_if_human_review_needed"
    fi
    echo "takeover_next_action: $next_action"
    write_json_report "$now" "$tmux_present" "$pane_bytes" "$codex_process_count" "$git_dirty" "$next_action"
    echo
  } >>"$LOG_FILE"
}

record_once

if [[ "$ONCE" == "1" ]]; then
  exit 0
fi

while true; do
  sleep "$INTERVAL_SECONDS"
  record_once
done
