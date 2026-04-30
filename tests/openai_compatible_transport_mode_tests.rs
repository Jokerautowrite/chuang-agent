use chuang_agent::provider_openai_compatible::{
    OpenAICompatibleProviderAdapter, ProviderTransport,
};
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};

#[test]
fn openai_compatible_adapter_respond_exposes_transport_mode_stub() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Stub);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 transport seam".to_string(),
        recall_hit_count: 1,
    });

    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("stub")
    );
}
