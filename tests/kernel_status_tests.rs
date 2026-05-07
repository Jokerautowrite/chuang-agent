use std::path::PathBuf;

use chuang_agent::chuang_kernel::{
    ChuangKernelConfig, IdentityBootstrapSnapshot, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::goal_run::{
    GoalCheckpoint, GoalIntegrationPolicy, GoalRun, GoalRunStore, GoalValidationPlan,
    GoalWorkerPlan, GoalWriteScope,
};
use chuang_agent::kernel_status::{build_chuang_mvp_status, summarize_goal_run_readiness};
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
    assert!(status.governance.ok);
    assert_eq!(status.governance.kind, "static_rule");
    assert!(status.governance.rules_loaded);
    assert!(status.governance.rules_core_path.ends_with("rules/core.md"));
    assert!(status.governance.rule_count > 0);
    assert!(status.governance.tool_surface_governed);
    assert_eq!(status.governance.read_only_decision, "allowed");
    assert_eq!(status.governance.dangerous_write_decision, "needs_approval");
    assert_eq!(status.governance.dangerous_shell_decision, "needs_approval");
    assert_eq!(status.governance.secret_shell_decision, "draft_only");
    assert!(!status.governance.goal_run_executes);
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
    assert_eq!(
        status.atomic_tools.mapped_atomic_tool_names,
        vec![
            "file_read".to_string(),
            "file_write".to_string(),
            "code_execute".to_string(),
        ]
    );
    assert_eq!(
        status.atomic_tools.interface_only_atomic_tool_names,
        vec![
            "mouse".to_string(),
            "keyboard".to_string(),
            "screenshot".to_string(),
            "locate".to_string(),
            "wait".to_string(),
            "human_suspend".to_string(),
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
    assert!(status.plugin_registry.evidence_available);
    assert!(status.plugin_registry.check_only);
    assert!(!status.plugin_registry.executes_plugins);
    assert!(!status.plugin_registry.reads_secret);
    assert!(status.plugin_registry.capability_count >= 5);
    assert!(status.local_contract_readiness.ok);
    assert_eq!(status.local_contract_readiness.overall_state, "ready");
    assert_eq!(status.local_contract_readiness.contract_count, 4);
    assert_eq!(status.local_contract_readiness.ready_count, 4);
    assert!(
        !status
            .local_contract_readiness
            .connects_real_external_services
    );
    assert!(!status.local_contract_readiness.writes_core_memory);
    assert!(!status.local_contract_readiness.executes_plugins);
    assert!(status
        .local_contract_readiness
        .contracts
        .iter()
        .any(|contract| contract.name == "knowledge_context_preview"
            && contract.state == "ready"
            && contract.read_only
            && !contract.connects_real_service));
    assert!(status
        .local_contract_readiness
        .contracts
        .iter()
        .any(|contract| contract.name == "skill_proposal_review"
            && contract.dry_run
            && !contract.writes_core_memory));
    assert!(status
        .local_contract_readiness
        .contracts
        .iter()
        .any(|contract| contract.name == "plugin_registry_evidence" && !contract.executes_plugins));
    assert!(status
        .local_contract_readiness
        .contracts
        .iter()
        .any(
            |contract| contract.name == "external_knowledge_source_contracts"
                && contract.boundary == "adapter_contract_only"
        ));
    assert!(status.project_readiness.ok);
    assert_eq!(
        status.project_readiness.overall_state,
        "mvp_ready_with_partial_modules"
    );
    assert!(status.project_readiness.ready_count >= 7);
    assert_eq!(status.project_readiness.partial_count, 0);
    assert!(status
        .project_readiness
        .modules
        .iter()
        .any(|module| module.name == "main_chain" && module.state == "ready"));
    assert!(status
        .project_readiness
        .modules
        .iter()
        .any(|module| module.name == "external_ai" && module.state == "ready"));
    assert!(status.release_readiness.ok);
    assert_eq!(status.release_readiness.release_name, "second_test_version");
    assert_eq!(
        status.release_readiness.overall_state,
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        status.release_readiness.readiness_scope,
        "readiness_and_smoke_acceptance_only_no_live_external_service_connection"
    );
    assert_eq!(status.release_readiness.partial_count, 0);
    assert_eq!(status.release_readiness.acceptance_count, 7);
    assert!(status.release_readiness.acceptance_ready_count >= 4);
    assert!(status.release_readiness.acceptance_partial_count >= 2);
    assert_eq!(status.release_readiness.acceptance_deferred_count, 1);
    assert!(!status.release_readiness.connects_real_external_services);
    assert!(!status.release_readiness.verifies_real_external_services);
    assert!(status.release_readiness.uses_stub_or_local_fixtures);
    assert!(!status.release_readiness.writes_repo_files);
    assert!(status
        .release_readiness
        .acceptance
        .iter()
        .any(|item| item.name == "real_external_services"
            && item.state == "deferred"
            && !item.connects_real_service));
    assert!(status
        .release_readiness
        .acceptance
        .iter()
        .any(|item| item.name == "channel_preflight_only"
            && item.state == "partial"
            && item.read_only
            && !item.connects_real_service));
    assert!(status.third_test_candidate.ok);
    assert_eq!(
        status.third_test_candidate.candidate_name,
        "third_test_candidate"
    );
    assert_eq!(
        status.third_test_candidate.overall_state,
        "local_gate_ready_requires_manual_live_check"
    );
    assert!(status.third_test_candidate.local_gate_ready);
    assert_eq!(
        status.third_test_candidate.smoke_script,
        "scripts/chuang-third-test-smoke.sh"
    );
    assert_eq!(
        status.third_test_candidate.marker,
        "third_test_candidate_smoke_ok"
    );
    assert!(status.third_test_candidate.requires_manual_live_check);
    assert!(!status.third_test_candidate.connects_real_external_services);
    assert!(!status.third_test_candidate.verifies_real_external_services);
    assert!(!status.third_test_candidate.real_live_ready);
    assert!(status.third_test_candidate.operator_env_blocks_100_percent);
    assert!(status.memory_readiness.ok);
    assert_eq!(status.memory_readiness.overall_state, "ready");
    assert_eq!(status.memory_readiness.layer_count, 5);
    assert!(status
        .memory_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "internal_identity" && layer.state == "ready"));
    assert!(status
        .memory_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "external_knowledge" && layer.state == "ready"));
    assert!(status
        .memory_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "external_knowledge"
            && layer.storage == "docs/external-knowledge-adapter.md"));
    assert!(status
        .memory_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "maintenance_loop" && layer.state == "ready"));
    assert!(status
        .memory_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "maintenance_loop"
            && layer.storage == "docs/memory-maintenance-loop.md"));
    assert!(status.channel_readiness.ok);
    assert_eq!(status.channel_readiness.overall_state, "ready");
    assert_eq!(status.channel_readiness.layer_count, 5);
    assert!(status
        .channel_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "app_server" && layer.state == "ready"));
    assert!(status
        .channel_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "dedicated_feishu_bridge" && layer.state == "ready"));
    assert!(status.subagent_readiness.ok);
    assert_eq!(
        status.subagent_readiness.overall_state,
        "queued_protocol_partial"
    );
    assert_eq!(status.subagent_readiness.mode, "fake");
    assert!(status.subagent_readiness.local_contract_ready);
    assert_eq!(status.subagent_readiness.local_contract_state, "ready");
    assert!(status
        .subagent_readiness
        .local_contract_reason
        .contains("protocol-ready"));
    assert!(!status.subagent_readiness.live_adapter_ready);
    assert_eq!(status.subagent_readiness.live_adapter_state, "partial");
    assert!(status
        .subagent_readiness
        .live_adapter_reason
        .contains("not yet connected"));
    assert!(status
        .subagent_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "command_runner"
            && layer.state == "ready"
            && layer.local_contract_ready
            && layer.local_contract_state == "ready"
            && layer.local_contract_reason.contains("protocol-ready")
            && !layer.live_adapter_ready
            && layer.live_adapter_state == "deferred"
            && layer
                .live_adapter_reason
                .contains("live runner adapters remain deferred")));
    assert!(status
        .subagent_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "multi_worker"
            && layer.state == "ready"
            && layer.local_contract_state == "ready"
            && layer.live_adapter_state == "deferred"));
    assert!(status.live_adapter_gates.ok);
    assert_eq!(
        status.live_adapter_gates.overall_state,
        "disabled_by_default"
    );
    assert_eq!(status.live_adapter_gates.gate_count, 3);
    assert_eq!(status.live_adapter_gates.enabled_count, 0);
    assert!(status.live_adapter_gates.gates.iter().any(|gate| {
        gate.name == "control_apply"
            && gate.required_env == "CHUANG_REAL_CONTROL_ENABLE"
            && gate.audit_label == "control.apply.live"
            && !gate.enabled
            && !gate.default_enabled
            && gate.env_value_state == "unset"
            && gate
                .preflight_checks
                .iter()
                .any(|check| check.contains("Chuang-only unit allowlist"))
            && gate
                .must_reject_capabilities
                .iter()
                .any(|capability| capability.contains("Codex or Hermes service control"))
            && gate.next_action.contains("preflight evidence")
    }));
    assert!(status.external_ai_readiness.ok);
    assert_eq!(status.external_ai_readiness.overall_state, "ready");
    assert!(status
        .external_ai_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "genesis_actuator" && layer.state == "ready"));
    assert!(status
        .external_ai_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "dispatch_sop" && layer.state == "ready"));
    assert!(status
        .external_ai_readiness
        .layers
        .iter()
        .any(|layer| layer.name == "unified_identity_engine" && layer.state == "ready"));
    assert!(status.goal_mode.ok);
    assert_eq!(status.goal_mode.cli_entrypoint, "run --goal TEXT");
    assert_eq!(status.goal_mode.default_goal_id, "mainline-mvp");
    assert!(!status.goal_mode.bypasses_governance);
    assert!(!status.goal_mode.adds_core_slot);
    assert_eq!(status.goal_run.goal_id, "mainline-mvp");
    assert!(status.goal_run.ok);
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

#[test]
fn goal_run_readiness_reports_absent_plan_without_failure() {
    let root = temp_goal_root("absent");

    let status = summarize_goal_run_readiness(&root, "mainline-mvp");

    assert!(status.ok);
    assert!(!status.plan_exists);
    assert_eq!(status.checkpoint_count, 0);
    assert_eq!(status.worker_count, 0);
    assert!(!status.checkpoint_log_complete);
    assert!(status.incomplete_reasons.is_empty());
    assert!(status.read_error.is_none());
    assert!(status.path.ends_with("mainline-mvp.json"));
}

#[test]
fn goal_run_readiness_reports_checkpoint_count_for_existing_plan() {
    let root = temp_goal_root("existing");
    let store = GoalRunStore::new(&root);
    let mut run = GoalRun::new(
        GoalSpec::mainline_mvp("surface checkpoint readiness"),
        vec![GoalWorkerPlan::new(
            "main-process",
            "continue from checkpoints",
            vec!["mainline".to_string()],
            vec!["cargo test -q --test kernel_status_tests".to_string()],
        )],
        vec![GoalWriteScope::new(
            "mainline",
            vec!["src/kernel_status.rs".to_string()],
        )],
        GoalValidationPlan::new(vec![
            "cargo fmt --all".to_string(),
            "cargo test -q --test kernel_status_tests".to_string(),
        ]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect("goal run should construct");
    run.record_checkpoint(GoalCheckpoint::new(
        "checkpoint-readiness",
        "readiness status can count checkpoints",
        vec!["main-process".to_string()],
        vec!["cargo test -q --test kernel_status_tests".to_string()],
    ))
    .expect("checkpoint should record");
    store.create(&run).expect("goal run should be stored");

    let status = summarize_goal_run_readiness(&root, "mainline-mvp");

    assert!(status.ok);
    assert!(status.plan_exists);
    assert_eq!(status.checkpoint_count, 1);
    assert_eq!(status.worker_count, 1);
    assert_eq!(status.validation_command_count, 2);
    assert!(status.checkpoint_log_complete);
    assert_eq!(
        status.last_checkpoint_id.as_deref(),
        Some("checkpoint-readiness")
    );
    assert_eq!(
        status.last_checkpoint_summary.as_deref(),
        Some("readiness status can count checkpoints")
    );
    assert!(status.incomplete_reasons.is_empty());
    assert!(status.read_error.is_none());
}

#[test]
fn goal_run_readiness_surfaces_incomplete_legacy_checkpoint() {
    let root = temp_goal_root("legacy-incomplete");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");

    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(
        &path,
        r#"{
  "goal_spec": {
    "goal_id": "mainline-mvp",
    "objective": "surface weak legacy checkpoints",
    "acceptance_checks": ["cargo fmt --all", "cargo test -q"],
    "budget": {
      "max_minutes": 60,
      "max_tool_rounds": 8,
      "max_subtasks": 0
    },
    "allowed_slots": ["context"],
    "checkpoint_policy": {
      "update_progress_log": true,
      "update_handoff": true,
      "commit_checkpoint": true
    },
    "final_report_policy": {
      "include_validation": true,
      "include_next_steps": true
    }
  },
  "worker_plan": [
    {
      "worker_id": "main-process",
      "objective": "continue from checkpoints",
      "write_scope_ids": ["mainline"],
      "validation_checks": ["cargo test -q --test kernel_status_tests"]
    }
  ],
  "disjoint_write_scopes": [
    {
      "scope_id": "mainline",
      "paths": ["src/kernel_status.rs"]
    }
  ],
  "validation_plan": {
    "commands": ["cargo test -q --test kernel_status_tests"]
  },
  "integration_policy": {
    "main_process_owns_integration": true,
    "workers_may_commit": false,
    "workers_may_touch_secrets": false,
    "require_worker_reports": true
  },
  "checkpoint_log": [
    {
      "checkpoint_id": "checkpoint-legacy-incomplete",
      "summary": "legacy checkpoint has no worker evidence",
      "completed_worker_ids": [],
      "validation_notes": ["cargo test -q --test kernel_status_tests"]
    }
  ]
}"#,
    )
    .expect("legacy run should be written");

    let status = summarize_goal_run_readiness(&root, "mainline-mvp");

    assert!(status.ok);
    assert!(status.plan_exists);
    assert_eq!(status.checkpoint_count, 1);
    assert!(!status.checkpoint_log_complete);
    assert_eq!(
        status.last_checkpoint_id.as_deref(),
        Some("checkpoint-legacy-incomplete")
    );
    assert!(status
        .incomplete_reasons
        .iter()
        .any(|reason| reason.contains("latest checkpoint is missing completed worker evidence")));
}

#[test]
fn goal_run_readiness_reports_invalid_plan_read_error() {
    let root = temp_goal_root("invalid-json");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");
    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(&path, "{not json").expect("invalid goal run should be written");

    let status = summarize_goal_run_readiness(&root, "mainline-mvp");

    assert!(!status.ok);
    assert!(status.plan_exists);
    assert!(!status.checkpoint_log_complete);
    assert!(status
        .read_error
        .as_deref()
        .expect("read error should be present")
        .contains("goal_run.json"));
    assert!(status
        .incomplete_reasons
        .contains(&"goal run could not be loaded".to_string()));
}

fn temp_goal_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chuang-kernel-status-goal-{name}-{}",
        std::process::id()
    ))
}
