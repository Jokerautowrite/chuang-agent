use chuang_agent::runtime_config::{ProviderConfig, SubagentConfig};
use chuang_agent::runtime_config_file::{parse_runtime_config_file, RuntimeConfigFileError};

#[test]
fn config_file_parses_simple_fake_runtime_config() {
    let config = parse_runtime_config_file(
        r#"
db_path = "./tmp/chuang.db"
recall_limit = 7
identity_memory_root = "./tmp/identity"
subagent = "queued_external"
subagent_queue_root = "./tmp/subagents"

[provider]
kind = "fake"
id = "fake-test"
model = "stub-test"

[context]
max_tokens = 256
reserve_system_tokens = 32
min_working_tokens = 4
max_tool_results = 2
max_memory_segments = 3
"#,
    )
    .expect("config should parse");

    assert_eq!(config.db_path.display().to_string(), "./tmp/chuang.db");
    assert_eq!(config.recall_limit, 7);
    assert_eq!(config.subagent, SubagentConfig::QueuedExternal);
    assert_eq!(
        config.subagent_queue.root.display().to_string(),
        "./tmp/subagents"
    );
    assert_eq!(config.context_budget.max_tokens, 256);
    assert_eq!(config.context_budget.reserve_system_tokens, 32);
    assert_eq!(config.context_budget.min_working_tokens, 4);
    assert_eq!(config.context_budget.max_tool_results, 2);
    assert_eq!(config.context_budget.max_memory_segments, 3);
    assert_eq!(
        config.summary().identity_memory_root,
        "./tmp/identity".to_string()
    );
    assert!(matches!(
        config.provider,
        ProviderConfig::Fake {
            provider_id,
            model_name
        } if provider_id == "fake-test" && model_name == "stub-test"
    ));
}

#[test]
fn config_file_parses_flat_maintenance_friendly_shape() {
    let config = parse_runtime_config_file(
        r#"
db_path = "./tmp/chuang.db"
recall_limit = 8
identity_memory_root = "./tmp/identity"
provider = "fake"
provider_id = "flat-fake"
model = "flat-stub"
subagent = "fake"
subagent_queue_root = "./tmp/subagents"
context_max_tokens = 300
context_reserve_system_tokens = 30
context_min_working_tokens = 2
context_max_tool_results = 3
context_max_memory_segments = 4
"#,
    )
    .expect("flat config should parse");

    assert_eq!(config.recall_limit, 8);
    assert_eq!(config.context_budget.max_tokens, 300);
    assert_eq!(config.context_budget.reserve_system_tokens, 30);
    assert_eq!(config.context_budget.min_working_tokens, 2);
    assert_eq!(config.context_budget.max_tool_results, 3);
    assert_eq!(config.context_budget.max_memory_segments, 4);
    assert!(matches!(
        config.provider,
        ProviderConfig::Fake {
            provider_id,
            model_name
        } if provider_id == "flat-fake" && model_name == "flat-stub"
    ));
}

#[test]
fn config_file_uses_env_name_for_openai_compatible_key() {
    std::env::set_var("CHUANG_AGENT_TEST_API_KEY", "test-key");
    let config = parse_runtime_config_file(
        r#"
[provider]
kind = "openai_compatible"
id = "test-provider"
base_url = "https://api.example.com/v1"
model = "gpt-test"
api_key_env = "CHUANG_AGENT_TEST_API_KEY"
transport = "stub"
"#,
    )
    .expect("config should parse");
    std::env::remove_var("CHUANG_AGENT_TEST_API_KEY");

    assert!(matches!(
        config.provider,
        ProviderConfig::OpenAICompatible(provider)
            if provider.provider_id == "test-provider"
                && provider.api_key == "test-key"
                && provider.model_name == "gpt-test"
    ));
}

#[test]
fn config_file_rejects_missing_provider_env() {
    std::env::remove_var("CHUANG_AGENT_MISSING_API_KEY");
    let err = parse_runtime_config_file(
        r#"
[provider]
kind = "openai_compatible"
base_url = "https://api.example.com/v1"
model = "gpt-test"
api_key_env = "CHUANG_AGENT_MISSING_API_KEY"
"#,
    )
    .expect_err("missing env should fail");

    assert_eq!(
        err,
        RuntimeConfigFileError::MissingEnv {
            name: "CHUANG_AGENT_MISSING_API_KEY".to_string()
        }
    );
}

#[test]
fn config_file_rejects_invalid_line() {
    let err = parse_runtime_config_file("db_path ./missing-equals")
        .expect_err("invalid line should fail");

    assert!(matches!(err, RuntimeConfigFileError::InvalidLine { .. }));
}
