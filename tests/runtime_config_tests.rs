use std::path::PathBuf;

use chuang_agent::provider_openai_compatible::ProviderTransport;
use chuang_agent::runtime_config::{
    ContextEngineConfig, ControlPlaneCommandConfig, ControlPlaneConfig, EvolutionConfig,
    IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig, RuntimeConfig, SubagentConfig,
    SubagentLiveWorkerConfig, SubagentQueueConfig,
};

#[test]
fn runtime_config_defaults_to_fake_provider_without_silent_network_use() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    config.validate().expect("default config should be valid");
    let summary = config.summary();

    assert_eq!(summary.provider_kind, "fake");
    assert_eq!(summary.model_name, "stub-responder");
    assert_eq!(summary.governance_kind, "static_rule");
    assert_eq!(summary.actuator_kind, "fake");
    assert_eq!(summary.subagent_kind, "fake");
    assert_eq!(summary.subagent_live_worker.enabled, false);
    assert_eq!(summary.subagent_live_worker.adapter_kind, "none");
    assert_eq!(summary.subagent_live_worker.status, "disabled");
    assert_eq!(summary.subagent_live_worker.starts_worker, false);
    assert_eq!(summary.subagent_live_worker.available, false);
    assert_eq!(summary.subagent_queue_root, "./data/subagent-queue");
    assert_eq!(summary.evolution_kind, "noop");
    assert_eq!(summary.control_plane_kind, "fake_local");
    assert_eq!(summary.control_command_timeout_ms, None);
    assert_eq!(summary.provider_tls_ca_cert_path, None);
    assert_eq!(summary.provider_request_timeout_ms, None);
    assert_eq!(summary.identity_memory_kind, "hermes_dual_file");
    assert_eq!(
        summary.identity_experiences_path,
        "./identity/experiences.md"
    );
    assert_eq!(summary.identity_user_max_chars, 1375);
    assert_eq!(summary.identity_memory_max_chars, 2200);
    assert_eq!(summary.context_engine_kind, "deterministic_budget");
    assert_eq!(summary.context_max_tokens, 272000);
    assert_eq!(summary.context_reserve_system_tokens, 4096);
    assert_eq!(summary.context_min_working_tokens, 1);
    assert_eq!(summary.context_max_tool_results, 5);
    assert_eq!(summary.context_max_memory_segments, 5);
    assert_eq!(summary.tool_loop_max_rounds, 4);
    assert_eq!(summary.tool_shell_timeout_ms, 120_000);
    assert_eq!(
        summary.tool_shell_risk_rule_counts,
        "delete_or_cleanup=14,privilege_escalation=5,service_change=7,network_change=9,secret_access=7"
    );
    assert_eq!(summary.permission_profile, "full_local_workspace");
    assert_eq!(summary.approval_policy, "auto_for_workspace");
    assert_eq!(
        summary.permission_workspace_root,
        "/home/user/projects/chuang-agent"
    );
    assert_eq!(summary.api_key_state, None);
}

#[test]
fn runtime_config_accepts_unrestricted_approval_policy() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.permission.approval_policy = "unrestricted".to_string();

    config
        .validate()
        .expect("unrestricted policy should validate");
    assert_eq!(config.summary().approval_policy, "unrestricted");
}

#[test]
fn runtime_config_rejects_unknown_approval_policy() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.permission.approval_policy = "bogus".to_string();

    let err = config
        .validate()
        .expect_err("bogus approval policy should be rejected");
    assert_eq!(err.field, "permission.approval_policy");
}

#[test]
fn subagent_live_worker_config_is_status_only_and_never_available_by_default() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent_live_worker = SubagentLiveWorkerConfig {
        enabled: true,
        adapter_kind: "command".to_string(),
        status: "configured_status_only".to_string(),
        starts_worker: false,
    };

    config
        .validate()
        .expect("status-only live worker config should validate");
    let summary = config.summary();

    assert_eq!(summary.subagent_live_worker.enabled, true);
    assert_eq!(summary.subagent_live_worker.adapter_kind, "command");
    assert_eq!(
        summary.subagent_live_worker.status,
        "configured_status_only"
    );
    assert_eq!(summary.subagent_live_worker.starts_worker, false);
    assert_eq!(summary.subagent_live_worker.available, false);
    assert!(summary.subagent_live_worker.reason.contains("status only"));
    assert!(summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("subagent_live_worker is status-only")));
}

#[test]
fn subagent_live_worker_config_rejects_starting_workers_in_runtime_config() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent_live_worker = SubagentLiveWorkerConfig {
        enabled: true,
        adapter_kind: "command".to_string(),
        status: "configured_status_only".to_string(),
        starts_worker: true,
    };

    let err = config
        .validate()
        .expect_err("status-only live worker config must not start workers");

    assert_eq!(err.field, "subagent_live_worker.starts_worker");
}

#[test]
fn runtime_config_rejects_invalid_tool_loop_rounds() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.tool_loop.max_rounds = 0;

    let err = config
        .validate()
        .expect_err("zero tool loop max rounds should fail");

    assert_eq!(err.field, "tool_loop.max_rounds");

    config.tool_loop.max_rounds = 257;
    let err = config
        .validate()
        .expect_err("oversized tool loop max rounds should fail");
    assert_eq!(err.field, "tool_loop.max_rounds");

    config.tool_loop.max_rounds = 4;
    config.tool_loop.shell_timeout_ms = 0;
    let err = config
        .validate()
        .expect_err("zero shell timeout should fail");
    assert_eq!(err.field, "tool_loop.shell_timeout_ms");

    config.tool_loop.shell_timeout_ms = 600_001;
    let err = config
        .validate()
        .expect_err("oversized shell timeout should fail");
    assert_eq!(err.field, "tool_loop.shell_timeout_ms");
}

#[test]
fn runtime_config_rejects_zero_recall_limit() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.recall_limit = 0;

    let err = config
        .validate()
        .expect_err("zero recall limit should fail");

    assert_eq!(err.field, "recall_limit");
}

#[test]
fn runtime_config_rejects_context_system_reserve_over_budget() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.context_budget.max_tokens = 32;
    config.context_budget.reserve_system_tokens = 64;

    let err = config
        .validate()
        .expect_err("oversized system reserve should fail");

    assert_eq!(err.field, "context.reserve_system_tokens");
}

#[test]
fn context_engine_config_exposes_deterministic_budget_kind() {
    let config = ContextEngineConfig::DeterministicBudget;

    config.validate().expect("deterministic config is valid");

    assert_eq!(config.kind(), "deterministic_budget");
}

#[test]
fn context_engine_config_exposes_summary_compression_kind() {
    let config = ContextEngineConfig::SummaryCompression;

    config
        .validate()
        .expect("summary compression config is valid");

    assert_eq!(config.kind(), "summary_compression");
}

#[test]
fn openai_provider_config_redacts_api_key_in_summary() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.provider = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "test-key".to_string(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
        endpoint: Default::default(),
        reasoning_effort: None,
        request_timeout_ms: None,
        tls_ca_cert_path: None,
    });

    config
        .validate()
        .expect("complete provider config should pass");
    let summary = config.summary();

    assert_eq!(summary.provider_kind, "openai_compatible");
    assert_eq!(summary.provider_id, "custom-openai");
    assert_eq!(summary.api_key_state, Some("<set>".to_string()));
    assert_eq!(summary.provider_tls_ca_cert_path, None);
    assert_eq!(summary.provider_request_timeout_ms, None);
    assert_eq!(summary.provider_reasoning_effort, None);
}

#[test]
fn openai_provider_config_exposes_request_timeout_in_summary() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.provider = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "test-key".to_string(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
        endpoint: Default::default(),
        reasoning_effort: Some("high".parse().expect("reasoning effort should parse")),
        request_timeout_ms: Some(12_345),
        tls_ca_cert_path: None,
    });

    config
        .validate()
        .expect("complete provider config should pass");
    let summary = config.summary();

    assert_eq!(summary.provider_request_timeout_ms, Some(12_345));
    assert_eq!(summary.provider_reasoning_effort.as_deref(), Some("high"));
}

#[test]
fn openai_provider_config_rejects_missing_required_fields() {
    let config = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: String::new(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
        endpoint: Default::default(),
        reasoning_effort: None,
        request_timeout_ms: None,
        tls_ca_cert_path: None,
    });

    let err = config
        .validate()
        .expect_err("missing api key should be rejected");

    assert_eq!(err.field, "provider.api_key");
}

#[test]
fn runtime_config_summary_exposes_all_slot_kinds_for_control_plane() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    let summary = config.summary();

    assert_eq!(summary.provider_kind, "fake");
    assert_eq!(summary.governance_kind, "static_rule");
    assert_eq!(summary.actuator_kind, "fake");
    assert_eq!(summary.subagent_kind, "fake");
    assert_eq!(summary.evolution_kind, "noop");
    assert_eq!(summary.control_plane_kind, "fake_local");
    assert_eq!(summary.identity_memory_kind, "hermes_dual_file");
}

#[test]
fn runtime_config_summary_exposes_dry_run_evolution_kind() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.evolution = EvolutionConfig::DryRun;

    config
        .validate()
        .expect("dry_run evolution config is valid");
    let summary = config.summary();

    assert_eq!(summary.evolution_kind, "dry_run");
}

#[test]
fn runtime_config_rejects_zero_command_control_timeout() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.control_plane = ControlPlaneConfig::Command(ControlPlaneCommandConfig {
        program: "printf".to_string(),
        list_args: "[]".to_string(),
        apply_args: "{}".to_string(),
        timeout_ms: 0,
    });

    let err = config
        .validate()
        .expect_err("zero command timeout should fail");

    assert_eq!(err.field, "control.timeout_ms");
}

#[test]
fn runtime_config_summary_exposes_command_control_timeout() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.control_plane = ControlPlaneConfig::Command(ControlPlaneCommandConfig {
        program: "printf".to_string(),
        list_args: "[]".to_string(),
        apply_args: "{}".to_string(),
        timeout_ms: 12_345,
    });

    config
        .validate()
        .expect("command control config should be valid");
    let summary = config.summary();

    assert_eq!(summary.control_plane_kind, "command");
    assert_eq!(summary.control_command_timeout_ms, Some(12_345));
}

#[test]
fn runtime_config_summary_can_expose_queued_subagent_kind() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: PathBuf::from("./tmp/subagent-queue"),
    };

    config.validate().expect("queued subagent config is valid");
    let summary = config.summary();

    assert_eq!(summary.subagent_kind, "queued_external");
    assert_eq!(summary.subagent_queue_root, "./tmp/subagent-queue");
}

#[test]
fn subagent_queue_config_builds_file_queue_config() {
    let config = SubagentQueueConfig {
        root: PathBuf::from("./data/subagent-queue"),
    };

    let queue = config
        .build_file_queue_config()
        .expect("queue config should build");

    assert_eq!(queue.root, PathBuf::from("./data/subagent-queue"));
    assert_eq!(queue.dispatch_dir, "dispatch");
    assert_eq!(queue.report_dir, "reports");
}

#[test]
fn subagent_queue_config_rejects_empty_root() {
    let config = SubagentQueueConfig {
        root: PathBuf::from(""),
    };

    let err = config
        .validate()
        .expect_err("empty queue root should be rejected");

    assert_eq!(err.field, "subagent_queue.root");
}

#[test]
fn identity_memory_config_builds_dual_file_config() {
    let config = IdentityMemoryConfig::HermesDualFile {
        root: PathBuf::from("./data/identity"),
        user_max_chars: 111,
        memory_max_chars: 222,
    };

    let dual_file = config
        .build_dual_file_config()
        .expect("dual file config should build");

    assert_eq!(dual_file.root, PathBuf::from("./data/identity"));
    assert_eq!(dual_file.user_max_chars, 111);
    assert_eq!(dual_file.memory_max_chars, 222);
}

#[test]
fn identity_memory_config_rejects_zero_limits() {
    let config = IdentityMemoryConfig::HermesDualFile {
        root: PathBuf::from("./data/identity"),
        user_max_chars: 0,
        memory_max_chars: 2200,
    };

    let err = config
        .validate()
        .expect_err("zero user limit should be rejected");

    assert_eq!(err.field, "identity_memory.user_max_chars");
}
