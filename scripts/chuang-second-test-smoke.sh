#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

CHUANG_SMOKE_NAME=second_test sh "$root_dir/scripts/chuang-mvp-smoke.sh"
