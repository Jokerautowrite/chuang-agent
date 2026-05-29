use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-wiki-live-receipt.sh")
}

fn spawn_post_mock_server() -> (String, Arc<Mutex<Option<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let port = listener
        .local_addr()
        .expect("local addr should exist")
        .port();
    let endpoint = format!("http://127.0.0.1:{port}/wiki/read");
    let captured_body = Arc::new(Mutex::new(None));
    let captured_body_thread = Arc::clone(&captured_body);

    let handle = thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("listener should be configurable");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut served = false;

        while Instant::now() < deadline && !served {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 8192];
                    stream
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .ok();
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    if read == 0 {
                        continue;
                    }

                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    if let Some((_, body)) = request.split_once("\r\n\r\n") {
                        *captured_body_thread
                            .lock()
                            .expect("body lock should succeed") = Some(body.to_string());
                    }

                    let response_body = r#"{"ok":true}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("response should be writable");
                    served = true;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    });

    (endpoint, captured_body, handle)
}

#[test]
fn wiki_live_receipt_script_static_safety_guards() {
    let script = fs::read_to_string(script_path()).expect("wiki receipt script readable");
    assert!(script.contains("Readonly wiki live receipt collector."));
    assert!(script.contains("\"source\": \"wiki\""));
    assert!(script.contains("\"read_only\": True"));
    assert!(script.contains("writes_automatically"));
    assert!(!script.contains("curl "));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("rm "));
}

#[test]
fn wiki_live_receipt_script_blocks_without_real_endpoint_or_token() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env_remove("CHUANG_WIKI_LIVE_ENDPOINT")
        .env_remove("CHUANG_WIKI_LIVE_TOKEN")
        .output()
        .expect("wiki receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(data["receipt_kind"], "wiki_live_readonly_receipt");
    assert_eq!(data["source"], "wiki");
    assert_eq!(data["read_only"], true);
    assert_eq!(data["writes_automatically"], false);
    assert_eq!(data["token_state"], "<missing>");
    assert_eq!(data["acceptance_status"], "blocked");
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(data["request_sent"], false);

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be an array")
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(blockers.contains(&"missing_wiki_endpoint"));
    assert!(blockers.contains(&"missing_wiki_token"));
}

#[test]
fn wiki_live_receipt_script_posts_readonly_wiki_payload_and_verifies() {
    let (endpoint, captured_body, server) = spawn_post_mock_server();
    let token = "top-secret-token-for-test";
    let query = "runtime governance";
    let limit = "3";

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_WIKI_LIVE_ENDPOINT", endpoint)
        .env("CHUANG_WIKI_LIVE_TOKEN", token)
        .env("CHUANG_WIKI_QUERY", query)
        .env("CHUANG_WIKI_LIMIT", limit)
        .output()
        .expect("wiki receipt script should execute");

    server.join().expect("mock server should finish");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(token),
        "receipt output must not expose token value"
    );

    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");
    assert_eq!(data["source"], "wiki");
    assert_eq!(data["read_only"], true);
    assert_eq!(data["writes_automatically"], false);
    assert_eq!(data["token_state"], "<set>");
    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(data["request_sent"], true);
    assert_eq!(data["http_status"], 200);

    let body = captured_body
        .lock()
        .expect("body lock should succeed")
        .clone()
        .expect("mock server should capture request body");
    let payload: Value = serde_json::from_str(body.trim()).expect("body should be json");

    assert_eq!(payload["source"], "wiki");
    assert_eq!(payload["query"], query);
    assert_eq!(payload["limit"], 3);
    assert_eq!(payload["read_only"], true);
}
