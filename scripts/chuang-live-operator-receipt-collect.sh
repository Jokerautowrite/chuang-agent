#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"
BASE_PATH=""
OVERLAY_PATHS=()

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-operator-receipt-collect.sh --base PATH [--overlay PATH ...] [--json]

Readonly overlay/merge collector for a manual Chuang live receipt.
The collector only merges local JSON artifacts. It does not connect to
Feishu, provider, desktop, browser, wiki, GBrain, or any service.

Arguments:
  --base PATH        base operator receipt JSON to overlay
  --overlay PATH     optional overlay JSON fragment; repeatable
  --json             emit JSON output
  -h, --help         show this message

Readonly boundaries:
  connects_real_feishu=false
  sends_feishu_messages=false
  connects_real_provider=false
  starts_workers=false
  dispatches_tasks=false
  performs_desktop_actions=false
  performs_browser_actions=false
  connects_real_wiki=false
  connects_real_gbrain=false
  reads_secret_values=false
  prints_secret_values=false
  starts_services=false
  stops_services=false
  touches_services=false
  modifies_repo=false
  deletes_files=false
  reuses_codex_or_hermes_credentials=false
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    --base)
      shift
      if [ "$#" -eq 0 ]; then
        echo "missing value for --base" >&2
        usage >&2
        exit 2
      fi
      BASE_PATH="$1"
      ;;
    --overlay)
      shift
      if [ "$#" -eq 0 ]; then
        echo "missing value for --overlay" >&2
        usage >&2
        exit 2
      fi
      OVERLAY_PATHS+=("$1")
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

if [ -z "$BASE_PATH" ]; then
  echo "missing required --base PATH" >&2
  usage >&2
  exit 2
fi

python3 - "$FORMAT" "$BASE_PATH" "${OVERLAY_PATHS[@]}" <<'PY'
import copy
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

FORMAT = sys.argv[1] if len(sys.argv) > 1 else "text"
BASE_PATH = Path(sys.argv[2])
OVERLAY_PATHS = [Path(arg) for arg in sys.argv[3:]]

if not BASE_PATH.exists():
    raise SystemExit(f"base receipt not found: {BASE_PATH}")

CANONICAL_SERVICE_IDS = [
    "feishu",
    "provider",
    "subagent_live_rehearsal",
    "desktop",
    "browser",
    "wiki",
    "gbrain",
]


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


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
        return copy.deepcopy(overlay)
    return copy.deepcopy(overlay)


def merge_service_receipts(base_items, overlay_items):
    if not isinstance(base_items, list):
        raise ValueError("service_receipts must be an array")
    if not isinstance(overlay_items, list):
        raise ValueError("service_receipts overlay must be an array")

    base_by_id = {}
    for item in base_items:
        if not isinstance(item, dict):
            raise ValueError("service_receipts entries must be objects")
        service_id = item.get("id")
        if service_id in base_by_id:
            raise ValueError(f"duplicate service_receipts id: {service_id}")
        base_by_id[service_id] = item

    overlay_by_id = {}
    overlay_ids = []
    for item in overlay_items:
        if not isinstance(item, dict):
            raise ValueError("service_receipts overlay entries must be objects")
        service_id = item.get("id")
        overlay_ids.append(service_id)
        overlay_by_id[service_id] = item

    if overlay_ids != CANONICAL_SERVICE_IDS:
        raise ValueError(
            "service_receipts overlay ids must match the canonical 7-slot order"
        )

    merged = []
    for service_id in CANONICAL_SERVICE_IDS:
        merged.append(deep_merge(base_by_id[service_id], overlay_by_id[service_id]))
    return merged


def validate_service_alignment(receipt):
    service_receipts = receipt.get("service_receipts")
    if service_receipts is not None:
        ids = [item.get("id") for item in service_receipts]
        if ids != CANONICAL_SERVICE_IDS:
            raise ValueError("service_receipts ids must stay aligned with the 7-slot order")

    service_evidence = receipt.get("service_evidence")
    if service_evidence is not None:
        keys = list(service_evidence.keys())
        if keys != CANONICAL_SERVICE_IDS:
            raise ValueError("service_evidence keys must stay aligned with the 7-slot order")

    real_live = receipt.get("real_live_acceptance")
    if isinstance(real_live, dict):
        services = real_live.get("services")
        if services is not None:
            ids = [item.get("id") for item in services]
            if ids != CANONICAL_SERVICE_IDS:
                raise ValueError(
                    "real_live_acceptance.services ids must stay aligned with the 7-slot order"
                )


def canonicalize_service_evidence(receipt):
    service_evidence = receipt.get("service_evidence")
    if service_evidence is None:
        return
    if not isinstance(service_evidence, dict):
        raise ValueError("service_evidence must be an object")
    evidence_ids = list(service_evidence.keys())
    if set(evidence_ids) != set(CANONICAL_SERVICE_IDS):
        raise ValueError("service_evidence keys must stay aligned with the 7-slot order")
    receipt["service_evidence"] = {
        service_id: service_evidence[service_id] for service_id in CANONICAL_SERVICE_IDS
    }


base = load_json(BASE_PATH)
if not isinstance(base, dict):
    raise SystemExit("base receipt must be a JSON object")

merged = copy.deepcopy(base)
for overlay_path in OVERLAY_PATHS:
    overlay = load_json(overlay_path)
    if not isinstance(overlay, dict):
        raise SystemExit(f"overlay receipt must be a JSON object: {overlay_path}")

    if "service_receipts" in overlay:
        merged["service_receipts"] = merge_service_receipts(
            merged.get("service_receipts", []), overlay["service_receipts"]
        )

    merged = deep_merge(merged, {k: v for k, v in overlay.items() if k != "service_receipts"})

canonicalize_service_evidence(merged)
validate_service_alignment(merged)

merged["schema_version"] = merged.get("schema_version", 1)
merged["receipt_kind"] = "live_operator_receipt_collected"
merged["tested_at"] = datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")
merged["readonly"] = True
merged["collect_overlay_count"] = len(OVERLAY_PATHS)
merged["collect_source"] = "scripts/chuang-live-operator-receipt-collect.sh"
merged["collect_mode"] = "overlay_merge"
merged["collect_can_connect_real_services"] = False

if FORMAT == "json":
    print(json.dumps(merged, ensure_ascii=False, indent=2, sort_keys=False))
else:
    service_ids = ",".join(CANONICAL_SERVICE_IDS)
    print("live_operator_receipt_collect: readonly=true overlay_merge=true")
    print(f"base={BASE_PATH}")
    print(f"overlay_count={len(OVERLAY_PATHS)}")
    print(f"service_ids={service_ids}")
    print(f"receipt_kind={merged['receipt_kind']}")
PY
