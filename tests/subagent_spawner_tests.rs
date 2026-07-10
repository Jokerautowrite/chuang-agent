use std::cell::Cell;
use std::collections::BTreeMap;

use chuang_agent::common::{AgentId, ReportId, TaskId, Timestamp};
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{
    ContextIsolation, FakeSubagentSpawner, KillReason, QueuedSubagentSpawner, SpawnRequest,
    SubagentError, SubagentSpawner, SubagentState, SubagentToolPolicy,
    QUEUED_STEER_MESSAGES_METADATA_KEY,
};

fn request(policy: SubagentToolPolicy) -> SpawnRequest {
    SpawnRequest {
        task_id: TaskId("task-1".to_string()),
        parent_agent_id: AgentId("xiaoce".to_string()),
        agent_name: "worker".to_string(),
        task: "审计 runtime 配置接缝".to_string(),
        tool_policy: policy,
        context_isolation: ContextIsolation::Isolated,
        token_budget: 1024,
        idle_timeout_ms: 30_000,
        recursive_spawn: false,
        metadata: BTreeMap::new(),
    }
}

#[test]
fn fake_spawner_spawns_isolated_subagent_and_returns_receipt() {
    let mut spawner = FakeSubagentSpawner::new();

    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Analyze))
        .expect("spawn should succeed");

    assert_eq!(receipt.run_id.0, "fake-run-1");
    assert_eq!(receipt.agent_id.0, "worker-1");
    assert_eq!(receipt.context_isolation, ContextIsolation::Isolated);
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Running)
    );
}

#[test]
fn fake_spawner_preserves_fork_context_budget_without_parent_context_payload() {
    let mut spawner = FakeSubagentSpawner::new();
    let mut spawn = request(SubagentToolPolicy::Execute);
    spawn.context_isolation = ContextIsolation::Forked {
        max_parent_tokens: 256,
    };

    let receipt = spawner.spawn(spawn).expect("spawn should succeed");

    assert_eq!(
        receipt.context_isolation,
        ContextIsolation::Forked {
            max_parent_tokens: 256
        }
    );
}

#[test]
fn fake_spawner_rejects_recursive_analyze_policy() {
    let mut spawner = FakeSubagentSpawner::new();
    let mut spawn = request(SubagentToolPolicy::Analyze);
    spawn.recursive_spawn = true;

    let err = spawner
        .spawn(spawn)
        .expect_err("analyze recursive spawn should be rejected");

    assert!(matches!(err, SubagentError::InvalidRequest(_)));
}

#[test]
fn fake_spawner_records_steer_messages_for_running_subagent() {
    let mut spawner = FakeSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("spawn should succeed");

    spawner
        .steer(&receipt.run_id, "补一条测试".to_string())
        .expect("steer should succeed");

    assert_eq!(
        spawner.messages(&receipt.run_id),
        Some(&["补一条测试".to_string()][..])
    );
}

#[test]
fn fake_spawner_collects_structured_subagent_report() {
    let mut spawner = FakeSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("spawn should succeed");

    let report = spawner
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("fake run should complete");

    assert_eq!(report.status, ExecutionStatus::Success);
    assert_eq!(report.task_id.0, "task-1");
    assert_eq!(report.agent_id, receipt.agent_id);
    assert_eq!(report.parent_agent_id, Some(AgentId("xiaoce".to_string())));
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Completed)
    );
}

#[test]
fn fake_spawner_kill_changes_report_status_and_blocks_steering() {
    let mut spawner = FakeSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Orchestrate))
        .expect("spawn should succeed");

    spawner
        .kill(&receipt.run_id, KillReason::Timeout)
        .expect("kill should succeed");
    let steer_err = spawner
        .steer(&receipt.run_id, "继续".to_string())
        .expect_err("killed run should not accept steering");
    let report = spawner
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("killed run should still report");

    assert!(matches!(steer_err, SubagentError::NotRunning(_)));
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Killed(KillReason::Timeout))
    );
    assert_eq!(report.status, ExecutionStatus::Cancelled);
    assert!(report.stderr_preview.unwrap().contains("Timeout"));
}

#[test]
fn queued_spawner_emits_dispatch_and_waits_for_attached_report() {
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("spawn should succeed");

    let pending = spawner.pending_dispatches();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].run_id, receipt.run_id);
    assert_eq!(pending[0].agent_id, receipt.agent_id);
    assert_eq!(pending[0].task, "审计 runtime 配置接缝");
    assert!(spawner
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .is_none());

    let dispatch = spawner
        .take_next_dispatch()
        .expect("dispatch should be available");
    assert_eq!(dispatch.run_id, receipt.run_id);
    assert!(spawner.pending_dispatches().is_empty());

    spawner
        .attach_report(&receipt.run_id, queued_report(&receipt.agent_id))
        .expect("attach report should succeed");
    let report = spawner
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("attached report should be available");

    assert_eq!(report.status, ExecutionStatus::Success);
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Completed)
    );
}

#[test]
fn queued_spawner_persists_steer_messages_in_run_inbox() {
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("spawn should succeed");

    spawner
        .steer(&receipt.run_id, "先补失败路径测试".to_string())
        .expect("first steer should succeed");
    spawner
        .steer(&receipt.run_id, "再核对审批回执".to_string())
        .expect("second steer should succeed");

    assert_eq!(
        spawner.steer_messages(&receipt.run_id),
        Some(&["先补失败路径测试".to_string(), "再核对审批回执".to_string()][..])
    );
}

#[test]
fn queued_spawner_exposes_steer_messages_in_dispatch_metadata() {
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("spawn should succeed");

    spawner
        .steer(&receipt.run_id, "先补失败路径测试".to_string())
        .expect("first steer should succeed");
    spawner
        .steer(&receipt.run_id, "再核对审批回执".to_string())
        .expect("second steer should succeed");

    let dispatch = spawner
        .pending_dispatches()
        .into_iter()
        .find(|dispatch| dispatch.run_id == receipt.run_id)
        .expect("dispatch should remain visible");
    let steer_messages = serde_json::from_str::<Vec<String>>(
        dispatch
            .metadata
            .get(QUEUED_STEER_MESSAGES_METADATA_KEY)
            .expect("dispatch metadata should carry steer messages"),
    )
    .expect("dispatch steer metadata should decode");
    let encoded = serde_json::to_string(&dispatch).expect("dispatch should serialize");

    assert_eq!(
        steer_messages,
        vec!["先补失败路径测试".to_string(), "再核对审批回执".to_string()]
    );
    assert!(encoded.contains(QUEUED_STEER_MESSAGES_METADATA_KEY));
    assert!(encoded.contains("先补失败路径测试"));
}

#[test]
fn queued_dispatch_can_roundtrip_as_json() {
    let mut spawner = QueuedSubagentSpawner::new();
    let mut spawn = request(SubagentToolPolicy::Execute);
    spawn.context_isolation = ContextIsolation::Forked {
        max_parent_tokens: 128,
    };
    spawn
        .metadata
        .insert("scope".to_string(), "mvp".to_string());
    let receipt = spawner.spawn(spawn).expect("spawn should succeed");
    let dispatch = spawner
        .take_next_dispatch()
        .expect("dispatch should be available");

    let encoded = serde_json::to_string(&dispatch).expect("dispatch should serialize");
    let decoded: chuang_agent::subagent_spawner::SubagentDispatch =
        serde_json::from_str(&encoded).expect("dispatch should deserialize");

    assert_eq!(decoded.run_id, receipt.run_id);
    assert_eq!(decoded.agent_id, receipt.agent_id);
    assert_eq!(decoded.metadata.get("scope"), Some(&"mvp".to_string()));
    assert!(encoded.contains("\"tool_policy\":\"Execute\""));
}

#[test]
fn queued_spawner_can_spawn_with_explicit_ids() {
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn_with_ids(
            request(SubagentToolPolicy::Execute),
            chuang_agent::subagent_spawner::RunId("queued-cli-1".to_string()),
            AgentId("worker-cli-1".to_string()),
        )
        .expect("explicit ids should spawn");
    let dispatch = spawner
        .take_next_dispatch()
        .expect("dispatch should be available");

    assert_eq!(receipt.run_id.0, "queued-cli-1");
    assert_eq!(receipt.agent_id.0, "worker-cli-1");
    assert_eq!(dispatch.run_id, receipt.run_id);
    assert_eq!(dispatch.agent_id, receipt.agent_id);
}

#[test]
fn queued_spawner_can_restore_persisted_dispatch_and_collect_report() {
    let mut original = QueuedSubagentSpawner::new();
    let receipt = original
        .spawn_with_ids(
            request(SubagentToolPolicy::Execute),
            chuang_agent::subagent_spawner::RunId("persisted-run-1".to_string()),
            AgentId("persisted-worker-1".to_string()),
        )
        .expect("explicit ids should spawn");
    let dispatch = original
        .pending_dispatches()
        .into_iter()
        .find(|dispatch| dispatch.run_id == receipt.run_id)
        .expect("dispatch should be pending");

    let mut restored = QueuedSubagentSpawner::new();
    restored
        .restore_dispatch(dispatch)
        .expect("dispatch should restore into running state");
    let report = queued_report(&receipt.agent_id);
    restored
        .attach_report(&receipt.run_id, report.clone())
        .expect("restored run should accept matching report");
    let collected = restored
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("report should be available");

    assert_eq!(collected, report);
}

#[test]
fn queued_spawner_restores_persisted_steer_messages_into_fresh_instance() {
    let mut original = QueuedSubagentSpawner::new();
    let receipt = original
        .spawn_with_ids(
            request(SubagentToolPolicy::Execute),
            chuang_agent::subagent_spawner::RunId("persisted-run-2".to_string()),
            AgentId("persisted-worker-2".to_string()),
        )
        .expect("explicit ids should spawn");
    original
        .steer(&receipt.run_id, "补失败路径".to_string())
        .expect("first steer should succeed");
    original
        .steer(&receipt.run_id, "补回执字段".to_string())
        .expect("second steer should succeed");
    let dispatch = original
        .pending_dispatches()
        .into_iter()
        .find(|dispatch| dispatch.run_id == receipt.run_id)
        .expect("dispatch should be pending");

    let mut restored = QueuedSubagentSpawner::new();
    restored
        .restore_dispatch(dispatch)
        .expect("dispatch should restore into running state");

    assert_eq!(
        restored.steer_messages(&receipt.run_id),
        Some(&["补失败路径".to_string(), "补回执字段".to_string()][..])
    );
}

#[test]
fn queued_spawner_restores_numbered_run_id_and_continues_from_max_suffix() {
    let mut spawner = QueuedSubagentSpawner::new();
    spawner
        .restore_dispatch(chuang_agent::subagent_spawner::SubagentDispatch {
            run_id: chuang_agent::subagent_spawner::RunId("queued-run-7".to_string()),
            agent_id: AgentId("worker-7".to_string()),
            task_id: TaskId("task-7".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "恢复旧 dispatch".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 1024,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: BTreeMap::new(),
        })
        .expect("restore should succeed");
    spawner
        .restore_dispatch(chuang_agent::subagent_spawner::SubagentDispatch {
            run_id: chuang_agent::subagent_spawner::RunId("queued-run-3".to_string()),
            agent_id: AgentId("worker-3".to_string()),
            task_id: TaskId("task-3".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "恢复较小编号 dispatch".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 1024,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: BTreeMap::new(),
        })
        .expect("restore should succeed");

    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("spawn should continue from max restored suffix");

    assert_eq!(receipt.run_id.0, "queued-run-8");
}

#[test]
fn queued_spawner_restoring_nonstandard_run_id_does_not_advance_counter() {
    let mut spawner = QueuedSubagentSpawner::new();
    spawner
        .restore_dispatch(chuang_agent::subagent_spawner::SubagentDispatch {
            run_id: chuang_agent::subagent_spawner::RunId("persisted-run-alpha".to_string()),
            agent_id: AgentId("worker-alpha".to_string()),
            task_id: TaskId("task-alpha".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "恢复自定义 run id".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 1024,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: BTreeMap::new(),
        })
        .expect("restore should succeed");

    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("nonstandard restored ids should not affect counter");

    assert_eq!(receipt.run_id.0, "queued-run-1");
}

#[test]
fn queued_spawner_persist_spawn_rollback_keeps_first_run_id_available() {
    let mut spawner = QueuedSubagentSpawner::new();
    let persist_calls = Cell::new(0);

    let err = spawner
        .persist_spawn(request(SubagentToolPolicy::Execute), |_| {
            persist_calls.set(persist_calls.get() + 1);
            Err(SubagentError::InvalidRequest(
                "dispatch persist failed".to_string(),
            ))
        })
        .expect_err("persist failure should abort spawn");

    assert!(matches!(err, SubagentError::InvalidRequest(_)));
    assert_eq!(persist_calls.get(), 1);
    assert!(spawner.pending_dispatches().is_empty());
    assert!(spawner
        .state(&chuang_agent::subagent_spawner::RunId(
            "queued-run-1".to_string()
        ))
        .is_none());

    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Execute))
        .expect("next spawn should still use first run id");
    assert_eq!(receipt.run_id.0, "queued-run-1");
}

#[test]
fn queued_spawner_rejects_duplicate_explicit_run_id() {
    let mut spawner = QueuedSubagentSpawner::new();
    spawner
        .spawn_with_ids(
            request(SubagentToolPolicy::Execute),
            chuang_agent::subagent_spawner::RunId("queued-cli-1".to_string()),
            AgentId("worker-cli-1".to_string()),
        )
        .expect("first explicit ids should spawn");

    let err = spawner
        .spawn_with_ids(
            request(SubagentToolPolicy::Execute),
            chuang_agent::subagent_spawner::RunId("queued-cli-1".to_string()),
            AgentId("worker-cli-2".to_string()),
        )
        .expect_err("duplicate run id should fail");

    assert!(matches!(err, SubagentError::InvalidRequest(_)));
}

#[test]
fn queued_spawner_rejects_mismatched_attached_report() {
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Analyze))
        .expect("spawn should succeed");

    let mut report = queued_report(&AgentId("wrong-agent".to_string()));
    report.task_id = TaskId("task-1".to_string());
    let err = spawner
        .attach_report(&receipt.run_id, report)
        .expect_err("mismatched report should be rejected");

    assert!(matches!(err, SubagentError::InvalidRequest(_)));
    assert_eq!(
        spawner.state(&receipt.run_id),
        Some(&SubagentState::Running)
    );
}

#[test]
fn queued_spawner_kill_removes_pending_dispatch() {
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(request(SubagentToolPolicy::Orchestrate))
        .expect("spawn should succeed");

    spawner
        .kill(&receipt.run_id, KillReason::UserRequested)
        .expect("kill should succeed");

    assert!(spawner.pending_dispatches().is_empty());
    let report = spawner
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("killed queued run should report cancellation");
    assert_eq!(report.status, ExecutionStatus::Cancelled);
}

fn queued_report(agent_id: &AgentId) -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId("report-queued-run-1".to_string()),
        task_id: TaskId("task-1".to_string()),
        agent_id: agent_id.clone(),
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
        skill_proposals: vec![],
    }
}
