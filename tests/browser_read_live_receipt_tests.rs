use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-browser-read-live-receipt.sh")
}

#[test]
fn browser_read_live_receipt_script_is_readonly_template() {
    let script = fs::read_to_string(script_path()).expect("browser read receipt script readable");
    assert!(script.contains("Readonly browser_read live receipt template."));
    assert!(script.contains("desktop_read_is_separate=true"));
    assert!(script.contains("browser_read_does_not_use_desktop_read=true"));
    assert!(script.contains("performs_browser_actions=false"));
    assert!(script.contains("writes_core_memory=false"));
    assert!(!script.contains("cargo run --quiet -- status"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("systemctl"));
}

#[test]
fn browser_read_live_receipt_script_outputs_readonly_json_template() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .output()
        .expect("browser read receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
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
            "adapter_kind".to_string(),
            "adapter_manifest_ref".to_string(),
            "adapter_state".to_string(),
            "blocked_reason".to_string(),
            "browser_snapshot_or_transcript_ref".to_string(),
            "can_mark_real_live_ready".to_string(),
            "cannot_mark_complete_without_operator_evidence".to_string(),
            "next_action".to_string(),
            "readonly".to_string(),
            "readonly_boundaries".to_string(),
            "receipt_kind".to_string(),
            "report_admission_ref".to_string(),
            "request_id".to_string(),
            "runtime_report_id".to_string(),
            "schema_version".to_string(),
            "session_scope_ref".to_string(),
        ]
    );

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "browser_read_live_readonly_receipt");
    assert_eq!(data["readonly"], true);
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["cannot_mark_complete_without_operator_evidence"], true);
    assert_eq!(data["request_id"], "<fill_after_test>");
    assert_eq!(data["adapter_kind"], "<fill_after_test>");
    assert_eq!(data["adapter_state"], "<fill_after_test>");
    assert_eq!(data["adapter_manifest_ref"], "<fill_after_test>");
    assert_eq!(data["session_scope_ref"], "<fill_after_test>");
    assert_eq!(
        data["browser_snapshot_or_transcript_ref"],
        "<fill_after_test>"
    );
    assert_eq!(data["report_admission_ref"], "<fill_after_test>");
    assert_eq!(data["runtime_report_id"], "<fill_after_test>");
    assert_eq!(data["blocked_reason"], "<fill_after_test>");
    assert_eq!(data["next_action"], "<fill_after_test>");

    let boundaries = &data["readonly_boundaries"];
    assert_eq!(boundaries["readonly"], true);
    assert_eq!(boundaries["desktop_read_is_separate"], true);
    assert_eq!(boundaries["browser_read_does_not_use_desktop_read"], true);
    assert_eq!(boundaries["performs_desktop_actions"], false);
    assert_eq!(boundaries["performs_browser_actions"], false);
    assert_eq!(boundaries["connects_real_browser"], false);
    assert_eq!(boundaries["connects_real_provider"], false);
    assert_eq!(boundaries["connects_real_wiki"], false);
    assert_eq!(boundaries["connects_real_gbrain"], false);
    assert_eq!(boundaries["writes_core_memory"], false);
    assert_eq!(boundaries["prints_secret_values"], false);
    assert_eq!(boundaries["modifies_repo"], false);
    assert_eq!(boundaries["deletes_files"], false);
}
