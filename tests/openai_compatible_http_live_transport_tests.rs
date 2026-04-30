use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use chuang_agent::provider_openai_compatible::{
    OpenAICompatibleProviderAdapter, ProviderTransport,
};
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};

#[test]
fn openai_compatible_http_transport_surfaces_response_parse_error_when_status_line_missing() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let _ = stream
            .read(&mut buffer)
            .expect("request should be readable");

        let body = r#"{"oops":true}"#;
        let response = format!(
            "\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("http://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 malformed status line".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

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
        Some("http_response")
    );
    assert!(response.body.contains("missing_status_code"));
}

#[test]
fn openai_compatible_http_transport_surfaces_response_parse_error_when_header_separator_missing() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let _ = stream
            .read(&mut buffer)
            .expect("request should be readable");

        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 14";
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("http://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 missing header separator".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

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
        Some("http_response")
    );
    assert!(response.body.contains("missing_header_separator"));
}

#[test]
fn openai_compatible_http_transport_returns_structured_error_when_server_unreachable() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    drop(listener);

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("http://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 http transport".to_string(),
        recall_hit_count: 2,
    });

    assert!(response
        .body
        .starts_with("CONFIG_ERROR: openai-compatible provider invalid field=http_connect"));
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
        Some("http_connect")
    );
    let expected_url = format!("http://{address}/v1/chat/completions");
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some(expected_url.as_str())
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
}

#[test]
fn openai_compatible_http_transport_surfaces_success_metadata_when_server_reachable() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let bytes = stream
            .read(&mut buffer)
            .expect("request should be readable");
        let request_text = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        assert!(request_text.starts_with("POST /v1/chat/completions HTTP/1.1"));

        let body = r#"{"id":"chatcmpl-local-2","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"http_live_ok"},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("http://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 http transport".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

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
            .get("request_method")
            .map(String::as_str),
        Some("POST")
    );
    let expected_url = format!("http://{address}/v1/chat/completions");
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some(expected_url.as_str())
    );
    assert_eq!(
        response.extra_meta.get("transport").map(String::as_str),
        Some("openai-compatible")
    );
    assert_eq!(
        response.extra_meta.get("status_code").map(String::as_str),
        Some("200")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response
            .extra_meta
            .get("response_finish_reason")
            .map(String::as_str),
        Some("stop")
    );
    assert_eq!(response.body, "http_live_ok");
}

#[test]
fn openai_compatible_http_transport_preserves_non_200_status_with_structured_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let _ = stream
            .read(&mut buffer)
            .expect("request should be readable");

        let body = r#"{"error":{"message":"rate limit hit"}}"#;
        let response = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("http://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Http);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 429 metadata".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("http")
    );
    assert_eq!(
        response.extra_meta.get("status_code").map(String::as_str),
        Some("429")
    );
    assert_eq!(
        response.extra_meta.get("response_kind").map(String::as_str),
        Some("unknown")
    );
    assert_eq!(
        response
            .extra_meta
            .get("response_finish_reason")
            .map(String::as_str),
        Some("http-openai-compatible")
    );
    assert_eq!(
        response.finish_reason.as_deref(),
        Some("http-openai-compatible")
    );
    assert!(response.body.contains("provider_response_missing_content"));
    assert!(response.trace.contains("status_code=429"));
}
