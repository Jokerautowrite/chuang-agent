#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
work_dir="${TMPDIR:-/tmp}/chuang-mainchain-acceptance-$$"
mkdir -p "$work_dir"
cd "$root_dir"

run_step() {
  name="$1"
  shift
  log_file="$work_dir/$name.log"
  printf '[mainchain] %-36s' "$name"
  if "$@" > "$log_file" 2>&1; then
    printf ' OK\n'
  else
    printf ' FAIL\n'
    printf '%s\n' "log=$log_file" >&2
    tail -n 80 "$log_file" >&2 || true
    exit 1
  fi
}

run_step "20-task-matrix" cargo test -q run_with_options_covers_mainchain_terminal_task_matrix
run_step "tool-runtime-contracts" cargo test -q --test tool_runtime_tests
run_step "cli-smoke" cargo test -q --test cli_smoke_tests
run_step "real-terminal-provider" sh "$root_dir/scripts/chuang-terminal-acceptance.sh"
run_step "real-natural-language" sh "$root_dir/scripts/chuang-real-natural-acceptance.sh"

printf '%s\n' "work_dir=$work_dir"
printf '%s\n' "chuang_mainchain_acceptance_ok"
