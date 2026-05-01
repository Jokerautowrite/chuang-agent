use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

#[test]
fn cli_run_with_provider_and_stub_transport_flag_surfaces_transport_mode() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 transport",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "stub",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("transport_mode: stub"), "stdout={stdout}");
}

#[test]
fn cli_run_with_provider_and_curl_transport_executes_local_post() {
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
        assert!(request_text.contains("Authorization: Bearer test-key"));
        assert!(request_text.contains("cli curl transport"));

        let body = r#"{"id":"chatcmpl-cli-curl-1","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"real_cli_curl_ok"},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "cli curl transport",
            "--provider-base-url",
            &format!("http://{address}/v1"),
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "curl",
        ])
        .output()
        .expect("cargo run should execute");

    server.join().expect("server thread should finish");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("real_cli_curl_ok"), "stdout={stdout}");
    assert!(stdout.contains("transport_mode: curl"), "stdout={stdout}");
    assert!(stdout.contains("status_code: 200"), "stdout={stdout}");
}
