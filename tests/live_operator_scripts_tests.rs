use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn live_operator_receipt_script_is_readonly_and_template_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-receipt.sh");
    let script = fs::read_to_string(&script_path).expect("receipt script should be readable");

    assert!(script.contains("Readonly receipt template for a manual Chuang live test."));
    assert!(script.contains("CHUANG_LIVE_OPERATOR"));
    assert!(script.contains("CHUANG_LIVE_ENV_FILE"));
    assert!(script.contains("\"connects_real_feishu\": False"));
    assert!(script.contains("\"reads_secret_values\": False"));
    assert!(script.contains("\"starts_services\": False"));
    assert!(script.contains("\"stops_services\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"deletes_files\": False"));
    assert!(script.contains("\"reuses_codex_or_hermes_credentials\": False"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
}

#[test]
fn live_operator_receipt_script_outputs_redacted_json_template() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-receipt.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-live-operator-receipt-smoke-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let env_file = temp_dir.join("live.env");
    fs::write(&env_file, "CHUANG_LIVE_PLACEHOLDER=1\n").expect("env file should write");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .env("CHUANG_LIVE_OPERATOR", "operator-x")
        .env("CHUANG_LIVE_ENV_FILE", &env_file)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("receipt script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(data["schema_version"], 1);
    assert!(data["tested_at"].as_str().is_some());
    assert_eq!(data["operator"], "operator-x");
    assert_eq!(data["env_file"], env_file.display().to_string());
    assert_eq!(data["workspace_root"], manifest_dir.display().to_string());
    assert_eq!(data["preflight_status"], "<fill_after_test>");
    assert_eq!(data["health_status"], "<fill_after_test>");
    assert_eq!(data["new_thread_status"], "<fill_after_test>");
    assert_eq!(data["session_status"], "<fill_after_test>");
    assert_eq!(data["runtime_report_id"], "<fill_after_test>");
    assert_eq!(data["provider_status"], "<fill_after_test>");
    assert_eq!(data["codex_hermes_isolation"], "<keep_codex_and_hermes_separate>");
    assert_eq!(data["notes"], serde_json::json!([]));
    assert_eq!(data["blockers"], serde_json::json!([]));
    assert_eq!(data["boundaries"]["readonly"], true);
    assert_eq!(data["boundaries"]["connects_real_feishu"], false);
    assert_eq!(data["boundaries"]["reads_secret_values"], false);
    assert_eq!(data["boundaries"]["starts_services"], false);
    assert_eq!(data["boundaries"]["stops_services"], false);
    assert_eq!(data["boundaries"]["modifies_repo"], false);
    assert_eq!(data["boundaries"]["deletes_files"], false);
    assert_eq!(data["boundaries"]["reuses_codex_or_hermes_credentials"], false);
    assert!(!stdout.contains("CHUANG_LIVE_PLACEHOLDER=1"));
}
