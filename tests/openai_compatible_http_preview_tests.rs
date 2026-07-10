use chuang_agent::provider_openai_compatible::{OpenAICompatibleProviderAdapter, ReasoningEffort};
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};

#[test]
fn openai_compatible_adapter_builds_http_request_preview() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    );

    let preview = adapter
        .build_http_request_preview(&ResponderRequest {
            prompt: "system+context prompt".to_string(),
            user_input: "继续推进创项目".to_string(),
            recall_hit_count: 2,
        })
        .expect("preview should build");

    assert_eq!(preview.method, "POST");
    assert_eq!(preview.url, "https://api.example.com/v1/responses");
    assert_eq!(
        preview.headers.get("authorization").map(String::as_str),
        Some("Bearer test-key")
    );
    assert_eq!(
        preview.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert!(preview.body_json.contains("\"model\":\"gpt-4.1-mini\""));
    assert!(preview
        .body_json
        .contains("\"instructions\":\"system+context prompt\""));
    assert!(preview.body_json.contains("\"input\":\"继续推进创项目\""));
    assert!(preview.body_json.contains("\"store\":false"));
    assert!(!preview.body_json.contains("\"reasoning\""));
}

#[test]
fn openai_compatible_adapter_includes_configured_reasoning_effort() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    )
    .with_reasoning_effort(Some(ReasoningEffort::XHigh));

    let preview = adapter
        .build_http_request_preview(&ResponderRequest {
            prompt: "system+context prompt".to_string(),
            user_input: "continue".to_string(),
            recall_hit_count: 0,
        })
        .expect("preview should build");
    let body: serde_json::Value =
        serde_json::from_str(&preview.body_json).expect("body should be valid json");

    assert_eq!(body["reasoning"]["effort"], "xhigh");
}

#[test]
fn openai_compatible_adapter_respond_exposes_http_request_preview_fields() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    );

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进创项目".to_string(),
        recall_hit_count: 2,
    });

    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some("https://api.example.com/v1/responses")
    );
    assert_eq!(
        response
            .extra_meta
            .get("request_method")
            .map(String::as_str),
        Some("POST")
    );
    assert_eq!(
        response
            .extra_meta
            .get("request_message_count")
            .map(String::as_str),
        Some("2")
    );
    assert!(response
        .trace
        .contains("request_url=https://api.example.com/v1/responses"));
}
