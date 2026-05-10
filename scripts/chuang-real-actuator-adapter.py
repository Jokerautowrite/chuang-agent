#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time


ACTUATOR_AUDIT_LABEL = "actuator.operation.live"
ACTUATOR_REQUIRED_ENV = "CHUANG_REAL_ACTUATOR_ENABLE"


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
        return observe_response(request)
    if action == "open_app":
        app_name = ((request.get("open_app") or {}).get("app_name") or "").strip()
        app = find_app(allowlist, app_name)
        if app is None:
            raise SystemExit(f"app not allowlisted: {app_name}")
        if live_enabled():
            launch_open_command(app["open_command"])
            message = boundary_message("open_app", real_execution=True)
        else:
            message = boundary_message("open_app")
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
        return click_response(allowlist, request)
    if action == "input_text":
        return input_response(allowlist, request)
    if action == "screenshot":
        if not allowlist.get("screenshot_allowed", False):
            raise SystemExit("screenshot not allowlisted")
        evidence_ref, message = screenshot_evidence()
        return response(
            evidence_ref=evidence_ref,
            message=message,
        )
    raise SystemExit(f"unsupported actuator action: {action}")


def observe_response(request: dict) -> dict:
    title, source, error = read_active_window_title()
    if title:
        summary = f"current_window_title={title} source={source}"
    else:
        summary = "current_window_title=unavailable"
        if error:
            summary += f" reason={sanitize_text(error)}"
    return response(
        observation={
            "target": request.get("observe_target"),
            "summary": summary,
            "evidence_ref": {"uri": f"chuang-actuator://observe/{source}"},
        },
        message=boundary_message("observe", read_only=True),
    )


def read_active_window_title():
    for command, source in [
        (["xdotool", "getactivewindow", "getwindowname"], "xdotool"),
        (["xdotool", "getwindowfocus", "getwindowname"], "xdotool-focus"),
    ]:
        try:
            result = subprocess.run(
                command,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=2,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            last_error = str(exc)
            continue
        if result.returncode == 0 and result.stdout.strip():
            return sanitize_text(result.stdout.strip()), source, None
        last_error = result.stderr.strip() or f"exit={result.returncode}"
    return None, "unavailable", last_error if "last_error" in locals() else "no title command"


def screenshot_evidence():
    output_dir = os.environ.get(
        "CHUANG_ACTUATOR_EVIDENCE_DIR",
        os.path.join(tempfile.gettempdir(), "chuang-actuator-evidence"),
    )
    os.makedirs(output_dir, exist_ok=True)
    path = os.path.join(output_dir, f"screenshot-{int(time.time() * 1000)}.png")
    for command in [
        ["spectacle", "-b", "-n", "-o", path],
        ["gnome-screenshot", "-f", path],
    ]:
        try:
            result = subprocess.run(
                command,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=8,
            )
        except (OSError, subprocess.TimeoutExpired):
            continue
        if result.returncode == 0 and os.path.exists(path):
            return (
                {"uri": f"file://{path}"},
                boundary_message("screenshot", read_only=True, evidence_path=path),
            )
    return (
        {"uri": "chuang-actuator://screenshot/unavailable"},
        boundary_message("screenshot", read_only=True, evidence_path="unavailable"),
    )


def click_response(allowlist: dict, request: dict) -> dict:
    if not allowlist.get("click_allowed", False):
        raise SystemExit("click not allowlisted")
    if live_enabled():
        x, y = extract_click_coordinates(request.get("click_target"))
        run_xdotool(["mousemove", str(x), str(y)])
        run_xdotool(["click", "1"])
        return response(message=boundary_message("click", real_execution=True))
    return response(message=boundary_message("click"))


def input_response(allowlist: dict, request: dict) -> dict:
    if not allowlist.get("input_allowed", False):
        raise SystemExit("input_text not allowlisted")
    if live_enabled():
        text_value, is_secret = extract_input_text(request.get("text"))
        if is_secret:
            raise SystemExit("secret input is not supported by this command adapter")
        run_xdotool(["type", "--clearmodifiers", "--delay", "0", text_value])
        return response(message=boundary_message("input_text", real_execution=True))
    return response(message=boundary_message("input_text"))


def guarded_noop(allowlist: dict, key: str, action: str) -> dict:
    if not allowlist.get(key, False):
        raise SystemExit(f"{action} not allowlisted")
    return response(message=boundary_message(action))


def boundary_message(
    action: str,
    real_execution: bool = False,
    read_only: bool = False,
    evidence_path: str = "",
) -> str:
    dry_run = "false" if real_execution else "true"
    state = "true" if real_execution else "false"
    live_gate_required = "true" if real_execution else "false"
    read_only_state = "true" if read_only else "false"
    if real_execution:
        prefix = "allowlisted live actuator operation requested"
    elif read_only:
        prefix = "allowlisted read-only actuator observation"
        dry_run = "false"
    else:
        prefix = "dry-run actuator operation accepted"
    message = (
        f"{prefix}; allowed=true dry_run={dry_run} action={action} real_execution={state} "
        f"read_only={read_only_state} live_gate_required={live_gate_required} "
        f"audit_label={ACTUATOR_AUDIT_LABEL} required_env={ACTUATOR_REQUIRED_ENV}"
    )
    if evidence_path:
        message += f" evidence_path={evidence_path}"
    return message


def sanitize_text(value: str) -> str:
    return " ".join(str(value).replace("\n", " ").replace("\r", " ").split())[:240]


def extract_click_coordinates(click_target):
    if not isinstance(click_target, dict):
        raise SystemExit("click target missing coordinates")
    coordinates = click_target.get("Coordinates")
    if not isinstance(coordinates, dict):
        raise SystemExit("click target missing Coordinates")
    try:
        return int(coordinates["x"]), int(coordinates["y"])
    except (KeyError, TypeError, ValueError) as exc:
        raise SystemExit(f"click target invalid coordinates: {exc}") from exc


def extract_input_text(text_value):
    if isinstance(text_value, dict):
        plain = text_value.get("Plain")
        if plain is not None:
            return str(plain), False
        secret = text_value.get("Secret")
        if isinstance(secret, dict):
            label = secret.get("label")
            return "" if label is None else str(label), True
    if isinstance(text_value, str):
        return text_value, False
    raise SystemExit("input text missing content")


def find_app(allowlist: dict, app_name: str):
    for app in allowlist.get("apps", []):
        if app.get("app_name") == app_name:
            command = app.get("open_command")
            if not isinstance(command, list) or not command:
                raise SystemExit(f"app missing open_command: {app_name}")
            return app
    return None


def launch_open_command(command: list) -> None:
    subprocess.Popen(
        command,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
        start_new_session=True,
    )


def response(observation=None, app_handle=None, evidence_ref=None, message="ok") -> dict:
    return {
        "observation": observation,
        "app_handle": app_handle,
        "evidence_ref": evidence_ref,
        "message": message,
    }


def live_enabled() -> bool:
    return os.environ.get("CHUANG_REAL_ACTUATOR_ENABLE") == "1"


def run_xdotool(args):
    result = subprocess.run(
        ["xdotool", *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=8,
    )
    if result.returncode != 0:
        stderr = sanitize_text(result.stderr)
        raise SystemExit(
            f"xdotool failed: command={' '.join(args)} status={result.returncode} stderr={stderr}"
        )


if __name__ == "__main__":
    raise SystemExit(main())
