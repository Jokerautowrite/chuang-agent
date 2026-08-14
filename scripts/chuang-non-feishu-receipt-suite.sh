#!/usr/bin/env bash
set -u -o pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-non-feishu-receipt-suite.sh [--json]

Readonly non-Feishu receipt/readiness suite for operator checks.
Runs only non-Feishu low-risk collectors by default and never performs real
desktop actions. Live provider calls are opt-in.

Environment:
  CHUANG_NON_FEISHU_SUITE_INCLUDE_PROVIDER_LIVE=1
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

ROOT="${CHUANG_AGENT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
INCLUDE_PROVIDER_LIVE="${CHUANG_NON_FEISHU_SUITE_INCLUDE_PROVIDER_LIVE:-0}"
export ROOT FORMAT INCLUDE_PROVIDER_LIVE

python3 - <<'PY'
import json
import os
import subprocess
import time

root = os.environ["ROOT"]
format_mode = os.environ["FORMAT"]
include_provider_live = os.environ.get("INCLUDE_PROVIDER_LIVE", "0") == "1"

children_spec = [
    {
        "name": "provider_readiness_check",
        "argv": ["bash", "scripts/chuang-provider-readiness-check.sh", "--json"],
    },
    {
        "name": "desktop_action_rehearsal_receipt",
        "argv": ["bash", "scripts/chuang-desktop-action-rehearsal-receipt.sh", "--json"],
    },
    {
        "name": "cdp_readonly_session_receipt",
        "argv": ["bash", "scripts/chuang-cdp-readonly-session-receipt.sh", "--json"],
    },
    {
        "name": "wiki_live_readonly_receipt",
        "argv": ["bash", "scripts/chuang-wiki-live-receipt.sh", "--json"],
    },
    {
        "name": "gbrain_live_readonly_receipt",
        "argv": ["bash", "scripts/chuang-gbrain-live-receipt.sh", "--json"],
    },
    {
        "name": "skill_proposal_verify_receipt",
        "argv": ["bash", "scripts/chuang-skill-proposal-verify-receipt.sh", "--json"],
    },
]

if include_provider_live:
    children_spec.insert(
        1,
        {
            "name": "provider_live_request_receipt",
            "argv": ["bash", "scripts/chuang-provider-live-request-receipt.sh", "--json"],
        },
    )

children = []
exit_codes = []
child_acceptance_statuses = []
for spec in children_spec:
    run = subprocess.run(
        spec["argv"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    stdout_text = run.stdout.strip()
    stderr_text = run.stderr.strip()
    child = {
        "name": spec["name"],
        "exit_code": int(run.returncode),
        "stderr_present": bool(stderr_text),
    }
    try:
        parsed = json.loads(stdout_text) if stdout_text else {}
        child["stdout_json_parse_ok"] = True
        if isinstance(parsed, dict):
            child_acceptance = parsed.get("acceptance_status")
            if child_acceptance is not None:
                child["acceptance_status"] = str(child_acceptance)
                child_acceptance_statuses.append(str(child_acceptance))
            summary = child_acceptance or parsed.get("receipt_kind") or "json_ok"
            child["summary"] = str(summary)
        else:
            child["summary"] = "json_ok"
    except json.JSONDecodeError:
        child["stdout_json_parse_ok"] = False
        child["summary"] = (stdout_text.splitlines()[0] if stdout_text else "<empty_stdout>")[:240]

    children.append(child)
    exit_codes.append(int(run.returncode))

any_nonzero = any(code != 0 for code in exit_codes)
if any_nonzero:
    acceptance_status = "blocked" if all(code in (0, 1) for code in exit_codes) else "failed"
elif any(status == "failed" for status in child_acceptance_statuses):
    acceptance_status = "failed"
elif any(status == "blocked" for status in child_acceptance_statuses):
    acceptance_status = "blocked"
else:
    acceptance_status = "verified"

result = {
    "schema_version": 1,
    "receipt_kind": "non_feishu_receipt_suite",
    "tested_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "acceptance_status": acceptance_status,
    "provider_live_request_opt_in": include_provider_live,
    "global_real_live_ready": False,
    "children": children,
}

if format_mode == "json":
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
else:
    print("non_feishu_receipt_suite")
    print(f"acceptance_status={acceptance_status}")
    print("global_real_live_ready=false")
    for child in children:
        print(
            f"- {child['name']}: exit_code={child['exit_code']} "
            f"stdout_json_parse_ok={str(child.get('stdout_json_parse_ok', False)).lower()} "
            f"stderr_present={str(child['stderr_present']).lower()} "
            f"summary={child.get('summary', '')}"
        )
PY
