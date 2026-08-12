use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use chuang_agent::provider_openai_compatible::{
    OpenAICompatibleProviderAdapter, ProviderTransport,
};
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};

#[test]
fn openai_compatible_curl_transport_executes_post_against_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let bytes = stream
            .read(&mut buffer)
            .expect("request should be readable");
        let request_text = String::from_utf8_lossy(&buffer[..bytes]).to_string();

        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_text.contains("Authorization: Bearer test-key"));
        assert!(request_text.contains("Content-Type: application/json"));
        assert!(request_text.contains("curl真实通道"));

        let body = r#"{"id":"chatcmpl-curl-1","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"real_curl_ok"},"finish_reason":"stop"}]}"#;
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
    .with_transport(ProviderTransport::Curl);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "curl真实通道".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(response.body, "real_curl_ok");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response
            .extra_meta
            .get("transport_mode")
            .map(String::as_str),
        Some("curl")
    );
    assert_eq!(
        response.extra_meta.get("status_code").map(String::as_str),
        Some("200")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_retryable")
            .map(String::as_str),
        Some("false")
    );
    assert!(response.trace.contains("transport_mode=curl"));
}

#[test]
fn openai_compatible_curl_transport_times_out_when_process_stalls() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    // provider 层 withRetry 会在超时后重试，curl 会再次 connect 到 mock；
    // 若 mock 只 accept 一次，第二次连接被拒会变成 curl_exit 而不是 curl_wait。
    // 这里非阻塞 accept 循环：每次收到连接就 hang 1000ms（保证每次都触发
    // curl_wait 超时），2s 无新连接即退出，避免 server.join() 永久等待。
    let server = thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let mut idle_polls = 0u32;
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    idle_polls = 0;
                    let mut buffer = [0u8; 4096];
                    let _ = stream.read(&mut buffer);
                    thread::sleep(Duration::from_millis(1000));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    idle_polls += 1;
                    if idle_polls > 20 {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }
    });

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("http://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Curl)
    .with_request_timeout_ms(20);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "curl timeout".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(response.finish_reason.as_deref(), Some("invalid-config"));
    assert_eq!(
        response
            .extra_meta
            .get("config_error_field")
            .map(String::as_str),
        Some("curl_wait")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_class")
            .map(String::as_str),
        Some("transport")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_retryable")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_ms")
            .map(String::as_str),
        Some("20")
    );
    assert!(response.body.contains("timed out after 20ms"));
}
