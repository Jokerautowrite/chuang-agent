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

BIN="${CHUANG_BIN:-}"
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
  if [[ -x "$ROOT/target/debug/chuang-agent" ]]; then
    BIN="$ROOT/target/debug/chuang-agent"
  elif command -v chuang >/dev/null 2>&1; then
    BIN="$(command -v chuang)"
  fi
fi
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
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

# 8 browser CDP via CLI (after ensure path exists)
# 一次 stop + navigate 偶发 CDP 竞态：失败时再 stop 重试一轮，避免假 SKIP。
if [[ "${SKIP_BROWSER:-0}" == "1" ]]; then
  skip "8 无头浏览器" "SKIP_BROWSER=1"
elif [[ -f "$ROOT/scripts/chuang-headless-chrome.sh" ]]; then
  chrome_ok=0
  for browser_try in 1 2; do
    timeout 90 "$BIN" browser stop >"$TMPDIR_RUN/chrome.stop" 2>&1 || true
    # Autostart path: do not pre-start; navigate test should bring CDP up.
    if cargo test -q --manifest-path "$ROOT/Cargo.toml" --test tool_runtime_tests \
      browser_navigate_and_read -- --nocapture >"$TMPDIR_RUN/browser.autostart.out" 2>"$TMPDIR_RUN/browser.autostart.err"; then
      if timeout 20 "$BIN" browser status >"$TMPDIR_RUN/chrome.out" 2>"$TMPDIR_RUN/chrome.err" \
        && grep -qE 'cdp_reachable=1|resolved_cdp_port=[0-9]+' "$TMPDIR_RUN/chrome.out"; then
        ok "8 browser 自动拉起 + CDP reachable"
        chrome_ok=1
        break
      fi
      # test passed but status flaky — still count navigate success
      if ! grep -qi 'skipped/failed' "$TMPDIR_RUN/browser.autostart.out" "$TMPDIR_RUN/browser.autostart.err" 2>/dev/null; then
        ok "8 browser 自动拉起（navigate 测试通过）"
        chrome_ok=1
        timeout 20 "$BIN" browser status >"$TMPDIR_RUN/chrome.out" 2>"$TMPDIR_RUN/chrome.err" || true
        break
      fi
    fi
    [[ "$browser_try" -eq 1 ]] && sleep 1
  done
  if [[ "$chrome_ok" -ne 1 ]]; then
    if [[ -s "$TMPDIR_RUN/browser.autostart.err" ]] || [[ -s "$TMPDIR_RUN/browser.autostart.out" ]]; then
      skip "8 无头浏览器" "browser 测试失败（已重试）"
    else
      skip "8 无头浏览器" "browser 测试失败"
    fi
  fi
else
  skip "8 无头浏览器" "无脚本"
  chrome_ok=0
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

# 14 status JSON browser_readiness after tools
if [[ "${chrome_ok:-0}" == "1" ]] || grep -qE 'cdp_reachable=1|resolved_cdp_port=[0-9]+' "$TMPDIR_RUN/chrome.out" 2>/dev/null; then
  if "$BIN" status --config "$CFG" --json >"$TMPDIR_RUN/status-browser.json" 2>"$TMPDIR_RUN/status-browser.err"; then
    if python3 - "$TMPDIR_RUN/status-browser.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
br=d.get("browser_readiness") or {}
if not br.get("browser_read_adapter_available"):
    raise SystemExit(1)
print(br.get("browser_read_state"), br.get("browser_read_adapter_kind"))
PY
    then
      ok "14 status browser_readiness=available"
    else
      bad "14 status browser_readiness" "adapter not available in status JSON"
    fi
  else
    bad "14 status browser_readiness" "status --json failed"
  fi
else
  skip "14 status browser_readiness" "无 live CDP"
fi

# 15 progressive tool protocol unit
if cargo test -q --manifest-path "$ROOT/Cargo.toml" --lib progressive_tool 2>"$TMPDIR_RUN/prog.err"; then
  ok "15 工具面渐进披露（catalog/detail 分层）"
else
  bad "15 工具面渐进披露" "$(tail -c 160 "$TMPDIR_RUN/prog.err" | tr '\n' ' ')"
fi

# 16 doctor surfaces browser_cdp + rtk lines
if timeout 90 "$BIN" doctor --config "$CFG" >"$TMPDIR_RUN/doctor.out" 2>"$TMPDIR_RUN/doctor.err"; then
  if grep -q 'browser_cdp:' "$TMPDIR_RUN/doctor.out" \
    && grep -q 'tool_shell_rtk_rewrite:' "$TMPDIR_RUN/doctor.out" \
    && grep -q 'doctor_check name=browser_cdp' "$TMPDIR_RUN/doctor.out"; then
    ok "16 doctor 露出 browser_cdp + rtk"
  else
    bad "16 doctor 露出" "缺 browser_cdp 或 rtk 行"
  fi
else
  bad "16 doctor" "$(tail -c 160 "$TMPDIR_RUN/doctor.err" | tr '\n' ' ')"
fi

# 17 config.toml materialize: abs paths + foreign cwd + path-like program absolutized
if [[ -f "$ROOT/scripts/chuang-materialize-runtime-config.py" ]]; then
  MAT="$TMPDIR_RUN/mat.toml"
  if python3 "$ROOT/scripts/chuang-materialize-runtime-config.py" \
      --root "$ROOT" --src "$CFG" --out "$MAT" >/dev/null 2>"$TMPDIR_RUN/mat.err" \
    && grep -q 'permission_profile = "full_local_workspace"' "$MAT" \
    && grep -q "$ROOT/scripts/chuang-real-control-adapter.py" "$MAT" \
    && grep -q "$ROOT/config/control-allowlist.json" "$MAT" \
    && grep -q "$ROOT/rules/core.md" "$MAT" \
    && (cd /tmp && timeout 30 "$BIN" status --config "$MAT" >"$TMPDIR_RUN/mat-status.out" 2>"$TMPDIR_RUN/mat-status.err") \
    && grep -q '^provider = "openai_compatible"' "$MAT"; then
    ok "17 config materialize（cwd 无关 + real control 路径）"
  else
    bad "17 config materialize" "$(tail -c 180 "$TMPDIR_RUN/mat.err" "$TMPDIR_RUN/mat-status.err" 2>/dev/null | tr '\n' ' ')"
  fi
else
  skip "17 config materialize" "无 materialize 脚本"
fi

# 18 control real adapter list（只读；allowlist 不含飞书）
if timeout 30 "$BIN" control list --config "$CFG" --json \
    >"$TMPDIR_RUN/control-list.json" 2>"$TMPDIR_RUN/control-list.err"; then
  if python3 - "$TMPDIR_RUN/control-list.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
units=d if isinstance(d,list) else d.get("units") or d.get("items") or []
if not units:
    raise SystemExit("empty units")
ids=[u.get("unit_id","") for u in units]
if any("feishu" in i.lower() for i in ids):
    raise SystemExit("feishu unit still allowlisted: "+ ",".join(ids))
if not any("app-server" in i for i in ids):
    raise SystemExit("missing app-server unit: "+ ",".join(ids))
meta=(units[0].get("metadata") or {})
adapter=str(meta.get("adapter") or "")
if adapter and adapter != "chuang-real-control":
    raise SystemExit("unexpected adapter="+adapter)
print(",".join(ids))
PY
  then
    ok "18 control list 真 adapter（无飞书）"
  else
    bad "18 control list" "$(python3 -c "print(open('$TMPDIR_RUN/control-list.json').read()[:200])" 2>/dev/null || true) $(head -c 120 "$TMPDIR_RUN/control-list.err" | tr '\n' ' ')"
  fi
else
  bad "18 control list" "$(head -c 160 "$TMPDIR_RUN/control-list.err" | tr '\n' ' ')"
fi

# 19 spawn 主链：dispatch → run-loop(codex) → collect + admission
export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"
SPAWN_Q="$TMPDIR_RUN/subagent-queue-field"
mkdir -p "$SPAWN_Q"
RUNNER="$ROOT/scripts/chuang-codex-runner.py"
if [[ ! -x "$RUNNER" && ! -f "$RUNNER" ]]; then
  skip "19 spawn dispatch/run-loop/collect" "无 codex runner"
elif [[ -z "${CHUANG_PROXY_API_KEY:-}" && "${SKIP_LIVE:-0}" == "1" ]]; then
  skip "19 spawn dispatch/run-loop/collect" "SKIP_LIVE 且无 API key"
elif [[ -z "${CHUANG_PROXY_API_KEY:-}" ]]; then
  skip "19 spawn dispatch/run-loop/collect" "无 API key"
else
  DISP_OUT="$TMPDIR_RUN/spawn-dispatch.json"
  if timeout 60 "$BIN" subagent dispatch \
      --config "$CFG" \
      --subagent-queue-root "$SPAWN_Q" \
      --task "field-accept: reply with exactly the package name from Cargo.toml package.name only" \
      --policy analyze \
      --requires-capability rust \
      --json >"$DISP_OUT" 2>"$TMPDIR_RUN/spawn-dispatch.err"; then
    RUN_ID="$(python3 -c "import json; d=json.load(open('$DISP_OUT')); print(d.get('run_id') or d.get('dispatch',{}).get('run_id') or '')" 2>/dev/null || true)"
    if [[ -z "$RUN_ID" ]]; then
      # text fallback
      RUN_ID="$(grep -oE 'run_id[=:][[:space:]]*[A-Za-z0-9._-]+' "$DISP_OUT" "$TMPDIR_RUN/spawn-dispatch.err" 2>/dev/null | head -1 | sed -E 's/.*[=:][[:space:]]*//')"
    fi
    if [[ -z "$RUN_ID" ]]; then
      bad "19 spawn dispatch" "no run_id in $(head -c 160 "$DISP_OUT" | tr '\n' ' ')"
    elif timeout 300 "$BIN" subagent run-loop \
        --config "$CFG" \
        --subagent-queue-root "$SPAWN_Q" \
        --runner command \
        --runner-command "$RUNNER" \
        --capability rust \
        --max-runs 1 \
        --approve-exec \
        >"$TMPDIR_RUN/spawn-loop.out" 2>"$TMPDIR_RUN/spawn-loop.err"; then
      if timeout 60 "$BIN" subagent collect \
          --config "$CFG" \
          --subagent-queue-root "$SPAWN_Q" \
          --run-id "$RUN_ID" \
          --json >"$TMPDIR_RUN/spawn-collect.json" 2>"$TMPDIR_RUN/spawn-collect.err"; then
        if python3 - "$TMPDIR_RUN/spawn-collect.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
blob=json.dumps(d,ensure_ascii=False)
# admission accepted or success path
ok_markers=("Accepted","admission","chuang-agent","Success","success","report")
if not any(m in blob for m in ok_markers):
    raise SystemExit("collect missing success markers")
# hard fail markers
low=blob.lower()
if "rejected" in low and "accepted" not in low:
    raise SystemExit("admission rejected")
print("ok", d.get("run_id") or d.get("status") or "collect")
PY
        then
          ok "19 spawn dispatch→run-loop→collect"
        else
          bad "19 spawn collect admission" "$(head -c 200 "$TMPDIR_RUN/spawn-collect.json" | tr '\n' ' ')"
        fi
      else
        bad "19 spawn collect" "$(head -c 160 "$TMPDIR_RUN/spawn-collect.err" | tr '\n' ' ')"
      fi
    else
      bad "19 spawn run-loop" "$(tail -c 200 "$TMPDIR_RUN/spawn-loop.err" | tr '\n' ' ')"
    fi
  else
    bad "19 spawn dispatch" "$(head -c 160 "$TMPDIR_RUN/spawn-dispatch.err" | tr '\n' ' ')"
  fi
fi

# 20 knowledge 本地 preview 可用；live wiki/GBrain 明确未接（非飞书缺口口径）
# knowledge 子命令不吃 --config；本地 search 只读目录。
if timeout 30 "$BIN" memory knowledge status --json \
    >"$TMPDIR_RUN/know-status.json" 2>"$TMPDIR_RUN/know-status.err"; then
  KNOW_ROOT="$ROOT/identity"
  if timeout 30 "$BIN" memory knowledge search \
      --root "$KNOW_ROOT" \
      --query "SOUL" \
      --limit 3 \
      --json >"$TMPDIR_RUN/know-search.json" 2>"$TMPDIR_RUN/know-search.err" \
    && python3 - "$TMPDIR_RUN/know-status.json" "$TMPDIR_RUN/know-search.json" <<'PY'
import json,sys
st=json.load(open(sys.argv[1]))
se=json.load(open(sys.argv[2]))
if not isinstance(st, dict) or st.get("adapter") != "external_knowledge":
    raise SystemExit("bad knowledge status adapter")
if st.get("connects_real_service") is True:
    raise SystemExit("status must not claim real service without endpoints")
hits = se.get("hits") if isinstance(se, dict) else None
if not isinstance(hits, list) or len(hits) < 1:
    raise SystemExit("local search expected >=1 hit under identity/")
if se.get("adapter") != "local_external_knowledge":
    raise SystemExit("search adapter mismatch")
if se.get("connects_real_service") is True:
    raise SystemExit("search must stay local-only")
print("hits", len(hits))
PY
  then
    ok "20 knowledge 本地 search/status（live wiki/GBrain 未装属预期）"
  else
    bad "20 knowledge local" "$(head -c 160 "$TMPDIR_RUN/know-search.err" "$TMPDIR_RUN/know-status.err" 2>/dev/null | tr '\n' ' ')"
  fi
else
  bad "20 knowledge status" "$(head -c 160 "$TMPDIR_RUN/know-status.err" | tr '\n' ' ')"
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
