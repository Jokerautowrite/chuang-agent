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
