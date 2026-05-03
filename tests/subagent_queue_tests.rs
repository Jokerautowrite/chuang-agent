use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, TaskId, Timestamp};
use chuang_agent::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{
    ContextIsolation, QueuedSubagentSpawner, RunId, SpawnRequest, SubagentDispatch,
    SubagentSpawner, SubagentState, SubagentToolPolicy,
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
    let read_back = queue
        .read_dispatch(&dispatch.run_id)
        .expect("dispatch should read")
        .expect("dispatch should exist");
    let payload = std::fs::read_to_string(path).expect("dispatch file should be readable");
    let decoded: SubagentDispatch =
        serde_json::from_str(&payload).expect("dispatch json should decode");

    assert_eq!(decoded, dispatch);
    assert_eq!(read_back, dispatch);
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
fn file_subagent_queue_lists_dispatches_and_report_run_ids() {
    let root = temp_root("list");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let mut dispatch = sample_dispatch();
    queue
        .write_dispatch(&dispatch)
        .expect("first dispatch should write");
    dispatch.run_id = RunId("queued-run-2".to_string());
    dispatch.task_id = TaskId("task-2".to_string());
    queue
        .write_dispatch(&dispatch)
        .expect("second dispatch should write");
    queue
        .write_report_for_test(&RunId("queued-run-2".to_string()), &sample_report())
        .expect("report should write");

    let dispatches = queue.list_dispatches().expect("dispatches should list");
    let report_run_ids = queue
        .list_report_run_ids()
        .expect("report run ids should list");

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].run_id.0, "queued-run-1");
    assert_eq!(dispatches[1].run_id.0, "queued-run-2");
    assert_eq!(report_run_ids, vec![RunId("queued-run-2".to_string())]);
}

#[test]
fn file_subagent_queue_claims_dispatch_once_without_deleting_dispatch() {
    let root = temp_root("claim");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let dispatch = sample_dispatch();
    queue
        .write_dispatch(&dispatch)
        .expect("dispatch should write");

    let first_claim = queue
        .claim_dispatch(&dispatch.run_id, "worker-a")
        .expect("first claim should succeed");
    let second_claim = queue
        .claim_dispatch(&dispatch.run_id, "worker-b")
        .expect("second claim should not error");
    let claim_run_ids = queue
        .list_claim_run_ids()
        .expect("claim run ids should list");

    assert!(first_claim.is_some());
    assert_eq!(second_claim, None);
    assert_eq!(claim_run_ids, vec![dispatch.run_id.clone()]);
    assert!(queue
        .read_dispatch(&dispatch.run_id)
        .expect("dispatch should read")
        .is_some());
    assert!(queue
        .is_claimed(&dispatch.run_id)
        .expect("claimed state should read"));
    assert!(root.join("claims").join("queued-run-1.json").exists());
    assert!(root.join("dispatch").join("queued-run-1.json").exists());
}

#[test]
fn file_subagent_queue_can_release_and_reclaim_without_deleting_history() {
    let root = temp_root("release-claim");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let dispatch = sample_dispatch();
    queue
        .write_dispatch(&dispatch)
        .expect("dispatch should write");
    queue
        .claim_dispatch(&dispatch.run_id, "worker-a")
        .expect("first claim should write")
        .expect("first claim should exist");

    let release_path = queue
        .release_claim(&dispatch.run_id, "operator", "manual release")
        .expect("release should write");
    assert!(!queue
        .is_claimed(&dispatch.run_id)
        .expect("released claim should not be active"));

    let reclaim = queue
        .claim_dispatch(&dispatch.run_id, "worker-b")
        .expect("reclaim should not error");

    assert!(release_path.exists());
    assert!(reclaim.is_some());
    assert!(queue
        .is_claimed(&dispatch.run_id)
        .expect("reclaimed claim should be active"));
    assert!(root.join("claims").join("queued-run-1.json").exists());
    assert!(root
        .join("claim-releases")
        .join("queued-run-1.json")
        .exists());
    assert!(root.join("dispatch").join("queued-run-1.json").exists());
}

#[test]
fn file_subagent_queue_can_reclaim_stale_claim_without_deleting_dispatch() {
    let root = temp_root("stale-claim");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let dispatch = sample_dispatch();
    queue
        .write_dispatch(&dispatch)
        .expect("dispatch should write");
    queue
        .claim_dispatch(&dispatch.run_id, "worker-a")
        .expect("first claim should write")
        .expect("first claim should exist");

    assert!(queue
        .is_claim_stale(&dispatch.run_id, 0)
        .expect("stale state should read"));
    let reclaim = queue
        .claim_dispatch_with_timeout(&dispatch.run_id, "worker-b", Some(0))
        .expect("stale reclaim should not error");
    let claim_payload = std::fs::read_to_string(queue.claim_path(&dispatch.run_id))
        .expect("claim payload should read");

    assert!(reclaim.is_some());
    assert!(claim_payload.contains("worker-b"));
    assert!(root.join("dispatch").join("queued-run-1.json").exists());
    assert!(!root
        .join("claim-releases")
        .join("queued-run-1.json")
        .exists());
}

#[test]
fn file_subagent_queue_list_ignores_non_json_files() {
    let root = temp_root("list-ignore");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    std::fs::write(root.join("dispatch").join("note.txt"), "ignore")
        .expect("non-json file should write");

    let dispatches = queue.list_dispatches().expect("dispatches should list");

    assert!(dispatches.is_empty());
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

#[test]
fn file_subagent_queue_attaches_present_report_to_spawner() {
    let root = temp_root("attach-report");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(sample_spawn_request("task-1"))
        .expect("spawn should succeed");
    queue
        .write_report_for_test(&receipt.run_id, &sample_report())
        .expect("report should write");

    let attached = queue
        .attach_report_if_present(&mut spawner, &receipt.run_id)
        .expect("report should attach");
    let report = spawner
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("attached report should be available");

    assert!(attached);
    assert_eq!(report.summary, "queued worker completed");
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Completed)
    );
}

#[test]
fn file_subagent_queue_attach_returns_false_when_report_missing() {
    let root = temp_root("attach-missing-report");
    let queue =
        FileSubagentQueue::open(FileSubagentQueueConfig::new(&root)).expect("queue should open");
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(sample_spawn_request("task-1"))
        .expect("spawn should succeed");

    let attached = queue
        .attach_report_if_present(&mut spawner, &receipt.run_id)
        .expect("missing report should not error");

    assert!(!attached);
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Running)
    );
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
        governance_decision: None,
        truncated: false,
    }
}
