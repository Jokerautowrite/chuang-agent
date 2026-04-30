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

    let run_id = parsed["run_id"].as_str().expect("run_id should be string");
    let agent_id = parsed["agent_id"]
        .as_str()
        .expect("agent_id should be string");
    assert!(run_id.starts_with("queued-cli-"));
    assert!(agent_id.starts_with("worker-"));
    assert_eq!(parsed["task_id"], "task-cli-1");
    assert_eq!(parsed["queue_root"], queue_root.display().to_string());

    let dispatch_path = queue_root.join("dispatch").join(format!("{run_id}.json"));
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
fn cli_subagent_dispatch_can_queue_multiple_tasks_in_same_directory() {
    let queue_root = temp_queue_root("multi-dispatch");
    let first = dispatch_task(&queue_root, "task-cli-1", "审计第一段");
    let second = dispatch_task(&queue_root, "task-cli-2", "审计第二段");

    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");

    assert_ne!(first_run, second_run);
    assert!(queue_root
        .join("dispatch")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("dispatch")
        .join(format!("{second_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_list_reports_dispatches_and_report_presence() {
    let queue_root = temp_queue_root("list");
    let first = dispatch_task(&queue_root, "task-cli-1", "审计第一段");
    let second = dispatch_task(&queue_root, "task-cli-2", "审计第二段");
    let second_run = second["run_id"].as_str().expect("second run id");
    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root
            .join("reports")
            .join(format!("{second_run}.json")),
        sample_report_json("second completed"),
    )
    .expect("report should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "list",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let first_run = first["run_id"].as_str().expect("first run id");

    assert_eq!(parsed["dispatch_count"], 2);
    assert_eq!(parsed["report_count"], 1);
    assert!(parsed["items"]
        .as_array()
        .expect("items should be array")
        .iter()
        .any(|item| item["run_id"] == first_run && item["has_report"] == false));
    assert!(parsed["items"]
        .as_array()
        .expect("items should be array")
        .iter()
        .any(|item| item["run_id"] == second_run && item["has_report"] == true));
}

#[test]
fn cli_subagent_run_once_fake_runner_writes_report_for_first_pending_dispatch() {
    let queue_root = temp_queue_root("run-once");
    let first = dispatch_task(&queue_root, "task-cli-1", "审计第一段");
    let second = dispatch_task(&queue_root, "task-cli-2", "审计第二段");
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "fake");
    assert_eq!(parsed["ran"], true);
    assert_eq!(parsed["run_id"], first_run);
    assert!(queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(!queue_root
        .join("reports")
        .join(format!("{second_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_run_once_reports_idle_when_no_pending_dispatch_exists() {
    let queue_root = temp_queue_root("run-once-idle");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "fake");
    assert_eq!(parsed["ran"], false);
    assert_eq!(parsed["summary"], "no pending dispatch");
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

fn dispatch_task(queue_root: &std::path::Path, task_id: &str, task: &str) -> Value {
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
            task,
            "--task-id",
            task_id,
            "--agent-name",
            "worker",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout should be json")
}
