#!/usr/bin/env bash
# Supervise chuang app-server for systemd user unit.
# Keeps a named-pipe stdin open so the stdio server stays alive for channel attach.
set -euo pipefail

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
WORKSPACE="${CHUANG_AGENT_WORKSPACE_ROOT:-$ROOT}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
BIN="${CHUANG_BIN:-$ROOT/target/debug/chuang-agent}"
FIFO_DIR="${XDG_RUNTIME_DIR:-/tmp}/chuang-agent"
FIFO="$FIFO_DIR/app-server.stdin"

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

mkdir -p "$FIFO_DIR"
if [[ ! -p "$FIFO" ]]; then
  rm -f "$FIFO"
  mkfifo "$FIFO"
fi

# Health is best-effort; do not block service start if provider env is mid-edit.
"$BIN" app-server health --workspace-root "$WORKSPACE" --json >/dev/null 2>&1 || true

# O_RDWR open on FIFO never blocks for lack of a peer (POSIX).
# CLI requires subcommand as argv[1] (not global flags first).
exec 3<>"$FIFO"
exec "$BIN" app-server <&3
