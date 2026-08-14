#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" == "--profile" ]]; then
  shift 2
fi

if [[ "${1:-}" == "app-server" ]]; then
  shift
fi

exec cargo run --quiet --manifest-path "${ROOT}/Cargo.toml" -- app-server "$@"
