#!/usr/bin/env bash
# 创 · 真派工压测（spawn_subagent 主链）
# 用法：
#   set -a; . ~/.config/chuang-agent/provider.env; set +a
#   bash scripts/chuang-spawn-stress.sh
# 可选：CHUANG_BIN / CHUANG_SPAWN_STRESS_ROOT / SKIP_CASE=1,2
set -euo pipefail

ROOT="${CHUANG_SPAWN_STRESS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$ROOT"

set -a
[[ -f "${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}" ]] \
  && . "${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
set +a

export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"
export CHUANG_REAL_ACTUATOR_ENABLE="${CHUANG_REAL_ACTUATOR_ENABLE:-1}"
export CHUANG_REAL_CONTROL_ENABLE="${CHUANG_REAL_CONTROL_ENABLE:-1}"
export CHUANG_AGENT_ROOT="${CHUANG_AGENT_ROOT:-$ROOT}"

if [[ -z "${CHUANG_PROXY_API_KEY:-}" ]]; then
  echo "chuang_spawn_stress_blocked: missing CHUANG_PROXY_API_KEY" >&2
  exit 2
fi

BIN="${CHUANG_BIN:-}"
if [[ -z "$BIN" ]]; then
  if [[ -x "$ROOT/target/debug/chuang-agent" ]]; then
    BIN="$ROOT/target/debug/chuang-agent"
  else
    BIN="cargo"
  fi
fi

run_chuang() {
  local input=$1
  local out=$2
  local err=$3
  if [[ "$BIN" == "cargo" ]]; then
    timeout 600 cargo run -q --manifest-path "$ROOT/Cargo.toml" -- run \
      --config "$ROOT/config.toml" --verbose --input "$input" >"$out" 2>"$err"
  else
    timeout 600 "$BIN" run --config "$ROOT/config.toml" --verbose --input "$input" >"$out" 2>"$err"
  fi
}

WORKDIR="${CHUANG_SPAWN_STRESS_WORKDIR:-$(mktemp -d /tmp/chuang-spawn-stress-XXXXXX)}"
mkdir -p "$WORKDIR"
echo "chuang_spawn_stress workdir=$WORKDIR root=$ROOT bin=$BIN"

PASS=0
FAIL=0
skip_has() {
  local n=$1
  [[ ",${SKIP_CASE:-}," == *",$n,"* ]]
}

score_case() {
  local id=$1
  local name=$2
  local out=$3
  local err=$4
  local need_parallel=${5:-0}
  local notes=()

  local ok=1
  if ! grep -q 'spawn_subagent' "$out" "$err" 2>/dev/null; then
    # verbose tool_calls_json should mention tool name
    if ! grep -qE '"tool"\s*:\s*"spawn_subagent"|SpawnSubagent|spawn_subagent' "$out" 2>/dev/null; then
      notes+=("no_spawn_subagent_in_output")
      ok=0
    fi
  fi
  if grep -qi 'subagent_runtime_unavailable' "$out" "$err" 2>/dev/null; then
    notes+=("subagent_runtime_unavailable")
    ok=0
  fi
  # tool_calls_json or events
  if grep -q 'tool_calls_json' "$out"; then
    if ! grep -q 'spawn_subagent' "$out"; then
      notes+=("tool_calls_json_missing_spawn")
      ok=0
    fi
  fi
  if [[ "$need_parallel" == "1" ]]; then
    # expect tasks array or batch/2 workers evidence
    if ! grep -qE 'tasks|max_concurrency|subagent_batch|workers=2|ran_count.:2|"workers":2' "$out" 2>/dev/null; then
      # queue may still have 2 dispatches in latest run dir
      notes+=("parallel_marker_weak")
    fi
  fi
  # must not hard-fail process for these cases
  if grep -qiE 'runtime_failed|BudgetExceeded' "$out" "$err" 2>/dev/null; then
    notes+=("runtime_failed_or_budget")
    ok=0
  fi

  if [[ "$ok" -eq 1 ]]; then
    PASS=$((PASS + 1))
    echo "PASS  $id $name${notes[*]:+  (${notes[*]})}"
  else
    FAIL=$((FAIL + 1))
    echo "FAIL  $id $name — ${notes[*]}"
    echo "  out: $out"
    echo "  err: $err"
  fi
}

# --- Case 1: single analyze ---
if ! skip_has 1; then
  echo "=== CASE 1 single analyze $(date +%H:%M:%S) ==="
  C1_OUT="$WORKDIR/case1.out"
  C1_ERR="$WORKDIR/case1.err"
  PROMPT1='强制使用工具 spawn_subagent 一次（policy=analyze，不要 file_read/code_execute）。任务：读取仓库根目录 Cargo.toml 的 package.name，把工人结果写进 FINAL，只保留包名。'
  if run_chuang "$PROMPT1" "$C1_OUT" "$C1_ERR"; then
    score_case 1 "single analyze Cargo.toml" "$C1_OUT" "$C1_ERR" 0
  else
    FAIL=$((FAIL + 1))
    echo "FAIL  1 single analyze — process_exit_nonzero"
  fi
  # body check soft
  if grep -q 'chuang-agent' "$C1_OUT" 2>/dev/null; then
    echo "  note: body has chuang-agent"
  fi
fi

# --- Case 2: parallel tasks[] ---
if ! skip_has 2; then
  echo "=== CASE 2 parallel tasks $(date +%H:%M:%S) ==="
  C2_OUT="$WORKDIR/case2.out"
  C2_ERR="$WORKDIR/case2.err"
  PROMPT2='强制调用一次 spawn_subagent，必须用 tasks 数组且 max_concurrency=2，policy=analyze。两个任务：
1) 只读 src/tool_runtime.rs 里函数名 build_subagent_tool_context 是否存在（是/否）
2) 只读 src/cli_runtime.rs 是否调用 build_subagent_tool_context（是/否）
禁止自己 file_read。FINAL 用两行：tool_runtime=是/否 与 cli_runtime=是/否。'
  if run_chuang "$PROMPT2" "$C2_OUT" "$C2_ERR"; then
    score_case 2 "parallel tasks max_concurrency=2" "$C2_OUT" "$C2_ERR" 1
  else
    FAIL=$((FAIL + 1))
    echo "FAIL  2 parallel — process_exit_nonzero"
  fi
  # queue evidence: newest run under data/subagent-queue with 2 reports
  QUEUE_ROOT="$ROOT/data/subagent-queue"
  if [[ -d "$QUEUE_ROOT" ]]; then
    newest=$(find "$QUEUE_ROOT" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' 2>/dev/null | sort -nr | head -1 | cut -d' ' -f2-)
    if [[ -n "$newest" ]]; then
      nrep=$(find "$newest/reports" -type f 2>/dev/null | wc -l | tr -d ' ')
      echo "  queue_newest=$newest reports=$nrep"
      if [[ "${nrep:-0}" -ge 2 ]]; then
        echo "  note: queue has >=2 reports (parallel evidence)"
      fi
    fi
  fi
fi

# --- Case 3: worker returns failure / empty should not crash parent ---
if ! skip_has 3; then
  echo "=== CASE 3 impossible path soft handling $(date +%H:%M:%S) ==="
  C3_OUT="$WORKDIR/case3.out"
  C3_ERR="$WORKDIR/case3.err"
  # Worker asked for a file that does not exist — runner should finish; parent should still FINAL
  PROMPT3='强制使用 spawn_subagent 一次（policy=analyze）。任务：读取不存在的文件 __no_such_file_chuang_stress__/missing.toml 并报告是否存在。不要自己直接读盘。FINAL 必须说明工人结论（存在/不存在/失败）。'
  if run_chuang "$PROMPT3" "$C3_OUT" "$C3_ERR"; then
    score_case 3 "missing file worker path" "$C3_OUT" "$C3_ERR" 0
  else
    FAIL=$((FAIL + 1))
    echo "FAIL  3 missing-file — process_exit_nonzero"
  fi
  if grep -qiE '不存在|missing|not found|失败|无法' "$C3_OUT" 2>/dev/null; then
    echo "  note: parent mentioned missing/failure"
  fi
fi

echo "=== 汇总 PASS=$PASS FAIL=$FAIL workdir=$WORKDIR ==="
if [[ "$FAIL" -gt 0 ]]; then
  echo "chuang_spawn_stress_failed"
  exit 1
fi
echo "chuang_spawn_stress_ok"
exit 0
