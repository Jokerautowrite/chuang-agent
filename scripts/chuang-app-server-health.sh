#!/bin/sh
set -eu

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
WORKSPACE_ROOT="${CHUANG_AGENT_WORKSPACE_ROOT:-$ROOT}"

exec cargo run --quiet --manifest-path "$ROOT/Cargo.toml" -- app-server health \
  --workspace-root "$WORKSPACE_ROOT" \
  --json
