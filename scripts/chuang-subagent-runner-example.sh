#!/bin/sh
set -eu

python3 -c '
import json
import sys

dispatch = json.load(sys.stdin)
run_id = dispatch["run_id"]
task_id = dispatch["task_id"]
agent_id = dispatch["agent_id"]
parent_agent_id = dispatch.get("parent_agent_id")
task = dispatch.get("task", "")
metadata = dispatch.get("metadata", {})
required_capabilities = metadata.get("required_capabilities", "")

report = {
    "schema_version": "1.0",
    "report_id": f"report-{run_id}",
    "task_id": task_id,
    "agent_id": agent_id,
    "parent_agent_id": parent_agent_id,
    "status": "Success",
    "started_at": "2026-05-02T00:00:00Z",
    "finished_at": "2026-05-02T00:00:01Z",
    "summary": f"example subagent runner accepted task {task_id}",
    "exit_code": 0,
    "stdout_preview": f"task={task}; required_capabilities={required_capabilities}",
    "stderr_preview": None,
    "resource_usage": {
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "wall_time_ms": 1000,
        "cpu_time_ms": 0,
        "peak_memory_bytes": 0,
    },
    "artifacts": [],
    "replay_ref": f"queued-subagent-example://{run_id}",
    "context_debug": None,
    "governance_decision": None,
    "truncated": False,
}

json.dump(report, sys.stdout, ensure_ascii=False)
sys.stdout.write("\n")
'
