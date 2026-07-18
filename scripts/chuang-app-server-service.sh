#!/usr/bin/env bash
# Supervise the canonical Chuang app-server Unix socket for the user service.
set -euo pipefail

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
WORKSPACE="${CHUANG_AGENT_WORKSPACE_ROOT:-$ROOT}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
BIN="${CHUANG_BIN:-$ROOT/target/debug/chuang-agent}"
SOCKET_DIR="${XDG_RUNTIME_DIR:-/tmp}/chuang-agent"
SOCKET="${CHUANG_APP_SERVER_SOCKET:-$SOCKET_DIR/app-server.sock}"

if [[ -f "$PROVIDER_ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$PROVIDER_ENV_FILE"
  set +a
fi

export CHUANG_REAL_CONTROL_ENABLE="${CHUANG_REAL_CONTROL_ENABLE:-1}"
export CHUANG_REAL_CONTROL_STATUS_ENABLE="${CHUANG_REAL_CONTROL_STATUS_ENABLE:-1}"
export CHUANG_REAL_ACTUATOR_ENABLE="${CHUANG_REAL_ACTUATOR_ENABLE:-1}"
export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"

if [[ ! -x "$BIN" ]]; then
  cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
  BIN="$ROOT/target/debug/chuang-agent"
fi

mkdir -p "$SOCKET_DIR"

# Health is best-effort; do not block service start if provider env is mid-edit.
"$BIN" app-server health --workspace-root "$WORKSPACE" --json >/dev/null 2>&1 || true

exec "$BIN" app-server daemon --socket "$SOCKET"
