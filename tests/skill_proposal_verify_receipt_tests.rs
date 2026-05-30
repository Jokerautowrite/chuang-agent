use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/chuang-skill-proposal-verify-receipt.sh")
}

fn make_temp_file(name: &str, content: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be available")
        .as_millis();
    let dir = std::env::temp_dir().join(format!(
        "chuang-skill-proposal-verify-{millis}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("temp dir should be creatable");
    let path = dir.join(name);
    fs::write(&path, content).expect("temp file should be writable");
    path
}

#[test]
fn skill_proposal_verify_receipt_script_static_safety_guards() {
    let script = fs::read_to_string(script_path()).expect("verify receipt script readable");

    assert!(
        script.contains("Read-only verify receipt for skill proposal JSON before manual solidify.")
    );
    assert!(script.contains("\"receipt_kind\": \"skill_proposal_verify_receipt\""));
    assert!(script.contains("\"read_only\": True"));
    assert!(script.contains("\"writes_automatically\": False"));
    assert!(script.contains("\"manual_approval_required\": True"));
    assert!(script.contains("\"global_real_live_ready\": False"));
    assert!(script.contains("missing_skill_proposal_file"));

    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("skill solidify"));
}

#[test]
fn skill_proposal_verify_receipt_defaults_to_blocked_without_file_env() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env_remove("CHUANG_SKILL_PROPOSAL_FILE")
        .output()
        .expect("verify receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value =
        serde_json::from_slice(&output.stdout).expect("script output should be valid json");

    assert_eq!(data["receipt_kind"], "skill_proposal_verify_receipt");
    assert_eq!(data["acceptance_status"], "blocked");
    assert_eq!(data["blocker"], "missing_skill_proposal_file");
    assert_eq!(data["read_only"], true);
    assert_eq!(data["writes_automatically"], false);
    assert_eq!(data["manual_approval_required"], true);
    assert_eq!(data["global_real_live_ready"], false);
}

#[test]
fn skill_proposal_verify_receipt_verifies_valid_proposal() {
    let proposal_path = make_temp_file(
        "proposal-valid.json",
        r#"{
  "id": "skill-001",
  "title": "Browser Read Guard",
  "description": "Adds guardrails for readonly browser evidence collection.",
  "evidence_refs": ["runtime_event:evt-01", "judge_receipt:judge-01"]
}"#,
    );

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_SKILL_PROPOSAL_FILE", &proposal_path)
        .output()
        .expect("verify receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value =
        serde_json::from_slice(&output.stdout).expect("script output should be valid json");

    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["blockers"], Value::Array(vec![]));
    assert_eq!(data["blocker"], Value::Null);
    assert_eq!(data["proposal_summary"]["id"], "skill-001");
    assert_eq!(data["proposal_summary"]["content_field"], "description");
    assert_eq!(data["proposal_summary"]["evidence_refs_count"], 2);
}

#[test]
fn skill_proposal_verify_receipt_blocks_invalid_json_or_missing_fields() {
    let invalid_json_path = make_temp_file("proposal-invalid.json", "{bad json");
    let invalid_output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_SKILL_PROPOSAL_FILE", &invalid_json_path)
        .output()
        .expect("verify receipt script should execute");
    assert!(
        invalid_output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&invalid_output.stderr)
    );
    let invalid_data: Value =
        serde_json::from_slice(&invalid_output.stdout).expect("script output should be valid json");
    assert_eq!(invalid_data["acceptance_status"], "blocked");
    assert_eq!(invalid_data["blocker"], "invalid_skill_proposal_json");

    let missing_fields_path = make_temp_file(
        "proposal-missing-fields.json",
        r#"{"id":"","name":"","evidence_refs":"bad"}"#,
    );
    let missing_output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_SKILL_PROPOSAL_FILE", &missing_fields_path)
        .output()
        .expect("verify receipt script should execute");
    assert!(
        missing_output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&missing_output.stderr)
    );
    let missing_data: Value =
        serde_json::from_slice(&missing_output.stdout).expect("script output should be valid json");
    assert_eq!(missing_data["acceptance_status"], "blocked");

    let blockers = missing_data["blockers"]
        .as_array()
        .expect("blockers should be an array");
    assert!(blockers.iter().any(|x| x == "missing_proposal_id"));
    assert!(blockers
        .iter()
        .any(|x| x == "missing_proposal_title_or_name"));
    assert!(blockers.iter().any(|x| x == "missing_proposal_content"));
    assert!(blockers.iter().any(|x| x == "invalid_evidence_refs_type"));
}
