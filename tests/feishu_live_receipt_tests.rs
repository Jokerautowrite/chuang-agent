use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-feishu-live-receipt.sh")
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}

#[test]
fn feishu_live_receipt_is_blocked_when_bridge_event_log_is_missing() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing_log_path = unique_temp_path("chuang-feishu-missing-events");

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .env("CHUANG_FEISHU_EVENT_LOG_FILE", &missing_log_path)
        .env("CHUANG_FEISHU_RECEIPT_SKIP_PREFLIGHT", "1")
        .current_dir(&manifest_dir)
        .output()
        .expect("receipt script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be valid json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "feishu_live_readonly_receipt");
    assert_eq!(data["acceptance_status"], "blocked");
    assert_eq!(data["readonly"], true);
    assert_eq!(data["connects_real_feishu"], false);
    assert_eq!(data["observed_live_feishu_events"], false);
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(
        data["feishu_live_evidence"]["event_log_file_state"],
        "<missing>"
    );
    assert!(data["blockers"]
        .as_array()
        .expect("blockers should be array")
        .iter()
        .any(|item| item == "missing_bridge_event_log"));
    assert!(!stdout.contains("app_secret"));
    assert!(!stdout.contains("token="));
    assert!(!stdout.contains("Authorization"));
}

#[test]
fn feishu_live_receipt_is_verified_with_recent_inbound_and_outbound_evidence() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let event_log_path = unique_temp_path("chuang-feishu-events");
    let log_body = [
        r#"{"at":"2026-05-30T00:00:00Z","kind":"inbound","messageId":"m-1","chatId":"c-1"}"#,
        r#"{"at":"2026-05-30T00:00:01Z","kind":"outbound_format","messageId":"m-1","chatId":"c-1"}"#,
    ]
    .join("\n");
    fs::write(&event_log_path, format!("{log_body}\n")).expect("fake event log should be written");

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .env("CHUANG_FEISHU_EVENT_LOG_FILE", &event_log_path)
        .env("CHUANG_FEISHU_EVENT_LOOKBACK_SECONDS", "315360000")
        .env("CHUANG_FEISHU_APP_SECRET", "test-secret-must-not-leak")
        .env("CHUANG_FEISHU_RECEIPT_SKIP_PREFLIGHT", "1")
        .current_dir(&manifest_dir)
        .output()
        .expect("receipt script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be valid json");

    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(data["observed_live_feishu_events"], true);
    assert_eq!(data["connects_real_feishu"], false);
    assert_eq!(
        data["feishu_live_evidence"]["event_counts"]["recent_inbound"],
        1
    );
    assert_eq!(
        data["feishu_live_evidence"]["event_counts"]
            ["recent_outbound_command_or_outbound_format"],
        1
    );
    assert!(data["blockers"]
        .as_array()
        .expect("blockers should be array")
        .is_empty());
    assert!(!stdout.contains("test-secret-must-not-leak"));
}

#[test]
fn feishu_live_receipt_script_stays_readonly_and_safe() {
    let script = fs::read_to_string(script_path()).expect("receipt script should be readable");

    assert!(script.contains("Readonly Feishu live receipt evidence collector."));
    assert!(script.contains("connects_real_feishu=false"));
    assert!(script.contains("observed_live_feishu_events"));
    assert!(script.contains("missing_bridge_event_log"));
    assert!(script.contains("missing_recent_inbound_outbound_pair"));
    assert!(!script.contains("systemctl restart"));
    assert!(!script.contains("curl "));
    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
}
