#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

cd "$root_dir"

status_short="$(git status --short)"
if [ -n "$status_short" ]; then
    printf '%s\n' "[final-verify] error: working tree must be clean before final verify" >&2
    printf '%s\n' "$status_short" >&2
    exit 2
fi

printf '%s\n' "[final-verify] complete local smoke"
sh "$root_dir/scripts/chuang-complete-local-smoke.sh"

printf '%s\n' "[final-verify] final diff check"
git diff --check

printf '%s\n' "chuang_final_verify_ok"
