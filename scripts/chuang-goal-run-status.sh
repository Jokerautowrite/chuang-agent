#!/usr/bin/env bash
set -euo pipefail

FORMAT="text"

usage() {
  cat <<'EOF'
usage: scripts/chuang-goal-run-status.sh [--json]

Readonly status view for Chuang terminal goal workers.

Environment overrides:
  CHUANG_GOAL_WATCHDOG_REPORT_FILE  watchdog JSON report path
  CHUANG_GOAL_RUN_ROOT              overnight run root
  CHUANG_GOAL_OVERNIGHT_STATUS_FILE overnight status JSON path
  CHUANG_GOAL_TMUX_SESSION          tmux session to observe for interactive goal mode

Readonly boundaries:
  dispatches_tasks=false
  starts_worker=false
  restarts_worker=false
  modifies_repo=false
  deletes_logs=false
  touches_services=false
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

HOME_DIR="${HOME:-/home/user}"
WATCHDOG_REPORT_FILE="${CHUANG_GOAL_WATCHDOG_REPORT_FILE:-$HOME_DIR/.codex/chuang-goal-interactive/latest-watchdog-report.json}"
RUN_ROOT="${CHUANG_GOAL_RUN_ROOT:-$HOME_DIR/.codex/chuang-goal-runs}"
OVERNIGHT_STATUS_FILE="${CHUANG_GOAL_OVERNIGHT_STATUS_FILE:-}"
TMUX_SESSION="${CHUANG_GOAL_TMUX_SESSION:-chuang-codex-claude-goal}"

export FORMAT WATCHDOG_REPORT_FILE RUN_ROOT OVERNIGHT_STATUS_FILE TMUX_SESSION

python3 - <<'PY'
import json
import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

FORMAT = os.environ["FORMAT"]
WATCHDOG_REPORT_FILE = Path(os.environ["WATCHDOG_REPORT_FILE"])
RUN_ROOT = Path(os.environ["RUN_ROOT"])
OVERNIGHT_STATUS_FILE = os.environ.get("OVERNIGHT_STATUS_FILE", "")
TMUX_SESSION = os.environ["TMUX_SESSION"]

BOUNDARIES = {
    "readonly": True,
    "dispatches_tasks": False,
    "starts_worker": False,
    "restarts_worker": False,
    "modifies_repo": False,
    "deletes_logs": False,
    "touches_services": False,
}
WATCHDOG_STALE_AFTER_SECONDS = 1800
OVERNIGHT_STALE_AFTER_SECONDS = 1800


def read_json_file(path: Path):
    if not path.exists():
        return {
            "available": False,
            "readable": False,
            "path": str(path),
            "error": "missing",
        }
    try:
        return {
            "available": True,
            "readable": True,
            "path": str(path),
            "data": json.loads(path.read_text(encoding="utf-8")),
        }
    except OSError:
        return {
            "available": True,
            "readable": False,
            "path": str(path),
            "error": "read_failed",
        }
    except json.JSONDecodeError:
        return {
            "available": True,
            "readable": True,
            "path": str(path),
            "error": "invalid_json",
        }


def list_run_dirs(root: Path):
    if not root.exists() or not root.is_dir():
        return []
    try:
        return sorted(
            [path for path in root.iterdir() if path.is_dir()],
            key=lambda path: path.name,
            reverse=True,
        )
    except OSError:
        return []


def parse_summary(summary_path: Path):
    fields = {}
    if not summary_path.exists():
        return fields
    try:
        for raw_line in summary_path.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line.startswith("- ") or ":" not in line:
                continue
            key, value = line[2:].split(":", 1)
            fields[key.strip().replace("-", "_")] = value.strip()
    except OSError:
        fields["summary_error"] = "read_failed"
    return fields


def tail_lines(path: Path, count: int):
    if not path.exists():
        return []
    try:
        return path.read_text(encoding="utf-8", errors="replace").splitlines()[-count:]
    except OSError:
        return []


def parse_iso8601(value):
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip().replace("Z", "+00:00")
    try:
        dt = datetime.fromisoformat(text)
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def compute_freshness(timestamp_text, stale_after_seconds):
    ts = parse_iso8601(timestamp_text)
    if ts is None:
        return {
            "available": False,
            "timestamp": timestamp_text,
            "age_seconds": None,
            "stale_after_seconds": stale_after_seconds,
            "stale": None,
        }
    now = datetime.now(timezone.utc)
    age = max(int((now - ts).total_seconds()), 0)
    return {
        "available": True,
        "timestamp": timestamp_text,
        "age_seconds": age,
        "stale_after_seconds": stale_after_seconds,
        "stale": age > stale_after_seconds,
    }


def run_tmux(*args):
    try:
        proc = subprocess.run(
            ["tmux", *args],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError:
        return None, None, "tmux_unavailable"
    return proc.returncode, proc.stdout, proc.stderr


def observe_tmux(session_name):
    has_code, _, _ = run_tmux("has-session", "-t", session_name)
    if has_code is None:
        return {
            "available": False,
            "session": session_name,
            "session_present": None,
            "error": "tmux_unavailable",
        }
    if has_code != 0:
        return {
            "available": True,
            "session": session_name,
            "session_present": False,
            "error": "session_missing",
        }

    win_code, win_out, _ = run_tmux(
        "list-windows",
        "-t",
        session_name,
        "-F",
        "window=#{window_index} name=#{window_name} active=#{window_active} panes=#{window_panes}",
    )
    pane_code, pane_out, _ = run_tmux(
        "list-panes",
        "-t",
        session_name,
        "-F",
        "pane=#{pane_id} active=#{pane_active} pid=#{pane_pid} cmd=#{pane_current_command}",
    )
    cap_code, cap_out, _ = run_tmux("capture-pane", "-pt", session_name, "-S", "-40")

    windows = (
        [line.strip() for line in (win_out or "").splitlines() if line.strip()]
        if win_code == 0
        else []
    )
    panes = (
        [line.strip() for line in (pane_out or "").splitlines() if line.strip()]
        if pane_code == 0
        else []
    )
    tail = (
        [line.rstrip() for line in (cap_out or "").splitlines()[-10:]]
        if cap_code == 0
        else []
    )
    return {
        "available": True,
        "session": session_name,
        "session_present": True,
        "window_count": len(windows),
        "pane_count": len(panes),
        "windows": windows[:5],
        "panes": panes[:10],
        "pane_tail": tail,
    }


def summarize_watchdog():
    status = read_json_file(WATCHDOG_REPORT_FILE)
    data = status.get("data")
    if not isinstance(data, dict):
        return status
    boundaries = data.get("boundaries") if isinstance(data.get("boundaries"), dict) else {}
    codex_processes = data.get("codex_processes") if isinstance(data.get("codex_processes"), dict) else {}
    git = data.get("git") if isinstance(data.get("git"), dict) else {}
    takeover = data.get("takeover") if isinstance(data.get("takeover"), dict) else {}
    pane = data.get("pane") if isinstance(data.get("pane"), dict) else {}
    pane_tail = []
    raw_pane_tail = data.get("pane_tail")
    if isinstance(raw_pane_tail, list):
        pane_tail = [str(line) for line in raw_pane_tail][-10:]
    elif isinstance(pane.get("tail"), list):
        pane_tail = [str(line) for line in pane.get("tail", [])][-10:]
    generated_at = data.get("generated_at")
    return {
        "available": True,
        "readable": True,
        "path": str(WATCHDOG_REPORT_FILE),
        "schema_version": data.get("schema_version"),
        "generated_at": generated_at,
        "freshness": compute_freshness(generated_at, WATCHDOG_STALE_AFTER_SECONDS),
        "readonly": data.get("readonly") is True,
        "project_root": data.get("project_root"),
        "session": data.get("session"),
        "tmux_session_present": data.get("tmux_session_present"),
        "pane_bytes": pane.get("bytes"),
        "pane_tail": pane_tail,
        "codex_process_count": codex_processes.get("count"),
        "git_dirty": git.get("dirty"),
        "git_status_count": len(git.get("status_short", [])) if isinstance(git.get("status_short"), list) else None,
        "next_action": takeover.get("next_action"),
        "boundaries": {
            "dispatches_tasks": boundaries.get("dispatches_tasks") is True,
            "modifies_repo": boundaries.get("modifies_repo") is True,
            "restarts_worker": boundaries.get("restarts_worker") is True,
            "touches_services": boundaries.get("touches_services") is True,
        },
    }


def candidate_status_files(latest_dir):
    candidates = []
    if OVERNIGHT_STATUS_FILE:
        candidates.append(Path(OVERNIGHT_STATUS_FILE))
    candidates.append(RUN_ROOT / "latest-run-status.json")
    candidates.append(RUN_ROOT / "status.json")
    if latest_dir is not None:
        candidates.extend(
            [
                latest_dir / "status.json",
                latest_dir / "run-status.json",
                latest_dir / "latest-run-status.json",
            ]
        )
    seen = set()
    result = []
    for path in candidates:
        key = str(path)
        if key not in seen:
            seen.add(key)
            result.append(path)
    return result


def summarize_overnight():
    run_dirs = list_run_dirs(RUN_ROOT)
    latest_dir = run_dirs[0] if run_dirs else None
    status_json = None
    for path in candidate_status_files(latest_dir):
        if path.exists():
            status_json = read_json_file(path)
            break
    if status_json is None:
        selected_status_path = Path(OVERNIGHT_STATUS_FILE) if OVERNIGHT_STATUS_FILE else RUN_ROOT / "latest-run-status.json"
        status_json = {
            "available": False,
            "readable": False,
            "path": str(selected_status_path),
            "error": "missing",
        }
    result = {
        "run_root": str(RUN_ROOT),
        "run_root_available": RUN_ROOT.is_dir(),
        "latest_run_dir": str(latest_dir) if latest_dir is not None else None,
        "status_json": status_json,
        "summary": None,
        "logs": None,
    }
    if latest_dir is not None:
        summary_path = latest_dir / "summary.md"
        run_log = latest_dir / "run.log"
        last_message = latest_dir / "last-message.md"
        events_jsonl = latest_dir / "events.jsonl"
        summary = parse_summary(summary_path)
        result["summary"] = {
            "path": str(summary_path),
            "available": summary_path.exists(),
            "fields": summary,
        }
        result["logs"] = {
            "run_log": str(run_log),
            "run_log_available": run_log.exists(),
            "last_message": str(last_message),
            "last_message_available": last_message.exists(),
            "events_jsonl": str(events_jsonl),
            "events_jsonl_available": events_jsonl.exists(),
            "run_log_tail": tail_lines(run_log, 5),
            "last_message_tail": tail_lines(last_message, 5),
        }
    status_data = result["status_json"].get("data")
    status_ts = None
    if isinstance(status_data, dict):
        for key in ("updated_at", "generated_at", "timestamp", "last_updated_at"):
            value = status_data.get(key)
            if isinstance(value, str) and value.strip():
                status_ts = value
                break
    result["freshness"] = compute_freshness(status_ts, OVERNIGHT_STALE_AFTER_SECONDS)
    return result


def infer_interactive_state(tmux_observation, watchdog):
    tail_tmux = tmux_observation.get("pane_tail")
    if not isinstance(tail_tmux, list):
        tail_tmux = []
    tail_watchdog = watchdog.get("pane_tail")
    if not isinstance(tail_watchdog, list):
        tail_watchdog = []
    merged_tail = [*tail_tmux, *tail_watchdog]
    corpus = "\n".join(str(line).strip() for line in merged_tail if str(line).strip()).lower()
    if not corpus:
        if tmux_observation.get("session_present") is False:
            return {
                "interactive_state": "session_missing",
                "activity_hint": "tmux session missing; interactive goal worker not observed",
            }
        return {
            "interactive_state": "unknown",
            "activity_hint": "no pane tail captured; inspect tmux pane for current state",
        }
    if "compacted" in corpus or "context compact" in corpus:
        return {
            "interactive_state": "compacting_context",
            "activity_hint": "context compaction detected; still processing",
        }
    if "working" in corpus:
        return {
            "interactive_state": "working",
            "activity_hint": "agent is working through current task steps",
        }
    if any(marker in corpus for marker in ("planning", "examining", "investigating", "exploring", "running")):
        return {
            "interactive_state": "working",
            "activity_hint": "agent is actively inspecting or executing task steps",
        }
    if "thinking" in corpus:
        return {
            "interactive_state": "thinking",
            "activity_hint": "agent is thinking; no immediate intervention needed",
        }
    if "press enter" in corpus or "waiting for input" in corpus:
        return {
            "interactive_state": "idle_waiting_input",
            "activity_hint": "prompt appears idle or waiting for input",
        }
    return {
        "interactive_state": "active_unclassified",
        "activity_hint": "interactive pane updated but state is not classified",
    }


def infer_overall(watchdog, overnight, tmux_observation):
    watchdog_fresh = (watchdog.get("freshness") or {}).get("stale") is False
    watchdog_worker_active = (
        watchdog.get("tmux_session_present") is True and (watchdog.get("codex_process_count") or 0) > 0
    )
    interactive_active = (
        watchdog_worker_active
        or (
            tmux_observation.get("session_present") is True
            and tmux_observation.get("pane_count", 0) > 0
        )
    )
    overnight_stale = (overnight.get("freshness") or {}).get("stale") is True

    if interactive_active and overnight_stale:
        return "interactive_active_overnight_stale"
    if interactive_active:
        return "interactive_active"
    if watchdog.get("available") and watchdog.get("readable"):
        if watchdog.get("tmux_session_present") is True:
            return "terminal_session_present_needs_inspection"
        if watchdog_fresh and watchdog.get("tmux_session_present") is False:
            return "terminal_session_missing_watchdog_fresh"
    if overnight_stale:
        return "overnight_stale_needs_refresh"
    status_json = overnight.get("status_json", {})
    data = status_json.get("data")
    if isinstance(data, dict):
        status = data.get("status")
        if status:
            return f"overnight_status_{status}"
    summary = overnight.get("summary") or {}
    fields = summary.get("fields") or {}
    if fields.get("status"):
        return f"overnight_summary_{fields['status']}"
    if overnight.get("latest_run_dir"):
        return "overnight_run_dir_observed"
    return "no_goal_worker_status_found"


watchdog = summarize_watchdog()
overnight = summarize_overnight()
tmux_observation = observe_tmux(TMUX_SESSION)
interactive = infer_interactive_state(tmux_observation, watchdog)
payload = {
    "ok": True,
    "schema_version": 1,
    "readonly_boundaries": BOUNDARIES,
    "watchdog": watchdog,
    "overnight": overnight,
    "tmux_observation": tmux_observation,
    "interactive_state": interactive["interactive_state"],
    "activity_hint": interactive["activity_hint"],
    "freshness": {
        "watchdog": watchdog.get("freshness"),
        "overnight": overnight.get("freshness"),
    },
    "overall_status": infer_overall(watchdog, overnight, tmux_observation),
}

if FORMAT == "json":
    print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
else:
    print("chuang_goal_run_status_ok: true")
    print("schema_version: 1")
    print("overall_status: " + payload["overall_status"])
    print("readonly: true")
    print("boundaries: dispatches_tasks=false starts_worker=false restarts_worker=false modifies_repo=false deletes_logs=false touches_services=false")
    print("watchdog_available: " + str(watchdog.get("available", False)).lower())
    print("watchdog_readable: " + str(watchdog.get("readable", False)).lower())
    print("watchdog_report_file: " + str(watchdog.get("path")))
    print("watchdog_generated_at: " + str(watchdog.get("generated_at") or "unknown"))
    print("watchdog_freshness_stale: " + str((watchdog.get("freshness") or {}).get("stale", "unknown")).lower())
    print("watchdog_freshness_age_seconds: " + str((watchdog.get("freshness") or {}).get("age_seconds", "unknown")))
    print("watchdog_session: " + str(watchdog.get("session") or "unknown"))
    print("watchdog_tmux_session_present: " + str(watchdog.get("tmux_session_present", "unknown")).lower())
    print("watchdog_codex_process_count: " + str(watchdog.get("codex_process_count") if watchdog.get("codex_process_count") is not None else "unknown"))
    print("watchdog_git_dirty: " + str(watchdog.get("git_dirty", "unknown")).lower())
    print("watchdog_next_action: " + str(watchdog.get("next_action") or watchdog.get("error") or "unknown"))
    print("overnight_run_root: " + str(overnight.get("run_root")))
    print("overnight_latest_run_dir: " + str(overnight.get("latest_run_dir") or "none"))
    status_json = overnight.get("status_json", {})
    print("overnight_status_json_available: " + str(status_json.get("available", False)).lower())
    print("overnight_status_json_file: " + str(status_json.get("path")))
    overnight_freshness = overnight.get("freshness") or {}
    print("overnight_freshness_stale: " + str(overnight_freshness.get("stale", "unknown")).lower())
    print("overnight_freshness_age_seconds: " + str(overnight_freshness.get("age_seconds", "unknown")))
    summary = overnight.get("summary") or {}
    fields = summary.get("fields") or {}
    print("overnight_summary_status: " + str(fields.get("status") or "unknown"))
    print("overnight_summary_iterations: " + str(fields.get("iterations") or "unknown"))
    tmux = payload.get("tmux_observation") or {}
    print("tmux_observation_session_present: " + str(tmux.get("session_present", "unknown")).lower())
    print("tmux_observation_window_count: " + str(tmux.get("window_count", "unknown")))
    print("tmux_observation_pane_count: " + str(tmux.get("pane_count", "unknown")))
    print("interactive_state: " + str(payload.get("interactive_state") or "unknown"))
    print("activity_hint: " + str(payload.get("activity_hint") or "unknown"))
PY
