use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-provider-live-receipt.sh")
}

#[test]
fn provider_live_receipt_script_outputs_readonly_template_without_readiness_surface() {
    let script = fs::read_to_string(script_path()).expect("provider live receipt script readable");
    assert!(script.contains("Readonly template for a provider live request receipt."));
    assert!(script.contains("provider_live_request_receipt_ref"));
    assert!(script.contains("sanitized_api_key_state"));
    assert!(!script.contains("cargo run --quiet -- status"));
    assert!(!script.contains("provider_readiness_check"));
    assert!(!script.contains("status --json"));

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .output()
        .expect("provider live receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("cargo run --quiet -- status"));
    assert!(!stdout.contains("provider_readiness_check"));

    let data: Value = serde_json::from_str(&stdout).expect("receipt output should be json");
    let mut keys = data
        .as_object()
        .expect("receipt should be a JSON object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "api_key_state".to_string(),
            "blocked_reason".to_string(),
            "connects_real_provider".to_string(),
            "next_action".to_string(),
            "prints_secret_values".to_string(),
            "provider_kind".to_string(),
            "provider_live_request_receipt_ref".to_string(),
            "readonly".to_string(),
            "request_id".to_string(),
            "runtime_report_id".to_string(),
            "schema_version".to_string(),
            "transport".to_string(),
        ]
    );
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["readonly"], true);
    assert_eq!(data["connects_real_provider"], false);
    assert_eq!(data["prints_secret_values"], false);
    assert_eq!(data["provider_kind"], "<fill_after_test>");
    assert_eq!(data["transport"], "<fill_after_test>");
    assert_eq!(data["api_key_state"], "<missing>");
    assert_eq!(data["request_id"], "<fill_after_test>");
    assert_eq!(data["runtime_report_id"], "<fill_after_test>");
    assert_eq!(
        data["provider_live_request_receipt_ref"],
        "<fill_after_test>"
    );
    assert_eq!(data["blocked_reason"], "<fill_after_test>");
    assert_eq!(data["next_action"], "<fill_after_test>");
}
