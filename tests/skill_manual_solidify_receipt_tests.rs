#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/chuang-skill-manual-solidify-receipt.sh")
}

#[test]
fn skill_manual_solidify_receipt_script_static_safety_guards() {
    let script =
        fs::read_to_string(script_path()).expect("manual solidify receipt script readable");

    assert!(
        script.contains("Manual-only dry-run receipt for skill proposal -> manual solidify path.")
    );
    assert!(script.contains("writes_automatically=false"));
    assert!(script.contains("requires_human_approval=true"));
    assert!(script.contains("writes_long_term_skills=false"));
    assert!(script.contains("modifies_real_skill_directory=false"));
    assert!(script.contains("global_real_live_ready=false"));
    assert!(script.contains("manual_confirmation_checklist"));
    assert!(script.contains("\"evidence_refs\""));

    assert!(!script.contains("cargo run --quiet -- skill solidify"));
    assert!(!script.contains("cargo run -- skill solidify"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
}

#[test]
fn skill_manual_solidify_receipt_script_outputs_manual_dry_run_json() {
    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .env("CHUANG_SKILL_PROPOSAL_ID", "Browser Read Replay Guard")
        .env("CHUANG_SKILL_PROPOSAL_REF", "runtime_event:evt-20260530-01")
        .env("CHUANG_SKILL_JUDGE_RECEIPT_REF", "judge_receipt:judge-01")
        .env(
            "CHUANG_SKILL_APPROVE_RECEIPT_REF",
            "approve_receipt:approve-01",
        )
        .env(
            "CHUANG_SKILL_OPERATOR_DECISION_REF",
            "operator_note:manual-approved",
        )
        .output()
        .expect("manual solidify receipt script should execute");

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
        "skill_manual_solidify_dry_run_receipt"
    );
    assert_eq!(data["mode"], "manual_dry_run");
    assert_eq!(data["dry_run"], true);
    assert_eq!(data["manual_only"], true);
    assert_eq!(data["writes_automatically"], false);
    assert_eq!(data["requires_human_approval"], true);
    assert_eq!(data["writes_skill_files"], false);
    assert_eq!(data["writes_long_term_skills"], false);
    assert_eq!(data["modifies_real_skill_directory"], false);
    assert_eq!(data["acceptance_status"], "pending_human_approval");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);

    assert_eq!(data["proposal_id"], "Browser Read Replay Guard");
    assert_eq!(data["proposed_skill_id"], "browser_read_replay_guard");
    assert_eq!(
        data["proposed_path"],
        "data/skills/browser_read_replay_guard.md"
    );

    let checklist = data["manual_confirmation_checklist"]
        .as_array()
        .expect("manual confirmation checklist should be an array");
    assert_eq!(checklist.len(), 4);
    for item in checklist {
        assert_eq!(item["status"], "pending_human_confirmation");
    }

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be array");
    assert!(blockers
        .iter()
        .any(|item| item == "manual_confirmation_required"));
    assert!(blockers
        .iter()
        .any(|item| item == "manual_write_step_not_executed"));

    let refs = &data["evidence_refs"];
    assert_eq!(refs["proposal_ref"], "runtime_event:evt-20260530-01");
    assert_eq!(refs["judge_receipt_ref"], "judge_receipt:judge-01");
    assert_eq!(refs["approve_receipt_ref"], "approve_receipt:approve-01");
    assert_eq!(
        refs["operator_decision_ref"],
        "operator_note:manual-approved"
    );

    let boundary = &data["boundary"];
    assert_eq!(boundary["reads_existing_skills"], false);
    assert_eq!(boundary["solidifies_skill"], false);
    assert_eq!(boundary["upserts_canonical_skill"], false);
    assert_eq!(boundary["connects_llm"], false);
    assert_eq!(boundary["connects_external_service"], false);
}
