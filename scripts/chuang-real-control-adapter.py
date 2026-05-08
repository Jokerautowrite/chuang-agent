#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys


STATUSES = {
    "active": "Running",
    "inactive": "Stopped",
    "failed": "Failed",
}

ADAPTER_NAME = "chuang-real-control"
CONTROL_AUDIT_LABEL = "control.apply.live"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["list", "apply"])
    parser.add_argument("--json", action="store_true", required=True)
    parser.add_argument("--allowlist", required=True)
    args = parser.parse_args()

    allowlist = load_allowlist(args.allowlist)
    if args.command == "list":
        print(json.dumps(list_units(allowlist), ensure_ascii=False))
        return 0
    if args.command == "apply":
        request = json.load(sys.stdin)
        receipt = apply_request(allowlist, request)
        print(json.dumps(receipt, ensure_ascii=False))
        return 0
    return 2


def load_allowlist(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as handle:
        data = json.load(handle)
    units = data.get("units")
    if not isinstance(units, list):
        raise SystemExit("allowlist must contain units array")
    return data


def list_units(allowlist: dict) -> list:
    return [unit_view(unit) for unit in allowlist["units"]]


def unit_view(unit: dict) -> dict:
    unit_id = require_str(unit, "unit_id")
    kind = require_str(unit, "kind")
    display_name = unit.get("display_name") or unit_id
    metadata = {
        **(unit.get("metadata") or {}),
        "adapter": ADAPTER_NAME,
        "dry_run": str(not live_enabled()).lower(),
        "live_enabled": str(live_enabled()).lower(),
        "audit_label": CONTROL_AUDIT_LABEL,
        "allowed_actions": ",".join(allowed_actions_for_unit(unit)),
    }
    return {
        "unit_id": unit_id,
        "display_name": display_name,
        "kind": kind,
        "status": status_for_unit(unit),
        "model_name": unit.get("model_name"),
        "metadata": metadata,
    }


def apply_request(allowlist: dict, request: dict) -> dict:
    unit_id = request.get("unit_id", "")
    action = request.get("action", "")
    unit = find_unit(allowlist, unit_id)
    if unit is None:
        raise SystemExit(f"unit not allowlisted: {unit_id}")
    if action not in {"start", "stop", "restart", "change_model"}:
        raise SystemExit(f"unsupported action: {action}")
    if action == "change_model":
        return change_model_receipt(unit, request)

    command = unit.get(f"{action}_command")
    if not command:
        raise SystemExit(f"action not allowlisted: {unit_id}:{action}")
    previous_status = status_for_unit(unit)
    if live_enabled():
        run_command(command)
        message = receipt_message("allowlisted command executed", dry_run=False)
    else:
        message = receipt_message(
            "dry-run accepted; set CHUANG_REAL_CONTROL_ENABLE=1 to execute",
            dry_run=True,
        )
    return {
        "unit_id": unit_id,
        "action": action,
        "previous_status": previous_status,
        "next_status": expected_next_status(action, previous_status),
        "model_name": None,
        "message": message,
    }


def change_model_receipt(unit: dict, request: dict) -> dict:
    model_name = request.get("model_name")
    if not model_name:
        raise SystemExit("change_model requires model_name")
    if not unit.get("model_env_file") or not unit.get("model_env_key"):
        raise SystemExit("change_model not allowlisted for unit")
    return {
        "unit_id": unit["unit_id"],
        "action": "change_model",
        "previous_status": status_for_unit(unit),
        "next_status": "Running",
        "model_name": model_name,
        "message": receipt_message(
            "dry-run model change accepted; env mutation is intentionally not implemented",
            dry_run=True,
        ),
    }


def find_unit(allowlist: dict, unit_id: str):
    for unit in allowlist["units"]:
        if unit.get("unit_id") == unit_id:
            return unit
    return None


def status_for_unit(unit: dict) -> str:
    command = unit.get("status_command")
    if not command or not live_status_enabled():
        return unit.get("default_status") or "Unknown"
    completed = subprocess.run(command, text=True, capture_output=True, check=False)
    raw = (completed.stdout or completed.stderr).strip()
    return STATUSES.get(raw, "Unknown")


def run_command(command: list) -> None:
    if not isinstance(command, list) or not command:
        raise SystemExit("allowlisted command must be a non-empty array")
    subprocess.run(command, text=True, capture_output=True, check=True)


def allowed_actions_for_unit(unit: dict) -> list:
    actions = []
    for action in ["start", "stop", "restart"]:
        if unit.get(f"{action}_command"):
            actions.append(action)
    if unit.get("model_env_file") and unit.get("model_env_key"):
        actions.append("change_model")
    return actions


def receipt_message(summary: str, dry_run: bool) -> str:
    return (
        f"{summary}; dry_run={str(dry_run).lower()} "
        f"live_enabled={str(live_enabled()).lower()} audit_label={CONTROL_AUDIT_LABEL}"
    )


def require_str(unit: dict, key: str) -> str:
    value = unit.get(key)
    if not isinstance(value, str) or not value.strip():
        raise SystemExit(f"unit missing {key}")
    return value


def expected_next_status(action: str, previous: str) -> str:
    if action == "stop":
        return "Stopped"
    if action in {"start", "restart"}:
        return "Running"
    return previous


def live_enabled() -> bool:
    return os.environ.get("CHUANG_REAL_CONTROL_ENABLE") == "1"


def live_status_enabled() -> bool:
    return os.environ.get("CHUANG_REAL_CONTROL_STATUS_ENABLE") == "1"


if __name__ == "__main__":
    raise SystemExit(main())
