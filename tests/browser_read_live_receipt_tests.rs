#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-browser-read-live-receipt.sh")
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
fn browser_read_live_receipt_script_static_safety_guards() {
    let script = fs::read_to_string(script_path()).expect("browser read receipt script readable");
    assert!(script.contains("Readonly browser_read live receipt evidence collector."));
    assert!(script.contains("desktop_read_is_separate=true"));
    assert!(script.contains("browser_read_does_not_use_desktop_read=true"));
    assert!(script.contains("performs_browser_actions=false"));
    assert!(script.contains("cargo run --quiet -- status --json"));
    assert!(!script.contains("systemctl restart"));
    assert!(!script.contains("curl "));
    assert!(!script.contains("xdotool"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("rm "));
}

#[test]
fn browser_read_live_receipt_script_blocks_when_status_skipped_and_cdp_port_missing() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_BROWSER_READ_RECEIPT_SKIP_STATUS", "1")
        .env_remove("CHUANG_CDP_PORT")
        .env(
            "CHUANG_HEADLESS_STATE_DIR",
            "/tmp/chuang-browser-receipt-no-state-dir",
        )
        .output()
        .expect("browser read receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "browser_read_live_readonly_receipt");
    assert_eq!(data["readonly"], true);
    assert_eq!(data["acceptance_status"], "blocked");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be an array")
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(blockers.contains(&"browser_read_adapter_unavailable"));
    assert!(blockers.contains(&"missing_chuang_cdp_port"));

    assert_eq!(data["desktop_read_is_separate"], true);
    assert_eq!(data["browser_read_does_not_use_desktop_read"], true);
    assert_eq!(data["performs_desktop_actions"], false);
    assert_eq!(data["performs_browser_actions"], false);

    let boundaries = &data["readonly_boundaries"];
    assert_eq!(boundaries["desktop_read_is_separate"], true);
    assert_eq!(boundaries["browser_read_does_not_use_desktop_read"], true);
    assert_eq!(boundaries["performs_desktop_actions"], false);
    assert_eq!(boundaries["performs_browser_actions"], false);

    assert_eq!(
        data["browser_read_evidence"]["status_collection"],
        "skipped"
    );
    assert_eq!(
        data["browser_read_evidence"]["cdp_metadata"]["metadata_state"],
        "missing_cdp_port"
    );
}

#[test]
fn browser_read_live_receipt_script_verifies_with_status_and_cdp_metadata() {
    let (port, server) = spawn_cdp_json_server();
    let status_json = r#"{
      "browser_readiness": {
        "browser_read_adapter_available": true,
        "browser_read_adapter_kind": "cdp",
        "browser_read_state": "cdp_connected",
        "browser_read_reason_code": "cdp_port_reachable",
        "browser_read_reason": "CDP adapter connected to localhost for receipt test",
        "browser_read_boundary": "browser_read_dom_url_title_contract",
        "browser_read_does_not_use_desktop_read": true,
        "browser_read_capabilities": ["url", "title", "dom_text"],
        "current": "browser_read live adapter is available via cdp",
        "next_action": "collect operator evidence before marking global live ready"
      }
    }"#;

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_CDP_PORT", port.to_string())
        .env("CHUANG_BROWSER_READ_RECEIPT_STATUS_JSON", status_json)
        .output()
        .expect("browser read receipt script should execute");

    server.join().expect("server thread should finish");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);

    let evidence = &data["browser_read_evidence"];
    assert_eq!(evidence["status_collection"], "ok");
    assert_eq!(evidence["adapter_available"], true);
    assert_eq!(evidence["adapter_kind"], "cdp");

    let cdp = &evidence["cdp_metadata"];
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

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be array");
    assert!(blockers.is_empty());
}
