use std::path::PathBuf;

use chuang_agent::responder::ProviderTransport;
use chuang_agent::runtime_config::{OpenAICompatibleConfig, ProviderConfig, RuntimeConfig};

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
    assert_eq!(summary.evolution_kind, "noop");
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
}
