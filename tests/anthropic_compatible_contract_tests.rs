//! Anthropic Messages API provider 合同测试。
//!
//! 覆盖：/v1/messages 请求构造、x-api-key 认证头、system/messages 分离、
//! stub transport 响应提取、slot_registry 构建、Fallback 链兼容、配置文件解析。

use chuang_agent::provider_anthropic_compatible::AnthropicCompatibleProviderAdapter;
use chuang_agent::provider_openai_compatible::ProviderTransport;
use chuang_agent::responder::{ProviderAdapterResponder, Responder, ResponderRequest};
use chuang_agent::runtime_config::{
    AnthropicApiEndpoint, AnthropicCompatibleConfig, ProviderConfig, ProviderFallbackPolicy,
    RuntimeConfig,
};
use chuang_agent::runtime_config_file::{
    parse_runtime_config_file, parse_runtime_config_file_with_options, RuntimeConfigFileOptions,
};
use chuang_agent::slot_registry::build_provider_responder;

fn request() -> ResponderRequest {
    ResponderRequest {
        prompt: "你是系统提示".to_string(),
        user_input: "用户问题".to_string(),
        recall_hit_count: 1,
    }
}

fn anthropic_config() -> AnthropicCompatibleConfig {
    AnthropicCompatibleConfig {
        provider_id: "anthropic-main".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        api_key: "test-key-123".to_string(),
        model_name: "claude-opus-4-1".to_string(),
        transport: ProviderTransport::Stub,
        endpoint: AnthropicApiEndpoint::Messages,
        reasoning_effort: None,
        request_timeout_ms: Some(30_000),
        tls_ca_cert_path: None,
    }
}

#[test]
fn build_http_request_preview_uses_v1_messages_and_anthropic_auth() {
    let adapter = AnthropicCompatibleProviderAdapter::new(
        "anthropic-main",
        "https://api.anthropic.com",
        "test-key-123",
        "claude-opus-4-1",
    );

    let preview = adapter
        .build_http_request_preview(&request())
        .expect("preview should build");

    assert_eq!(preview.method, "POST");
    assert_eq!(preview.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(
        preview.headers.get("x-api-key").map(String::as_str),
        Some("test-key-123")
    );
    assert_eq!(
        preview.headers.get("anthropic-version").map(String::as_str),
        Some("2023-06-01")
    );
    assert_eq!(
        preview.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert!(
        !preview.headers.contains_key("authorization"),
        "anthropic must not use Bearer authorization"
    );
}

#[test]
fn request_body_keeps_system_top_level_and_messages_user_only() {
    let adapter = AnthropicCompatibleProviderAdapter::new(
        "anthropic-main",
        "https://api.anthropic.com",
        "test-key-123",
        "claude-opus-4-1",
    )
    .with_max_output_tokens(Some(2048));

    let preview = adapter
        .build_http_request_preview(&request())
        .expect("preview should build");
    let body: serde_json::Value =
        serde_json::from_str(&preview.body_json).expect("body should be valid json");

    assert_eq!(body["model"], "claude-opus-4-1");
    assert_eq!(body["system"], "你是系统提示");
    assert_eq!(body["max_tokens"], 2048);
    assert_eq!(body["temperature"], 0.7);

    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "用户问题");
    for message in messages {
        assert_ne!(
            message["role"], "system",
            "system prompt must stay at top-level `system`, not in messages"
        );
    }
}

#[test]
fn stub_transport_respond_extracts_anthropic_message_content() {
    let adapter = AnthropicCompatibleProviderAdapter::new(
        "anthropic-main",
        "https://api.anthropic.com",
        "test-key-123",
        "claude-opus-4-1",
    )
    .with_transport(ProviderTransport::Stub);

    let response = adapter.respond(&request());
    assert!(response.body.contains("stubbed_post_ok"));
    assert_eq!(response.finish_reason.as_deref(), Some("end_turn"));
    assert_eq!(
        response.extra_meta.get("transport").map(String::as_str),
        Some("anthropic-compatible")
    );
    assert_eq!(
        response.extra_meta.get("response_kind").map(String::as_str),
        Some("message")
    );
    assert_eq!(
        response.extra_meta.get("response_finish_reason").map(String::as_str),
        Some("end_turn")
    );
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some("https://api.anthropic.com/v1/messages")
    );
}

#[test]
fn slot_registry_builds_anthropic_slot_and_generates() {
    let config = ProviderConfig::AnthropicCompatible(anthropic_config());
    let slot = build_provider_responder(&config).expect("slot should build");

    assert_eq!(slot.provider_name(), "anthropic-main");
    assert_eq!(slot.model_name(), "claude-opus-4-1");

    let output = slot.generate(&request());
    assert!(output.body.contains("stubbed_post_ok"));
    assert_eq!(output.meta.provider.as_deref(), Some("anthropic-main"));
    assert_eq!(output.model_name, "claude-opus-4-1");
    assert_eq!(
        output.meta.extra.get("provider_fallback_configured").map(String::as_str),
        Some("false")
    );
}

#[test]
fn anthropic_primary_with_fake_fallback_generates_via_fallback_chain() {
    let config = ProviderConfig::Fallback {
        primary: Box::new(ProviderConfig::AnthropicCompatible(anthropic_config())),
        fallback: Box::new(ProviderConfig::Fake {
            provider_id: "fallback-fake".to_string(),
            model_name: "stub-fallback".to_string(),
        }),
        policy: ProviderFallbackPolicy {
            on_retryable: true,
            status_codes: vec![401, 402],
            error_classes: Vec::new(),
        },
    };
    let slot = build_provider_responder(&config).expect("fallback slot should build");

    assert_eq!(slot.provider_name(), "anthropic-main");
    assert_eq!(slot.model_name(), "claude-opus-4-1");

    // stub transport 成功，不走 fallback。
    let output = slot.generate(&request());
    assert!(output.body.contains("stubbed_post_ok"));
    assert_eq!(
        output.meta.extra.get("provider_fallback_configured").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        output.meta.extra.get("provider_fallback_used").map(String::as_str),
        Some("false")
    );
}

#[test]
fn config_file_parses_anthropic_compatible_primary_with_fallback() {
    std::env::set_var("CHUANG_AGENT_ANTHROPIC_TEST_API_KEY", "primary-key");
    std::env::set_var("CHUANG_AGENT_ANTHROPIC_FALLBACK_API_KEY", "fallback-key");
    let config = parse_runtime_config_file(
        r#"
provider = "anthropic_compatible"
provider_id = "anthropic-primary"
base_url = "https://api.anthropic.com"
model = "claude-opus-4-1"
api_key_env = "CHUANG_AGENT_ANTHROPIC_TEST_API_KEY"
transport = "stub"
endpoint = "messages"
reasoning_effort = "medium"
provider_timeout_ms = "45000"

fallback_provider = "anthropic_compatible"
fallback_provider_id = "anthropic-fallback"
fallback_base_url = "https://api.anthropic.com"
fallback_model = "claude-sonnet-4-5"
fallback_api_key_env = "CHUANG_AGENT_ANTHROPIC_FALLBACK_API_KEY"
fallback_transport = "stub"
fallback_on_retryable = "false"
fallback_status_codes = "401,402,529"
"#,
    )
    .expect("anthropic fallback config should parse");
    std::env::remove_var("CHUANG_AGENT_ANTHROPIC_TEST_API_KEY");
    std::env::remove_var("CHUANG_AGENT_ANTHROPIC_FALLBACK_API_KEY");

    assert_eq!(config.summary().provider_kind, "fallback");
    assert_eq!(
        config.summary().provider_id,
        "anthropic-primary->anthropic-fallback"
    );
    assert_eq!(
        config.summary().model_name,
        "claude-opus-4-1->claude-sonnet-4-5"
    );
    assert_eq!(config.summary().provider_reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(
        config.summary().provider_fallback_policy.as_deref(),
        Some("retryable=false status_codes=401,402,529 error_classes=none")
    );

    match &config.provider {
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => {
            assert!(matches!(
                primary.as_ref(),
                ProviderConfig::AnthropicCompatible(primary)
                    if primary.provider_id == "anthropic-primary"
                        && primary.model_name == "claude-opus-4-1"
                        && primary.api_key == "primary-key"
                        && primary.endpoint == AnthropicApiEndpoint::Messages
                        && primary.request_timeout_ms == Some(45_000)
            ));
            assert!(matches!(
                fallback.as_ref(),
                ProviderConfig::AnthropicCompatible(fallback)
                    if fallback.provider_id == "anthropic-fallback"
                        && fallback.model_name == "claude-sonnet-4-5"
                        && fallback.api_key == "fallback-key"
            ));
        }
        other => panic!("expected fallback provider, got {other:?}"),
    }
}

#[test]
fn config_file_accepts_allow_missing_env_for_anthropic_key() {
    std::env::remove_var("CHUANG_AGENT_ANTHROPIC_MISSING_API_KEY");
    let config = parse_runtime_config_file_with_options(
        r#"
[provider]
kind = "anthropic_compatible"
id = "anthropic-main"
base_url = "https://api.anthropic.com"
model = "claude-opus-4-1"
api_key_env = "CHUANG_AGENT_ANTHROPIC_MISSING_API_KEY"
transport = "stub"
"#,
        RuntimeConfigFileOptions::allow_missing_env(),
    )
    .expect("anthropic config should parse in diagnostic mode");

    assert!(matches!(
        &config.provider,
        ProviderConfig::AnthropicCompatible(provider)
            if provider.provider_id == "anthropic-main"
                && provider.api_key == "__MISSING_ENV:CHUANG_AGENT_ANTHROPIC_MISSING_API_KEY__"
    ));
    assert_eq!(
        config.summary().api_key_state.as_deref(),
        Some("<missing:CHUANG_AGENT_ANTHROPIC_MISSING_API_KEY>")
    );
}

#[test]
fn runtime_with_anthropic_provider_validates_and_builds_slots() {
    std::env::set_var("CHUANG_AGENT_ANTHROPIC_RUNTIME_API_KEY", "runtime-key");
    let mut config = parse_runtime_config_file(
        r#"
[provider]
kind = "anthropic_compatible"
id = "anthropic-main"
base_url = "https://api.anthropic.com"
model = "claude-opus-4-1"
api_key_env = "CHUANG_AGENT_ANTHROPIC_RUNTIME_API_KEY"
transport = "stub"
"#,
    )
    .expect("config should parse");
    let runtime: &mut RuntimeConfig = &mut config;
    runtime
        .provider
        .validate()
        .expect("anthropic provider config should validate");
    std::env::remove_var("CHUANG_AGENT_ANTHROPIC_RUNTIME_API_KEY");

    let slot = build_provider_responder(&runtime.provider).expect("slot should build");
    assert_eq!(slot.provider_name(), "anthropic-main");
}
