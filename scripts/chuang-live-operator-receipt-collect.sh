#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"
BASE_FILE="-"
OVERLAY_FILES=()

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-operator-receipt-collect.sh [--json] [--base-file PATH] [--overlay-file PATH ...]

Readonly local receipt collector for operator live receipts.

Input:
  base receipt JSON from stdin or --base-file PATH
  overlay receipt JSON from repeated --overlay-file PATH
  use "-" as --base-file to read base JSON from stdin

Boundaries:
  readonly=true
  connects_real_feishu=false
  sends_feishu_messages=false
  connects_real_provider=false
  reads_secret_values=false
  prints_secret_values=false
  starts_services=false
  touches_services=false
  modifies_repo=false
  deletes_files=false
  can_mark_real_live_ready=derived_from_complete_canonical_evidence
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    --base-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --base-file" >&2
        usage >&2
        exit 2
      fi
      BASE_FILE="$2"
      shift
      ;;
    --base-file=*)
      BASE_FILE="${1#--base-file=}"
      ;;
    --overlay-file)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --overlay-file" >&2
        usage >&2
        exit 2
      fi
      OVERLAY_FILES+=("$2")
      shift
      ;;
    --overlay-file=*)
      OVERLAY_FILES+=("${1#--overlay-file=}")
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

ROOT="${CHUANG_AGENT_ROOT:-/home/user/projects/chuang-agent}"
ENV_FILE="${CHUANG_LIVE_ENV_FILE:-${CHUANG_LIVE_OPERATOR_ENV_FILE:-${CHUANG_FEISHU_ENV_FILE:-/home/user/.codex-im/chuang-feishu-bridge.env}}}"
OPERATOR="${CHUANG_LIVE_OPERATOR:-${USER:-<operator>}}"
REQUEST_ID="${CHUANG_LIVE_REQUEST_ID:-<fill_request_id>}"

if [ "$BASE_FILE" = "-" ]; then
  if [ -t 0 ]; then
    BASE_JSON=""
  else
    BASE_JSON="$(cat)"
  fi
else
  BASE_JSON=""
fi

if [ "${#OVERLAY_FILES[@]}" -gt 0 ]; then
  OVERLAY_FILES_JOINED="$(printf '%s\n' "${OVERLAY_FILES[@]}")"
else
  OVERLAY_FILES_JOINED=""
fi

export FORMAT ROOT ENV_FILE OPERATOR REQUEST_ID BASE_FILE BASE_JSON
export OVERLAY_FILES_JOINED

python3 - <<'PY'
import copy
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

FORMAT = os.environ["FORMAT"]
ROOT = os.environ["ROOT"]
ENV_FILE = os.environ["ENV_FILE"]
OPERATOR = os.environ["OPERATOR"]
REQUEST_ID = os.environ["REQUEST_ID"]
BASE_FILE = os.environ["BASE_FILE"]
BASE_JSON = os.environ["BASE_JSON"].strip()
OVERLAY_FILES = [item for item in os.environ.get("OVERLAY_FILES_JOINED", "").splitlines() if item]

BOUNDARIES = {
    "readonly": True,
    "connects_real_feishu": False,
    "sends_feishu_messages": False,
    "connects_real_provider": False,
    "starts_workers": False,
    "dispatches_tasks": False,
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
}

SERVICE_DEFINITIONS = [
    {
        "id": "feishu",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "health_transcript_ref": "<fill_after_test>",
            "session_transcript_ref": "<fill_after_test>",
            "tools_or_capabilities_transcript_ref": "<fill_after_test>",
            "normal_message_transcript_ref": "<fill_after_test>",
            "runtime_report_id": "<fill_after_test>",
        },
        "required": [
            "/health transcript with redacted secret states",
            "/session transcript with active chat/thread binding",
            "/tools or /capabilities boundary transcript",
            "normal non-secret text reply with runtime report id when applicable",
        ],
    },
    {
        "id": "provider",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "provider_kind": "<fill_after_test>",
            "transport": "<fill_after_test>",
            "api_key_state": "<set|missing>",
            "provider_live_request_receipt_ref": "<fill_after_test>",
            "runtime_report_id": "<fill_after_test>",
            "does_not_call_provider": True,
            "does_not_read_provider_readiness": True,
        },
        "required": [
            "provider transport is not stub/fake",
            "api_key_state is recorded only as <set>/<missing>",
            "provider live request receipt ref or runtime report id exists",
            "no fake-responder fallback",
        ],
    },
    {
        "id": "subagent_live_rehearsal",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "dispatch_id": "<fill_after_test>",
            "worker_id": "<fill_after_test>",
            "gate_receipt_ref": "<fill_after_test>",
            "allowlist_receipt_ref": "<fill_after_test>",
            "capability_routing_ref": "<fill_after_test>",
            "report_admission_ref": "<fill_after_test>",
        },
        "required": [
            "single worker only",
            "gate receipt is explicit",
            "allowlist receipt is explicit",
            "capability routing receipt is explicit",
            "report admission receipt or blocked reason is explicit",
        ],
    },
    {
        "id": "desktop",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "audit_label": "<fill_after_test>",
            "action_receipt_ref": "<fill_after_test>",
            "governance_receipt_ref": "<fill_after_test>",
            "real_execution": "<true|false|not_attempted>",
        },
        "required": [
            "exact desktop action approved by governance",
            "audit label and action receipt exist",
            "real_execution=true only in an external audited receipt",
        ],
    },
    {
        "id": "browser",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "adapter_manifest_ref": "<fill_after_test>",
            "session_scope_ref": "<fill_after_test>",
            "browser_snapshot_or_transcript_ref": "<fill_after_test>",
            "report_admission_ref": "<fill_after_test>",
        },
        "required": [
            "audited adapter manifest exists",
            "browser/session scope is explicit",
            "URL/title/DOM or transcript evidence is referenced",
        ],
    },
    {
        "id": "wiki",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "source_contract_ref": "<fill_after_test>",
            "query_receipt_ref": "<fill_after_test>",
            "provenance_ref": "<fill_after_test>",
            "writes_core_memory": False,
        },
        "required": [
            "read-only source contract is explicit",
            "retrieval provenance is visible",
            "no automatic core-memory write",
        ],
    },
    {
        "id": "gbrain",
        "status": "<not_verified|verified|blocked>",
        "evidence": {
            "source_contract_ref": "<fill_after_test>",
            "query_receipt_ref": "<fill_after_test>",
            "provenance_ref": "<fill_after_test>",
            "writes_core_memory": False,
        },
        "required": [
            "read-only source contract is explicit",
            "retrieval provenance is visible",
            "no automatic core-memory write",
        ],
    },
]

CANONICAL_SERVICE_IDS = [item["id"] for item in SERVICE_DEFINITIONS]
SERVICE_STATUS_BY_ID = {item["id"]: item["status"] for item in SERVICE_DEFINITIONS}
SERVICE_REQUIRED_BY_ID = {item["id"]: list(item["required"]) for item in SERVICE_DEFINITIONS}
SERVICE_EVIDENCE_BY_ID = {item["id"]: copy.deepcopy(item["evidence"]) for item in SERVICE_DEFINITIONS}
REAL_LIVE_SERVICE_BY_ID = {
    item["id"]: {
        "id": item["id"],
        "completion_state": "not_verified",
        "manual_live_required": True,
        "must_not_count_as_complete": True,
        "required": list(item["required"]),
    }
    for item in SERVICE_DEFINITIONS
}
TOP_LEVEL_OVERRIDE_KEYS = [
    "tested_at",
    "request_id",
    "operator",
    "approval_scope",
    "rollback_condition",
    "preflight_status",
    "health_status",
    "new_thread_status",
    "session_status",
    "runtime_report_id",
    "provider_status",
]
_MISSING = object()
ALLOWED_REDACTED_VALUES = {"<set>"}


def deep_merge(base, overlay):
    if isinstance(base, dict) and isinstance(overlay, dict):
        merged = copy.deepcopy(base)
        for key, value in overlay.items():
            if key in merged:
                merged[key] = deep_merge(merged[key], value)
            else:
                merged[key] = copy.deepcopy(value)
        return merged
    if isinstance(base, list) and isinstance(overlay, list):
        base_has_ids = all(isinstance(item, dict) and "id" in item for item in base)
        overlay_has_ids = all(isinstance(item, dict) and "id" in item for item in overlay)
        if base_has_ids and overlay_has_ids:
            merged = [copy.deepcopy(item) for item in base]
            index = {item["id"]: idx for idx, item in enumerate(merged)}
            for item in overlay:
                item_id = item["id"]
                if item_id in index:
                    merged[index[item_id]] = deep_merge(merged[index[item_id]], item)
                else:
                    merged.append(copy.deepcopy(item))
            return merged
        return copy.deepcopy(overlay)
    return copy.deepcopy(overlay)


def build_template():
    return {
        "schema_version": 1,
        "tested_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
        "request_id": REQUEST_ID,
        "operator": OPERATOR,
        "env_file": ENV_FILE,
        "workspace_root": ROOT,
        "approval_scope": "<fill_exact_live_scope>",
        "rollback_condition": "<fill_abort_or_rollback_condition>",
        "acceptance_status": "not_verified",
        "can_mark_real_live_ready": False,
        "cannot_mark_complete_without_operator_evidence": True,
        "preflight_status": "<fill_after_test>",
        "health_status": "<fill_after_test>",
        "new_thread_status": "<fill_after_test>",
        "session_status": "<fill_after_test>",
        "runtime_report_id": "<fill_after_test>",
        "provider_status": "<fill_after_test>",
        "readonly_boundaries": copy.deepcopy(BOUNDARIES),
        "service_evidence": {service_id: copy.deepcopy(evidence) for service_id, evidence in SERVICE_EVIDENCE_BY_ID.items()},
        "service_receipts": [
            {
                "id": service_id,
                "status": SERVICE_STATUS_BY_ID[service_id],
                "evidence": copy.deepcopy(SERVICE_EVIDENCE_BY_ID[service_id]),
                "required": list(SERVICE_REQUIRED_BY_ID[service_id]),
            }
            for service_id in CANONICAL_SERVICE_IDS
        ],
        "real_live_acceptance": {
            "complete": False,
            "status": "not_verified",
            "gap_count": len(CANONICAL_SERVICE_IDS),
            "cannot_mark_complete_from_template": True,
            "requires_operator_evidence": True,
            "services": [copy.deepcopy(REAL_LIVE_SERVICE_BY_ID[service_id]) for service_id in CANONICAL_SERVICE_IDS],
        },
        "codex_hermes_isolation": "<keep_codex_and_hermes_separate>",
        "notes": [],
        "blockers": [],
        "boundaries": copy.deepcopy(BOUNDARIES),
    }


def load_json_text(path):
    if path == "-":
        return BASE_JSON or None
    file_path = Path(path)
    if not file_path.is_file():
        print(f"receipt_collect_error: missing json file: {path}", file=sys.stderr)
        sys.exit(2)
    return file_path.read_text(encoding="utf-8")


def parse_json_source(path, label):
    text = load_json_text(path)
    if text is None or not text.strip():
        return None
    try:
        value = json.loads(text)
    except json.JSONDecodeError as exc:
        print(f"receipt_collect_error: invalid {label} json: {exc}", file=sys.stderr)
        sys.exit(2)
    if not isinstance(value, dict):
        print(f"receipt_collect_error: {label} json must be an object", file=sys.stderr)
        sys.exit(2)
    return value


def validate_service_items(items, label):
    if items is None:
        return {}
    if not isinstance(items, list):
        print(f"receipt_collect_error: {label} must be an array", file=sys.stderr)
        sys.exit(2)
    seen = {}
    extras = []
    for item in items:
        if not isinstance(item, dict):
            print(f"receipt_collect_error: {label} entries must be objects", file=sys.stderr)
            sys.exit(2)
        item_id = item.get("id")
        if not isinstance(item_id, str):
            print(f"receipt_collect_error: {label} entry missing string id", file=sys.stderr)
            sys.exit(2)
        if item_id in seen:
            print(f"receipt_collect_error: duplicate {label} id: {item_id}", file=sys.stderr)
            sys.exit(2)
        if item_id not in CANONICAL_SERVICE_IDS:
            extras.append(item_id)
        seen[item_id] = item
    if extras:
        print(
            "receipt_collect_error: unexpected service ids in "
            f"{label}: {', '.join(extras)}",
            file=sys.stderr,
        )
        sys.exit(2)
    return seen


def validate_service_evidence(value):
    if value is None:
        return {}
    if not isinstance(value, dict):
        print("receipt_collect_error: service_evidence must be an object", file=sys.stderr)
        sys.exit(2)
    extras = [key for key in value if key not in CANONICAL_SERVICE_IDS]
    if extras:
        print(
            "receipt_collect_error: unexpected service ids in service_evidence: "
            + ", ".join(extras),
            file=sys.stderr,
        )
        sys.exit(2)
    invalid = [key for key, entry in value.items() if not isinstance(entry, dict)]
    if invalid:
        print(
            "receipt_collect_error: service_evidence entries must be objects for ids: "
            + ", ".join(invalid),
            file=sys.stderr,
        )
        sys.exit(2)
    return value


def merge_non_default(base, overlay, defaults):
    if isinstance(base, dict) and isinstance(overlay, dict) and isinstance(defaults, dict):
        merged = copy.deepcopy(base)
        for key, value in overlay.items():
            default_value = defaults.get(key, _MISSING)
            if isinstance(value, dict) and isinstance(default_value, dict) and isinstance(merged.get(key), dict):
                merged[key] = merge_non_default(merged[key], value, default_value)
            elif default_value is _MISSING or value != default_value:
                merged[key] = copy.deepcopy(value)
        return merged
    if defaults is _MISSING or overlay != defaults:
        return copy.deepcopy(overlay)
    return copy.deepcopy(base)


def normalize_service_bundle(result):
    receipts_by_id = validate_service_items(result.get("service_receipts"), "service_receipts")
    evidence_by_id = validate_service_evidence(result.get("service_evidence"))
    acceptance = result.get("real_live_acceptance")
    if acceptance is None:
        acceptance = {}
    if not isinstance(acceptance, dict):
        print("receipt_collect_error: real_live_acceptance must be an object", file=sys.stderr)
        sys.exit(2)
    acceptance_services_by_id = validate_service_items(acceptance.get("services"), "real_live_acceptance.services")

    canonical_receipts = []
    canonical_evidence = {}
    canonical_services = []

    for service_id in CANONICAL_SERVICE_IDS:
        receipt = {
            "id": service_id,
            "status": SERVICE_STATUS_BY_ID[service_id],
            "evidence": copy.deepcopy(SERVICE_EVIDENCE_BY_ID[service_id]),
            "required": list(SERVICE_REQUIRED_BY_ID[service_id]),
        }
        if service_id in receipts_by_id:
            receipt = deep_merge(receipt, receipts_by_id[service_id])
        if service_id in evidence_by_id:
            receipt["evidence"] = merge_non_default(
                receipt["evidence"],
                evidence_by_id[service_id],
                SERVICE_EVIDENCE_BY_ID[service_id],
            )
        receipt["required"] = list(SERVICE_REQUIRED_BY_ID[service_id])

        service_entry = copy.deepcopy(REAL_LIVE_SERVICE_BY_ID[service_id])
        if service_id in acceptance_services_by_id:
            service_entry = deep_merge(service_entry, acceptance_services_by_id[service_id])
        service_entry["required"] = list(SERVICE_REQUIRED_BY_ID[service_id])

        canonical_receipts.append(receipt)
        canonical_evidence[service_id] = copy.deepcopy(receipt["evidence"])
        canonical_services.append(service_entry)

    result["service_receipts"] = canonical_receipts
    result["service_evidence"] = canonical_evidence
    acceptance["services"] = canonical_services
    result["real_live_acceptance"] = acceptance
    return result


def is_placeholder_value(value):
    if isinstance(value, str):
        if value in ALLOWED_REDACTED_VALUES:
            return False
        stripped = value.strip()
        return (
            "<fill" in stripped
            or stripped in {"<not_verified|verified|blocked>", "<true|false|not_attempted>"}
            or (stripped.startswith("<") and stripped.endswith(">"))
        )
    if isinstance(value, dict):
        return any(is_placeholder_value(item) for item in value.values())
    if isinstance(value, list):
        return any(is_placeholder_value(item) for item in value)
    return False


def has_non_placeholder_string(value):
    return isinstance(value, str) and value.strip() and not is_placeholder_value(value)


def evidence_schema_blockers(service_id, evidence):
    if not isinstance(evidence, dict):
        return ["evidence_not_object"]

    blockers = []

    def require_string(key):
        if not has_non_placeholder_string(evidence.get(key)):
            blockers.append(f"{key}_missing_or_placeholder")

    def require_exact(key, expected):
        if evidence.get(key) != expected:
            blockers.append(f"{key}_not_{expected}")

    def require_bool(key, expected):
        if evidence.get(key) is not expected:
            blockers.append(f"{key}_not_{str(expected).lower()}")

    def require_true(key):
        value = evidence.get(key)
        if value is not True and value != "true":
            blockers.append(f"{key}_not_true")

    if service_id == "feishu":
        for key in [
            "health_transcript_ref",
            "session_transcript_ref",
            "tools_or_capabilities_transcript_ref",
            "normal_message_transcript_ref",
            "runtime_report_id",
        ]:
            require_string(key)
    elif service_id == "provider":
        for key in [
            "provider_kind",
            "transport",
            "provider_live_request_receipt_ref",
            "runtime_report_id",
        ]:
            require_string(key)
        require_exact("api_key_state", "<set>")
        require_bool("does_not_call_provider", False)
        require_bool("does_not_read_provider_readiness", False)
    elif service_id == "subagent_live_rehearsal":
        for key in [
            "dispatch_id",
            "worker_id",
            "gate_receipt_ref",
            "allowlist_receipt_ref",
            "capability_routing_ref",
            "report_admission_ref",
        ]:
            require_string(key)
    elif service_id == "desktop":
        for key in ["audit_label", "action_receipt_ref", "governance_receipt_ref"]:
            require_string(key)
        require_true("real_execution")
    elif service_id == "browser":
        for key in [
            "adapter_manifest_ref",
            "session_scope_ref",
            "browser_snapshot_or_transcript_ref",
            "report_admission_ref",
        ]:
            require_string(key)
    elif service_id in {"wiki", "gbrain"}:
        for key in ["source_contract_ref", "query_receipt_ref", "provenance_ref"]:
            require_string(key)
        require_bool("writes_core_memory", False)
    else:
        blockers.append("unknown_canonical_service")

    return blockers


def evaluate_real_live_acceptance(result):
    blockers = []
    verified_service_count = 0
    acceptance = result["real_live_acceptance"]
    acceptance_services = {
        item["id"]: item for item in acceptance.get("services", []) if isinstance(item, dict)
    }

    for receipt in result["service_receipts"]:
        service_id = receipt["id"]
        service_blocked = False
        if receipt.get("status") != "verified":
            blockers.append(f"{service_id}: service_receipt_not_verified")
            service_blocked = True
        if is_placeholder_value(receipt.get("evidence", {})):
            blockers.append(f"{service_id}: evidence_has_template_placeholders")
            service_blocked = True
        schema_blockers = evidence_schema_blockers(service_id, receipt.get("evidence", {}))
        if schema_blockers:
            blockers.extend(f"{service_id}: {blocker}" for blocker in schema_blockers)
            service_blocked = True
        service_entry = acceptance_services.get(service_id, {})
        if service_entry.get("completion_state") != "verified":
            blockers.append(f"{service_id}: acceptance_not_verified")
            service_blocked = True
        if service_entry.get("manual_live_required") is True:
            blockers.append(f"{service_id}: manual_live_required")
            service_blocked = True
        if service_entry.get("must_not_count_as_complete") is True:
            blockers.append(f"{service_id}: must_not_count_as_complete")
            service_blocked = True
        if not service_blocked:
            verified_service_count += 1

    existing_blockers = result.get("blockers", [])
    if isinstance(existing_blockers, list):
        blockers.extend(str(item) for item in existing_blockers if str(item).strip())
    elif existing_blockers:
        blockers.append("top_level_blockers_not_array")

    unique_blockers = list(dict.fromkeys(blockers))
    complete = verified_service_count == len(CANONICAL_SERVICE_IDS) and not unique_blockers

    acceptance["gap_count"] = len(CANONICAL_SERVICE_IDS) - verified_service_count
    acceptance["complete"] = complete
    acceptance["status"] = "verified" if complete else "not_verified"
    acceptance["cannot_mark_complete_from_template"] = not complete
    acceptance["requires_operator_evidence"] = not complete
    for service in acceptance["services"]:
        service["manual_live_required"] = not complete
        service["must_not_count_as_complete"] = not complete

    result["acceptance_status"] = "verified" if complete else "not_verified"
    result["can_mark_real_live_ready"] = complete
    result["cannot_mark_complete_without_operator_evidence"] = not complete
    result["blockers"] = unique_blockers
    result["real_live_acceptance"] = acceptance
    return result


def normalize(result):
    defaults = build_template()
    for key, value in defaults.items():
        result.setdefault(key, copy.deepcopy(value))

    result["schema_version"] = 1
    result["acceptance_status"] = "not_verified"
    result["can_mark_real_live_ready"] = False
    result["cannot_mark_complete_without_operator_evidence"] = True
    result["readonly_boundaries"] = copy.deepcopy(BOUNDARIES)
    result["boundaries"] = copy.deepcopy(BOUNDARIES)
    result["real_live_acceptance"] = result.get("real_live_acceptance") or {}
    if not isinstance(result["real_live_acceptance"], dict):
        print("receipt_collect_error: real_live_acceptance must be an object", file=sys.stderr)
        sys.exit(2)

    normalized = normalize_service_bundle(result)
    normalized["schema_version"] = 1
    normalized["readonly_boundaries"] = copy.deepcopy(BOUNDARIES)
    normalized["boundaries"] = copy.deepcopy(BOUNDARIES)
    return evaluate_real_live_acceptance(normalized)


def merge_sources():
    result = build_template()
    top_level_overrides = {}
    base = parse_json_source(BASE_FILE, "base")
    if base is not None:
        result = deep_merge(result, base)
        for key in TOP_LEVEL_OVERRIDE_KEYS:
            if key in base:
                result[key] = copy.deepcopy(base[key])
                top_level_overrides[key] = copy.deepcopy(base[key])
    for overlay_file in OVERLAY_FILES:
        overlay = parse_json_source(overlay_file, f"overlay {overlay_file}")
        if overlay is not None:
            result = deep_merge(result, overlay)
            for key in TOP_LEVEL_OVERRIDE_KEYS:
                if key in overlay:
                    result[key] = copy.deepcopy(overlay[key])
                    top_level_overrides[key] = copy.deepcopy(overlay[key])
    return result, top_level_overrides


result, top_level_overrides = merge_sources()
result = normalize(result)
for key, value in top_level_overrides.items():
    result[key] = value

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2))
else:
    print(f"schema_version={result['schema_version']}")
    print(f"request_id={result['request_id']}")
    print(f"operator={result['operator']}")
    print(f"approval_scope={result['approval_scope']}")
    print(f"rollback_condition={result['rollback_condition']}")
    print(f"acceptance_status={result['acceptance_status']}")
    print(f"can_mark_real_live_ready={str(result['can_mark_real_live_ready']).lower()}")
    for key, value in result["readonly_boundaries"].items():
        print(f"readonly_boundaries.{key}={str(value).lower()}")
    print("service_receipts=" + ",".join(item["id"] for item in result["service_receipts"]))
    print("real_live_acceptance.complete=" + str(result["real_live_acceptance"]["complete"]).lower())
    print("real_live_acceptance.status=" + result["real_live_acceptance"]["status"])
PY
