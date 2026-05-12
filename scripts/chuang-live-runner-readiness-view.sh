#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"
ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
CONFIG_PATH=""
WORKSPACE_ROOT="${CHUANG_LIVE_RUNNER_WORKSPACE_ROOT:-$ROOT}"
BINARY_PATH="${CHUANG_AGENT_BIN:-}"
BINARY_PATH_EXPLICIT=0
BINARY_BLOCKED_REASON=""

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-runner-readiness-view.sh [--json] [--config PATH] [--workspace-root PATH] [--binary PATH]

Readonly local live runner readiness view.

Sources:
  subagent live-preflight
  status --json
  doctor --json
  app-server health --diagnostic --json

Boundaries:
  readonly=true
  starts_external_worker=false
  connects_real_provider=false
  connects_real_feishu=false
  connects_hermes=false
  reads_secret_values=false
  prints_secret_values=false
  writes_core_memory=false
  modifies_repo=false
  deletes_files=false
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    --config)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --config" >&2
        usage >&2
        exit 2
      fi
      CONFIG_PATH="$2"
      shift
      ;;
    --config=*)
      CONFIG_PATH="${1#--config=}"
      ;;
    --workspace-root)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --workspace-root" >&2
        usage >&2
        exit 2
      fi
      WORKSPACE_ROOT="$2"
      shift
      ;;
    --workspace-root=*)
      WORKSPACE_ROOT="${1#--workspace-root=}"
      ;;
    --binary)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --binary" >&2
        usage >&2
        exit 2
      fi
      BINARY_PATH="$2"
      BINARY_PATH_EXPLICIT=1
      shift
      ;;
    --binary=*)
      BINARY_PATH="${1#--binary=}"
      BINARY_PATH_EXPLICIT=1
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

if [ -z "$CONFIG_PATH" ] && [ -f "$ROOT/config.toml" ]; then
  CONFIG_PATH="$ROOT/config.toml"
fi

if [ -n "$BINARY_PATH" ]; then
  if [ ! -x "$BINARY_PATH" ]; then
    BINARY_BLOCKED_REASON="missing or non-executable local chuang-agent binary: $BINARY_PATH"
    BINARY_PATH=""
  fi
fi

if [ -z "$BINARY_PATH" ] && [ "$BINARY_PATH_EXPLICIT" -eq 0 ] && [ -z "$BINARY_BLOCKED_REASON" ]; then
  if [ -x "$ROOT/target/debug/chuang-agent" ]; then
    BINARY_PATH="$ROOT/target/debug/chuang-agent"
  elif [ -x "$ROOT/target/release/chuang-agent" ]; then
    BINARY_PATH="$ROOT/target/release/chuang-agent"
  fi
fi

if [ -z "$BINARY_PATH" ] && [ -z "$BINARY_BLOCKED_REASON" ]; then
  BINARY_BLOCKED_REASON="missing local chuang-agent binary; readonly view will not run cargo or build artifacts"
fi

unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

for secret_prefix in \
  CODEX_PPTOKEN_ \
  OPENAI_ \
  ANTHROPIC_ \
  GEMINI_ \
  GOOGLE_ \
  DEEPSEEK_ \
  OPENROUTER_ \
  FEISHU_ \
  LARK_ \
  CODEX_FEISHU_ \
  CHUANG_FEISHU_ \
  CHUANG_LARK_ \
  HERMES_
do
  while IFS= read -r secret_name; do
    unset "$secret_name"
  done < <(compgen -e "$secret_prefix" || true)
done

export CODEX_PPTOKEN_API_KEY="${CODEX_PPTOKEN_API_KEY:-test-key}"

export FORMAT ROOT CONFIG_PATH WORKSPACE_ROOT BINARY_PATH BINARY_BLOCKED_REASON

python3 - <<'PY'
import copy
import json
import os
import shlex
import subprocess
import sys
from pathlib import Path

FORMAT = os.environ["FORMAT"]
ROOT = Path(os.environ["ROOT"])
CONFIG_PATH = os.environ.get("CONFIG_PATH", "")
WORKSPACE_ROOT = os.environ["WORKSPACE_ROOT"]
BINARY_PATH = os.environ.get("BINARY_PATH", "")
BINARY_BLOCKED_REASON = os.environ.get("BINARY_BLOCKED_REASON", "")

if BINARY_PATH:
    LIVE_PREFLIGHT_COMMAND = [
        BINARY_PATH,
        "subagent",
        "live-preflight",
        "--runner-command",
        "scripts/chuang-codex-runner.py",
        "--allow-runner-command",
        "scripts/chuang-codex-runner.py",
        "--requires-capability",
        "rehearsal",
        "--capability",
        "rehearsal",
        "--json",
    ]

    STATUS_COMMAND = [BINARY_PATH, "status", "--json"]
    DOCTOR_COMMAND = [BINARY_PATH, "doctor", "--json"]
    APP_SERVER_HEALTH_COMMAND = [
        BINARY_PATH,
        "app-server",
        "health",
        "--workspace-root",
        WORKSPACE_ROOT,
        "--diagnostic",
        "--json",
    ]
else:
    LIVE_PREFLIGHT_COMMAND = []
    STATUS_COMMAND = []
    DOCTOR_COMMAND = []
    APP_SERVER_HEALTH_COMMAND = []

if CONFIG_PATH:
    if STATUS_COMMAND:
        STATUS_COMMAND[2:2] = ["--config", CONFIG_PATH]
    if DOCTOR_COMMAND:
        DOCTOR_COMMAND[2:2] = ["--config", CONFIG_PATH]


def command_ref(command):
    return shlex.join(command)


def run_json_command(name, command):
    if not command:
        blocked_reason = BINARY_BLOCKED_REASON or "missing local chuang-agent binary; readonly view will not run cargo or build artifacts"
        return {
            "name": name,
            "command": "missing local chuang-agent binary",
            "evidence_ref": "missing local chuang-agent binary",
            "available": False,
            "exit_code": None,
            "stdout_preview": "",
            "stderr_preview": "",
            "blocked_reason": blocked_reason,
            "next_action": "build chuang-agent outside this readonly view or pass --binary PATH",
        }

    proc = subprocess.run(
        command,
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    result = {
        "name": name,
        "command": command_ref(command),
        "evidence_ref": command_ref(command),
        "available": proc.returncode == 0,
        "exit_code": proc.returncode,
        "stdout_preview": proc.stdout[:2000],
        "stderr_preview": proc.stderr[:2000],
    }
    if proc.returncode != 0:
        stderr = proc.stderr.strip()
        result["blocked_reason"] = stderr or f"{name} exited with code {proc.returncode}"
        result["next_action"] = stderr or "inspect command output and re-run local readonly view"
        return result

    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        result["available"] = False
        result["blocked_reason"] = f"invalid_json: {exc}"
        result["next_action"] = "inspect command output and fix malformed json"
        return result

    result["payload"] = payload
    result["blocked_reason"] = None
    result["next_action"] = None
    return result


def get_path(value, *keys, default=None):
    current = value
    for key in keys:
        if not isinstance(current, dict) or key not in current:
            return default
        current = current[key]
    return current


def get_first(items, default=None):
    if isinstance(items, list) and items:
        return items[0]
    return default


def first_present(*values, default=None):
    for value in values:
        if value is not None:
            return value
    return default


def first_nonempty(*values, default=None):
    for value in values:
        if isinstance(value, str):
            if value.strip():
                return value
        elif value not in (None, [], {}):
            return value
    return default


def format_text_list(values):
    if not isinstance(values, list) or not values:
        return "none"
    return ",".join(str(value) for value in values)


def find_layer(subagent_readiness, name):
    layers = subagent_readiness.get("layers")
    if not isinstance(layers, list):
        return None
    for layer in layers:
        if isinstance(layer, dict) and layer.get("name") == name:
            return layer
    return None


sources = {
    "subagent_live_preflight": run_json_command("subagent_live_preflight", LIVE_PREFLIGHT_COMMAND),
    "status_json": run_json_command("status_json", STATUS_COMMAND),
    "doctor_json": run_json_command("doctor_json", DOCTOR_COMMAND),
    "app_server_health": run_json_command("app_server_health", APP_SERVER_HEALTH_COMMAND),
}

preflight = sources["subagent_live_preflight"]
status_source = sources["status_json"]
doctor_source = sources["doctor_json"]
health_source = sources["app_server_health"]

preflight_rehearsal = preflight.get("payload", {}).get("rehearsal", {}) if preflight["available"] else {}
status_payload = status_source.get("payload", {}) if status_source["available"] else {}
doctor_payload = doctor_source.get("payload", {}) if doctor_source["available"] else {}
health_payload = health_source.get("payload", {}) if health_source["available"] else {}

status_subagent = get_path(status_payload, "subagent_readiness", default={}) or {}
doctor_subagent = get_path(doctor_payload, "status", "subagent_readiness", default={}) or {}
health_subagent = get_path(health_payload, "subagent_readiness", default={}) or {}

runtime_report_surface = first_nonempty(
    get_path(status_payload, "runtime_report_surface", default=None),
    get_path(doctor_payload, "status", "runtime_report_surface", default=None),
    get_path(health_payload, "runtime_report_surface", default=None),
)
if not isinstance(runtime_report_surface, dict):
    runtime_report_surface = {
        "ok": False,
        "artifact_count": 0,
        "observability_field_count": 0,
        "artifact_locators": [],
        "observability_fields": [],
        "blocked_reason": "runtime_report_surface unavailable in readonly sources",
    }

policy_tool_status = first_nonempty(
    get_path(status_payload, "policy_tool_status", default=None),
    get_path(doctor_payload, "status", "policy_tool_status", default=None),
    get_path(health_payload, "policy_tool_status", default=None),
)
if not isinstance(policy_tool_status, dict):
    policy_tool_status = {
        "active_permission_profile": "unknown",
        "tool_descriptor_count": 0,
        "ga_tool_descriptor_mapped_count": 0,
        "ga_tool_descriptor_missing": [],
        "ga_tool_descriptors": [],
        "blocked_reason": "policy_tool_status unavailable in readonly sources",
    }

live_readiness = first_nonempty(
    get_path(status_payload, "live_readiness", default=None),
    get_path(doctor_payload, "status", "live_readiness", default=None),
    get_path(health_payload, "live_readiness", default=None),
)
if not isinstance(live_readiness, dict):
    live_readiness = {
        "ok": False,
        "overall_state": "unavailable",
        "local_ready_scope": "unknown",
        "ga_local_mapped_only": False,
        "desktop_browser_live_gated": False,
        "browser_worker_frozen": False,
        "live_worker_available": False,
        "real_external_acceptance_pending": True,
        "provider_live_request_verified_by_status": False,
        "mapped_does_not_mean_live": True,
        "gated_does_not_mean_ready": True,
        "frozen_does_not_mean_ready": True,
        "ready_does_not_mean_live": True,
        "blocked_reason": "live_readiness unavailable in readonly sources",
    }

status_layer = find_layer(status_subagent, "live_runner_rehearsal") if status_subagent else None
doctor_layer = find_layer(doctor_subagent, "live_runner_rehearsal") if doctor_subagent else None
health_layer = find_layer(health_subagent, "live_runner_rehearsal") if health_subagent else None

subagent_layer_source = status_layer or doctor_layer or health_layer or {}
subagent_ready_source = status_subagent or doctor_subagent or health_subagent or {}
source_order = ["subagent_live_preflight", "status_json", "doctor_json", "app_server_health"]

missing_sources = [name for name, source in sources.items() if not source["available"]]
sources_complete = not missing_sources

runtime_blocked_reason_from_sources = first_nonempty(
    get_path(status_subagent, "worker_runtime_blocked_reason", default=None),
    get_path(doctor_subagent, "worker_runtime_blocked_reason", default=None),
    get_path(health_subagent, "worker_runtime_blocked_reason", default=None),
    get_path(subagent_layer_source, "blocked_reason", default=None),
)
capability_mismatch_reason = first_nonempty(
    get_path(status_subagent, "capability_mismatch_reason", default=None),
    get_path(doctor_subagent, "capability_mismatch_reason", default=None),
    get_path(health_subagent, "capability_mismatch_reason", default=None),
    get_path(subagent_layer_source, "capability_mismatch_reason", default=None),
)

capability_mismatch_blocks_live = bool(
    get_path(status_subagent, "capability_mismatch_blocks_live", default=False)
    or get_path(doctor_subagent, "capability_mismatch_blocks_live", default=False)
    or get_path(health_subagent, "capability_mismatch_blocks_live", default=False)
    or (not sources_complete)
)

ready_for_live = bool(preflight_rehearsal.get("ready_for_live", False)) and sources_complete and not capability_mismatch_blocks_live
starts_external_worker = False

if missing_sources:
    blocked_reason = next(
        (sources[name]["blocked_reason"] for name in source_order if name in missing_sources),
        "missing readonly source",
    )
elif capability_mismatch_blocks_live:
    blocked_reason = (
        capability_mismatch_reason
        or runtime_blocked_reason_from_sources
        or preflight_rehearsal.get("worker_runtime_reason")
        or preflight_rehearsal.get("next_action")
        or "capability mismatch blocks live runner readiness"
    )
elif not ready_for_live:
    blocked_reason = (
        preflight_rehearsal.get("worker_runtime_reason")
        or preflight_rehearsal.get("worker_runtime_blocked_reason")
        or preflight_rehearsal.get("next_action")
        or runtime_blocked_reason_from_sources
        or "live runner rehearsal remains read-only"
    )
else:
    blocked_reason = "none"

missing_source_next_action = first_nonempty(
    *(sources[name].get("next_action") for name in source_order if name in missing_sources)
)

next_action = first_nonempty(
    missing_source_next_action,
    preflight_rehearsal.get("next_action"),
    get_path(status_payload, "next_action", default=None),
    get_first(get_path(status_payload, "next_actions", default=[])),
    get_path(doctor_payload, "next_action", default=None),
    get_first(get_path(doctor_payload, "next_actions", default=[])),
    get_path(health_payload, "next_action", default=None),
    get_first(get_path(health_payload, "next_actions", default=[])),
    get_path(subagent_layer_source, "next_action", default=None),
    blocked_reason,
)

live_runner_rehearsal_state = (
    get_path(subagent_layer_source, "state", default=None)
    or get_path(subagent_ready_source, "overall_state", default=None)
    or ("blocked" if not ready_for_live else "ready_but_no_start")
)
if not sources_complete:
    live_runner_rehearsal_state = "blocked"

live_runner_rehearsal = {
    "name": "live_runner_rehearsal",
    "state": live_runner_rehearsal_state,
    "overall_state": get_path(subagent_ready_source, "overall_state", default="blocked"),
    "boundary": first_nonempty(get_path(subagent_layer_source, "boundary", default=None), default="read_only_preflight"),
    "current": first_nonempty(
        get_path(subagent_layer_source, "current", default=None),
        preflight_rehearsal.get("adapter_entrypoint"),
        default="local readonly live runner readiness view",
    ),
    "ready_for_live": ready_for_live,
    "starts_external_worker": starts_external_worker,
    "live_worker_available": first_present(
        preflight_rehearsal.get("live_worker_available"),
        get_path(subagent_ready_source, "live_worker_available", default=None),
        get_path(subagent_layer_source, "live_worker_available", default=None),
        default=False,
    ),
    "worker_runtime_state": first_nonempty(
        preflight_rehearsal.get("worker_runtime_state"),
        get_path(subagent_ready_source, "worker_runtime_state", default=None),
        get_path(subagent_layer_source, "worker_runtime_state", default=None),
        default="local_contract_only",
    ),
    "worker_runtime_reason": first_nonempty(
        preflight_rehearsal.get("worker_runtime_reason"),
        get_path(subagent_ready_source, "worker_runtime_reason", default=None),
        default=blocked_reason,
    ),
    "worker_runtime_blocked_reason": first_nonempty(
        preflight_rehearsal.get("worker_runtime_blocked_reason"),
        runtime_blocked_reason_from_sources,
        default=blocked_reason,
    ),
    "capability_route_state": first_nonempty(
        get_path(subagent_ready_source, "capability_route_state", default=None),
        get_path(subagent_layer_source, "capability_route_state", default=None),
        default="requires_dispatch_required_capabilities",
    ),
    "capability_mismatch_blocks_live": capability_mismatch_blocks_live,
    "capability_mismatch_reason": first_nonempty(
        capability_mismatch_reason,
        default=blocked_reason,
    ),
    "local_contract_ready": first_present(
        get_path(subagent_layer_source, "local_contract_ready", default=None),
        get_path(subagent_ready_source, "local_contract_ready", default=None),
        default=False,
    ),
    "local_contract_state": first_nonempty(
        get_path(subagent_layer_source, "local_contract_state", default=None),
        get_path(subagent_ready_source, "local_contract_state", default=None),
        default="unknown",
    ),
    "local_contract_reason": first_nonempty(
        get_path(subagent_layer_source, "local_contract_reason", default=None),
        get_path(subagent_ready_source, "local_contract_reason", default=None),
        default="source unavailable",
    ),
    "live_adapter_ready": first_present(
        get_path(subagent_layer_source, "live_adapter_ready", default=None),
        get_path(subagent_ready_source, "live_adapter_ready", default=None),
        default=False,
    ),
    "live_adapter_state": first_nonempty(
        get_path(subagent_layer_source, "live_adapter_state", default=None),
        get_path(subagent_ready_source, "live_adapter_state", default=None),
        default="deferred",
    ),
    "live_adapter_reason": first_nonempty(
        get_path(subagent_layer_source, "live_adapter_reason", default=None),
        get_path(subagent_ready_source, "live_adapter_reason", default=None),
        default="real worker execution remains gated and deferred",
    ),
    "blocked_reason": blocked_reason,
    "next_action": next_action,
    "source_evidence_refs": {
        "subagent_live_preflight": preflight["evidence_ref"],
        "status_json": status_source["evidence_ref"],
        "doctor_json": doctor_source["evidence_ref"],
        "app_server_health": health_source["evidence_ref"],
    },
    "missing_sources": missing_sources,
    "sources": sources,
    "subagent_readiness": {
        "status": copy.deepcopy(status_subagent),
        "doctor": copy.deepcopy(doctor_subagent),
        "app_server_health": copy.deepcopy(health_subagent),
    },
    "layers": {
        "status": copy.deepcopy(status_layer) if status_layer else None,
        "doctor": copy.deepcopy(doctor_layer) if doctor_layer else None,
        "app_server_health": copy.deepcopy(health_layer) if health_layer else None,
    },
}

result = {
    "schema_version": 1,
    "readonly": True,
    "connects_real_provider": False,
    "connects_real_feishu": False,
    "connects_hermes": False,
    "starts_worker": False,
    "starts_external_worker": False,
    "reads_secret_values": False,
    "prints_secret_values": False,
    "writes_core_memory": False,
    "modifies_repo": False,
    "deletes_files": False,
    "binary_path": BINARY_PATH or None,
    "binary_blocked_reason": BINARY_BLOCKED_REASON or None,
    "workspace_root": WORKSPACE_ROOT,
    "config_path": CONFIG_PATH or None,
    "runtime_report_surface": runtime_report_surface,
    "policy_tool_status": policy_tool_status,
    "live_readiness": live_readiness,
    "live_runner_rehearsal": live_runner_rehearsal,
    "source_evidence_refs": live_runner_rehearsal["source_evidence_refs"],
    "sources": sources,
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print("live_runner_readiness_view_ok=true")
    print("schema_version=1")
    print("readonly=true")
    print("binary_path=" + str(result["binary_path"] or "none"))
    print("binary_blocked_reason=" + str(result["binary_blocked_reason"] or "none"))
    print("workspace_root=" + str(result["workspace_root"]))
    print("config_path=" + str(result["config_path"] or "none"))
    print("runtime_report_surface.ok=" + str(runtime_report_surface["ok"]).lower())
    print("runtime_report_surface.artifact_count=" + str(runtime_report_surface["artifact_count"]))
    print("runtime_report_surface.observability_field_count=" + str(runtime_report_surface["observability_field_count"]))
    print("runtime_report_surface.artifact_locators=" + format_text_list(runtime_report_surface.get("artifact_locators", [])))
    print("runtime_report_surface.observability_fields=" + format_text_list(runtime_report_surface.get("observability_fields", [])))
    print("runtime_report_surface.blocked_reason=" + str(runtime_report_surface.get("blocked_reason", "none")))
    print("policy_tool_status.active_permission_profile=" + str(policy_tool_status.get("active_permission_profile", "unknown")))
    print("policy_tool_status.ga_tool_descriptors=" + str(policy_tool_status.get("ga_tool_descriptor_mapped_count", 0)) + "/" + str(policy_tool_status.get("tool_descriptor_count", 0)))
    print("policy_tool_status.missing=" + format_text_list(policy_tool_status.get("ga_tool_descriptor_missing", [])))
    print("policy_tool_status.blocked_reason=" + str(policy_tool_status.get("blocked_reason", "none")))
    print("live_readiness.ok=" + str(live_readiness.get("ok", False)).lower())
    print("live_readiness.state=" + str(live_readiness.get("overall_state", "unknown")))
    print("live_readiness.ga_local_mapped_only=" + str(live_readiness.get("ga_local_mapped_only", False)).lower())
    print("live_readiness.desktop_browser_live_gated=" + str(live_readiness.get("desktop_browser_live_gated", False)).lower())
    print("live_readiness.browser_worker_frozen=" + str(live_readiness.get("browser_worker_frozen", False)).lower())
    print("live_readiness.live_worker_available=" + str(live_readiness.get("live_worker_available", False)).lower())
    print("live_readiness.real_external_acceptance_pending=" + str(live_readiness.get("real_external_acceptance_pending", False)).lower())
    print("live_readiness.provider_live_request_verified_by_status=" + str(live_readiness.get("provider_live_request_verified_by_status", False)).lower())
    print("live_readiness.ready_does_not_mean_live=" + str(live_readiness.get("ready_does_not_mean_live", False)).lower())
    print("live_runner_rehearsal.state=" + str(live_runner_rehearsal["state"]))
    print("live_runner_rehearsal.ready_for_live=" + str(live_runner_rehearsal["ready_for_live"]).lower())
    print("live_runner_rehearsal.starts_external_worker=" + str(live_runner_rehearsal["starts_external_worker"]).lower())
    print("live_runner_rehearsal.capability_mismatch_blocks_live=" + str(live_runner_rehearsal["capability_mismatch_blocks_live"]).lower())
    print("live_runner_rehearsal.blocked_reason=" + str(live_runner_rehearsal["blocked_reason"]))
    print("live_runner_rehearsal.next_action=" + str(live_runner_rehearsal["next_action"]))
    for name, ref in result["source_evidence_refs"].items():
        print(f"source_evidence_ref.{name}={ref}")
PY
