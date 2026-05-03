#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import time


def main() -> int:
    dispatch = json.load(sys.stdin)
    report = run_dispatch(dispatch)
    json.dump(report, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


def run_dispatch(dispatch: dict) -> dict:
    started = timestamp()
    run_id = dispatch["run_id"]
    task_id = dispatch["task_id"]
    agent_id = dispatch["agent_id"]
    parent_agent_id = dispatch.get("parent_agent_id")
    enabled = os.environ.get("CHUANG_CODEX_RUNNER_ENABLE") == "1"
    codex_bin = os.environ.get("CHUANG_CODEX_BIN", "codex")
    workspace = os.environ.get("CHUANG_CODEX_RUNNER_WORKSPACE", os.getcwd())
    timeout_ms = int(dispatch.get("idle_timeout_ms") or 30000)

    if not enabled:
        return report(
            dispatch,
            started,
            status="Failed",
            exit_code=2,
            summary=f"codex runner disabled for {task_id}; set CHUANG_CODEX_RUNNER_ENABLE=1",
            stdout="",
            stderr="codex runner disabled by default",
            replay_ref=f"queued-subagent-codex://{run_id}",
        )

    prompt = build_prompt(dispatch)
    try:
        completed = subprocess.run(
            [codex_bin, "exec", prompt],
            cwd=workspace,
            text=True,
            capture_output=True,
            timeout=max(timeout_ms / 1000.0, 1.0),
            check=False,
        )
        status = "Success" if completed.returncode == 0 else "Failed"
        return report(
            dispatch,
            started,
            status=status,
            exit_code=completed.returncode,
            summary=f"codex runner completed {task_id} exit_code={completed.returncode}",
            stdout=completed.stdout,
            stderr=completed.stderr,
            replay_ref=f"queued-subagent-codex://{run_id}",
        )
    except subprocess.TimeoutExpired as error:
        return report(
            dispatch,
            started,
            status="TimedOut",
            exit_code=None,
            summary=f"codex runner timed out {task_id} after {timeout_ms}ms",
            stdout=error.stdout or "",
            stderr=error.stderr or f"timed out after {timeout_ms}ms",
            replay_ref=f"queued-subagent-codex://{run_id}",
        )
    except OSError as error:
        return report(
            dispatch,
            started,
            status="Failed",
            exit_code=None,
            summary=f"codex runner spawn failed {task_id}",
            stdout="",
            stderr=str(error),
            replay_ref=f"queued-subagent-codex://{run_id}",
        )


def build_prompt(dispatch: dict) -> str:
    metadata = dispatch.get("metadata") or {}
    return "\n".join(
        [
            "You are a Chuang queued subagent runner.",
            "Follow the dispatch exactly and keep changes scoped.",
            f"task_id: {dispatch.get('task_id', '')}",
            f"tool_policy: {dispatch.get('tool_policy', '')}",
            f"required_capabilities: {metadata.get('required_capabilities', '')}",
            "",
            "Task:",
            dispatch.get("task", ""),
            "",
            "Return a concise summary of completed work and verification.",
        ]
    )


def report(
    dispatch: dict,
    started: str,
    *,
    status: str,
    exit_code,
    summary: str,
    stdout: str,
    stderr: str,
    replay_ref: str,
) -> dict:
    return {
        "schema_version": "1.0",
        "report_id": f"report-{dispatch['run_id']}",
        "task_id": dispatch["task_id"],
        "agent_id": dispatch["agent_id"],
        "parent_agent_id": dispatch.get("parent_agent_id"),
        "status": status,
        "started_at": started,
        "finished_at": timestamp(),
        "summary": summary,
        "exit_code": exit_code,
        "stdout_preview": preview(stdout),
        "stderr_preview": preview(stderr) if stderr else None,
        "resource_usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "wall_time_ms": 0,
            "cpu_time_ms": 0,
            "peak_memory_bytes": 0,
        },
        "artifacts": [],
        "replay_ref": replay_ref,
        "context_debug": None,
        "governance_decision": None,
        "truncated": len(stdout) > 1200 or len(stderr) > 1200,
    }


def preview(value) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return str(value)[:1200]


def timestamp() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


if __name__ == "__main__":
    raise SystemExit(main())
