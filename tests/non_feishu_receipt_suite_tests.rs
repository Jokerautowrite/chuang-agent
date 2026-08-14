#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-non-feishu-receipt-suite.sh")
}

#[test]
fn non_feishu_receipt_suite_static_no_feishu_live_script_reference() {
    let script = fs::read_to_string(script_path()).expect("suite script should be readable");
    assert!(script.contains("non_feishu_receipt_suite"));
    assert!(!script.contains("chuang-feishu-live-receipt.sh"));
    assert!(script.contains("CHUANG_NON_FEISHU_SUITE_INCLUDE_PROVIDER_LIVE=1"));
    assert!(script.contains("include_provider_live"));
}

#[test]
fn non_feishu_receipt_suite_json_output_is_parseable() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env_remove("CHUANG_NON_FEISHU_SUITE_INCLUDE_PROVIDER_LIVE")
        .output()
        .expect("suite script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("suite output should be json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "non_feishu_receipt_suite");
    assert_eq!(data["acceptance_status"], "blocked");
    assert_eq!(data["global_real_live_ready"], false);
    assert_eq!(data["provider_live_request_opt_in"], false);

    let children = data["children"]
        .as_array()
        .expect("children should be an array");
    assert!(!children.is_empty(), "children should not be empty");

    let child_names = children
        .iter()
        .filter_map(|child| child.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(
        !child_names.contains(&"provider_live_request_receipt"),
        "provider live request must be opt-in, not part of the default suite"
    );
    assert!(child_names.contains(&"gbrain_live_readonly_receipt"));
    assert!(child_names.contains(&"skill_proposal_verify_receipt"));

    for child in children {
        assert!(
            child.get("name").and_then(Value::as_str).is_some(),
            "child.name should be present"
        );
        assert!(
            child.get("exit_code").and_then(Value::as_i64).is_some(),
            "child.exit_code should be present"
        );
        assert!(
            child
                .get("stderr_present")
                .and_then(Value::as_bool)
                .is_some(),
            "child.stderr_present should be present"
        );
        let has_parse_ok = child
            .get("stdout_json_parse_ok")
            .and_then(Value::as_bool)
            .is_some();
        let has_summary = child.get("summary").and_then(Value::as_str).is_some();
        assert!(
            has_parse_ok || has_summary,
            "child must include stdout_json_parse_ok or summary"
        );
    }

    assert!(
        children
            .iter()
            .any(|child| child.get("acceptance_status") == Some(&Value::String("blocked".into()))),
        "default suite should surface blocked child receipts instead of claiming full verification"
    );
}
