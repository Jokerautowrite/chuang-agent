#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMAT="text"
LIVE_MODE="0"

usage() {
  cat <<'USAGE'
usage: scripts/chuang-global-live-verify.sh [--json] [--live]

Run the canonical 7-slot global real-live receipt verification.
Default: read-only rehearsal aggregation (provider/desktop stay blocked).
--live: also run provider live request and desktop real execution (operator
        opt-in equivalent). Requires CHUANG_REAL_ACTUATOR_ENABLE=1.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      FORMAT="json"
      ;;
    --live)
      LIVE_MODE="1"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ "$LIVE_MODE" = "1" ]; then
  if [ "${CHUANG_REAL_ACTUATOR_ENABLE:-}" != "1" ]; then
    printf '%s\n' "error: --live requires CHUANG_REAL_ACTUATOR_ENABLE=1" >&2
    exit 2
  fi
  export CHUANG_GLOBAL_RECEIPT_INCLUDE_PROVIDER_LIVE=1
  export CHUANG_GLOBAL_RECEIPT_INCLUDE_DESKTOP_LIVE=1
fi

set +e
RAW_AGGREGATION="$(
  cd "$ROOT" &&
    bash scripts/chuang-global-real-live-receipt.sh --json
)"
AGGREGATION_STATUS=$?
set -e

if [ "$FORMAT" = "json" ]; then
  if [ -n "$RAW_AGGREGATION" ]; then
    printf '%s\n' "$RAW_AGGREGATION"
  fi
  set +e
  RAW_AGGREGATION="$RAW_AGGREGATION" python3 - <<'PY'
import json
import os
import sys

try:
    receipt = json.loads(os.environ.get("RAW_AGGREGATION", ""))
except json.JSONDecodeError:
    sys.exit(1)

slots = {
    "feishu",
    "provider",
    "subagent_live_rehearsal",
    "desktop",
    "browser",
    "wiki",
    "gbrain",
}
services = receipt.get("service_receipts")
verified = {
    item.get("id")
    for item in services
    if isinstance(item, dict) and item.get("status") == "verified"
} if isinstance(services, list) else set()
sys.exit(0 if verified == slots else 1)
PY
  VERIFY_STATUS=$?
  set -e
  if [ "$AGGREGATION_STATUS" -ne 0 ] && [ "$VERIFY_STATUS" -eq 0 ]; then
    exit 1
  fi
  exit "$VERIFY_STATUS"
else
  set +e
  RAW_AGGREGATION="$RAW_AGGREGATION" python3 - <<'PY'
import json
import os
import sys

CANONICAL_SLOTS = [
    "feishu",
    "provider",
    "subagent_live_rehearsal",
    "desktop",
    "browser",
    "wiki",
    "gbrain",
]

try:
    receipt = json.loads(os.environ.get("RAW_AGGREGATION", ""))
except json.JSONDecodeError as exc:
    print(f"GLOBAL_LIVE_VERIFY_BLOCKED")
    print(f"blockers:")
    print(f"- aggregation: invalid_json:{exc.msg}")
    sys.exit(1)

receipts = receipt.get("service_receipts")
by_id = {
    item.get("id"): item
    for item in receipts
    if isinstance(item, dict) and item.get("id")
} if isinstance(receipts, list) else {}

verified = 0
blockers = []
print("GLOBAL LIVE VERIFY SUMMARY")
for slot in CANONICAL_SLOTS:
    item = by_id.get(slot)
    status = item.get("status") if isinstance(item, dict) else None
    if status == "verified":
        verified += 1
        print(f"- {slot}: verified")
    else:
        print(f"- {slot}: blocked")

        if isinstance(item, dict):
            reason = item.get("blocked_reason") or item.get("reason")
        else:
            reason = None
        blockers.append(f"{slot}: {reason or 'service_receipt_not_verified'}")

receipt_blockers = receipt.get("blockers")
for blocker in receipt_blockers if isinstance(receipt_blockers, list) else []:
    blocker = str(blocker).strip()
    if blocker:
        blockers.append(blocker)

blockers = list(dict.fromkeys(blockers))

print(f"verified: {verified}/{len(CANONICAL_SLOTS)}")
print("blockers:")
if blockers:
    for blocker in blockers:
        print(f"- {blocker}")
else:
    print("- none")

if verified == len(CANONICAL_SLOTS):
    print("GLOBAL_LIVE_VERIFY_OK")
    sys.exit(0)

print("GLOBAL_LIVE_VERIFY_BLOCKED")
sys.exit(1)
PY
  SUMMARY_STATUS=$?
  set -e
  if [ "$AGGREGATION_STATUS" -ne 0 ] && [ "$SUMMARY_STATUS" -eq 0 ]; then
    exit 1
  fi
  exit "$SUMMARY_STATUS"
fi
