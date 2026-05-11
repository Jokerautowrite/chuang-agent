#!/usr/bin/env bash
set -eu

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-live-operator-checklist.sh [--json]

Readonly operator checklist for the first manual Chuang live check.

Environment overrides:
  CHUANG_FEISHU_ENV_FILE          Chuang Feishu env file
  CHUANG_LIVE_OPERATOR_ENV_FILE   Same as CHUANG_FEISHU_ENV_FILE, takes priority
  CHUANG_AGENT_ROOT               Chuang repo root

Readonly boundaries:
  connects_real_feishu=false
  sends_feishu_messages=false
  connects_real_provider=false
  performs_desktop_actions=false
  performs_browser_actions=false
  connects_real_wiki=false
  connects_real_gbrain=false
  starts_services=false
  starts_workers=false
  dispatches_tasks=false
  touches_services=false
  modifies_repo=false
  prints_secret_values=false
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
HOME_DIR="${HOME:-/home/user}"
ENV_FILE="${CHUANG_LIVE_OPERATOR_ENV_FILE:-${CHUANG_FEISHU_ENV_FILE:-$HOME_DIR/.codex-im/chuang-feishu-bridge.env}}"

export FORMAT ROOT ENV_FILE

python3 - <<'PY'
import json
import os
from pathlib import Path

FORMAT = os.environ["FORMAT"]
ROOT = Path(os.environ["ROOT"])
ENV_FILE = Path(os.environ["ENV_FILE"])

FEISHU_REQUIRED = [
    "CHUANG_FEISHU_APP_ID",
    "CHUANG_FEISHU_APP_SECRET",
    "CHUANG_AGENT_WORKSPACE_ROOT",
]
FEISHU_OPTIONAL = [
    "CHUANG_FEISHU_BOT_ID",
    "CHUANG_FEISHU_VERIFICATION_TOKEN",
    "CHUANG_FEISHU_ENCRYPT_KEY",
    "CHUANG_FEISHU_CONNECTION_MODE",
]
FORBIDDEN_FEISHU_NAMES = [
    "FEISHU_APP_ID",
    "FEISHU_APP_SECRET",
    "FEISHU_BOT_ID",
    "FEISHU_VERIFICATION_TOKEN",
    "FEISHU_ENCRYPT_KEY",
    "HERMES_FEISHU_APP_ID",
    "HERMES_FEISHU_APP_SECRET",
    "HERMES_FEISHU_BOT_ID",
    "HERMES_FEISHU_VERIFICATION_TOKEN",
    "HERMES_FEISHU_ENCRYPT_KEY",
    "CODEX_FEISHU_APP_ID",
    "CODEX_FEISHU_APP_SECRET",
    "CODEX_FEISHU_BOT_ID",
    "CODEX_FEISHU_VERIFICATION_TOKEN",
    "CODEX_FEISHU_ENCRYPT_KEY",
]

BOUNDARIES = {
    "readonly": True,
    "connects_real_feishu": False,
    "sends_feishu_messages": False,
    "connects_real_provider": False,
    "performs_desktop_actions": False,
    "performs_browser_actions": False,
    "connects_real_wiki": False,
    "connects_real_gbrain": False,
    "starts_services": False,
    "starts_workers": False,
    "dispatches_tasks": False,
    "touches_services": False,
    "modifies_repo": False,
    "prints_secret_values": False,
    "reuses_codex_or_hermes_credentials": False,
}


def parse_env_file(path: Path):
    values = {}
    if not path.exists():
        return values, "missing"
    try:
        for raw_line in path.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            key = key.strip().removeprefix("export ").strip()
            values[key] = value.strip().strip('"').strip("'")
    except OSError:
        return values, "read_failed"
    return values, ""


def state(value):
    return "<set>" if str(value or "").strip() else "<missing>"


def parse_provider_env(path_text):
    if not path_text:
        return {}, "missing_path"
    path = Path(path_text).expanduser()
    values, error = parse_env_file(path)
    return values, error


env_values, env_error = parse_env_file(ENV_FILE)
workspace_root = Path(
    env_values.get("CHUANG_AGENT_WORKSPACE_ROOT")
    or os.environ.get("CHUANG_AGENT_WORKSPACE_ROOT")
    or ROOT
).expanduser()
default_provider_env_file = Path("~/.config/chuang-agent/provider.env").expanduser()
provider_env_file = env_values.get("CHUANG_PROVIDER_ENV_FILE") or os.environ.get("CHUANG_PROVIDER_ENV_FILE", "")
suggested_provider_env_file = None
if not provider_env_file:
    provider_env_file = str(default_provider_env_file)
    suggested_provider_env_file = {
        "path": str(default_provider_env_file),
        "exists": default_provider_env_file.is_file(),
        "state": state(default_provider_env_file if default_provider_env_file.is_file() else ""),
    }
provider_env_path = Path(provider_env_file).expanduser()
provider_values, provider_error = parse_provider_env(provider_env_file)
connection_mode = env_values.get("CHUANG_FEISHU_CONNECTION_MODE", "websocket")

feishu_required_states = {name: state(env_values.get(name)) for name in FEISHU_REQUIRED}
feishu_optional_states = {name: state(env_values.get(name)) for name in FEISHU_OPTIONAL}
provider_required_states = {
    "CHUANG_PROVIDER_ENV_FILE": state(provider_env_path if provider_env_path.is_file() else ""),
    "CODEX_PPTOKEN_API_KEY": state(provider_values.get("CODEX_PPTOKEN_API_KEY")),
}
forbidden_in_file = [name for name in FORBIDDEN_FEISHU_NAMES if name in env_values]
inherited_forbidden_states = {
    name: ("<set_ignored>" if os.environ.get(name) else "<unset>") for name in FORBIDDEN_FEISHU_NAMES
}

blockers = []
if env_error:
    blockers.append(f"env_file_{env_error}")
for name, value in feishu_required_states.items():
    if value != "<set>":
        blockers.append(f"missing_{name}")
if forbidden_in_file:
    blockers.append("forbidden_codex_or_hermes_feishu_names_in_env_file")
if not workspace_root.is_dir():
    blockers.append("workspace_root_missing")
if not (workspace_root / "config.toml").is_file():
    blockers.append("workspace_config_missing")
if provider_error:
    blockers.append(f"provider_env_file_{provider_error}")
if provider_required_states["CODEX_PPTOKEN_API_KEY"] != "<set>":
    blockers.append("missing_CODEX_PPTOKEN_API_KEY")

warnings = []
if connection_mode not in {"websocket", "webhook"}:
    warnings.append("unexpected_CHUANG_FEISHU_CONNECTION_MODE")

status = "blocked" if blockers else ("warning" if warnings else "ready")
commands = {
    "local_preflight": (
        f"node scripts/chuang-feishu-live-preflight.js --env-file {ENV_FILE} "
        f"--workspace-root {workspace_root} --json"
    ),
    "local_readiness_gate": "sh scripts/chuang-live-readonly-preflight.sh",
    "provider_readiness_check": (
        f"bash scripts/chuang-provider-readiness-check.sh --config {workspace_root / 'config.toml'}"
    ),
    "operator_receipt_template": "scripts/chuang-live-operator-receipt.sh --json",
    "final_verify": "sh scripts/chuang-final-verify.sh",
    "goal_status": "scripts/chuang-goal-run-status.sh --json",
    "bridge_health_command": "send /health to the Chuang Feishu bot",
    "bridge_session_command": "send /session to the Chuang Feishu bot",
    "new_thread_command": "send /new to the Chuang Feishu bot",
    "bridge_tools_command": "send /tools to the Chuang Feishu bot",
    "bridge_capabilities_command": "send /capabilities to the Chuang Feishu bot",
    "status_surface": f"cargo run --quiet -- status --config {workspace_root / 'config.toml'} --json",
    "doctor_surface": f"cargo run --quiet -- doctor --config {workspace_root / 'config.toml'} --json",
    "console_snapshot_surface": f"cargo run --quiet -- console snapshot --config {workspace_root / 'config.toml'} --json",
    "knowledge_status": "cargo run --quiet -- memory knowledge status --json",
    "wiki_source_contract": "cargo run --quiet -- memory knowledge source-contract --source wiki --json",
    "gbrain_source_contract": "cargo run --quiet -- memory knowledge source-contract --source gbrain --json",
    "external_ai_dry_run": (
        "cargo run --quiet -- external-ai dispatch --platform <platform> "
        "--task <non-secret bounded task> --context <non-secret bounded context> --dry-run --json"
    ),
}
if suggested_provider_env_file is not None:
    commands["provider_env_next_step"] = (
        "set CHUANG_PROVIDER_ENV_FILE to "
        f"{suggested_provider_env_file['path']} in the Chuang Feishu env, "
        "or export it explicitly before rerunning the checklist"
    )
manual_steps = ["run local_preflight and confirm status is ready or only expected warnings"]
if suggested_provider_env_file is not None:
    manual_steps.append(
        "if CHUANG_PROVIDER_ENV_FILE is missing, point it at "
        f"{suggested_provider_env_file['path']} or set it explicitly, then rerun this checklist"
    )
manual_steps.extend(
    [
        "run provider_readiness_check and confirm provider_kind, transport, request_timeout_ms, api_key_state, current, and next_action are visible without secret values",
        "generate operator_receipt_template and fill request_id, approval_scope, rollback_condition, and per-service evidence after manual live checks",
        "run local_readiness_gate and confirm it reports watchdog readonly evidence, diagnostics, provider readiness check, and complete local smoke",
        "start or confirm the Chuang-only Feishu bridge outside this checklist",
        "send /health to the Chuang bot and confirm secrets show only <set>/<missing>",
        "send /tools and confirm the mounted local capabilities and boundaries are visible",
        "send /capabilities and confirm it matches /tools",
        "send /new, then send one normal text message",
        "send /session and confirm the active chat binding changed after /new",
        "confirm the reply is not fake-responder and includes a runtime report id when applicable",
        "record real live acceptance as incomplete until Feishu, provider, desktop, browser, wiki, and GBrain each have operator evidence",
        "run status_surface and confirm release_readiness still says connects_real_external_services=false unless a separate live receipt exists",
        "run knowledge_status, wiki_source_contract, and gbrain_source_contract to confirm wiki/GBrain remain documented/read-only unless audited live adapters are configured",
        "run external_ai_dry_run only with non-secret bounded context; do not treat dry-run as real browser completion",
        "confirm Codex Feishu and Hermes channels still operate independently",
        "after the test, run final_verify before committing any follow-up changes",
    ]
)

mounted_feishu_capabilities = [
    {
        "command": "/health",
        "capability": "bridge health and runtime readiness summary",
        "expected_evidence": ["redacted secret states", "health/readiness status"],
    },
    {
        "command": "/tools",
        "capability": "mounted local command and boundary list",
        "expected_evidence": [
            "/health",
            "/new",
            "/session",
            "/tools",
            "/capabilities",
            "live-check",
            "image OCR",
            "normal text to app-server",
            "does not reuse Codex/Hermes credentials",
        ],
    },
    {
        "command": "/capabilities",
        "capability": "alias of /tools for mounted local capabilities",
        "expected_evidence": ["same local capability and boundary list as /tools"],
    },
    {
        "command": "/new",
        "capability": "new Feishu chat/topic/thread binding guidance",
        "expected_evidence": ["handled locally", "does not consume an agent runtime turn"],
    },
    {
        "command": "/session",
        "capability": "current Feishu chat binding evidence",
        "expected_evidence": ["active session/binding state", "redacted identifiers only"],
    },
]

provider_readiness_evidence = {
    "command": commands["provider_readiness_check"],
    "script": "scripts/chuang-provider-readiness-check.sh",
    "readonly": True,
    "source_status_surface": "cargo run --quiet -- status --json",
    "connects_real_provider": False,
    "prints_secret_values": False,
    "expected_fields": [
        "source_status_surface",
        "provider_kind",
        "transport",
        "request_timeout_ms",
        "api_key_state",
        "overall_state",
        "placeholder_warning_count",
        "current",
        "next_action",
        "blocked_reason",
    ],
    "api_key_state_values": ["<set>", "<missing>"],
}

local_readonly_evidence = {
    "command": commands["local_readiness_gate"],
    "script": "scripts/chuang-live-readonly-preflight.sh",
    "readonly": True,
    "starts_workers": False,
    "dispatches_tasks": False,
    "touches_services": False,
    "modifies_repo": False,
    "expected_steps": [
        "watchdog readonly once",
        "status diagnostic",
        "doctor diagnostic",
        "app-server health diagnostic",
        "console snapshot diagnostic",
        "provider readiness check",
        "complete local smoke",
    ],
}

external_live_acceptance_matrix = [
    {
        "id": "feishu",
        "service": "Feishu Chuang bot",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": commands["local_preflight"],
        "local_evidence": ["local_preflight", "bridge_tools_command", "bridge_health_command"],
        "manual_live_required": True,
        "required_evidence": [
            "/health and /session operator transcript",
            "/tools or /capabilities boundary transcript",
            "normal non-secret text reply with runtime report id when applicable",
            "Codex and Hermes credentials are not reused",
        ],
        "connects_real_service_in_checklist": False,
        "prints_secret_values": False,
    },
    {
        "id": "provider",
        "service": "OpenAI-compatible provider",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": commands["provider_readiness_check"],
        "local_evidence": ["provider_readiness_check", "status.provider_readiness"],
        "manual_live_required": True,
        "required_evidence": [
            "provider transport is not stub",
            "api_key_state is <set> without printing the key",
            "bounded live call receipt or runtime report id exists",
            "no fake-responder fallback",
        ],
        "connects_real_service_in_checklist": False,
        "prints_secret_values": False,
    },
    {
        "id": "subagent_live_rehearsal",
        "service": "single subagent live rehearsal",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": "bash scripts/chuang-live-runner-rehearsal-smoke.sh",
        "local_evidence": [
            "subagent_live_preflight",
            "gate_allowlist_capability_routing_report_admission_contract",
        ],
        "manual_live_required": True,
        "evidence_refs": {
            "gate": "<fill_after_test>",
            "allowlist": "<fill_after_test>",
            "capability_routing": "<fill_after_test>",
            "report_admission": "<fill_after_test>",
        },
        "required_evidence": [
            "single worker only",
            "gate receipt is explicit",
            "allowlist receipt is explicit",
            "capability routing receipt is explicit",
            "report admission receipt or blocked reason is explicit",
        ],
        "connects_real_service_in_checklist": False,
        "starts_worker_in_checklist": False,
        "prints_secret_values": False,
    },
    {
        "id": "desktop",
        "service": "desktop actuator",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": commands["status_surface"],
        "local_evidence": ["status.live_adapter_gates", "actuator_command_contract"],
        "manual_live_required": True,
        "required_evidence": [
            "audit label and action receipt",
            "governance allowed the exact desktop action",
            "real_execution=true only in the external audited receipt",
            "no desktop operation is performed by this checklist",
        ],
        "connects_real_service_in_checklist": False,
        "performs_action_in_checklist": False,
        "prints_secret_values": False,
    },
    {
        "id": "browser",
        "service": "browser / external AI session",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": commands["external_ai_dry_run"],
        "local_evidence": ["external_ai_readiness", "genesis_actuator_contract"],
        "manual_live_required": True,
        "required_evidence": [
            "audited adapter manifest",
            "platform session scope",
            "browser transcript or snapshot reference",
            "subagent report admission accepted",
        ],
        "connects_real_service_in_checklist": False,
        "performs_action_in_checklist": False,
        "prints_secret_values": False,
    },
    {
        "id": "wiki",
        "service": "wiki external knowledge",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": commands["wiki_source_contract"],
        "local_evidence": ["memory_knowledge_source_contract_wiki"],
        "manual_live_required": True,
        "required_evidence": [
            "source contract is explicit",
            "retrieval is read-only",
            "hit provenance is visible",
            "no automatic core-memory write",
        ],
        "connects_real_service_in_checklist": False,
        "prints_secret_values": False,
    },
    {
        "id": "gbrain",
        "service": "GBrain external knowledge",
        "completion_state": "not_verified",
        "must_not_count_as_complete": True,
        "readonly_probe": commands["gbrain_source_contract"],
        "local_evidence": ["memory_knowledge_source_contract_gbrain"],
        "manual_live_required": True,
        "required_evidence": [
            "source contract is explicit",
            "retrieval is read-only",
            "hit provenance is visible",
            "no automatic core-memory write",
        ],
        "connects_real_service_in_checklist": False,
        "prints_secret_values": False,
    },
]

real_live_acceptance = {
    "complete": False,
    "status": "not_verified",
    "gap_count": len(external_live_acceptance_matrix),
    "checklist_is_readonly": True,
    "cannot_mark_complete_from_readonly_checklist": True,
    "operator_receipt_template": commands["operator_receipt_template"],
    "operator_receipt_template_can_mark_complete": False,
    "required_receipt_service_ids": [
        "feishu",
        "provider",
        "subagent_live_rehearsal",
        "desktop",
        "browser",
        "wiki",
        "gbrain",
    ],
    "services": external_live_acceptance_matrix,
    "summary": "real live acceptance remains incomplete until each service has separate operator evidence",
}

result = {
    "schema_version": 1,
    "ok": not blockers,
    "status": status,
    "readonly_boundaries": BOUNDARIES,
    "paths": {
        "agent_root": str(ROOT),
        "env_file": str(ENV_FILE),
        "workspace_root": str(workspace_root),
        "workspace_config": str(workspace_root / "config.toml"),
        "provider_env_file": str(provider_env_path),
    },
    "checks": {
        "feishu_env_file": {
            "exists": ENV_FILE.is_file(),
            "error": env_error or None,
            "required": feishu_required_states,
            "optional": feishu_optional_states,
            "connection_mode": connection_mode,
            "forbidden_credential_env_names_in_file": forbidden_in_file,
            "inherited_forbidden_credential_env_states": inherited_forbidden_states,
        },
        "workspace": {
            "root_exists": workspace_root.is_dir(),
            "config_exists": (workspace_root / "config.toml").is_file(),
        },
        "provider_env_file": {
            "exists": provider_env_path.is_file(),
            "error": provider_error or None,
            "required": provider_required_states,
            "contains_feishu_credential_names": any(name in provider_values for name in FEISHU_REQUIRED + FEISHU_OPTIONAL),
        },
    },
    "mounted_feishu_capabilities": mounted_feishu_capabilities,
    "provider_readiness_evidence": provider_readiness_evidence,
    "local_readonly_evidence": local_readonly_evidence,
    "external_live_acceptance_matrix": external_live_acceptance_matrix,
    "real_live_acceptance": real_live_acceptance,
    "blockers": blockers,
    "warnings": warnings,
    "commands": commands,
    "manual_steps": manual_steps,
    "suggested_provider_env_file": suggested_provider_env_file,
}

if FORMAT == "json":
    print(json.dumps(result, ensure_ascii=False, indent=2))
else:
    print(f"chuang_live_operator_checklist status={status} ok={str(not blockers).lower()} readonly=true")
    print(f"env_file={ENV_FILE}")
    print(f"workspace_root={workspace_root}")
    print(f"provider_env_file={provider_env_path if provider_env_file else '<missing>'}")
    print("mounted_feishu_capabilities=/health,/tools,/capabilities,/new,/session")
    print(
        "provider_readiness_evidence="
        "script=scripts/chuang-provider-readiness-check.sh source_status_surface=status --json "
        "fields=source_status_surface,provider_kind,transport,request_timeout_ms,api_key_state,current,next_action "
        "connects_real_provider=false prints_secret_values=false"
    )
    print(
        "local_readonly_evidence="
        "script=scripts/chuang-live-readonly-preflight.sh "
        "starts_workers=false dispatches_tasks=false touches_services=false modifies_repo=false"
    )
    print(
        "external_live_acceptance_matrix="
        "feishu,provider,subagent_live_rehearsal,desktop,browser,wiki,gbrain completion_state=not_verified "
        "connects_real_service_in_checklist=false"
    )
    print(
        "real_live_acceptance="
        "complete=false status=not_verified checklist_is_readonly=true "
        "cannot_mark_complete_from_readonly_checklist=true "
        "operator_receipt_template_can_mark_complete=false"
    )
    for gap in external_live_acceptance_matrix:
        print(
            "live_gap "
            f"id={gap['id']} completion_state={gap['completion_state']} "
            f"must_not_count_as_complete={str(gap['must_not_count_as_complete']).lower()} "
            f"readonly_probe={gap['readonly_probe']}"
        )
    if suggested_provider_env_file is not None:
        print(
            "suggested_provider_env_file="
            f"{suggested_provider_env_file['path']} state={suggested_provider_env_file['state']}"
        )
    for name, value in feishu_required_states.items():
        print(f"feishu_required {name}={value}")
    for name, value in provider_required_states.items():
        print(f"provider_required {name}={value}")
    for blocker in blockers:
        print(f"blocker={blocker}")
    for warning in warnings:
        print(f"warning={warning}")
    print("commands:")
    for name, command in commands.items():
        print(f"- {name}: {command}")
    print("manual_steps:")
    for index, step in enumerate(manual_steps, 1):
        print(f"{index}. {step}")

raise SystemExit(0 if not blockers else 1)
PY
