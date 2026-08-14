#!/usr/bin/env bash
set -u

ROOT="${CHUANG_AGENT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
PROVIDER_ENV_FILE="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
CALLER_CWD="$(pwd)"
export CHUANG_REPL_WORKSPACE_ROOT="${CHUANG_REPL_WORKSPACE_ROOT:-$CALLER_CWD}"
cd "$ROOT" || exit 1

export CHUANG_REAL_ACTUATOR_ENABLE="${CHUANG_REAL_ACTUATOR_ENABLE:-1}"
export CHUANG_REAL_CONTROL_ENABLE="${CHUANG_REAL_CONTROL_ENABLE:-1}"
export CHUANG_CODEX_RUNNER_ENABLE="${CHUANG_CODEX_RUNNER_ENABLE:-1}"
# Interactive turns use the canonical app-server socket unless explicitly local.
export CHUANG_APP_SERVER_MODE="${CHUANG_APP_SERVER_MODE:-socket}"

if [[ "${CHUANG_REPL_STUB:-0}" == "1" ]]; then
  if [[ -t 0 && -t 1 ]]; then
    printf '%s\n' "Chuang 本地对话已启动（stub 模式）。输入内容后回车；输入 quit 或 exit 退出。"
  fi
  CODEX_PPTOKEN_API_KEY=stub-key CHUANG_PROXY_API_KEY=stub-key cargo run --quiet -- repl \
    --config config.toml \
    --provider-base-url https://api.example.com/v1 \
    --provider-api-key stub-key \
    --provider-model stub-responder \
    --provider-id local-stub \
    --provider-transport stub
  status=$?
else
  if [[ -f "$PROVIDER_ENV_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    . "$PROVIDER_ENV_FILE"
    set +a
  fi

  if [[ -z "${CHUANG_PROXY_API_KEY:-}" ]]; then
    printf '%s\n' "缺少真实 provider 环境变量：CHUANG_PROXY_API_KEY"
    printf '%s\n' ""
    printf '%s\n' "真实对话前先在当前终端设置它，然后重跑："
    printf '%s\n' "  . \"$PROVIDER_ENV_FILE\""
    printf '%s\n' "  ./scripts/launch-chuang-agent-repl.sh"
    printf '%s\n' ""
    printf '%s\n' "或者直接 export 后重跑："
    printf '%s\n' "  export CHUANG_PROXY_API_KEY='<set>'"
    printf '%s\n' "  ./scripts/launch-chuang-agent-repl.sh"
    printf '%s\n' ""
    printf '%s\n' "只验证本地链路可用时可以跑 stub 模式："
    printf '%s\n' "  CHUANG_REPL_STUB=1 ./scripts/launch-chuang-agent-repl.sh"
    status=2
  else
    if [[ -t 0 && -t 1 ]]; then
      printf '%s\n' "Chuang 本地对话已启动。输入内容后回车；输入 quit 或 exit 退出。"
    fi
    cargo run --quiet -- repl --config config.toml
    status=$?
  fi
fi

echo
if [[ -t 0 && -t 1 ]]; then
  read -n 1 -s -r -p "按任意键关闭..."
fi
exit $status
