use chuang_agent::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};

#[test]
fn openai_compatible_adapter_stub_post_call_returns_preview_body() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    );

    let result = adapter
        .execute_stub_post_call(&ResponderRequest {
            prompt: "system+context prompt".to_string(),
            user_input: "继续推进创项目".to_string(),
            recall_hit_count: 2,
        })
        .expect("stub post should build");

    assert_eq!(result.status_code, 200);
    assert_eq!(result.url, "https://api.example.com/v1/responses");
    assert!(result
        .request_body_json
        .contains("\"model\":\"gpt-4.1-mini\""));
    assert!(result.response_body_json.contains("\"stubbed\":true"));
    assert!(result
        .response_body_json
        .contains("\"provider_id\":\"custom-openai\""));
}

#[test]
fn openai_compatible_adapter_respond_surfaces_stub_post_artifacts() {
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
        response
            .extra_meta
            .get("stub_status_code")
            .map(String::as_str),
        Some("200")
    );
    assert_eq!(
        response
            .extra_meta
            .get("stub_response_kind")
            .map(String::as_str),
        Some("response")
    );
    assert!(response.body.contains("stubbed_post_ok"));
    assert!(response.trace.contains("status_code=200"));
}
