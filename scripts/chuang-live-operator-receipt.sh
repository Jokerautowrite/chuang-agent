#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-operator-receipt.sh [--json]

Readonly receipt template for a manual Chuang live test.

Environment overrides:
  CHUANG_LIVE_OPERATOR      operator name to record
  CHUANG_AGENT_ROOT         Chuang repo root
  CHUANG_LIVE_ENV_FILE      env file path to record

Readonly boundaries:
  connects_real_feishu=false
  reads_secret_values=false
  starts_services=false
  stops_services=false
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
ENV_FILE="${CHUANG_LIVE_ENV_FILE:-/home/user/.codex-im/chuang-feishu-bridge.env}"
OPERATOR="${CHUANG_LIVE_OPERATOR:-${USER:-<operator>}}"

export FORMAT ROOT ENV_FILE OPERATOR

python3 - <<'PY'
import json
import os
from datetime import datetime, timezone

FORMAT = os.environ["FORMAT"]
ROOT = os.environ["ROOT"]
ENV_FILE = os.environ["ENV_FILE"]
OPERATOR = os.environ["OPERATOR"]

BOUNDARIES = {
    "readonly": True,
    "connects_real_feishu": False,
    "reads_secret_values": False,
    "starts_services": False,
    "stops_services": False,
    "modifies_repo": False,
    "deletes_files": False,
    "reuses_codex_or_hermes_credentials": False,
}

result = {
    "schema_version": 1,
    "tested_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
    "operator": OPERATOR,
    "env_file": ENV_FILE,
    "workspace_root": ROOT,
    "preflight_status": "<fill_after_test>",
    "health_status": "<fill_after_test>",
    "new_thread_status": "<fill_after_test>",
    "session_status": "<fill_after_test>",
    "runtime_report_id": "<fill_after_test>",
    "provider_status": "<fill_after_test>",
    "codex_hermes_isolation": "<keep_codex_and_hermes_separate>",
    "notes": [],
    "blockers": [],
    "boundaries": BOUNDARIES,
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2))
else:
    print(f"tested_at={result['tested_at']}")
    print(f"operator={result['operator']}")
    print(f"env_file={result['env_file']}")
    print(f"workspace_root={result['workspace_root']}")
    print(f"preflight_status={result['preflight_status']}")
    print(f"health_status={result['health_status']}")
    print(f"new_thread_status={result['new_thread_status']}")
    print(f"session_status={result['session_status']}")
    print(f"runtime_report_id={result['runtime_report_id']}")
    print(f"provider_status={result['provider_status']}")
    print(f"codex_hermes_isolation={result['codex_hermes_isolation']}")
    print("notes=[]")
    print("blockers=[]")
    for key, value in BOUNDARIES.items():
        print(f"boundaries.{key}={str(value).lower()}")
PY
