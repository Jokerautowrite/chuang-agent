#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true", required=True)
    parser.add_argument("--allowlist", required=True)
    args = parser.parse_args()

    allowlist = load_allowlist(args.allowlist)
    request = json.load(sys.stdin)
    response = handle_request(allowlist, request)
    print(json.dumps(response, ensure_ascii=False))
    return 0


def load_allowlist(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def handle_request(allowlist: dict, request: dict) -> dict:
    action = request.get("action", "")
    if action == "observe":
        return response(
            observation={
                "target": request.get("observe_target"),
                "summary": "allowlisted actuator ready; no desktop operation was performed",
                "evidence_ref": {"uri": "chuang-actuator://observe/dry-run"},
            },
            message="dry-run observe",
        )
    if action == "open_app":
        app_name = ((request.get("open_app") or {}).get("app_name") or "").strip()
        app = find_app(allowlist, app_name)
        if app is None:
            raise SystemExit(f"app not allowlisted: {app_name}")
        if live_enabled():
            subprocess.Popen(app["open_command"])
            message = "allowlisted app launch requested"
        else:
            message = "dry-run accepted; set CHUANG_REAL_ACTUATOR_ENABLE=1 to launch"
        return response(
            app_handle={
                "app_name": app_name,
                "handle_id": f"chuang-actuator://app/{app_name}",
            },
            message=message,
        )
    if action == "focus":
        return guarded_noop(allowlist, "focus_allowed", "focus")
    if action == "click":
        return guarded_noop(allowlist, "click_allowed", "click")
    if action == "input_text":
        return guarded_noop(allowlist, "input_allowed", "input_text")
    if action == "screenshot":
        if not allowlist.get("screenshot_allowed", False):
            raise SystemExit("screenshot not allowlisted")
        return response(
            evidence_ref={"uri": "chuang-actuator://screenshot/dry-run"},
            message="dry-run screenshot",
        )
    raise SystemExit(f"unsupported actuator action: {action}")


def guarded_noop(allowlist: dict, key: str, action: str) -> dict:
    if not allowlist.get(key, False):
        raise SystemExit(f"{action} not allowlisted")
    return response(message=f"dry-run {action}; live operation not implemented")


def find_app(allowlist: dict, app_name: str):
    for app in allowlist.get("apps", []):
        if app.get("app_name") == app_name:
            command = app.get("open_command")
            if not isinstance(command, list) or not command:
                raise SystemExit(f"app missing open_command: {app_name}")
            return app
    return None


def response(observation=None, app_handle=None, evidence_ref=None, message="ok") -> dict:
    return {
        "observation": observation,
        "app_handle": app_handle,
        "evidence_ref": evidence_ref,
        "message": message,
    }


def live_enabled() -> bool:
    return os.environ.get("CHUANG_REAL_ACTUATOR_ENABLE") == "1"


if __name__ == "__main__":
    raise SystemExit(main())
