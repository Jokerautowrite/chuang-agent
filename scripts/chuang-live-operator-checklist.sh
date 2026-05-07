#!/usr/bin/env bash
set -euo pipefail

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
  starts_services=false
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
    "HERMES_FEISHU_APP_ID",
    "HERMES_FEISHU_APP_SECRET",
    "CODEX_FEISHU_APP_ID",
    "CODEX_FEISHU_APP_SECRET",
]

BOUNDARIES = {
    "readonly": True,
    "connects_real_feishu": False,
    "sends_feishu_messages": False,
    "starts_services": False,
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
provider_env_file = env_values.get("CHUANG_PROVIDER_ENV_FILE") or os.environ.get("CHUANG_PROVIDER_ENV_FILE", "")
provider_values, provider_error = parse_provider_env(provider_env_file)
suggested_provider_env_file = None
if not provider_env_file:
    candidate_provider_env_file = Path("~/.config/chuang-agent/provider.env").expanduser()
    suggested_provider_env_file = {
        "path": str(candidate_provider_env_file),
        "exists": candidate_provider_env_file.is_file(),
        "state": state(candidate_provider_env_file if candidate_provider_env_file.is_file() else ""),
    }
connection_mode = env_values.get("CHUANG_FEISHU_CONNECTION_MODE", "websocket")

feishu_required_states = {name: state(env_values.get(name)) for name in FEISHU_REQUIRED}
feishu_optional_states = {name: state(env_values.get(name)) for name in FEISHU_OPTIONAL}
provider_required_states = {
    "CHUANG_PROVIDER_ENV_FILE": state(provider_env_file),
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
    "final_verify": "sh scripts/chuang-final-verify.sh",
    "goal_status": "scripts/chuang-goal-run-status.sh --json",
    "bridge_health_command": "send /health to the Chuang Feishu bot",
    "bridge_session_command": "send /session to the Chuang Feishu bot",
    "new_thread_command": "send /new to the Chuang Feishu bot",
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
        "start or confirm the Chuang-only Feishu bridge outside this checklist",
        "send /health to the Chuang bot and confirm secrets show only <set>/<missing>",
        "send /new, then send one normal text message",
        "send /session and confirm the active chat binding changed after /new",
        "confirm the reply is not fake-responder and includes a runtime report id when applicable",
        "confirm Codex Feishu and Hermes channels still operate independently",
        "after the test, run final_verify before committing any follow-up changes",
    ]
)

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
        "provider_env_file": provider_env_file or "",
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
            "exists": bool(provider_env_file) and Path(provider_env_file).expanduser().is_file(),
            "error": provider_error or None,
            "required": provider_required_states,
            "contains_feishu_credential_names": any(name in provider_values for name in FEISHU_REQUIRED + FEISHU_OPTIONAL),
        },
    },
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
    print(f"provider_env_file={provider_env_file or '<missing>'}")
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
