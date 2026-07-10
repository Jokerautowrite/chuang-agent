#!/usr/bin/env bash
set -euo pipefail

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
mode="text"
work_dir="${TMPDIR:-/tmp}/chuang-agent-live-gaps-check-$$"
status_json="$work_dir/status.json"
preflight_json="$work_dir/live-preflight.json"
provider_env_file="${CHUANG_PROVIDER_ENV_FILE:-$HOME/.config/chuang-agent/provider.env}"
chuang_agent_bin="${CHUANG_AGENT_BIN:-}"

run_chuang() {
    if [ -n "$chuang_agent_bin" ]; then
        "$chuang_agent_bin" "$@"
    else
        cargo run --quiet -- "$@"
    fi
}

if [ "${1:-}" = "--json" ]; then
    mode="json"
elif [ "${1:-}" = "--help" ]; then
    cat <<'EOF'
Usage: scripts/chuang-live-gaps-check.sh [--json]

Read-only live readiness gap matrix.

It distinguishes:
- local_contract: local status/contracts are ready.
- preflight_ready_but_no_start: live runner preflight checks pass, but no worker starts.
- real_live: still pending until operator-approved live evidence exists.

This script does not connect real Feishu/provider services, does not start workers,
does not enable live gates, and does not print secret values.
EOF
    exit 0
elif [ "${1:-}" != "" ]; then
    printf '%s\n' "error: unsupported argument: $1" >&2
    exit 2
fi

mkdir -p "$work_dir"

# Keep this check local-only even if the operator shell has live gates set.
unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

if [ -f "$provider_env_file" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$provider_env_file"
    set +a
fi

cd "$root_dir"

run_chuang status --json > "$status_json"
run_chuang subagent live-preflight \
  --runner-command scripts/chuang-codex-runner.py \
  --allow-runner-command scripts/chuang-codex-runner.py \
  --requires-capability rehearsal \
  --capability rehearsal \
  --json > "$preflight_json"

python3 - "$status_json" "$preflight_json" "$mode" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    status = json.load(handle)
with open(sys.argv[2], encoding="utf-8") as handle:
    preflight = json.load(handle)["rehearsal"]
mode = sys.argv[3]

subagent = status["subagent_readiness"]
third_test = status["third_test_candidate"]
release = status["release_readiness"]
provider = status.get("provider_readiness", {})

local_contract_ready = (
    subagent["ok"] is True
    and subagent["local_contract_ready"] is True
    and subagent["local_contract_state"] == "ready"
    and subagent["ready_count"] == subagent["layer_count"]
)
preflight_ready_but_no_start = (
    preflight["ok"] is True
    and preflight["readonly"] is True
    and preflight["runner_allowlist_ok"] is True
    and preflight["capability_routing_ok"] is True
    and preflight["report_admission_ok"] is True
    and preflight["approval_audit_prerequisites_ok"] is True
    and preflight["ready_for_live"] is False
    and preflight["starts_external_worker"] is False
    and preflight["live_worker_available"] is False
)
real_live_pending = (
    third_test["real_live_ready"] is False
    and third_test["connects_real_external_services"] is False
    and release["connects_real_external_services"] is False
    and subagent["live_worker_available"] is False
)
real_live_ready = (
    third_test["real_live_ready"] is True
    and third_test["connects_real_external_services"] is True
    and release["connects_real_external_services"] is True
    and release["verifies_real_external_services"] is True
)

provider_state = provider.get("overall_state", "unknown")
provider_api_key_state_raw = str(provider.get("api_key_state", "<unknown>"))

def sanitize_env_state(value):
    if value.startswith("<missing"):
        return "<missing>"
    if value in ("<set>", "<unknown>"):
        return value
    if "<missing" in value and "<set>" not in value:
        return "<missing>"
    return "<set>"

provider_api_key_state = sanitize_env_state(provider_api_key_state_raw)
provider_env_gap = provider_api_key_state == "<missing>"

gaps = []
if not real_live_ready:
    gaps.extend(
        [
            {
                "id": "live_worker_adapter_pending",
                "state": "pending",
                "reason": subagent["worker_runtime_blocked_reason"],
            },
            {
                "id": "live_runner_gate_disabled",
                "state": "pending",
                "reason": preflight["gate"]["reason"],
            },
            {
                "id": "manual_operator_live_receipt_missing",
                "state": "pending",
                "reason": third_test["next_action"],
            },
            {
                "id": "real_external_services_not_verified",
                "state": "pending",
                "reason": "status/release readiness explicitly does not verify real external services",
            },
        ]
    )
if provider_env_gap and not real_live_ready:
    gaps.append(
        {
            "id": "provider_env_pending",
            "state": "pending",
            "reason": provider.get(
                "next_action",
                "provider env must be set before claiming live provider readiness",
            ),
        }
    )

result = {
    "ok": local_contract_ready
    and preflight_ready_but_no_start
    and (real_live_pending or real_live_ready),
    "check_name": "live-gaps",
    "marker": "live_gaps_check_ok",
    "summary": "local_contract=ready preflight=ready_but_no_start real_live="
    + ("ready" if real_live_ready else "pending"),
    "work_dir": sys.argv[1].rsplit("/", 1)[0],
    "boundaries": {
        "readonly": True,
        "connects_real_feishu": False,
        "connects_real_provider": False,
        "starts_external_worker": False,
        "enables_live_gate": False,
        "performs_desktop_actions": False,
        "performs_browser_actions": False,
        "modifies_repo": False,
        "prints_secret_values": False,
    },
    "matrix": [
        {
            "name": "local_contract",
            "state": "ready" if local_contract_ready else "blocked",
            "ready": local_contract_ready,
            "evidence": "status.subagent_readiness local contracts are ready",
            "live_worker_available": subagent["live_worker_available"],
            "worker_runtime_state": subagent["worker_runtime_state"],
        },
        {
            "name": "preflight_ready_but_no_start",
            "state": "ready_but_no_start"
            if preflight_ready_but_no_start
            else "blocked",
            "ready": preflight_ready_but_no_start,
            "ready_for_live": preflight["ready_for_live"],
            "starts_external_worker": preflight["starts_external_worker"],
            "live_worker_available": preflight["live_worker_available"],
            "worker_runtime_state": preflight["worker_runtime_state"],
            "adapter_entrypoint": preflight["adapter_entrypoint"],
        },
        {
            "name": "real_live",
            "state": "ready" if real_live_ready else "pending",
            "ready": real_live_ready,
            "requires_manual_live_check": third_test["requires_manual_live_check"],
            "connects_real_external_services": third_test[
                "connects_real_external_services"
            ],
            "verifies_real_external_services": third_test[
                "verifies_real_external_services"
            ],
            "real_live_ready": third_test["real_live_ready"],
            "gap_count": len(gaps),
        },
    ],
    "provider_readiness": {
        "overall_state": provider_state,
        "api_key_state": provider_api_key_state,
        "uses_redacted_state_only": True,
    },
    "gaps": gaps,
    "next_action": "global real-live receipt verified; keep receipt path configured"
    if real_live_ready
    else "keep candidate/third-test as local gates; collect operator-approved live receipt before claiming real live readiness",
}

if not result["ok"]:
    result["marker"] = "live_gaps_check_blocked"

if mode == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(f"live_gaps_check_ok={str(result['ok']).lower()}")
    for item in result["matrix"]:
        if item["name"] == "local_contract":
            print(
                "local_contract_state="
                + item["state"]
                + " live_worker_available="
                + str(item["live_worker_available"]).lower()
                + " worker_runtime_state="
                + item["worker_runtime_state"]
            )
        elif item["name"] == "preflight_ready_but_no_start":
            print(
                "preflight_state="
                + item["state"]
                + " ready_for_live="
                + str(item["ready_for_live"]).lower()
                + " starts_external_worker="
                + str(item["starts_external_worker"]).lower()
                + " live_worker_available="
                + str(item["live_worker_available"]).lower()
                + " worker_runtime_state="
                + item["worker_runtime_state"]
            )
        else:
            print(
                "real_live_state="
                + item["state"]
                + " real_live_ready="
                + str(item["real_live_ready"]).lower()
                + " connects_real_external_services="
                + str(item["connects_real_external_services"]).lower()
                + " gap_count="
                + str(item["gap_count"])
            )
    print("provider_readiness_state=" + provider_state)
    print("provider_api_key_state=" + provider_api_key_state)
    print("marker=" + result["marker"])

sys.exit(0 if result["ok"] else 1)
PY
