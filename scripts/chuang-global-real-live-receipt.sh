#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"
ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
BASE_FILE=""
INCLUDE_PROVIDER_LIVE="${CHUANG_GLOBAL_RECEIPT_INCLUDE_PROVIDER_LIVE:-0}"
WORK_DIR="${CHUANG_GLOBAL_RECEIPT_WORK_DIR:-}"

FEISHU_FILE=""
PROVIDER_FILE=""
SUBAGENT_FILE=""
DESKTOP_FILE=""
BROWSER_FILE=""
WIKI_FILE=""
GBRAIN_FILE=""

usage() {
  cat <<'USAGE'
usage: scripts/chuang-global-real-live-receipt.sh [--json] [--base-file PATH]
       [--feishu-file PATH] [--provider-file PATH] [--subagent-file PATH]
       [--desktop-file PATH] [--browser-file PATH] [--wiki-file PATH] [--gbrain-file PATH]
       [--include-provider-live]

Collect or consume per-service live receipts, map them to the canonical 7-slot
global receipt overlay, then delegate final readiness derivation to
chuang-live-operator-receipt-collect.sh.

Default boundaries:
  provider live request is not executed unless --include-provider-live or
  CHUANG_GLOBAL_RECEIPT_INCLUDE_PROVIDER_LIVE=1 is set.
  desktop dry-run rehearsal is not promoted to real_execution=true.
  no service restart, no repository mutation, no file deletion.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    --base-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --base-file" >&2
        exit 2
      fi
      BASE_FILE="$2"
      shift
      ;;
    --base-file=*)
      BASE_FILE="${1#--base-file=}"
      ;;
    --feishu-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --feishu-file" >&2
        exit 2
      fi
      FEISHU_FILE="$2"
      shift
      ;;
    --feishu-file=*)
      FEISHU_FILE="${1#--feishu-file=}"
      ;;
    --provider-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --provider-file" >&2
        exit 2
      fi
      PROVIDER_FILE="$2"
      shift
      ;;
    --provider-file=*)
      PROVIDER_FILE="${1#--provider-file=}"
      ;;
    --subagent-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --subagent-file" >&2
        exit 2
      fi
      SUBAGENT_FILE="$2"
      shift
      ;;
    --subagent-file=*)
      SUBAGENT_FILE="${1#--subagent-file=}"
      ;;
    --desktop-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --desktop-file" >&2
        exit 2
      fi
      DESKTOP_FILE="$2"
      shift
      ;;
    --desktop-file=*)
      DESKTOP_FILE="${1#--desktop-file=}"
      ;;
    --browser-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --browser-file" >&2
        exit 2
      fi
      BROWSER_FILE="$2"
      shift
      ;;
    --browser-file=*)
      BROWSER_FILE="${1#--browser-file=}"
      ;;
    --wiki-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --wiki-file" >&2
        exit 2
      fi
      WIKI_FILE="$2"
      shift
      ;;
    --wiki-file=*)
      WIKI_FILE="${1#--wiki-file=}"
      ;;
    --gbrain-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --gbrain-file" >&2
        exit 2
      fi
      GBRAIN_FILE="$2"
      shift
      ;;
    --gbrain-file=*)
      GBRAIN_FILE="${1#--gbrain-file=}"
      ;;
    --include-provider-live)
      INCLUDE_PROVIDER_LIVE="1"
      ;;
    --work-dir)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --work-dir" >&2
        exit 2
      fi
      WORK_DIR="$2"
      shift
      ;;
    --work-dir=*)
      WORK_DIR="${1#--work-dir=}"
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

if [ -z "$WORK_DIR" ]; then
  WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chuang-global-real-live-receipt.XXXXXX")"
else
  mkdir -p "$WORK_DIR"
fi

receipt_path() {
  printf '%s/%s.json' "$WORK_DIR" "$1"
}

run_receipt_script() {
  local output_file="$1"
  shift
  set +e
  "$@" >"$output_file" 2>"$output_file.stderr"
  local status=$?
  set -e
  printf '%s' "$status" >"$output_file.exit"
}

if [ -z "$FEISHU_FILE" ]; then
  FEISHU_FILE="$(receipt_path feishu)"
  run_receipt_script "$FEISHU_FILE" bash "$ROOT/scripts/chuang-feishu-live-receipt.sh" --json
fi

if [ -z "$PROVIDER_FILE" ]; then
  PROVIDER_FILE="$(receipt_path provider)"
  if [ "$INCLUDE_PROVIDER_LIVE" = "1" ]; then
    run_receipt_script "$PROVIDER_FILE" bash "$ROOT/scripts/chuang-provider-live-request-receipt.sh" --json
  else
    printf '%s\n' '{"schema_version":1,"receipt_kind":"provider_live_request_receipt","status":"blocked","ok":false,"blocked_reason":"provider_live_request_not_enabled","request_path":"<missing>","provider_response_ok":"<missing>","provider_fallback_used":"false","api_key_state":"<missing>","runtime_report_id":"<missing>"}' >"$PROVIDER_FILE"
    printf '%s' 0 >"$PROVIDER_FILE.exit"
  fi
fi

if [ -z "$SUBAGENT_FILE" ]; then
  SUBAGENT_FILE="$(receipt_path subagent)"
  run_receipt_script "$SUBAGENT_FILE" bash "$ROOT/scripts/chuang-live-runner-rehearsal-receipt.sh" --json
fi

if [ -z "$DESKTOP_FILE" ]; then
  DESKTOP_FILE="$(receipt_path desktop)"
  run_receipt_script "$DESKTOP_FILE" bash "$ROOT/scripts/chuang-desktop-action-rehearsal-receipt.sh" --json
fi

if [ -z "$BROWSER_FILE" ]; then
  BROWSER_FILE="$(receipt_path browser)"
  run_receipt_script "$BROWSER_FILE" bash "$ROOT/scripts/chuang-browser-read-live-receipt.sh" --json
fi

if [ -z "$WIKI_FILE" ]; then
  WIKI_FILE="$(receipt_path wiki)"
  run_receipt_script "$WIKI_FILE" bash "$ROOT/scripts/chuang-wiki-live-receipt.sh" --json
fi

if [ -z "$GBRAIN_FILE" ]; then
  GBRAIN_FILE="$(receipt_path gbrain)"
  run_receipt_script "$GBRAIN_FILE" bash "$ROOT/scripts/chuang-gbrain-live-receipt.sh" --json
fi

OVERLAY_FILE="$WORK_DIR/global-overlay.json"
export FEISHU_FILE PROVIDER_FILE SUBAGENT_FILE DESKTOP_FILE BROWSER_FILE WIKI_FILE GBRAIN_FILE
export OVERLAY_FILE WORK_DIR

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone
from pathlib import Path

SERVICE_IDS = [
    "feishu",
    "provider",
    "subagent_live_rehearsal",
    "desktop",
    "browser",
    "wiki",
    "gbrain",
]


def load(path):
    file_path = Path(path)
    try:
        return json.loads(file_path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        return {
            "acceptance_status": "blocked",
            "status": "blocked",
            "blockers": [f"receipt_json_unavailable:{type(exc).__name__}"],
            "receipt_file": str(file_path),
        }


def text(value, default="<missing>"):
    value = str(value or "").strip()
    return value if value else default


def verified_service(service_id, evidence):
    return {
        "id": service_id,
        "status": "verified",
        "evidence": evidence,
    }


def blocked_service(service_id, evidence):
    return {
        "id": service_id,
        "status": "blocked",
        "evidence": evidence,
    }


def acceptance(service_id, is_verified):
    return {
        "id": service_id,
        "completion_state": "verified" if is_verified else "blocked",
        "manual_live_required": not is_verified,
        "must_not_count_as_complete": not is_verified,
    }


def receipt_ref(service, receipt, suffix):
    request_id = text(receipt.get("request_id") or receipt.get("runtime_report_id"), "unknown")
    return f"receipt://{service}/{request_id}/{suffix}"


def has_no_blockers(receipt):
    blockers = receipt.get("blockers")
    return not isinstance(blockers, list) or len(blockers) == 0


def map_feishu(receipt):
    ok = receipt.get("receipt_kind") == "feishu_live_readonly_receipt"
    ok = ok and receipt.get("acceptance_status") == "verified" and has_no_blockers(receipt)
    evidence = {
        "health_transcript_ref": receipt_ref("feishu", receipt, "preflight"),
        "session_transcript_ref": receipt_ref("feishu", receipt, "session-state"),
        "tools_or_capabilities_transcript_ref": receipt_ref("feishu", receipt, "capabilities"),
        "normal_message_transcript_ref": receipt_ref("feishu", receipt, "event-log"),
        "runtime_report_id": text(receipt.get("request_id"), "feishu-live-receipt"),
    }
    return ok, evidence


def map_provider(receipt):
    fallback = str(receipt.get("provider_fallback_used", "")).lower()
    response_ok = str(receipt.get("provider_response_ok", "")).lower()
    ok = receipt.get("status") == "verified" or receipt.get("ok") is True
    ok = (
        ok
        and receipt.get("request_path") == "/v1/responses"
        and response_ok == "true"
        and fallback == "false"
        and receipt.get("api_key_state") == "<set>"
    )
    evidence = {
        "provider_kind": text(receipt.get("provider_kind")),
        "transport": text(receipt.get("transport_mode") or receipt.get("transport")),
        "api_key_state": text(receipt.get("api_key_state")),
        "provider_live_request_receipt_ref": receipt_ref("provider", receipt, "live-request"),
        "runtime_report_id": text(receipt.get("runtime_report_id")),
        "does_not_call_provider": False,
        "does_not_read_provider_readiness": False,
    }
    return ok, evidence


def map_subagent(receipt):
    real = receipt.get("real_live_acceptance") if isinstance(receipt.get("real_live_acceptance"), dict) else {}
    dispatch = receipt.get("dispatch") if isinstance(receipt.get("dispatch"), dict) else {}
    worker = receipt.get("worker_execution") if isinstance(receipt.get("worker_execution"), dict) else {}
    collect = receipt.get("collect") if isinstance(receipt.get("collect"), dict) else {}
    run_ids = worker.get("run_ids") if isinstance(worker.get("run_ids"), list) else []
    ok = receipt.get("receipt_kind") == "single_worker_rehearsal_live_receipt"
    ok = ok and real.get("single_worker_rehearsal_complete") is True
    ok = ok and collect.get("admission_status") == "Accepted" and has_no_blockers(receipt)
    evidence = {
        "dispatch_id": text(dispatch.get("run_id") or dispatch.get("task_id")),
        "worker_id": text(run_ids[0] if run_ids else collect.get("report_id")),
        "gate_receipt_ref": receipt_ref("subagent", receipt, "live-gate"),
        "allowlist_receipt_ref": receipt_ref("subagent", receipt, "allowlist"),
        "capability_routing_ref": receipt_ref("subagent", receipt, "capability-routing"),
        "report_admission_ref": receipt_ref("subagent", receipt, "report-admission"),
    }
    return ok, evidence


def map_desktop(receipt):
    real_execution = receipt.get("real_execution")
    ok = receipt.get("receipt_kind") in {
        "desktop_action_rehearsal_receipt",
        "desktop_action_live_receipt",
    }
    ok = ok and (real_execution is True or real_execution == "true") and has_no_blockers(receipt)
    evidence = {
        "audit_label": text(receipt.get("audit_label")),
        "action_receipt_ref": receipt_ref("desktop", receipt, "action"),
        "governance_receipt_ref": receipt_ref("desktop", receipt, "governance"),
        "real_execution": "true" if ok else "false",
    }
    return ok, evidence


def map_browser(receipt):
    evidence_source = receipt.get("browser_read_evidence")
    if not isinstance(evidence_source, dict):
        evidence_source = {}
    ok = receipt.get("receipt_kind") == "browser_read_live_readonly_receipt"
    ok = ok and receipt.get("acceptance_status") == "verified" and has_no_blockers(receipt)
    evidence = {
        "adapter_manifest_ref": receipt_ref("browser", receipt, "adapter-manifest"),
        "session_scope_ref": receipt_ref("browser", receipt, "session-scope"),
        "browser_snapshot_or_transcript_ref": receipt_ref("browser", receipt, "snapshot"),
        "report_admission_ref": receipt_ref("browser", receipt, "report-admission"),
    }
    if evidence_source.get("adapter_kind"):
        evidence["adapter_kind"] = text(evidence_source.get("adapter_kind"))
    return ok, evidence


def map_knowledge(receipt, service_id):
    expected_kind = f"{service_id}_live_readonly_receipt"
    ok = receipt.get("receipt_kind") == expected_kind
    ok = ok and receipt.get("acceptance_status") == "verified"
    ok = ok and receipt.get("request_sent") is True
    ok = ok and receipt.get("read_only") is True
    ok = ok and receipt.get("writes_automatically") is False
    ok = ok and has_no_blockers(receipt)
    evidence = {
        "source_contract_ref": receipt_ref(service_id, receipt, "source-contract"),
        "query_receipt_ref": receipt_ref(service_id, receipt, "query"),
        "provenance_ref": receipt_ref(service_id, receipt, "provenance"),
        "writes_core_memory": False,
    }
    return ok, evidence


receipts = {
    "feishu": load(os.environ["FEISHU_FILE"]),
    "provider": load(os.environ["PROVIDER_FILE"]),
    "subagent_live_rehearsal": load(os.environ["SUBAGENT_FILE"]),
    "desktop": load(os.environ["DESKTOP_FILE"]),
    "browser": load(os.environ["BROWSER_FILE"]),
    "wiki": load(os.environ["WIKI_FILE"]),
    "gbrain": load(os.environ["GBRAIN_FILE"]),
}

mappers = {
    "feishu": map_feishu,
    "provider": map_provider,
    "subagent_live_rehearsal": map_subagent,
    "desktop": map_desktop,
    "browser": map_browser,
    "wiki": lambda receipt: map_knowledge(receipt, "wiki"),
    "gbrain": lambda receipt: map_knowledge(receipt, "gbrain"),
}

service_evidence = {}
service_receipts = []
services = []
blockers = []

for service_id in SERVICE_IDS:
    receipt = receipts[service_id]
    ok, evidence = mappers[service_id](receipt)
    service_evidence[service_id] = evidence
    service_receipts.append(
        verified_service(service_id, evidence) if ok else blocked_service(service_id, evidence)
    )
    services.append(acceptance(service_id, ok))
    if not ok:
        reason = receipt.get("blocked_reason")
        source_blockers = receipt.get("blockers")
        if isinstance(source_blockers, list) and source_blockers:
            reason = ",".join(str(item) for item in source_blockers)
        blockers.append(f"{service_id}: {text(reason, 'source_receipt_not_verified')}")

overlay = {
    "schema_version": 1,
    "receipt_kind": "global_real_live_receipt_overlay",
    "tested_at": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
    "workspace_root": os.environ.get("CHUANG_AGENT_ROOT", "/home/user/projects/chuang-agent"),
    "collect_mode": "global_receipt_aggregation",
    "source_receipt_files": {
        "feishu": os.environ["FEISHU_FILE"],
        "provider": os.environ["PROVIDER_FILE"],
        "subagent_live_rehearsal": os.environ["SUBAGENT_FILE"],
        "desktop": os.environ["DESKTOP_FILE"],
        "browser": os.environ["BROWSER_FILE"],
        "wiki": os.environ["WIKI_FILE"],
        "gbrain": os.environ["GBRAIN_FILE"],
    },
    "service_evidence": service_evidence,
    "service_receipts": service_receipts,
    "real_live_acceptance": {
        "complete": False,
        "status": "not_verified",
        "services": services,
    },
    "blockers": blockers,
}

Path(os.environ["OVERLAY_FILE"]).write_text(
    json.dumps(overlay, ensure_ascii=False, indent=2, sort_keys=True),
    encoding="utf-8",
)
PY

collector_args=(--overlay-file "$OVERLAY_FILE")
if [ -n "$BASE_FILE" ]; then
  collector_args=(--base-file "$BASE_FILE" "${collector_args[@]}")
fi
if [ "$FORMAT" = "json" ]; then
  collector_args=(--json "${collector_args[@]}")
fi

bash "$ROOT/scripts/chuang-live-operator-receipt-collect.sh" "${collector_args[@]}"
