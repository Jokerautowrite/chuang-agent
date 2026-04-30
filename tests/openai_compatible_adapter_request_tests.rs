use chuang_agent::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use chuang_agent::responder::{Responder, ResponderRequest};

#[test]
fn openai_compatible_adapter_rejects_missing_base_url() {
    let adapter =
        OpenAICompatibleProviderAdapter::new("custom-openai", "", "test-key", "gpt-4.1-mini");

    let error = adapter
        .validate_config()
        .expect_err("empty base_url should fail");

    assert_eq!(error.field, "base_url");
    assert!(error.message.contains("must not be empty"));
}

#[test]
fn openai_compatible_adapter_rejects_missing_model_name() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "",
    );

    let error = adapter
        .validate_config()
        .expect_err("empty model name should fail");

    assert_eq!(error.field, "model_name");
    assert!(error.message.contains("must not be empty"));
}

#[test]
fn openai_compatible_adapter_emits_structured_request_envelope() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    );

    let envelope = adapter
        .build_request_envelope(&ResponderRequest {
            prompt: "system+context prompt".to_string(),
            user_input: "继续推进创项目".to_string(),
            recall_hit_count: 3,
        })
        .expect("valid config should build envelope");

    assert_eq!(envelope.provider_id, "custom-openai");
    assert_eq!(envelope.model, "gpt-4.1-mini");
    assert_eq!(envelope.base_url, "https://api.example.com/v1");
    assert_eq!(envelope.messages.len(), 2);
    assert_eq!(envelope.messages[0].role, "system");
    assert_eq!(envelope.messages[1].role, "user");
    assert!(envelope.messages[0]
        .content
        .contains("system+context prompt"));
    assert!(envelope.messages[1].content.contains("继续推进创项目"));
}

#[test]
fn openai_compatible_adapter_generate_surfaces_request_shape_in_trace() {
    let responder = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    );

    let output = responder.generate(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进创项目".to_string(),
        recall_hit_count: 1,
    });

    assert!(output.trace.contains("transport=openai-compatible"));
    assert!(output.trace.contains("message_count=2"));
    assert!(output.trace.contains("base_url=https://api.example.com/v1"));
}
