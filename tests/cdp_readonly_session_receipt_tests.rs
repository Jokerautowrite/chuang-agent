use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-cdp-readonly-session-receipt.sh")
}

fn spawn_cdp_json_server() -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let port = listener
        .local_addr()
        .expect("local addr should exist")
        .port();

    let handle = thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("listener should be configurable");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut served_json = false;

        while Instant::now() < deadline && !served_json {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 4096];
                    stream
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .ok();
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    if read == 0 {
                        continue;
                    }

                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let request_line = request.lines().next().unwrap_or_default();

                    if request_line.starts_with("GET /json ") {
                        let body = r#"[
  {
    "id": "page-1",
    "type": "page",
    "url": "https://example.com/path?query=redacted",
    "title": "Example Page"
  }
]"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream
                            .write_all(response.as_bytes())
                            .expect("response should be writable");
                        served_json = true;
                    } else {
                        let body = "[]";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream
                            .write_all(response.as_bytes())
                            .expect("fallback response should be writable");
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    });

    (port, handle)
}

#[test]
fn cdp_readonly_session_receipt_script_static_safety_guards() {
    let script = fs::read_to_string(script_path()).expect("cdp readonly receipt script readable");
    assert!(script.contains("Controlled CDP readonly session receipt collector."));
    assert!(script.contains("Checks only CHUANG_CDP_PORT and CDP /json metadata."));
    assert!(script.contains("\"receipt_kind\": \"controlled_cdp_readonly_session_receipt\""));
    assert!(script.contains("\"performs_desktop_actions\": False"));
    assert!(script.contains("\"performs_browser_actions\": False"));
    assert!(script.contains("\"global_real_live_ready\": False"));
    assert!(!script.contains("playwright"));
    assert!(!script.contains("xdotool"));
    assert!(!script.contains("chromium"));
    assert!(!script.contains("ws://"));
    assert!(!script.contains("wss://"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("rm "));
}

#[test]
fn cdp_readonly_session_receipt_blocks_when_cdp_port_missing() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env_remove("CHUANG_CDP_PORT")
        .output()
        .expect("cdp readonly receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(
        data["receipt_kind"],
        "controlled_cdp_readonly_session_receipt"
    );
    assert_eq!(data["readonly"], true);
    assert_eq!(data["acceptance_status"], "blocked");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(data["performs_desktop_actions"], false);
    assert_eq!(data["performs_browser_actions"], false);

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be an array")
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(blockers.contains(&"missing_chuang_cdp_port"));

    let cdp = &data["cdp_metadata"];
    assert_eq!(cdp["metadata_state"], "missing_cdp_port");
    assert_eq!(cdp["target_count"], 0);
}

#[test]
fn cdp_readonly_session_receipt_verifies_local_json_metadata() {
    let (port, server) = spawn_cdp_json_server();

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_CDP_PORT", port.to_string())
        .output()
        .expect("cdp readonly receipt script should execute");

    server.join().expect("server thread should finish");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(
        data["receipt_kind"],
        "controlled_cdp_readonly_session_receipt"
    );
    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(data["performs_desktop_actions"], false);
    assert_eq!(data["performs_browser_actions"], false);

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be an array");
    assert!(blockers.is_empty());

    let cdp = &data["cdp_metadata"];
    assert_eq!(cdp["metadata_state"], "ok");
    assert_eq!(cdp["target_count"], 1);

    let target = &cdp["target_summaries"]
        .as_array()
        .expect("target summaries should be an array")[0];
    assert_eq!(target["target_type"], "page");
    assert_eq!(target["url_scheme"], "https");
    assert_eq!(target["url_host"], "example.com");

    let title_chars = target["title_chars"]
        .as_u64()
        .expect("title_chars should be numeric");
    assert!(title_chars > 0);

    let url_ref = target["url_ref"].as_str().unwrap_or_default();
    let title_ref = target["title_ref"].as_str().unwrap_or_default();
    assert!(url_ref.starts_with("sha256:"));
    assert!(title_ref.starts_with("sha256:"));
}
