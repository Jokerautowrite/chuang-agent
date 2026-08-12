use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::{fs, io::Cursor, sync::Arc};

use chuang_agent::provider_openai_compatible::{
    OpenAICompatibleProviderAdapter, ProviderTransport,
};
use chuang_agent::responder::{ProviderAdapterResponder, ResponderRequest};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use rustls_pemfile::{certs, private_key};

fn generate_tls_materials() -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "chuang-agent-native-tls-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic enough")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("tls temp dir should create");

    let ca_key = root.join("ca.key");
    let ca_crt = root.join("ca.crt");
    let server_key = root.join("server.key");
    let server_csr = root.join("server.csr");
    let server_crt = root.join("server.crt");
    let server_ext = root.join("server.ext");

    fs::write(
        &server_ext,
        "[v3_req]\nsubjectAltName = IP:127.0.0.1,DNS:localhost\nbasicConstraints = CA:FALSE\nkeyUsage = critical, digitalSignature, keyEncipherment\nextendedKeyUsage = serverAuth\n",
    )
    .expect("server ext file should write");

    run_openssl(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "1",
        "-subj",
        "/CN=chuang-agent-test-ca",
        "-addext",
        "basicConstraints=critical,CA:TRUE",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
        "-keyout",
        ca_key.to_str().expect("ca key path should be utf-8"),
        "-out",
        ca_crt.to_str().expect("ca crt path should be utf-8"),
    ]);
    run_openssl(&[
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-subj",
        "/CN=127.0.0.1",
        "-keyout",
        server_key
            .to_str()
            .expect("server key path should be utf-8"),
        "-out",
        server_csr
            .to_str()
            .expect("server csr path should be utf-8"),
    ]);
    run_openssl(&[
        "x509",
        "-req",
        "-in",
        server_csr
            .to_str()
            .expect("server csr path should be utf-8"),
        "-CA",
        ca_crt.to_str().expect("ca crt path should be utf-8"),
        "-CAkey",
        ca_key.to_str().expect("ca key path should be utf-8"),
        "-CAcreateserial",
        "-out",
        server_crt
            .to_str()
            .expect("server crt path should be utf-8"),
        "-days",
        "1",
        "-extfile",
        server_ext
            .to_str()
            .expect("server ext path should be utf-8"),
        "-extensions",
        "v3_req",
    ]);

    (ca_crt, server_crt, server_key)
}

fn run_openssl(args: &[&str]) {
    let output = Command::new("openssl")
        .args(args)
        .output()
        .expect("openssl should execute");
    assert!(
        output.status.success(),
        "openssl failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

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
    let expected_url = format!("http://{address}/v1/responses");
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
fn openai_compatible_http_transport_times_out_when_server_stalls() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    // withRetry 会在超时后重试，mock 需接受所有连接并每次 hang 1000ms，
    // 保证每次都触发 http_timeout。非阻塞 accept，2s 无新连接即退出。
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
    .with_transport(ProviderTransport::Http)
    .with_request_timeout_ms(20);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "http timeout".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(response.finish_reason.as_deref(), Some("invalid-config"));
    assert_eq!(
        response
            .extra_meta
            .get("config_error_field")
            .map(String::as_str),
        Some("http_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("request_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_category")
            .map(String::as_str),
        Some("timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_reason_code")
            .map(String::as_str),
        Some("request_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_category")
            .map(String::as_str),
        Some("timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_ms")
            .map(String::as_str),
        Some("20")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_class")
            .map(String::as_str),
        Some("transport")
    );
    assert!(response.body.contains("field=http_timeout"));
}

#[test]
fn openai_compatible_native_transport_times_out_when_server_stalls() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    // withRetry 会在超时后重试，mock 需接受所有连接并每次 hang 1000ms，
    // 保证每次都触发 native_http_timeout。非阻塞 accept，2s 无新连接即退出。
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
    .with_transport(ProviderTransport::Native)
    .with_request_timeout_ms(20);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "native timeout".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(response.finish_reason.as_deref(), Some("invalid-config"));
    assert_eq!(
        response
            .extra_meta
            .get("config_error_field")
            .map(String::as_str),
        Some("native_http_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("request_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_category")
            .map(String::as_str),
        Some("timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_reason_code")
            .map(String::as_str),
        Some("request_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_category")
            .map(String::as_str),
        Some("timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_ms")
            .map(String::as_str),
        Some("20")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_class")
            .map(String::as_str),
        Some("transport")
    );
    assert!(response.body.contains("timed out after 20ms"));
}

#[test]
fn openai_compatible_curl_transport_times_out_when_server_stalls() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    // withRetry 会在超时后重试，mock 需接受所有连接并每次 hang 1000ms，
    // 保证每次都触发 curl_wait。非阻塞 accept，2s 无新连接即退出。
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
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("request_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_category")
            .map(String::as_str),
        Some("timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_reason_code")
            .map(String::as_str),
        Some("request_timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_category")
            .map(String::as_str),
        Some("timeout")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_timeout_ms")
            .map(String::as_str),
        Some("20")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_class")
            .map(String::as_str),
        Some("transport")
    );
    assert!(response.body.contains("timed out after 20ms"));
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
        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));

        let body = r#"{"id":"chatcmpl-local-2","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"http_live_ok"},"finish_reason":"stop"}]}"#;
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
    let expected_url = format!("http://{address}/v1/responses");
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
    assert_eq!(
        response
            .extra_meta
            .get("provider_response_ok")
            .map(String::as_str),
        Some("true")
    );
    assert!(
        !response
            .extra_meta
            .contains_key("provider_failure_reason_code"),
        "successful provider responses must not carry failure reason metadata"
    );
    assert!(
        !response
            .extra_meta
            .contains_key("provider_failure_category"),
        "successful provider responses must not carry failure category metadata"
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

    // 429 属于可重试状态码：adapter 会最多重试 3 次，server 需循环接受多次连接。
    let server = thread::spawn(move || {
        for _ in 0..3 {
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
        }
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
    assert_eq!(response.finish_reason.as_deref(), Some("http-error-429"));
    assert!(response.body.contains("PROVIDER_HTTP_ERROR"));
    assert!(response.body.contains("status_code=429"));
    assert!(response.trace.contains("provider_http_error"));
    assert!(!response.body.contains("provider_response_missing_content"));
    assert_eq!(
        response
            .extra_meta
            .get("provider_response_ok")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_class")
            .map(String::as_str),
        Some("http_status")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_message")
            .map(String::as_str),
        Some("rate limit hit")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("rate_limited")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_category")
            .map(String::as_str),
        Some("rate_limit")
    );
}

#[test]
fn openai_compatible_http_transport_surfaces_capacity_metadata_on_plain_text_429() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    // 429 属于可重试状态码：adapter 会最多重试 3 次，server 需循环接受多次连接。
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().expect("connection should be accepted");
            let mut buffer = [0u8; 4096];
            let _ = stream
                .read(&mut buffer)
                .expect("request should be readable");

            let body = "at capacity";
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be writable");
        }
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
        user_input: "继续推进 at capacity".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert_eq!(
        response
            .extra_meta
            .get("provider_response_ok")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_message")
            .map(String::as_str),
        Some("at capacity")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("model_capacity")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_category")
            .map(String::as_str),
        Some("capacity")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("http-error-429"));
}

#[test]
fn openai_compatible_http_transport_marks_200_missing_content_as_structured_provider_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let _ = stream
            .read(&mut buffer)
            .expect("request should be readable");

        let body = r#"{"id":"chatcmpl-empty","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":""},"finish_reason":"stop"}]}"#;
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
        user_input: "继续推进 missing content".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

    assert!(response.body.starts_with("PROVIDER_MISSING_CONTENT"));
    assert!(!response.body.contains("provider_response_missing_content"));
    assert_eq!(
        response.finish_reason.as_deref(),
        Some("provider-error-missing-content")
    );
    assert_eq!(
        response.extra_meta.get("status_code").map(String::as_str),
        Some("200")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_response_ok")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_error_class")
            .map(String::as_str),
        Some("missing_content")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_retryable")
            .map(String::as_str),
        // 28723fb 起：200+空 content 被标记为可重试（推理模型偶发断流，重试一次通常恢复）
        Some("true")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("missing_content")
    );
    assert_eq!(
        response
            .extra_meta
            .get("provider_failure_category")
            .map(String::as_str),
        Some("response")
    );
}

#[test]
fn openai_compatible_native_transport_accepts_https_scheme_and_attempts_tls() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let (ca_path, server_cert_path, server_key_path) = generate_tls_materials();
    let cert_chain = certs(&mut Cursor::new(
        fs::read(&server_cert_path).expect("server cert should read"),
    ))
    .collect::<Result<Vec<_>, _>>()
    .expect("certificate chain should parse");
    let private_key = private_key(&mut Cursor::new(
        fs::read(&server_key_path).expect("server key should read"),
    ))
    .expect("private key parser should read")
    .expect("private key should exist");
    let server_config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .expect("server config should build"),
    );

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("connection should be accepted");
        let conn = ServerConnection::new(server_config).expect("server connection should build");
        let mut tls_stream = StreamOwned::new(conn, stream);
        let mut request_bytes = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let bytes = tls_stream
                .read(&mut buffer)
                .expect("tls request bytes should be readable");
            if bytes == 0 {
                break;
            }
            request_bytes.extend_from_slice(&buffer[..bytes]);
            if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let request_text = String::from_utf8_lossy(&request_bytes).to_string();
        let request_lower = request_text.to_lowercase();

        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_lower.contains("authorization: bearer test-key"));
        assert!(request_lower.contains("content-type: application/json"));
        assert!(request_text.contains("https tls attempt"));

        let body = r#"{"id":"chatcmpl-local-tls-1","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"real_https_ok"},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        tls_stream
            .write_all(response.as_bytes())
            .expect("tls response should be writable");
        tls_stream.flush().expect("tls stream should flush");
    });

    let adapter = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        format!("https://{address}/v1"),
        "test-key",
        "gpt-4.1-mini",
    )
    .with_transport(ProviderTransport::Native)
    .with_tls_ca_cert_path(Some(ca_path))
    .with_request_timeout_ms(20);

    let response = adapter.respond(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "https tls attempt".to_string(),
        recall_hit_count: 2,
    });

    server.join().expect("server thread should finish");

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
    let expected_url = format!("https://{address}/v1/responses");
    assert_eq!(
        response.extra_meta.get("request_url").map(String::as_str),
        Some(expected_url.as_str())
    );
    assert_eq!(response.body, "real_https_ok");
    assert!(response.trace.contains("status_code=200"));
}
