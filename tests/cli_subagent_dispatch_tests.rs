use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_queue_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-subagent-{name}-{nanos}"))
}

#[test]
fn cli_subagent_dispatch_writes_queued_dispatch_json() {
    let queue_root = temp_queue_root("dispatch");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "dispatch",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--task",
            "审计 runtime 子代理队列",
            "--task-id",
            "task-cli-1",
            "--agent-name",
            "worker",
            "--policy",
            "execute",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["run_id"], "queued-run-1");
    assert_eq!(parsed["agent_id"], "worker-1");
    assert_eq!(parsed["task_id"], "task-cli-1");
    assert_eq!(parsed["queue_root"], queue_root.display().to_string());

    let dispatch_path = queue_root.join("dispatch").join("queued-run-1.json");
    assert_eq!(parsed["dispatch_path"], dispatch_path.display().to_string());
    let dispatch: Value = serde_json::from_str(
        &std::fs::read_to_string(dispatch_path).expect("dispatch file should exist"),
    )
    .expect("dispatch should be json");

    assert_eq!(dispatch["task_id"], "task-cli-1");
    assert_eq!(dispatch["task"], "审计 runtime 子代理队列");
    assert_eq!(dispatch["tool_policy"], "Execute");
    assert_eq!(dispatch["metadata"]["source"], "cli");
}

#[test]
fn cli_subagent_dispatch_requires_task() {
    let queue_root = temp_queue_root("missing-task");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "dispatch",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("subagent dispatch requires --task"));
}

#[test]
fn cli_subagent_report_reads_available_report_json() {
    let queue_root = temp_queue_root("report-available");
    let reports = queue_root.join("reports");
    std::fs::create_dir_all(&reports).expect("reports dir should be created");
    std::fs::write(
        reports.join("queued-run-1.json"),
        sample_report_json("queued worker completed"),
    )
    .expect("report should be written");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "queued-run-1",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["run_id"], "queued-run-1");
    assert_eq!(parsed["available"], true);
    assert_eq!(parsed["report"]["summary"], "queued worker completed");
    assert_eq!(parsed["report"]["status"], "Success");
}

#[test]
fn cli_subagent_report_marks_missing_report_without_error() {
    let queue_root = temp_queue_root("report-missing");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "queued-run-404",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["run_id"], "queued-run-404");
    assert_eq!(parsed["available"], false);
    assert_eq!(parsed["report"], Value::Null);
}

fn sample_report_json(summary: &str) -> String {
    format!(
        r#"{{
  "schema_version": "1.0",
  "report_id": "report-queued-run-1",
  "task_id": "task-cli-1",
  "agent_id": "worker-1",
  "parent_agent_id": "chuang-cli",
  "status": "Success",
  "started_at": "2026-05-01T00:00:00Z",
  "finished_at": "2026-05-01T00:00:01Z",
  "summary": "{summary}",
  "exit_code": 0,
  "stdout_preview": "ok",
  "stderr_preview": null,
  "resource_usage": {{
    "wall_time_ms": 1000,
    "prompt_tokens": 10,
    "completion_tokens": 5,
    "cpu_time_ms": 0,
    "peak_memory_bytes": 0
  }},
  "artifacts": [],
  "replay_ref": "queued-subagent://queued-run-1",
  "context_debug": null,
  "truncated": false
}}"#
    )
}
