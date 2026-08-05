#!/bin/sh
set -eu

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
ENV_FILE="${CHUANG_FEISHU_ENV_FILE:-$HOME/.codex-im/chuang-feishu-bridge.env}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
FEISHU_SDK_MODULES="${CHUANG_FEISHU_SDK_NODE_MODULES:-/home/user/.codex/codex-feishu-bridge-current/node_modules}"

detect_desktop_env() {
  uid="$(id -u)"
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$uid}"
  if [ -z "${XDG_RUNTIME_DIR:-}" ] && [ -d "$runtime_dir" ]; then
    export XDG_RUNTIME_DIR="$runtime_dir"
  fi

  if [ -z "${XAUTHORITY:-}" ]; then
    for candidate in \
      "${XDG_RUNTIME_DIR:-}/.Xauthority" \
      "/run/user/$uid/.Xauthority" \
      "$HOME/.Xauthority"
    do
      if [ -n "$candidate" ] && [ -r "$candidate" ]; then
        export XAUTHORITY="$candidate"
        break
      fi
    done
  fi

  if [ -z "${DISPLAY:-}" ] && [ -d /tmp/.X11-unix ]; then
    for socket in /tmp/.X11-unix/X*; do
      if [ -S "$socket" ]; then
        export DISPLAY=":${socket##*/X}"
        break
      fi
    done
  fi

  if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -d "${XDG_RUNTIME_DIR:-}" ]; then
    for socket in "$XDG_RUNTIME_DIR"/wayland-*; do
      if [ -S "$socket" ]; then
        export WAYLAND_DISPLAY="${socket##*/}"
        break
      fi
    done
  fi
}

if [ -f "$ENV_FILE" ]; then
  set -a
  . "$ENV_FILE"
  set +a
fi

ROOT="${CHUANG_AGENT_ROOT:-$ROOT}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$PROVIDER_ENV_FILE}"
FEISHU_SDK_MODULES="${CHUANG_FEISHU_SDK_NODE_MODULES:-$FEISHU_SDK_MODULES}"
BIN="${CHUANG_BIN:-$ROOT/target/debug/chuang-agent}"

if [ -f "$PROVIDER_ENV_FILE" ]; then
  set -a
  . "$PROVIDER_ENV_FILE"
  set +a
fi

detect_desktop_env

if [ ! -x "$BIN" ]; then
  printf '%s\n' "chuang-agent binary is missing or not executable: $BIN" >&2
  exit 1
fi

"$BIN" channel feishu-check \
  --env-file "$ENV_FILE" \
  --json >/dev/null

export NODE_PATH="$FEISHU_SDK_MODULES${NODE_PATH:+:$NODE_PATH}"
export CHUANG_AGENT_ROOT="$ROOT"
export CHUANG_AGENT_WORKSPACE_ROOT="${CHUANG_AGENT_WORKSPACE_ROOT:-$ROOT}"
export CHUANG_FEISHU_ENV_FILE="$ENV_FILE"
export CHUANG_PROVIDER_ENV_FILE="$PROVIDER_ENV_FILE"
export CHUANG_REAL_ACTUATOR_ENABLE="${CHUANG_REAL_ACTUATOR_ENABLE:-1}"
export CHUANG_REAL_CONTROL_ENABLE="${CHUANG_REAL_CONTROL_ENABLE:-1}"
export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"

exec node "$ROOT/scripts/chuang-feishu-bridge.js"
