use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::runtime_config::{
    ActuatorConfig, ContextEngineConfig, ControlPlaneConfig, EvolutionConfig, ProviderConfig,
    SubagentConfig,
};
use chuang_agent::runtime_config_file::{
    parse_runtime_config_file, parse_runtime_config_file_with_options, RuntimeConfigFileError,
    RuntimeConfigFileOptions,
};

#[test]
fn config_file_parses_simple_fake_runtime_config() {
    let config = parse_runtime_config_file(
        r#"
db_path = "./tmp/chuang.db"
recall_limit = 7
tool_max_rounds = 6
tool_shell_timeout_ms = 45000
identity_memory_root = "./tmp/identity"
identity_root = "./tmp/bootstrap"
soul_path = "./tmp/bootstrap/SOUL.md"
story_path = "./tmp/bootstrap/STORY.md"
first_wake_path = "./tmp/bootstrap/FIRST_WAKE.md"
agents_registry_path = "./tmp/bootstrap/agents.toml"
rules_root = "./tmp/rules"
rules_core_path = "./tmp/rules/core.md"
	subagent = "queued_external"
	subagent_live_worker_enabled = "true"
	subagent_live_worker_adapter_kind = "command"
	subagent_live_worker_status = "configured_status_only"
	subagent_queue_root = "./tmp/subagents"
context_engine = "summary_compression"

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
    assert_eq!(config.tool_loop.max_rounds, 6);
    assert_eq!(config.tool_loop.shell_timeout_ms, 45_000);
    assert_eq!(config.subagent, SubagentConfig::QueuedExternal);
    assert_eq!(config.subagent_live_worker.enabled, true);
    assert_eq!(config.subagent_live_worker.adapter_kind, "command");
    assert_eq!(config.subagent_live_worker.status, "configured_status_only");
    assert_eq!(config.subagent_live_worker.starts_worker, false);
    assert_eq!(
        config.context_engine,
        ContextEngineConfig::SummaryCompression
    );
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
    assert_eq!(
        config.summary().identity_experiences_path,
        "./tmp/identity/experiences.md".to_string()
    );
    assert_eq!(config.summary().identity_root, "./tmp/bootstrap");
    assert_eq!(config.summary().soul_path, "./tmp/bootstrap/SOUL.md");
    assert_eq!(
        config.summary().first_wake_path,
        "./tmp/bootstrap/FIRST_WAKE.md"
    );
    assert_eq!(config.summary().rules_root, "./tmp/rules");
    assert_eq!(config.summary().rules_core_path, "./tmp/rules/core.md");
    assert_eq!(config.summary().tool_loop_max_rounds, 6);
    assert_eq!(config.summary().tool_shell_timeout_ms, 45_000);
    assert_eq!(config.summary().provider_tls_ca_cert_path, None);
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
tls_ca_path = "./tmp/provider-ca.pem"
subagent = "fake"
subagent_queue_root = "./tmp/subagents"
context_max_tokens = 300
context_reserve_system_tokens = 30
context_min_working_tokens = 2
context_max_tool_results = 3
context_max_memory_segments = 4

[tool_loop]
max_rounds = 9
shell_timeout_ms = 120000

[tool_loop.risk]
network_change = " make deploy, fly deploy"
secret_access = " .env.local, vault token"
"#,
    )
    .expect("flat config should parse");

    assert_eq!(config.recall_limit, 8);
    assert_eq!(config.context_budget.max_tokens, 300);
    assert_eq!(config.context_budget.reserve_system_tokens, 30);
    assert_eq!(config.context_budget.min_working_tokens, 2);
    assert_eq!(config.context_budget.max_tool_results, 3);
    assert_eq!(config.context_budget.max_memory_segments, 4);
    assert_eq!(config.tool_loop.max_rounds, 9);
    assert_eq!(config.tool_loop.shell_timeout_ms, 120_000);
    assert_eq!(
        config.tool_loop.shell_risk_rules.network_change,
        vec!["make deploy".to_string(), "fly deploy".to_string()]
    );
    assert_eq!(
        config.tool_loop.shell_risk_rules.secret_access,
        vec![".env.local".to_string(), "vault token".to_string()]
    );
    assert!(matches!(
        config.provider,
        ProviderConfig::Fake {
            provider_id,
            model_name
        } if provider_id == "flat-fake" && model_name == "flat-stub"
    ));
}

#[test]
fn config_file_parses_external_knowledge_read_contract_fields() {
    let config = parse_runtime_config_file(
        r#"
db_path = "./tmp/chuang.db"
identity_memory_root = "./tmp/identity"

[external_knowledge.wiki]
endpoint = "https://wiki.example.com/api"
token_env = "CHUANG_WIKI_TOKEN"
timeout_ms = 5000

[external_knowledge.gbrain]
endpoint = "https://gbrain.example.com/api"
token_env = "CHUANG_GBRAIN_TOKEN"
timeout_ms = 7000
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config.external_knowledge.wiki.endpoint.as_deref(),
        Some("https://wiki.example.com/api")
    );
    assert_eq!(
        config.external_knowledge.wiki.token_env.as_deref(),
        Some("CHUANG_WIKI_TOKEN")
    );
    assert_eq!(config.external_knowledge.wiki.timeout_ms, Some(5000));
    assert_eq!(
        config.external_knowledge.gbrain.endpoint.as_deref(),
        Some("https://gbrain.example.com/api")
    );
    assert_eq!(
        config.external_knowledge.gbrain.token_env.as_deref(),
        Some("CHUANG_GBRAIN_TOKEN")
    );
    assert_eq!(config.external_knowledge.gbrain.timeout_ms, Some(7000));

    let summary = config.summary();
    assert_eq!(
        summary.external_knowledge_wiki_endpoint.as_deref(),
        Some("https://wiki.example.com/api")
    );
    assert_eq!(
        summary.external_knowledge_wiki_token_env.as_deref(),
        Some("CHUANG_WIKI_TOKEN")
    );
    assert_eq!(summary.external_knowledge_wiki_timeout_ms, Some(5000));
    assert_eq!(
        summary.external_knowledge_gbrain_endpoint.as_deref(),
        Some("https://gbrain.example.com/api")
    );
    assert_eq!(
        summary.external_knowledge_gbrain_token_env.as_deref(),
        Some("CHUANG_GBRAIN_TOKEN")
    );
    assert_eq!(summary.external_knowledge_gbrain_timeout_ms, Some(7000));
}

#[test]
fn config_file_parses_section_subagent_live_worker_status_only_shape() {
    let config = parse_runtime_config_file(
        r#"
[subagent_live_worker]
enabled = "true"
adapter_kind = "command"
status = "configured_status_only"
starts_worker = "false"
"#,
    )
    .expect("section live worker config should parse");

    let summary = config.summary().subagent_live_worker;
    assert_eq!(config.subagent_live_worker.enabled, true);
    assert_eq!(config.subagent_live_worker.adapter_kind, "command");
    assert_eq!(config.subagent_live_worker.status, "configured_status_only");
    assert_eq!(config.subagent_live_worker.starts_worker, false);
    assert_eq!(summary.enabled, true);
    assert_eq!(summary.adapter_kind, "command");
    assert_eq!(summary.status, "configured_status_only");
    assert_eq!(summary.starts_worker, false);
    assert_eq!(summary.available, false);
    assert!(summary.reason.contains("status only"));
}

#[test]
fn config_file_rejects_subagent_live_worker_starts_worker_flat_key() {
    let err = parse_runtime_config_file(
        r#"
subagent_live_worker_enabled = "true"
subagent_live_worker_adapter_kind = "command"
subagent_live_worker_status = "configured_status_only"
subagent_live_worker_starts_worker = "true"
"#,
    )
    .expect_err("flat starts_worker=true should be rejected");

    assert_eq!(
        err,
        RuntimeConfigFileError::InvalidValue {
            key: "subagent_live_worker.starts_worker".to_string(),
            value: "true".to_string()
        }
    );
}

#[test]
fn config_file_rejects_subagent_live_worker_starts_worker_section_key() {
    let err = parse_runtime_config_file(
        r#"
[subagent_live_worker]
enabled = "true"
adapter_kind = "command"
status = "configured_status_only"
starts_worker = "true"
"#,
    )
    .expect_err("section starts_worker=true should be rejected");

    assert_eq!(
        err,
        RuntimeConfigFileError::InvalidValue {
            key: "subagent_live_worker.starts_worker".to_string(),
            value: "true".to_string()
        }
    );
}

#[test]
fn config_file_parses_openai_tls_ca_path() {
    let tls_path = temp_test_path("provider-ca.pem");
    fs::create_dir_all(tls_path.parent().expect("temp parent")).expect("temp dir should exist");
    fs::write(&tls_path, "dummy-ca").expect("tls ca file should write");
    std::env::set_var("CHUANG_AGENT_TLS_TEST_API_KEY", "test-key");

    let config = parse_runtime_config_file(&format!(
        r#"
[provider]
kind = "openai_compatible"
id = "test-provider"
base_url = "https://api.example.com/v1"
model = "gpt-test"
api_key_env = "CHUANG_AGENT_TLS_TEST_API_KEY"
transport = "native"
tls_ca_path = "{}"
"#,
        tls_path.display()
    ))
    .expect("config should parse");

    let validated = config.clone();
    validated.validate().expect("config should validate");
    std::env::remove_var("CHUANG_AGENT_TLS_TEST_API_KEY");

    assert!(matches!(
        &config.provider,
        ProviderConfig::OpenAICompatible(provider)
            if provider.provider_id == "test-provider"
                && provider.model_name == "gpt-test"
                && provider.transport.as_str() == "native"
                && provider.tls_ca_cert_path.as_ref() == Some(&tls_path)
    ));
    assert_eq!(
        config.summary().provider_tls_ca_cert_path,
        Some(tls_path.display().to_string())
    );
}

#[test]
fn config_file_parses_command_control_plane() {
    let config = parse_runtime_config_file(
        r#"
[control]
kind = "command"
program = "printf"
list_args = "[]"
apply_args = "{}"
timeout_ms = 1234
"#,
    )
    .expect("command control config should parse");

    assert!(matches!(
        &config.control_plane,
        ControlPlaneConfig::Command(control)
            if control.program == "printf"
                && control.list_args == "[]"
                && control.apply_args == "{}"
                && control.timeout_ms == 1234
    ));
    assert_eq!(config.summary().control_plane_kind, "command");
    assert_eq!(config.summary().control_command_timeout_ms, Some(1234));
}

#[test]
fn config_file_parses_command_actuator() {
    let config = parse_runtime_config_file(
        r#"
[actuator]
kind = "command"
program = "sh"
args = "./scripts/chuang-actuator-adapter-example.sh --json"
timeout_ms = 2345
"#,
    )
    .expect("command actuator config should parse");

    assert!(matches!(
        &config.actuator,
        ActuatorConfig::Command(actuator)
            if actuator.program == "sh"
                && actuator.args == "./scripts/chuang-actuator-adapter-example.sh --json"
                && actuator.timeout_ms == 2345
    ));
    assert_eq!(config.summary().actuator_kind, "command");
    assert_eq!(config.summary().actuator_command_timeout_ms, Some(2345));
}

#[test]
fn config_file_defaults_command_control_timeout() {
    let config = parse_runtime_config_file(
        r#"
control = "command"
program = "printf"
list_args = "[]"
apply_args = "{}"
"#,
    )
    .expect("command control config should parse");

    assert!(matches!(
        &config.control_plane,
        ControlPlaneConfig::Command(control) if control.timeout_ms == 30_000
    ));
}

#[test]
fn config_file_parses_dry_run_evolution_kind() {
    let config = parse_runtime_config_file(
        r#"
evolution = "dry_run"
"#,
    )
    .expect("dry_run evolution config should parse");

    assert_eq!(config.evolution, EvolutionConfig::DryRun);
    assert_eq!(config.summary().evolution_kind, "dry_run");
}

#[test]
fn config_file_rejects_unknown_evolution_kind() {
    let err = parse_runtime_config_file(r#"evolution = "unknown_mode""#)
        .expect_err("unknown evolution kind should fail");

    assert_eq!(
        err,
        RuntimeConfigFileError::InvalidValue {
            key: "evolution.kind".to_string(),
            value: "unknown_mode".to_string()
        }
    );
}

#[test]
fn config_file_rejects_unknown_context_engine() {
    let err = parse_runtime_config_file(r#"context_engine = "unknown_engine""#)
        .expect_err("unknown context engine should fail");

    assert_eq!(
        err,
        RuntimeConfigFileError::InvalidValue {
            key: "context.engine".to_string(),
            value: "unknown_engine".to_string()
        }
    );
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
reasoning_effort = "xhigh"
"#,
    )
    .expect("config should parse");
    std::env::remove_var("CHUANG_AGENT_TEST_API_KEY");

    assert!(matches!(
        &config.provider,
        ProviderConfig::OpenAICompatible(provider)
            if provider.provider_id == "test-provider"
                && provider.api_key == "test-key"
                && provider.model_name == "gpt-test"
                && provider.reasoning_effort.map(|effort| effort.as_str()) == Some("xhigh")
    ));
    assert_eq!(
        config.summary().provider_reasoning_effort.as_deref(),
        Some("xhigh")
    );
}

#[test]
fn config_file_rejects_unsupported_reasoning_effort() {
    std::env::set_var("CHUANG_AGENT_REASONING_TEST_API_KEY", "test-key");
    let error = parse_runtime_config_file(
        r#"
[provider]
kind = "openai_compatible"
base_url = "https://api.example.com/v1"
model = "gpt-test"
api_key_env = "CHUANG_AGENT_REASONING_TEST_API_KEY"
reasoning_effort = "extreme"
"#,
    )
    .expect_err("unsupported reasoning effort should be rejected");
    std::env::remove_var("CHUANG_AGENT_REASONING_TEST_API_KEY");

    assert_eq!(
        error,
        RuntimeConfigFileError::InvalidValue {
            key: "provider.reasoning_effort".to_string(),
            value: "extreme".to_string(),
        }
    );
}

#[test]
fn config_file_parses_explicit_provider_fallback() {
    std::env::set_var("CHUANG_AGENT_PRIMARY_TEST_API_KEY", "primary-key");
    let config = parse_runtime_config_file(
        r#"
provider = "openai_compatible"
provider_id = "primary"
base_url = "http://127.0.0.1:8317/v1"
model = "gpt-primary"
api_key_env = "CHUANG_AGENT_PRIMARY_TEST_API_KEY"
transport = "native"

fallback_provider = "fake"
fallback_provider_id = "fallback"
fallback_model = "gpt-fallback"
fallback_on_retryable = "false"
fallback_status_codes = "401,402,429"
fallback_error_classes = "transport,tls"
"#,
    )
    .expect("fallback config should parse");
    std::env::remove_var("CHUANG_AGENT_PRIMARY_TEST_API_KEY");

    assert!(matches!(
        &config.provider,
        ProviderConfig::Fallback {
            primary,
            fallback,
            policy,
        }
            if matches!(primary.as_ref(), ProviderConfig::OpenAICompatible(primary_config)
                if primary_config.provider_id == "primary"
                    && primary_config.model_name == "gpt-primary")
                && matches!(fallback.as_ref(), ProviderConfig::Fake { provider_id, model_name }
                    if provider_id == "fallback" && model_name == "gpt-fallback")
                && !policy.on_retryable
                && policy.status_codes == vec![401, 402, 429]
                && policy.error_classes == vec!["transport".to_string(), "tls".to_string()]
    ));
    assert_eq!(config.summary().provider_kind, "fallback");
    assert_eq!(config.summary().provider_id, "primary->fallback");
    assert_eq!(config.summary().model_name, "gpt-primary->gpt-fallback");
    assert_eq!(
        config.summary().provider_fallback_policy.as_deref(),
        Some("retryable=false status_codes=401,402,429 error_classes=transport,tls")
    );
}

#[test]
fn config_file_parses_provider_fallback_example_without_secret_values() {
    let content = fs::read_to_string("config.example-provider-fallback.toml")
        .expect("provider fallback example should be readable");

    let config = parse_runtime_config_file_with_options(
        &content,
        RuntimeConfigFileOptions::allow_missing_env(),
    )
    .expect("provider fallback example should parse in diagnostic mode");

    assert_eq!(config.summary().provider_kind, "fallback");
    assert_eq!(
        config.summary().provider_id,
        "primary-openai-compatible->backup-openai-compatible"
    );
    assert_eq!(config.summary().model_name, "primary-model->backup-model");
    assert_eq!(
        config.summary().api_key_state,
        Some(
            "primary:<missing:CHUANG_AGENT_PRIMARY_API_KEY> fallback:<missing:CHUANG_AGENT_FALLBACK_API_KEY>"
                .to_string()
        )
    );
    assert_eq!(
        config.summary().provider_fallback_policy.as_deref(),
        Some("retryable=true status_codes=429,500,502,503,504 error_classes=transport,tls")
    );

    match &config.provider {
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => {
            assert!(matches!(
                primary.as_ref(),
                ProviderConfig::OpenAICompatible(primary)
                    if primary.api_key == "__MISSING_ENV:CHUANG_AGENT_PRIMARY_API_KEY__"
            ));
            assert!(matches!(
                fallback.as_ref(),
                ProviderConfig::OpenAICompatible(fallback)
                    if fallback.api_key == "__MISSING_ENV:CHUANG_AGENT_FALLBACK_API_KEY__"
            ));
        }
        other => panic!("expected fallback provider, got {other:?}"),
    }
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

fn temp_test_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-runtime-config-{name}-{nanos}"))
}
