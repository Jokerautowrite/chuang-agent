#!/bin/sh
set -eu

ROOT="${CHUANG_AGENT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WORKSPACE_ROOT="${CHUANG_AGENT_WORKSPACE_ROOT:-$ROOT}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"

if [ -f "$PROVIDER_ENV_FILE" ]; then
  set -a
  . "$PROVIDER_ENV_FILE"
  set +a
fi

exec cargo run --quiet --manifest-path "$ROOT/Cargo.toml" -- app-server health \
  --workspace-root "$WORKSPACE_ROOT" \
  --json
