use chuang_agent::responder::{
    OpenAICompatibleProviderAdapter, ProviderAdapterResponder, ProviderTransport, ResponderRequest,
};

#[test]
fn openai_compatible_http_transport_reports_config_error_for_https_until_tls_client_exists() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 http transport".to_string(),
        recall_hit_count: 2,
    });

    assert_eq!(response.finish_reason.as_deref(), Some("invalid-config"));
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some("https://api.example.com/v1/chat/completions")
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
    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("http")
    );
    assert_eq!(
        response
            .extra_meta
            .get("config_error_field")
            .map(String::as_str),
        Some("base_url")
    );
    assert!(response.body.contains("unsupported_http_scheme"));
}

#[test]
fn openai_compatible_http_transport_reports_connect_error_for_invalid_port_shape() {
    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "http://127.0.0.1:notaport/v1",
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 invalid port".to_string(),
        recall_hit_count: 2,
    });

    assert_eq!(response.finish_reason.as_deref(), Some("invalid-config"));
    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("http")
    );
    assert_eq!(
        response
            .extra_meta
            .get("config_error_field")
            .map(String::as_str),
        Some("base_url")
    );
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some("http://127.0.0.1:notaport/v1/chat/completions")
    );
    assert!(response
        .body
        .contains("invalid_port:http://127.0.0.1:notaport/v1/chat/completions"));
}
