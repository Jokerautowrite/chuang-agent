#!/usr/bin/env bash
set -eu

ROOT="/home/user/projects/chuang-agent"

exec "$ROOT/scripts/chuang-live-readonly-preflight.sh" "$@"
