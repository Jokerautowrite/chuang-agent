use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, TaskId, Timestamp};
use chuang_agent::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{
    ContextIsolation, QueuedSubagentSpawner, RunId, SpawnRequest, SubagentDispatch,
    SubagentSpawner, SubagentToolPolicy,
};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-subagent-queue-{name}-{nanos}"))
}

#[test]
fn file_subagent_queue_writes_dispatch_json() {
    let root = temp_root("dispatch");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let dispatch = sample_dispatch();

    let path = queue
        .write_dispatch(&dispatch)
        .expect("dispatch should write");
    let payload = std::fs::read_to_string(path).expect("dispatch file should be readable");
    let decoded: SubagentDispatch =
        serde_json::from_str(&payload).expect("dispatch json should decode");

    assert_eq!(decoded, dispatch);
    assert!(root.join("dispatch").join("queued-run-1.json").exists());
}

#[test]
fn file_subagent_queue_returns_none_when_report_is_missing() {
    let root = temp_root("missing-report");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");

    let report = queue
        .read_report(&RunId("queued-run-1".to_string()))
        .expect("missing report should not error");

    assert_eq!(report, None);
}

#[test]
fn file_subagent_queue_reads_report_json() {
    let root = temp_root("report");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let run_id = RunId("queued-run-1".to_string());
    let report = sample_report();

    queue
        .write_report_for_test(&run_id, &report)
        .expect("report should write");
    let loaded = queue
        .read_report(&run_id)
        .expect("report should read")
        .expect("report should exist");

    assert_eq!(loaded, report);
}

#[test]
fn file_subagent_queue_flushes_pending_dispatches_from_spawner() {
    let root = temp_root("flush");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let mut spawner = QueuedSubagentSpawner::new();
    spawner
        .spawn(sample_spawn_request("task-1"))
        .expect("first spawn should succeed");
    spawner
        .spawn(sample_spawn_request("task-2"))
        .expect("second spawn should succeed");

    let paths = queue
        .flush_pending_dispatches(&spawner)
        .expect("pending dispatches should flush");

    assert_eq!(paths.len(), 2);
    assert!(root.join("dispatch").join("queued-run-1.json").exists());
    assert!(root.join("dispatch").join("queued-run-2.json").exists());
}

fn sample_dispatch() -> SubagentDispatch {
    SubagentDispatch {
        run_id: RunId("queued-run-1".to_string()),
        agent_id: AgentId("worker-1".to_string()),
        task_id: TaskId("task-1".to_string()),
        parent_agent_id: AgentId("xiaoce".to_string()),
        agent_name: "worker".to_string(),
        task: "生成文件队列任务".to_string(),
        tool_policy: SubagentToolPolicy::Analyze,
        context_isolation: ContextIsolation::Isolated,
        token_budget: 512,
        idle_timeout_ms: 30_000,
        recursive_spawn: false,
        metadata: BTreeMap::from([("scope".to_string(), "mvp".to_string())]),
    }
}

fn sample_spawn_request(task_id: &str) -> SpawnRequest {
    SpawnRequest {
        task_id: TaskId(task_id.to_string()),
        parent_agent_id: AgentId("xiaoce".to_string()),
        agent_name: "worker".to_string(),
        task: "生成文件队列任务".to_string(),
        tool_policy: SubagentToolPolicy::Analyze,
        context_isolation: ContextIsolation::Isolated,
        token_budget: 512,
        idle_timeout_ms: 30_000,
        recursive_spawn: false,
        metadata: BTreeMap::new(),
    }
}

fn sample_report() -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId("report-queued-run-1".to_string()),
        task_id: TaskId("task-1".to_string()),
        agent_id: AgentId("worker-1".to_string()),
        parent_agent_id: Some(AgentId("xiaoce".to_string())),
        status: ExecutionStatus::Success,
        started_at: Timestamp("2026-05-01T00:00:00Z".to_string()),
        finished_at: Timestamp("2026-05-01T00:00:01Z".to_string()),
        summary: "queued worker completed".to_string(),
        exit_code: Some(0),
        stdout_preview: Some("ok".to_string()),
        stderr_preview: None,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some("queued-subagent://queued-run-1".to_string()),
        context_debug: None,
        truncated: false,
    }
}
