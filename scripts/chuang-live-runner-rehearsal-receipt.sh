#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"
SMOKE_NAME="${CHUANG_LIVE_REHEARSAL_SMOKE_NAME:-single_live_worker_rehearsal}"
chuang_agent_bin="${CHUANG_AGENT_BIN:-}"

run_chuang() {
  if [ -n "$chuang_agent_bin" ]; then
    "$chuang_agent_bin" "$@"
  else
    cargo run --quiet -- "$@"
  fi
}

usage() {
  cat <<'USAGE'
usage: scripts/chuang-live-runner-rehearsal-receipt.sh [--json]

Run a bounded single-worker rehearsal receipt using the existing queued subagent protocol.
Path: dispatch -> run-loop(command runner) -> report -> collect -> admission refs.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
work_dir="${TMPDIR:-/tmp}/chuang-agent-${SMOKE_NAME}-receipt-$$"
config_path="$work_dir/config.toml"
queue_root="$work_dir/subagent-queue"
preflight_json="$work_dir/live-preflight.json"
dispatch_json="$work_dir/dispatch.json"
run_loop_json="$work_dir/run-loop.json"
report_json="$work_dir/report.json"
collect_json="$work_dir/collect.json"
run_id_file="$work_dir/run-id.txt"

mkdir -p "$work_dir"

# Keep this receipt bounded to local command-runner rehearsal.
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE
export CHUANG_CODEX_RUNNER_ENABLE=1

cd "$root_dir"

cat > "$config_path" <<CFG
db_path = "$work_dir/chuang-agent.db"
identity_memory_root = "$work_dir/identity-memory"
provider = "fake"
provider_id = "fake-runtime"
model = "stub-responder"
subagent = "queued_external"
subagent_queue_root = "$queue_root"
CFG

printf '%s\n' "[rehearsal] live preflight with explicit live gate" >&2
run_chuang subagent live-preflight \
  --runner-command scripts/chuang-subagent-runner-example.sh \
  --allow-runner-command scripts/chuang-subagent-runner-example.sh \
  --requires-capability rehearsal \
  --capability rehearsal \
  --json > "$preflight_json"

printf '%s\n' "[rehearsal] dispatch one bounded task" >&2
run_chuang subagent dispatch \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --task "single live worker rehearsal via command runner example" \
  --requires-capability rehearsal \
  --json > "$dispatch_json"

printf '%s\n' "[rehearsal] run one command worker with capability match" >&2
run_chuang subagent run-loop \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --runner command \
  --runner-command scripts/chuang-subagent-runner-example.sh \
  --allow-runner-command scripts/chuang-subagent-runner-example.sh \
  --approve-exec \
  --require-live-gate \
  --capability rehearsal \
  --max-runs 1 \
  --max-concurrency 1 \
  --json > "$run_loop_json"

export FORMAT work_dir preflight_json dispatch_json run_loop_json run_id_file

python3 - <<'PY'
import json
import os
from pathlib import Path

preflight = json.loads(Path(os.environ["preflight_json"]).read_text(encoding="utf-8"))["rehearsal"]
dispatch = json.loads(Path(os.environ["dispatch_json"]).read_text(encoding="utf-8"))
run_loop = json.loads(Path(os.environ["run_loop_json"]).read_text(encoding="utf-8"))
dispatch_payload = json.loads(
    Path(dispatch["dispatch_path"]).read_text(encoding="utf-8")
)

assert preflight["ok"] is True
assert preflight["ready_for_live"] is True
assert preflight["readonly"] is True
assert preflight["starts_external_worker"] is False
assert preflight["gate_enabled"] is True
assert preflight["runner_allowlist_ok"] is True
assert preflight["capability_routing_ok"] is True
assert preflight["report_admission_ok"] is True

required_capabilities_raw = dispatch_payload.get("metadata", {}).get("required_capabilities", "")
required_capabilities = [
    item.strip() for item in required_capabilities_raw.split(",") if item.strip()
]
assert required_capabilities == ["rehearsal"]

assert run_loop["runner"] == "command"
assert run_loop["max_runs"] == 1
assert run_loop["max_concurrency"] == 1
assert run_loop["ran_count"] == 1
assert run_loop["run_ids"] and len(run_loop["run_ids"]) == 1
assert run_loop["report_admissions"] and len(run_loop["report_admissions"]) == 1
admission_from_run_loop = run_loop["report_admissions"][0]
assert admission_from_run_loop["status"] == "Accepted"
assert admission_from_run_loop["reason_code"] == "report_validated"
assert admission_from_run_loop["controller_agent_id"] == "cli-subagent-controller"

Path(os.environ["run_id_file"]).write_text(run_loop["run_ids"][0], encoding="utf-8")
PY

run_id="$(cat "$run_id_file")"

printf '%s\n' "[rehearsal] collect report and admission evidence" >&2
run_chuang subagent report \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --run-id "$run_id" \
  --json > "$report_json"

run_chuang subagent collect \
  --config "$config_path" \
  --subagent queued_external \
  --subagent-queue-root "$queue_root" \
  --run-id "$run_id" \
  --json > "$collect_json"

export FORMAT work_dir queue_root preflight_json dispatch_json run_loop_json report_json collect_json

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone
from pathlib import Path

FORMAT = os.environ["FORMAT"]
work_dir = os.environ["work_dir"]
queue_root = os.environ["queue_root"]
preflight = json.loads(Path(os.environ["preflight_json"]).read_text(encoding="utf-8"))["rehearsal"]
dispatch = json.loads(Path(os.environ["dispatch_json"]).read_text(encoding="utf-8"))
run_loop = json.loads(Path(os.environ["run_loop_json"]).read_text(encoding="utf-8"))
report_output = json.loads(Path(os.environ["report_json"]).read_text(encoding="utf-8"))
collect_output = json.loads(Path(os.environ["collect_json"]).read_text(encoding="utf-8"))

assert report_output["available"] is True
assert collect_output["dispatch_available"] is True
assert collect_output["report_available"] is True

report = collect_output["report"]
admission = collect_output["report_admission"]

assert report["status"] == "Success"
assert report["exit_code"] == 0
assert report["governance_decision"]["decision"] == "allowed"
assert report["governance_decision"]["reason"] == "approval_receipt=cli_flag:--approve-exec"
assert admission["status"] == "Accepted"
assert admission["reason_code"] == "report_validated"
assert admission["controller_agent_id"] == "cli-subagent-controller"
assert admission["report_id"] == report["report_id"]
assert admission["task_id"] == report["task_id"]
assert admission["agent_id"] == report["agent_id"]

admission_ref = {
    "admission_status": admission["status"],
    "reason_code": admission["reason_code"],
    "report_id": admission["report_id"],
    "task_id": admission["task_id"],
    "agent_id": admission["agent_id"],
    "controller_agent_id": admission["controller_agent_id"],
    "decided_at": admission["decided_at"],
    "evidence_ref": f"report://{admission['report_id']}",
}

result = {
    "schema_version": 1,
    "tested_at": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
    "receipt_kind": "single_worker_rehearsal_live_receipt",
    "work_dir": work_dir,
    "queue_root": queue_root,
    "readonly_boundaries": {
        "readonly": False,
        "connects_real_feishu": False,
        "sends_feishu_messages": False,
        "connects_real_provider": False,
        "starts_external_worker": True,
        "enables_live_gate": True,
        "starts_workers": True,
        "dispatches_tasks": True,
        "restarts_worker": False,
        "performs_desktop_actions": False,
        "performs_browser_actions": False,
        "connects_real_wiki": False,
        "connects_real_gbrain": False,
        "reads_secret_values": False,
        "prints_secret_values": False,
        "starts_services": False,
        "stops_services": False,
        "touches_services": False,
        "modifies_repo": False,
        "deletes_files": False,
        "reuses_codex_or_hermes_credentials": False,
    },
    "preflight": {
        "ok": preflight["ok"],
        "ready_for_live": preflight["ready_for_live"],
        "readonly": preflight["readonly"],
        "starts_external_worker": preflight["starts_external_worker"],
        "gate_enabled": preflight["gate_enabled"],
        "runner_allowlist_ok": preflight["runner_allowlist_ok"],
        "capability_routing_ok": preflight["capability_routing_ok"],
        "report_admission_ok": preflight["report_admission_ok"],
        "audit_label": preflight["gate"]["audit_label"],
        "required_env": preflight["gate"]["required_env"],
    },
    "dispatch": {
        "run_id": dispatch["run_id"],
        "task_id": dispatch["task_id"],
        "agent_id": dispatch["agent_id"],
        "required_capabilities": ["rehearsal"],
    },
    "worker_execution": {
        "runner": run_loop["runner"],
        "max_runs": run_loop["max_runs"],
        "max_concurrency": run_loop["max_concurrency"],
        "ran_count": run_loop["ran_count"],
        "idle": run_loop["idle"],
        "run_ids": run_loop["run_ids"],
        "report_paths": run_loop["report_paths"],
        "report_admissions": run_loop["report_admissions"],
    },
    "report": {
        "available": report_output["available"],
        "report_id": report["report_id"],
        "status": report["status"],
        "summary": report["summary"],
        "exit_code": report["exit_code"],
        "governance_decision": report["governance_decision"],
        "report_admission": report_output["report_admission"],
    },
    "collect": {
        "dispatch_available": collect_output["dispatch_available"],
        "report_available": collect_output["report_available"],
        "report_id": report["report_id"],
        "admission_status": admission["status"],
        "admission_reason_code": admission["reason_code"],
        "admission_refs": [admission_ref],
    },
    "real_live_acceptance": {
        "single_worker_rehearsal_complete": True,
        "status": "single_worker_rehearsal_completed",
        "global_real_live_ready": False,
        "remaining_gap_count": 3,
        "next_gaps": [
            "feishu_live_receipt",
            "browser_desktop_boundary_receipt",
            "wiki_gbrain_readonly_adapter_receipt",
        ],
    },
    "notes": [],
    "blockers": [],
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print(
        "single_worker_rehearsal_live_receipt "
        f"run_id={result['dispatch']['run_id']} "
        f"report_id={result['report']['report_id']} "
        f"admission={result['collect']['admission_status']} "
        f"reason={result['collect']['admission_reason_code']} "
        f"global_real_live_ready={str(result['real_live_acceptance']['global_real_live_ready']).lower()}"
    )
PY
