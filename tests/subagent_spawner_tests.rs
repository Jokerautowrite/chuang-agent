use std::collections::BTreeMap;

use chuang_agent::common::{AgentId, TaskId};
use chuang_agent::subagent_report::ExecutionStatus;
use chuang_agent::subagent_spawner::{
    ContextIsolation, FakeSubagentSpawner, KillReason, SpawnRequest, SubagentError,
    SubagentSpawner, SubagentState, SubagentToolPolicy,
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
