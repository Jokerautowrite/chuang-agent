#!/usr/bin/env python3
"""Chuang queued subagent → local Codex exec runner.

Reads one dispatch JSON from stdin, writes one report JSON to stdout.
Hardening (2026-07-18):
- codex child gets stdin=DEVNULL (avoid "Reading additional input from stdin" races)
- on failure, stderr_preview keeps HEAD+TAIL so the real error is not truncated away
- one quick retry when codex exits non-zero in under a few seconds (transient provider blips)
- wall_time_ms recorded for diagnostics
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time


PREVIEW_LIMIT = 1600
QUICK_FAIL_RETRY_SEC = 5.0


def main() -> int:
    dispatch = json.load(sys.stdin)
    report = run_dispatch(dispatch)
    json.dump(report, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


def run_dispatch(dispatch: dict) -> dict:
    started = timestamp()
    started_mono = time.monotonic()
    run_id = dispatch["run_id"]
    task_id = dispatch["task_id"]
    enabled = os.environ.get("CHUANG_CODEX_RUNNER_ENABLE") == "1"
    codex_bin = os.environ.get("CHUANG_CODEX_BIN", "codex")
    workspace = os.environ.get("CHUANG_CODEX_RUNNER_WORKSPACE", os.getcwd())
    model = os.environ.get("CHUANG_CODEX_RUNNER_MODEL", "gpt-5.6-luna")
    timeout_ms = int(dispatch.get("idle_timeout_ms") or 30000)

    if not enabled:
        return report(
            dispatch,
            started,
            started_mono,
            status="Failed",
            exit_code=2,
            summary=f"codex runner disabled for {task_id}; set CHUANG_CODEX_RUNNER_ENABLE=1",
            stdout="",
            stderr="codex runner disabled by default",
            replay_ref=f"queued-subagent-codex://{run_id}",
        )

    prompt = build_prompt(dispatch)
    sandbox = "read-only" if str(dispatch.get("tool_policy")) == "Analyze" else "workspace-write"
    attempts: list[tuple[int | None, str, str, float]] = []

    try:
        for attempt in range(2):
            attempt_started = time.monotonic()
            completed = subprocess.run(
                [
                    codex_bin,
                    "exec",
                    "--model",
                    model,
                    "--cd",
                    workspace,
                    "--sandbox",
                    sandbox,
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "-c",
                    'approval_policy="never"',
                    prompt,
                ],
                cwd=workspace,
                text=True,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=max(timeout_ms / 1000.0, 1.0),
                check=False,
            )
            elapsed = time.monotonic() - attempt_started
            attempts.append(
                (completed.returncode, completed.stdout or "", completed.stderr or "", elapsed)
            )
            if completed.returncode == 0:
                break
            # One quick retry for near-instant provider blips (rate limit / auth race).
            if attempt == 0 and elapsed < QUICK_FAIL_RETRY_SEC:
                time.sleep(0.8)
                continue
            break

        returncode, stdout, stderr, last_elapsed = attempts[-1]
        # If stdout empty, try to salvage final agent text from codex transcript on stderr.
        if not stdout.strip():
            salvaged = extract_codex_final_message(stderr)
            if salvaged:
                stdout = salvaged + "\n"

        status = "Success" if returncode == 0 else "Failed"
        summary = f"codex runner completed {task_id} exit_code={returncode}"
        if returncode != 0:
            err_tail = error_tail(stderr)
            if err_tail:
                summary = f"{summary}; error_tail={err_tail}"
            if len(attempts) > 1:
                summary = f"{summary}; attempts={len(attempts)}"
        return report(
            dispatch,
            started,
            started_mono,
            status=status,
            exit_code=returncode,
            summary=summary,
            stdout=stdout,
            stderr=stderr,
            replay_ref=f"queued-subagent-codex://{run_id}",
            last_elapsed_ms=int(last_elapsed * 1000),
        )
    except subprocess.TimeoutExpired as error:
        return report(
            dispatch,
            started,
            started_mono,
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
            started_mono,
            status="Failed",
            exit_code=None,
            summary=f"codex runner spawn failed {task_id}: {error}",
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
            "Work autonomously inside the provided workspace. Read, write, patch, build, test, and scan when the assigned policy permits.",
            "Never delete, clean, reset, uninstall, use sudo/root, alter system services or network settings, handle payments or verification codes, export secrets, or push externally.",
            "Do not spawn another subagent. Do not write Chuang core memory.",
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
    started_mono: float,
    *,
    status: str,
    exit_code,
    summary: str,
    stdout: str,
    stderr: str,
    replay_ref: str,
    last_elapsed_ms: int | None = None,
) -> dict:
    wall = int((time.monotonic() - started_mono) * 1000)
    if last_elapsed_ms is not None:
        wall = max(wall, last_elapsed_ms)
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
        "stdout_preview": preview_prefer_tail(stdout, prefer_tail=False),
        "stderr_preview": preview_prefer_tail(stderr, prefer_tail=(status != "Success")),
        "resource_usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "wall_time_ms": wall,
            "cpu_time_ms": 0,
            "peak_memory_bytes": 0,
        },
        "artifacts": [],
        "replay_ref": replay_ref,
        "context_debug": None,
        "governance_decision": None,
        "truncated": len(stdout) > PREVIEW_LIMIT or len(stderr) > PREVIEW_LIMIT,
    }


def preview_prefer_tail(value, *, prefer_tail: bool) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    text = str(value)
    if len(text) <= PREVIEW_LIMIT:
        return text
    if not prefer_tail:
        return text[:PREVIEW_LIMIT]
    # Failure path: keep a short head (session banner) + long tail (actual error).
    head = 280
    tail = PREVIEW_LIMIT - head - 24
    return text[:head] + "\n...[truncated middle]...\n" + text[-tail:]


def error_tail(stderr: str, max_chars: int = 240) -> str:
    if not stderr:
        return ""
    # Prefer lines after last "codex" marker or last non-empty lines.
    lines = [ln.rstrip() for ln in stderr.splitlines() if ln.strip()]
    if not lines:
        return ""
    chunk = " | ".join(lines[-6:])
    chunk = " ".join(chunk.split())
    if len(chunk) > max_chars:
        chunk = "…" + chunk[-(max_chars - 1) :]
    return chunk


def extract_codex_final_message(stderr: str) -> str:
    """Best-effort: Codex CLI often prints the final agent message under a `codex` heading."""
    if not stderr:
        return ""
    lines = stderr.splitlines()
    # Find last standalone 'codex' line, then take following non-meta lines until tokens/footer.
    last_idx = -1
    for i, line in enumerate(lines):
        if line.strip() == "codex":
            last_idx = i
    if last_idx < 0:
        return ""
    body: list[str] = []
    for line in lines[last_idx + 1 :]:
        s = line.strip()
        if not s:
            if body:
                break
            continue
        if s.startswith("tokens used") or s.startswith("--------") or s == "user":
            break
        body.append(line.rstrip())
    return "\n".join(body).strip()


def timestamp() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


if __name__ == "__main__":
    raise SystemExit(main())
