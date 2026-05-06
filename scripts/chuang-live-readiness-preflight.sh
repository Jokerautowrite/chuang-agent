#!/usr/bin/env bash
set -eu

ROOT="/home/user/projects/chuang-agent"

cd "$ROOT"

echo "[live] provider fallback smoke"
sh scripts/chuang-provider-fallback-smoke.sh

echo "[live] feishu live preflight smoke"
node scripts/chuang-feishu-live-preflight-smoke.js

echo "[live] subagent live preflight"
cargo run --quiet -- subagent live-preflight \
  --runner-command scripts/chuang-codex-runner.py \
  --allow-runner-command scripts/chuang-codex-runner.py \
  --requires-capability rust \
  --capability rust \
  --json >/tmp/chuang-live-readiness-subagent.json

echo "[live] watchdog readonly once"
./scripts/chuang-goal-watchdog.sh --once >/tmp/chuang-live-readiness-watchdog.log

echo "[live] console snapshot"
cargo run --quiet -- console snapshot --json >/tmp/chuang-live-readiness-console.json

echo "[live] complete local smoke"
sh scripts/chuang-complete-local-smoke.sh

echo "live_readiness_preflight_ok"
