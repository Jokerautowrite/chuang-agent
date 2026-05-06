#!/bin/sh
set -eu

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
ENV_FILE="${CHUANG_FEISHU_ENV_FILE:-$HOME/.codex-im/chuang-feishu-bridge.env}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
FEISHU_SDK_MODULES="${CHUANG_FEISHU_SDK_NODE_MODULES:-/home/user/.codex/codex-feishu-bridge/node_modules}"

if [ -f "$ENV_FILE" ]; then
  set -a
  . "$ENV_FILE"
  set +a
fi

if [ -f "$PROVIDER_ENV_FILE" ]; then
  set -a
  . "$PROVIDER_ENV_FILE"
  set +a
fi

cargo run --quiet --manifest-path "$ROOT/Cargo.toml" -- channel feishu-check \
  --env-file "$ENV_FILE" \
  --json >/dev/null

export NODE_PATH="$FEISHU_SDK_MODULES${NODE_PATH:+:$NODE_PATH}"
export CHUANG_AGENT_ROOT="$ROOT"
export CHUANG_FEISHU_ENV_FILE="$ENV_FILE"
export CHUANG_PROVIDER_ENV_FILE="$PROVIDER_ENV_FILE"

exec node "$ROOT/scripts/chuang-feishu-bridge.js"
