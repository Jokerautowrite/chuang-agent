#!/usr/bin/env bash
# Manage a dedicated headless Chrome for Chuang browser_read / browser_navigate.
# Default CDP port: 9222  (override with CHUANG_CDP_PORT)
set -euo pipefail

PORT="${CHUANG_CDP_PORT:-9222}"
STATE_DIR="${CHUANG_HEADLESS_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/chuang-agent/headless-chrome}"
USER_DATA_DIR="${CHUANG_HEADLESS_USER_DATA_DIR:-$STATE_DIR/user-data}"
PID_FILE="$STATE_DIR/chrome.pid"
LOG_FILE="$STATE_DIR/chrome.log"
PORT_FILE="$STATE_DIR/cdp.port"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-headless-chrome.sh <start|stop|restart|status|env>

Manage Chuang's managed headless Chrome (CDP remote debugging).

Commands:
  start     Start headless Chrome if not already running
  stop      Stop the managed instance
  restart   stop + start
  status    Show pid/port/reachable
  env       Print export lines for the current shell

Environment:
  CHUANG_CDP_PORT                 default 9222
  CHUANG_HEADLESS_STATE_DIR       state/pid/log dir
  CHUANG_HEADLESS_USER_DATA_DIR   Chrome user-data-dir
  CHUANG_HEADLESS_CHROME_BIN      chrome binary override
USAGE
}

find_chrome() {
  if [[ -n "${CHUANG_HEADLESS_CHROME_BIN:-}" && -x "$CHUANG_HEADLESS_CHROME_BIN" ]]; then
    echo "$CHUANG_HEADLESS_CHROME_BIN"
    return 0
  fi
  local cand
  for cand in \
    google-chrome-stable \
    google-chrome \
    chromium-browser \
    chromium \
    /usr/bin/google-chrome-stable \
    /usr/bin/google-chrome \
    /usr/bin/chromium-browser \
    /usr/bin/chromium; do
    if command -v "$cand" >/dev/null 2>&1; then
      command -v "$cand"
      return 0
    fi
    if [[ -x "$cand" ]]; then
      echo "$cand"
      return 0
    fi
  done
  return 1
}

port_open() {
  # Prefer bash /dev/tcp; fall back to curl.
  if (echo >/dev/tcp/127.0.0.1/"$PORT") >/dev/null 2>&1; then
    return 0
  fi
  if command -v curl >/dev/null 2>&1; then
    curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/json/version" >/dev/null 2>&1
    return $?
  fi
  return 1
}

is_running() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
  fi
  return 1
}

cmd_status() {
  local running=0 reachable=0 pid="-"
  if is_running; then
    running=1
    pid="$(cat "$PID_FILE")"
  fi
  if port_open; then
    reachable=1
  fi
  echo "state_dir=$STATE_DIR"
  echo "port=$PORT"
  echo "running=$running"
  echo "pid=$pid"
  echo "cdp_reachable=$reachable"
  if [[ "$reachable" -eq 1 ]]; then
    echo "cdp_url=http://127.0.0.1:${PORT}"
    echo "export_hint=export CHUANG_CDP_PORT=${PORT}"
  fi
  if [[ "$running" -eq 1 && "$reachable" -eq 1 ]]; then
    return 0
  fi
  if [[ "$reachable" -eq 1 ]]; then
    # Port open but not our pid file — still usable.
    return 0
  fi
  return 1
}

cmd_start() {
  mkdir -p "$STATE_DIR" "$USER_DATA_DIR"
  if port_open; then
    echo "headless chrome already reachable on port $PORT"
    echo "$PORT" >"$PORT_FILE"
    if is_running; then
      echo "pid=$(cat "$PID_FILE")"
    else
      echo "pid=foreign-or-unknown"
    fi
    echo "export CHUANG_CDP_PORT=$PORT"
    return 0
  fi
  if is_running; then
    # Stale process without open port.
    cmd_stop || true
  fi

  local chrome
  if ! chrome="$(find_chrome)"; then
    echo "error: no Chrome/Chromium binary found" >&2
    echo "install google-chrome or set CHUANG_HEADLESS_CHROME_BIN" >&2
    return 1
  fi

  # --headless=new is the modern headless mode; remote debugging for CDP.
  # --no-first-run / --disable-gpu keep cold starts quiet on servers/desktops.
  setsid "$chrome" \
    --headless=new \
    --disable-gpu \
    --remote-debugging-address=127.0.0.1 \
    --remote-debugging-port="$PORT" \
    --user-data-dir="$USER_DATA_DIR" \
    --no-first-run \
    --no-default-browser-check \
    --disable-background-networking \
    --disable-features=Translate,MediaRouter \
    --disable-sync \
    --metrics-recording-only \
    --mute-audio \
    --window-size=1280,800 \
    about:blank \
    >"$LOG_FILE" 2>&1 &
  local pid=$!
  echo "$pid" >"$PID_FILE"
  echo "$PORT" >"$PORT_FILE"

  # Wait for CDP
  local i
  for i in $(seq 1 40); do
    if port_open; then
      echo "started headless chrome pid=$pid port=$PORT"
      echo "log=$LOG_FILE"
      echo "export CHUANG_CDP_PORT=$PORT"
      return 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "error: chrome exited early; see $LOG_FILE" >&2
      tail -n 40 "$LOG_FILE" >&2 || true
      rm -f "$PID_FILE"
      return 1
    fi
    sleep 0.15
  done
  echo "error: chrome started but CDP port $PORT not ready; see $LOG_FILE" >&2
  tail -n 40 "$LOG_FILE" >&2 || true
  return 1
}

cmd_stop() {
  if is_running; then
    local pid
    pid="$(cat "$PID_FILE")"
    kill "$pid" 2>/dev/null || true
    local i
    for i in $(seq 1 30); do
      if ! kill -0 "$pid" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
    echo "stopped pid=$pid"
  else
    echo "not running (no live pid file)"
    rm -f "$PID_FILE"
  fi
  return 0
}

cmd_env() {
  echo "export CHUANG_CDP_PORT=$PORT"
  echo "export CHUANG_HEADLESS_STATE_DIR=$STATE_DIR"
}

main() {
  local cmd="${1:-}"
  case "$cmd" in
    start) cmd_start ;;
    stop) cmd_stop ;;
    restart) cmd_stop; cmd_start ;;
    status) cmd_status ;;
    env) cmd_env ;;
    -h|--help|help|"") usage; [[ -n "$cmd" ]] || exit 2 ;;
    *)
      echo "unknown command: $cmd" >&2
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
