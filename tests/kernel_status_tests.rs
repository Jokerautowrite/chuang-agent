use std::path::PathBuf;

use chuang_agent::chuang_kernel::{
    ChuangKernelConfig, IdentityBootstrapSnapshot, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::runtime_config::RuntimeConfig;

#[test]
fn kernel_status_exposes_mvp_config_slots_and_kernel_snapshot() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let kernel = ChuangKernelConfig {
        agent_id: "chuang-cli".to_string(),
        parent_agent_id: None,
        recall_limit: config.recall_limit,
        metadata: config.metadata.clone(),
        context_budget: Some(config.context_budget.clone()),
        context_engine_kind: None,
        memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        identity_snapshot: None,
        identity_bootstrap_snapshot: None,
    };

    let status = build_chuang_mvp_status(&config, &kernel).expect("status should build");

    assert_eq!(status.kernel.agent_id, "chuang-cli");
    assert_eq!(status.kernel.turn_count, 0);
    assert_eq!(status.config.provider_kind, "fake");
    assert_eq!(status.config.model_name, "stub-responder");
    assert_eq!(status.slots.provider, "fake");
    assert_eq!(status.slots.governance, "static_rule");
    assert_eq!(status.slots.execution, "generic_agent_mvp");
    assert_eq!(status.slots.subagent, "fake");
    assert_eq!(status.slots.control_plane, "fake_local");
    assert!(status.atomic_tools.ok);
    assert_eq!(status.atomic_tools.manifest_schema_version, 1);
    assert_eq!(
        status.atomic_tools.manifest_schema_fields,
        vec![
            "kind",
            "name",
            "source",
            "status",
            "implementation",
            "description",
        ]
    );
    assert_eq!(status.atomic_tools.tool_action_schema_version, 1);
    assert!(status
        .atomic_tools
        .tool_action_schema_fields
        .iter()
        .any(|field| field == "type"));
    assert!(status
        .atomic_tools
        .tool_action_call_schema_fields
        .iter()
        .any(|field| field == "tool"));
    assert_eq!(status.atomic_tools.tool_report_schema_version, 6);
    assert!(status
        .atomic_tools
        .tool_call_schema_fields
        .iter()
        .any(|field| field == "atomic_tool_name"));
    assert!(status.plugin_registry.available);
    assert!(status.plugin_registry.ok);
    assert_eq!(status.plugin_registry.plugin_count, 5);
    assert!(status.goal_mode.ok);
    assert_eq!(status.goal_mode.cli_entrypoint, "run --goal TEXT");
    assert_eq!(status.goal_mode.default_goal_id, "mainline-mvp");
    assert!(!status.goal_mode.bypasses_governance);
    assert!(!status.goal_mode.adds_core_slot);
}

#[test]
fn kernel_status_exposes_identity_bootstrap_presence_flags() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    let kernel = ChuangKernelConfig {
        agent_id: "chuang-cli".to_string(),
        parent_agent_id: None,
        recall_limit: config.recall_limit,
        metadata: config.metadata.clone(),
        context_budget: Some(config.context_budget.clone()),
        context_engine_kind: None,
        memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        identity_snapshot: None,
        identity_bootstrap_snapshot: Some(IdentityBootstrapSnapshot {
            soul: "创的核心锚点".to_string(),
            soul_exists: true,
            story: String::new(),
            story_exists: false,
            first_wake: String::new(),
            first_wake_exists: false,
            agents_registry: String::new(),
            agents_registry_exists: false,
        }),
    };

    let status = build_chuang_mvp_status(&config, &kernel).expect("status should build");

    assert_eq!(
        status.kernel.identity_soul_chars,
        Some("创的核心锚点".chars().count())
    );
    assert_eq!(status.kernel.identity_soul_exists, Some(true));
    assert_eq!(status.kernel.identity_story_chars, Some(0));
    assert_eq!(status.kernel.identity_story_exists, Some(false));
    assert_eq!(status.kernel.identity_first_wake_chars, Some(0));
    assert_eq!(status.kernel.identity_first_wake_exists, Some(false));
    assert_eq!(status.kernel.identity_agents_registry_chars, Some(0));
    assert_eq!(status.kernel.identity_agents_registry_exists, Some(false));
}

#[test]
fn kernel_status_rejects_invalid_runtime_config() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.recall_limit = 0;
    let kernel = ChuangKernelConfig {
        agent_id: "chuang-cli".to_string(),
        parent_agent_id: None,
        recall_limit: config.recall_limit,
        metadata: config.metadata.clone(),
        context_budget: Some(config.context_budget.clone()),
        context_engine_kind: None,
        memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        identity_snapshot: None,
        identity_bootstrap_snapshot: None,
    };

    let err = build_chuang_mvp_status(&config, &kernel).expect_err("invalid config should fail");

    assert_eq!(err.field, "recall_limit");
}
