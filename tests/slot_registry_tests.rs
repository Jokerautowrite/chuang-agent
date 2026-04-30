use std::path::PathBuf;

use chuang_agent::actuator::{Actuator, ObserveTarget};
use chuang_agent::governance::{ActionKind, Governance, ProposedAction, RiskDecision};
use chuang_agent::runtime_config::RuntimeConfig;
use chuang_agent::skill_evolver::{EvolutionScope, SkillEvolver};
use chuang_agent::slot_registry::{build_runtime_slots, summarize_runtime_slots};
use chuang_agent::subagent_spawner::{
    ContextIsolation, SpawnRequest, SubagentSpawner, SubagentToolPolicy,
};
use chuang_agent::{common::AgentId, common::TaskId};

#[test]
fn slot_registry_builds_all_current_runtime_slots_from_config() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    let mut slots = build_runtime_slots(&config).expect("default slots should build");

    let decision = slots
        .governance
        .classify(&ProposedAction {
            action_id: "action-1".to_string(),
            kind: ActionKind::Observe,
            target: "screen".to_string(),
            summary: "observe screen".to_string(),
        })
        .expect("governance should classify");
    let observation = slots
        .actuator
        .observe(ObserveTarget::Screen)
        .expect("fake actuator should observe");
    let proposals = slots
        .evolution
        .propose(EvolutionScope {
            agent_id: "xiaoce".to_string(),
            task_kind: None,
            max_proposals: 1,
        })
        .expect("noop evolver should accept valid scope");

    assert!(matches!(decision, RiskDecision::Allowed { .. }));
    assert_eq!(observation.summary, "fake observation");
    assert!(proposals.is_empty());
}

#[test]
fn slot_registry_builds_subagent_spawner_that_can_spawn_and_collect() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let mut slots = build_runtime_slots(&config).expect("default slots should build");

    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-1".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "验证 slot registry".to_string(),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 512,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("spawn should succeed");
    let report = slots
        .subagent
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("fake report should exist");

    assert_eq!(report.agent_id, receipt.agent_id);
    assert_eq!(report.task_id.0, "task-1");
}

#[test]
fn slot_registry_summary_matches_runtime_config_slot_kinds() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    let summary = summarize_runtime_slots(&config);

    assert_eq!(summary.governance, "static_rule");
    assert_eq!(summary.actuator, "fake");
    assert_eq!(summary.subagent, "fake");
    assert_eq!(summary.evolution, "noop");
}
