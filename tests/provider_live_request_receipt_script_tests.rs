#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/chuang-provider-live-request-receipt.sh")
}

fn write_local_provider_config(base_url: &str, env_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-provider-live-request-receipt-{nanos}"));
    fs::create_dir_all(&root).expect("config root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"openai_compatible\"\nprovider_id = \"cliproxy-local\"\nbase_url = \"{}\"\nmodel = \"gpt-5.5\"\napi_key_env = \"{}\"\ntransport = \"native\"\nprovider_timeout_ms = 5000\n",
            root.join("memory.db").display(),
            root.join("identity").display(),
            base_url,
            env_name,
        ),
    )
    .expect("provider config should be written");
    config_path
}

#[test]
fn provider_live_request_receipt_script_executes_real_request_and_emits_receipt_json() {
    let script = fs::read_to_string(script_path()).expect("provider live request script readable");
    assert!(script.contains("connects_real_provider=true"));
    assert!(script.contains("cargo run --quiet -- run"));
    assert!(script.contains("request_path_must_be=/v1/responses"));
    assert!(!script.contains("status --json only"));

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 8192];
        let bytes = stream
            .read(&mut buffer)
            .expect("request should be readable");
        let request_text = String::from_utf8_lossy(&buffer[..bytes]).to_string();
        let request_lower = request_text.to_lowercase();

        assert!(request_text.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(request_lower.contains("authorization: bearer test-live-key"));
        assert!(request_lower.contains("content-type: application/json"));

        let body = r#"{"id":"resp-local-1","object":"response","status":"completed","output":[{"id":"msg-local-1","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"local_provider_ok","annotations":[]}]}],"output_text":"local_provider_ok"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    let config_path =
        write_local_provider_config(&format!("http://{address}/v1"), "CHUANG_PROVIDER_TEST_KEY");

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .arg("--config")
        .arg(config_path)
        .arg("--input")
        .arg("provider live receipt test")
        .env("CHUANG_PROVIDER_TEST_KEY", "test-live-key")
        .output()
        .expect("provider live request script should execute");

    server.join().expect("server should complete");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("script output should be json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["ok"], true);
    assert_eq!(data["status"], "verified");
    assert_eq!(data["readonly"], false);
    assert_eq!(data["connects_real_provider"], true);
    assert_eq!(data["does_not_call_provider"], false);
    assert_eq!(data["prints_secret_values"], false);
    assert_eq!(data["provider_kind"], "openai_compatible");
    assert_eq!(data["provider_id"], "cliproxy-local");
    assert_eq!(data["model_name"], "gpt-5.5");
    assert_eq!(data["transport_mode"], "native");
    assert_eq!(data["api_key_state"], "<set>");
    assert_eq!(data["request_method"], "POST");
    assert_eq!(data["request_path"], "/v1/responses");
    assert_eq!(data["status_code"], 200);
    assert_eq!(data["provider_response_ok"], "true");
    assert_eq!(data["provider_fallback_used"], "false");
    assert_ne!(data["runtime_report_id"], "<missing>");
    assert!(data["response_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("chars="));
    assert!(data["response_summary"]
        .as_str()
        .unwrap_or_default()
        .contains("redacted=true"));
    assert!(!stdout.contains("test-live-key"));
    assert!(!stdout.contains("local_provider_ok"));
}
