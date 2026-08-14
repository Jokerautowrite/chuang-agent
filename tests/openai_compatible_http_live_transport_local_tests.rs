use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use chuang_agent::provider_openai_compatible::{
    OpenAICompatibleProviderAdapter, ProviderTransport,
};
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};

#[test]
fn openai_compatible_http_transport_executes_real_post_against_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let bytes = stream
            .read(&mut buffer)
            .expect("request should be readable");
        let request_text = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        let request_lower = request_text.to_lowercase();

        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_lower.contains("authorization: bearer test-key"));
        assert!(request_lower.contains("content-type: application/json"));
        assert!(request_text.contains("继续推进真实http"));

        let body = r#"{"id":"chatcmpl-local-1","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"real_http_ok"},"finish_reason":"stop"}]}"#;
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
        user_input: "继续推进真实http".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(response.body, "real_http_ok");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("http")
    );
    assert_eq!(
        response.extra_meta.get("status_code").map(String::as_str),
        Some("200")
    );
    let expected_url = format!("http://{address}/v1/responses");
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some(expected_url.as_str())
    );
    assert!(response.trace.contains("status_code=200"));
}

#[test]
#[cfg(unix)]
fn openai_compatible_native_transport_executes_real_post_against_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let bytes = stream
            .read(&mut buffer)
            .expect("request should be readable");
        let request_text = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        let request_lower = request_text.to_lowercase();

        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_lower.contains("authorization: bearer test-key"));
        assert!(request_lower.contains("content-type: application/json"));
        assert!(request_text.contains("继续推进 native transport"));

        let body = r#"{"id":"chatcmpl-local-native-1","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"real_native_ok"},"finish_reason":"stop"}]}"#;
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
    .with_transport(ProviderTransport::Native);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进 native transport".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(response.body, "real_native_ok");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("native")
    );
    assert_eq!(
        response.extra_meta.get("status_code").map(String::as_str),
        Some("200")
    );
    let expected_url = format!("http://{address}/v1/responses");
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some(expected_url.as_str())
    );
    assert!(response.trace.contains("status_code=200"));
}
