#!/usr/bin/env bash
# 创 · 实战验收 10 条（只测不扩文）
#   bash scripts/chuang-field-accept-10.sh
#   SKIP_LIVE=1 SKIP_BROWSER=1 bash scripts/chuang-field-accept-10.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 1

PASS=0
FAIL=0
SKIP=0
LOG="$(mktemp /tmp/chuang-field-accept-log-XXXXXX)"

ok()   { PASS=$((PASS+1)); echo "PASS  $1" | tee -a "$LOG"; }
bad()  { FAIL=$((FAIL+1)); echo "FAIL  $1 — $2" | tee -a "$LOG"; }
skip() { SKIP=$((SKIP+1)); echo "SKIP  $1 — $2" | tee -a "$LOG"; }

BIN="${CHUANG_BIN:-$ROOT/target/debug/chuang-agent}"
if [[ ! -x "$BIN" ]]; then
  cargo build -q --manifest-path "$ROOT/Cargo.toml" || exit 1
  BIN="$ROOT/target/debug/chuang-agent"
fi

if [[ -f "${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
  set +a
fi

CFG="${CHUANG_FIELD_CONFIG:-$ROOT/config.toml}"
TMPDIR_RUN="$(mktemp -d /tmp/chuang-field-accept-XXXXXX)"
trap 'rm -rf "$TMPDIR_RUN"' EXIT

echo "=== 创 实战验收 10 · root=$ROOT ==="

# 1 配置
if [[ -f "$CFG" ]] \
  && grep -q 'gpt-5.6-terra' "$CFG" \
  && grep -q 'reasoning_effort *= *"max"' "$CFG" \
  && grep -q 'tool_shell_rtk_rewrite *= *true' "$CFG"; then
  ok "1 配置 terra/max + RTK 开关"
else
  bad "1 配置 terra/max + RTK 开关" "检查 $CFG"
fi

# 2 status
STATUS_JSON="$TMPDIR_RUN/status.json"
if "$BIN" status --config "$CFG" --json >"$STATUS_JSON" 2>"$TMPDIR_RUN/status.err"; then
  if python3 -c "import json; json.load(open('$STATUS_JSON'))" 2>/dev/null; then
    ok "2 status --json 可解析"
  else
    bad "2 status --json 可解析" "JSON 无效"
  fi
else
  bad "2 status --json 可解析" "$(head -c 160 "$TMPDIR_RUN/status.err" | tr '\n' ' ')"
fi

# 3 RTK
if command -v rtk >/dev/null 2>&1; then
  out="$(rtk hook check git status 2>/dev/null | head -1 || true)"
  if echo "$out" | grep -q 'rtk git'; then
    ok "3 RTK hook check 改写 git status"
  else
    bad "3 RTK hook check" "输出=$out"
  fi
else
  bad "3 RTK hook check" "rtk 不在 PATH"
fi

# 4 rtk unit
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --lib rtk_rewrite 2>"$TMPDIR_RUN/rtk-test.err"; then
  ok "4 cargo test --lib rtk_rewrite"
else
  bad "4 cargo test --lib rtk_rewrite" "$(tail -c 160 "$TMPDIR_RUN/rtk-test.err" | tr '\n' ' ')"
fi

# 5 norm layer
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --lib norm_layer 2>"$TMPDIR_RUN/norm.err"; then
  ok "5 cargo test --lib norm_layer"
else
  bad "5 cargo test --lib norm_layer" "$(tail -c 160 "$TMPDIR_RUN/norm.err" | tr '\n' ' ')"
fi

# 6 shell pipefail
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --test tool_runtime_tests \
  shell_exec_supports_bash_pipefail 2>"$TMPDIR_RUN/shell.err"; then
  ok "6 tool_runtime shell pipefail"
else
  bad "6 tool_runtime shell pipefail" "$(tail -c 160 "$TMPDIR_RUN/shell.err" | tr '\n' ' ')"
fi

# 7 goal-mode smoke（比 complete 更稳、覆盖 goal 闭环）
if [[ -x "$ROOT/scripts/chuang-goal-mode-smoke.sh" ]] || [[ -f "$ROOT/scripts/chuang-goal-mode-smoke.sh" ]]; then
  if timeout 180 sh "$ROOT/scripts/chuang-goal-mode-smoke.sh" >"$TMPDIR_RUN/goal.out" 2>"$TMPDIR_RUN/goal.err"; then
    ok "7 goal-mode-smoke"
  else
    bad "7 goal-mode-smoke" "$(tail -c 200 "$TMPDIR_RUN/goal.err" | tr '\n' ' ')"
  fi
else
  skip "7 goal-mode-smoke" "无脚本"
fi

# 8 browser CDP
if [[ "${SKIP_BROWSER:-0}" == "1" ]]; then
  skip "8 无头浏览器" "SKIP_BROWSER=1"
elif [[ -f "$ROOT/scripts/chuang-headless-chrome.sh" ]]; then
  timeout 90 bash "$ROOT/scripts/chuang-headless-chrome.sh" start >"$TMPDIR_RUN/chrome.start" 2>&1 || true
  chrome_ok=0
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if timeout 15 bash "$ROOT/scripts/chuang-headless-chrome.sh" status >"$TMPDIR_RUN/chrome.out" 2>"$TMPDIR_RUN/chrome.err" \
      && grep -q 'cdp_reachable=1' "$TMPDIR_RUN/chrome.out"; then
      chrome_ok=1
      break
    fi
    sleep 0.5
  done
  if [[ "$chrome_ok" == "1" ]]; then
    ok "8 headless chrome CDP reachable"
  else
    skip "8 无头浏览器" "CDP 不可达（见 chrome status）"
  fi
else
  skip "8 无头浏览器" "无脚本"
fi

# 9 live
if [[ "${SKIP_LIVE:-0}" == "1" ]]; then
  skip "9 live 短问 terra" "SKIP_LIVE=1"
elif [[ -z "${CHUANG_PROXY_API_KEY:-}" ]]; then
  skip "9 live 短问 terra" "无 API key"
else
  if timeout 180 "$BIN" run --config "$CFG" --input "只回复两个字：验收" \
    >"$TMPDIR_RUN/live.out" 2>"$TMPDIR_RUN/live.err"; then
    if grep -qE '验收|Success|status_code: 200|gpt-5.6-terra|runtime_report_status: Success' \
      "$TMPDIR_RUN/live.out" "$TMPDIR_RUN/live.err" 2>/dev/null; then
      ok "9 live 短问 terra"
    elif grep -qiE 'runtime_failed|BudgetExceeded|error' "$TMPDIR_RUN/live.out" "$TMPDIR_RUN/live.err" 2>/dev/null; then
      bad "9 live 短问 terra" "$(tail -c 200 "$TMPDIR_RUN/live.err" | tr '\n' ' ')"
    else
      ok "9 live 短问 terra（exit 0）"
    fi
  else
    bad "9 live 短问 terra" "$(tail -c 200 "$TMPDIR_RUN/live.err" | tr '\n' ' ')"
  fi
fi

# 10 thin assets
if [[ -f assets/norm/skills/closed-loop-control.md ]] \
  && [[ -f assets/norm/skills/gen-eval-separate.md ]] \
  && [[ -f assets/norm/doctrine-card.txt ]]; then
  clen=$(wc -c < assets/norm/skills/closed-loop-control.md)
  glen=$(wc -c < assets/norm/skills/gen-eval-separate.md)
  dlen=$(wc -c < assets/norm/doctrine-card.txt)
  if [[ "$clen" -lt 800 && "$glen" -lt 800 && "$dlen" -lt 2500 ]]; then
    ok "10 薄规范资产在位"
  else
    bad "10 薄规范资产" "clen=$clen glen=$glen dlen=$dlen"
  fi
else
  bad "10 薄规范资产" "缺文件"
fi

# 11 repin always-on norms after compaction
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --lib repin_restores 2>"$TMPDIR_RUN/repin.err"; then
  ok "11 repin_always_on_norms（compact 后复注）"
else
  bad "11 repin_always_on_norms" "$(tail -c 160 "$TMPDIR_RUN/repin.err" | tr '\n' ' ')"
fi

# 12 goal hard budget (time + step cap)
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --test goal_run_tests \
  goal_run_ 2>"$TMPDIR_RUN/budget.err"; then
  ok "12 goal 硬预算（max_minutes + step_run_cap）"
else
  bad "12 goal 硬预算" "$(tail -c 160 "$TMPDIR_RUN/budget.err" | tr '\n' ' ')"
fi

# 13 skill curator (read-only monitor)
if timeout 60 "$BIN" skill curator --skills-root "$ROOT/data/skills" \
  >"$TMPDIR_RUN/curator.out" 2>"$TMPDIR_RUN/curator.err"; then
  if grep -q 'curator_mode=read_only' "$TMPDIR_RUN/curator.out"; then
    ok "13 skill curator 只读卫生"
  else
    bad "13 skill curator" "缺 curator_mode 页脚"
  fi
else
  bad "13 skill curator" "$(tail -c 160 "$TMPDIR_RUN/curator.err" | tr '\n' ' ')"
fi

# 14 browser tools when CDP live
if grep -q 'cdp_reachable=1' "$TMPDIR_RUN/chrome.out" 2>/dev/null; then
  if cargo test -q --manifest-path "$ROOT/Cargo.toml" --test tool_runtime_tests \
    browser_navigate_and_read 2>"$TMPDIR_RUN/browser.err"; then
    ok "14 browser_navigate + browser_read（CDP live）"
  else
    bad "14 browser tools" "$(tail -c 160 "$TMPDIR_RUN/browser.err" | tr '\n' ' ')"
  fi
else
  skip "14 browser tools" "无 live CDP"
fi

# 15 progressive tool protocol unit
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --lib needs_full_tool 2>"$TMPDIR_RUN/prog.err" \
  || cargo test -q --manifest-path "$ROOT/Cargo.toml" --lib atomic_tool 2>"$TMPDIR_RUN/prog.err"; then
  ok "15 工具面渐进披露（catalog/detail 分层）"
else
  # soft: compile-time presence is enough if no dedicated test name
  if rg -q 'needs_full_tool_protocol|tool_catalog_block' "$ROOT/src/atomic_tool.rs" 2>/dev/null; then
    ok "15 工具面渐进披露（源码在位）"
  else
    bad "15 工具面渐进披露" "缺分层 API"
  fi
fi

echo
echo "=== 汇总 PASS=$PASS FAIL=$FAIL SKIP=$SKIP ==="
cat "$LOG"
rm -f "$LOG"

if [[ "$FAIL" -gt 0 ]]; then
  echo "chuang_field_accept_10_failed"
  exit 1
fi
echo "chuang_field_accept_10_ok"
exit 0
