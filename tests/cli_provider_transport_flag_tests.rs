use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-agent-provider-transport-{name}-{nanos}"));
    fs::create_dir_all(&root).expect("config root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("fake config should be written");
    config_path
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut expected_len = None;
    loop {
        let bytes = stream
            .read(&mut buffer)
            .expect("request should be readable");
        if bytes == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..bytes]);
        if expected_len.is_none() {
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_len = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                expected_len = Some(header_end + 4 + content_len);
            }
        }
        if expected_len
            .map(|len| request.len() >= len)
            .unwrap_or(false)
        {
            break;
        }
    }
    request
}

#[test]
fn cli_run_with_provider_and_stub_transport_flag_surfaces_transport_mode() {
    let config_path = write_fake_config("stub");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
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
    let config_path = write_fake_config("curl");
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let request = read_http_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request).to_string();
        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_text.contains("Authorization: Bearer test-key"));
        assert!(request_text.contains("cli curl transport"));

        let body = r#"{"id":"chatcmpl-cli-curl-1","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"real_cli_curl_ok"},"finish_reason":"stop"}]}"#;
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
            "--config",
            config_path.to_str().expect("config path should be utf8"),
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

#[test]
#[cfg(unix)]
fn cli_run_with_provider_and_native_transport_executes_local_post() {
    let config_path = write_fake_config("native");
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut request = [0u8; 4096];
        let bytes = stream
            .read(&mut request)
            .expect("request should be readable");
        let request_text = String::from_utf8_lossy(&request[..bytes]).to_string();
        let request_lower = request_text.to_lowercase();
        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_lower.contains("authorization: bearer test-key"));

        let body = r#"{"id":"chatcmpl-cli-native-1","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":"real_cli_native_ok"},"finish_reason":"stop"}]}"#;
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
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--input",
            "cli native transport",
            "--provider-base-url",
            &format!("http://{address}/v1"),
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "native",
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
    assert!(stdout.contains("real_cli_native_ok"), "stdout={stdout}");
    assert!(stdout.contains("transport_mode: native"), "stdout={stdout}");
    assert!(stdout.contains("status_code: 200"), "stdout={stdout}");
}
