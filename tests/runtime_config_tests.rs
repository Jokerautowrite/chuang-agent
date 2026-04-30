use std::path::PathBuf;

use chuang_agent::responder::ProviderTransport;
use chuang_agent::runtime_config::{
    IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig, RuntimeConfig, SubagentConfig,
    SubagentQueueConfig,
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
    assert_eq!(summary.subagent_queue_root, "./data/subagent-queue");
    assert_eq!(summary.evolution_kind, "noop");
    assert_eq!(summary.control_plane_kind, "fake_local");
    assert_eq!(summary.identity_memory_kind, "hermes_dual_file");
    assert_eq!(summary.identity_user_max_chars, 1375);
    assert_eq!(summary.identity_memory_max_chars, 2200);
    assert_eq!(summary.context_max_tokens, 512);
    assert_eq!(summary.context_reserve_system_tokens, 32);
    assert_eq!(summary.context_min_working_tokens, 1);
    assert_eq!(summary.context_max_tool_results, 5);
    assert_eq!(summary.context_max_memory_segments, 5);
    assert_eq!(summary.api_key_state, None);
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
fn openai_provider_config_redacts_api_key_in_summary() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.provider = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "test-key".to_string(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
    });

    config
        .validate()
        .expect("complete provider config should pass");
    let summary = config.summary();

    assert_eq!(summary.provider_kind, "openai_compatible");
    assert_eq!(summary.provider_id, "custom-openai");
    assert_eq!(summary.api_key_state, Some("<set>".to_string()));
}

#[test]
fn openai_provider_config_builds_adapter_only_for_openai_kind() {
    let fake = ProviderConfig::Fake {
        provider_id: "fake-runtime".to_string(),
        model_name: "stub-responder".to_string(),
    };
    assert!(fake
        .build_openai_compatible()
        .expect("fake config is valid")
        .is_none());

    let openai = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "test-key".to_string(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
    });

    assert!(openai
        .build_openai_compatible()
        .expect("openai config should build")
        .is_some());
}

#[test]
fn openai_provider_config_rejects_missing_required_fields() {
    let config = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: String::new(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
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
