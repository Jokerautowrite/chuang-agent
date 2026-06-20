#!/bin/sh
set -eu

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
work_dir="${TMPDIR:-/tmp}/chuang-real-natural-acceptance-$$"
workspace="$work_dir/workspace"
provider_env_file="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"

mkdir -p "$workspace/src" "$workspace/logs" "$workspace/reports"
cd "$root_dir"

export CHUANG_REAL_ACTUATOR_ENABLE="${CHUANG_REAL_ACTUATOR_ENABLE:-1}"
export CHUANG_REAL_CONTROL_ENABLE="${CHUANG_REAL_CONTROL_ENABLE:-1}"
export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"

if [ -f "$provider_env_file" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$provider_env_file"
  set +a
fi

if [ -z "${CHUANG_PROXY_API_KEY:-}" ]; then
  printf '%s\n' "[real-natural] error: missing CHUANG_PROXY_API_KEY" >&2
  printf '%s\n' "[real-natural] provider_env_file=$provider_env_file" >&2
  exit 3
fi

cat > "$workspace/src/calc.py" <<'PY'
def add(a, b):
    return a - b

if __name__ == "__main__":
    print(add(2, 3))
PY

cat > "$workspace/test_calc.py" <<'PY'
import unittest
from src.calc import add

class CalcTest(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2, 3), 5)

if __name__ == "__main__":
    unittest.main()
PY

cat > "$workspace/logs/app.log" <<'EOF'
2026-06-20 10:00:00 INFO boot ok
2026-06-20 10:01:00 ERROR payment timeout order=demo-001
2026-06-20 10:02:00 INFO recovered
EOF

(
  cd "$workspace"
  git init -q
  git config user.email "chuang-acceptance@example.invalid"
  git config user.name "Chuang Acceptance"
  git add .
  git commit -q -m "seed acceptance workspace"
)

run_task_once() {
  name="$1"
  prompt="$2"
  log_file="$work_dir/$name.txt"
  (
    cd "$workspace"
    chuang ask "$prompt"
  ) > "$log_file" 2>&1
}

assert_no_provider_error() {
  file="$1"
  python3 - "$file" <<'PY'
import sys
text = open(sys.argv[1], encoding="utf-8").read()
bad = [
    "PROVIDER_HTTP_ERROR:",
    "provider_response_ok: false",
    "provider_failure_category:",
    "status_code: 503",
]
for marker in bad:
    if marker in text:
        raise SystemExit(f"provider error marker found: {marker}")
PY
}

retry_task() {
  name="$1"
  prompt="$2"
  check_cmd="$3"
  attempt=1
  printf '[real-natural] %-32s' "$name"
  while [ "$attempt" -le 3 ]; do
    if run_task_once "$name" "$prompt" \
      && assert_no_provider_error "$work_dir/$name.txt" \
      && sh -c "$check_cmd"; then
      printf ' OK\n'
      return 0
    fi
    if [ "$attempt" -lt 3 ]; then
      printf ' retry%s' "$attempt"
      sleep 5
    fi
    attempt=$((attempt + 1))
  done
  printf ' FAIL\n'
  printf '%s\n' "log=$work_dir/$name.txt" >&2
  tail -n 120 "$work_dir/$name.txt" >&2 || true
  exit 1
}

assert_contains() {
  file="$1"
  needle="$2"
  python3 - "$file" "$needle" <<'PY'
import sys
path, needle = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read()
if needle not in text:
    raise SystemExit(f"missing needle={needle!r} file={path}")
PY
}

assert_file_text() {
  file="$1"
  expected="$2"
  python3 - "$file" "$expected" <<'PY'
import sys
path, expected = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read().strip()
if text != expected:
    raise SystemExit(f"unexpected file content path={path} actual={text!r} expected={expected!r}")
PY
}

retry_task "git-status-report" \
  "请查看当前目录的 git 状态和文件列表，必须用工具完成，然后把结果写入 reports/status.txt。文件最终内容必须严格等于两行：第一行 git-status-ok，第二行 files-ok。" \
  "python3 - '$work_dir/git-status-report.txt' '$workspace/reports/status.txt' <<'PY'
import sys
log, report = sys.argv[1], sys.argv[2]
assert 'tool_calls_json:' in open(log, encoding='utf-8').read()
assert open(report, encoding='utf-8').read().strip() == 'git-status-ok\nfiles-ok'
PY"

retry_task "read-log-report" \
  "请读取 logs/app.log，找出 ERROR 行，把错误摘要写入 reports/log-summary.txt。文件内容必须包含 payment timeout。" \
  "python3 - '$work_dir/read-log-report.txt' '$workspace/reports/log-summary.txt' <<'PY'
import sys
log, report = sys.argv[1], sys.argv[2]
assert 'tool_calls_json:' in open(log, encoding='utf-8').read()
assert 'payment timeout' in open(report, encoding='utf-8').read()
PY"

retry_task "fix-test-failure" \
  "请运行 python3 -m unittest -q。现在测试应该失败。你需要读取失败原因，修复 src/calc.py，再重新运行 python3 -m unittest -q，最后把 unittest-ok 写入 reports/test-result.txt。" \
  "python3 - '$work_dir/fix-test-failure.txt' '$workspace/reports/test-result.txt' <<'PY'
import sys
log, report = sys.argv[1], sys.argv[2]
text = open(log, encoding='utf-8').read()
assert 'tool_calls_json:' in text
assert 'unittest' in text
assert open(report, encoding='utf-8').read().strip() == 'unittest-ok'
PY"
(
  cd "$workspace"
  python3 -m unittest -q > "$work_dir/final-unittest.log" 2>&1
)

retry_task "final-report" \
  "请用工具一次性读取 reports/status.txt、reports/log-summary.txt、reports/test-result.txt，并生成 reports/final.md。reports/final.md 必须包含自然语言验收通过。写完文件后必须给出 FINAL 回复，不要继续调用工具。" \
  "python3 - '$work_dir/final-report.txt' '$workspace/reports/final.md' <<'PY'
import sys
log, report = sys.argv[1], sys.argv[2]
text = open(log, encoding='utf-8').read()
assert 'tool_calls_json:' in text
assert 'runtime_failed:' not in text
assert '自然语言验收通过' in open(report, encoding='utf-8').read()
PY"

printf '%s\n' "work_dir=$work_dir"
printf '%s\n' "workspace=$workspace"
printf '%s\n' "chuang_real_natural_acceptance_ok"
