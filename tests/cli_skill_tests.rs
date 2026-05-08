use std::process::Command;

use serde_json::Value;

#[test]
fn cli_skill_propose_outputs_dry_run_review_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "propose",
            "--event-id",
            "event-1",
            "--task-id",
            "task-1",
            "--kind",
            "manual_observation",
            "--summary",
            "用户反复要求把候选技能先人工审阅",
            "--metadata",
            "task_kind=skill-review",
            "--agent-id",
            "xiaoce",
            "--task-kind",
            "skill-review",
            "--json",
        ])
        .output()
        .expect("skill propose should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("skill propose should output json");

    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["writes_skills"], false);
    assert_eq!(parsed["requires_approval"], true);
    assert_eq!(parsed["approval_boundary_explicit"], true);
    assert_eq!(parsed["proposal_count"], 1);
    assert_eq!(parsed["validation_count"], 1);
    assert_eq!(parsed["approval_ticket_count"], 1);
    assert_eq!(parsed["validation_accepted_count"], 1);
    assert_eq!(parsed["proposals"][0]["dry_run"], true);
    assert_eq!(parsed["proposals"][0]["writes_skills"], false);
    assert_eq!(parsed["proposals"][0]["requires_approval"], true);
    assert_eq!(
        parsed["proposals"][0]["provenance"][0]["source_event_id"],
        "event-1"
    );
    assert_eq!(
        parsed["proposal_validations"][0]["proposal_id"],
        parsed["proposals"][0]["proposal_id"]
    );
    assert_eq!(parsed["proposal_validations"][0]["accepted"], true);
    assert!(parsed["proposal_validations"][0]["reasons"][0]
        .as_str()
        .expect("validation reason")
        .contains("structurally valid"));
    assert_eq!(
        parsed["approval_tickets"][0]["proposal_id"],
        parsed["proposals"][0]["proposal_id"]
    );
    assert_eq!(parsed["approval_tickets"][0]["dry_run"], true);
    assert_eq!(parsed["approval_tickets"][0]["writes_skills"], false);
    assert_eq!(parsed["approval_tickets"][0]["solidifies_skill"], false);
    assert_eq!(parsed["approval_tickets"][0]["local_only"], true);
    assert_eq!(
        parsed["approval_tickets"][0]["approval_receipt"]["proposal_id"],
        parsed["proposals"][0]["proposal_id"]
    );
    assert_eq!(
        parsed["approval_tickets"][0]["approval_receipt"]["validation_report"]["proposal_id"],
        parsed["proposals"][0]["proposal_id"]
    );
    assert_eq!(
        parsed["approval_tickets"][0]["approval_receipt"]["validation_report"]["accepted"],
        true
    );
    assert_eq!(
        parsed["approval_tickets"][0]["approval_receipt"]["approved"],
        false
    );
    assert_eq!(
        parsed["approval_tickets"][0]["approval_receipt"]["approval_source"],
        "pending_operator_approval"
    );
    assert_eq!(
        parsed["approval_tickets"][0]["approval_receipt"]["approved_at"],
        Value::Null
    );
    assert_eq!(parsed["boundary"]["writes_skill_files"], false);
    assert_eq!(parsed["boundary"]["solidifies_skill"], false);
    assert_eq!(parsed["boundary"]["emits_approval_ticket"], true);
    assert_eq!(parsed["boundary"]["connects_llm"], false);
    assert_eq!(parsed["boundary"]["connects_external_service"], false);
}

#[test]
fn cli_skill_propose_text_keeps_write_boundary_visible() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "propose",
            "--event-id",
            "event-2",
            "--task-id",
            "task-2",
            "--summary",
            "人工审阅候选技能",
        ])
        .output()
        .expect("skill propose should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skill_propose dry_run=true"));
    assert!(stdout.contains("writes_skills=false"));
    assert!(stdout.contains("requires_approval=true"));
    assert!(stdout.contains("approval_boundary_explicit=true"));
    assert!(stdout.contains("validations=1"));
    assert!(stdout.contains("accepted=1"));
    assert!(stdout.contains("approval_tickets=1"));
    assert!(stdout.contains("validation proposal_id=dry-run-xiaoce-event-2 accepted=true"));
    assert!(stdout.contains("writes_skill_files=false"));
    assert!(stdout.contains("solidifies_skill=false"));
    assert!(stdout.contains("emits_approval_ticket=true"));
    assert!(stdout.contains("approval_ticket id=pending-solidify-dry-run-xiaoce-event-2"));
    assert!(stdout.contains("approved=false"));
    assert!(stdout.contains("solidifies_skill=false"));
}

#[test]
fn cli_skill_approve_outputs_local_approval_receipt() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "approve",
            "--event-id",
            "event-3",
            "--task-id",
            "task-3",
            "--summary",
            "批准本地技能回执",
            "--approval-source",
            "cli-review",
            "--approved-at",
            "2026-05-08T10:00:00Z",
            "--approval-note",
            "本地审阅已通过",
            "--json",
        ])
        .output()
        .expect("skill approve should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("skill approve should output json");

    assert_eq!(parsed["approved"], true);
    assert_eq!(parsed["writes_skills"], false);
    assert_eq!(parsed["solidifies_skill"], false);
    assert_eq!(parsed["approval_receipt_count"], 1);
    assert_eq!(
        parsed["approval_receipts"][0]["approval_receipt"]["approved"],
        true
    );
    assert_eq!(
        parsed["approval_receipts"][0]["approval_receipt"]["approval_source"],
        "cli-review"
    );
    assert_eq!(
        parsed["approval_receipts"][0]["approval_receipt"]["approved_at"],
        "2026-05-08T10:00:00Z"
    );
    assert_eq!(
        parsed["approval_receipts"][0]["approval_receipt"]["approval_note"],
        "本地审阅已通过"
    );
    assert_eq!(parsed["boundary"]["validates_proposal"], true);
    assert_eq!(parsed["boundary"]["emits_approval_receipt"], true);
    assert_eq!(parsed["boundary"]["writes_skill_files"], false);
    assert_eq!(parsed["boundary"]["solidifies_skill"], false);
    assert_eq!(parsed["boundary"]["connects_llm"], false);
    assert_eq!(parsed["boundary"]["connects_external_service"], false);
}

#[test]
fn cli_skill_approve_text_keeps_approval_boundary_visible() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "approve",
            "--event-id",
            "event-4",
            "--task-id",
            "task-4",
            "--summary",
            "本地审批文本输出",
        ])
        .output()
        .expect("skill approve should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skill_approve approved=true"));
    assert!(stdout.contains("writes_skills=false"));
    assert!(stdout.contains("solidifies_skill=false"));
    assert!(stdout.contains("approval_receipts=1"));
    assert!(stdout.contains("approval_receipt id=approved-solidify-dry-run-xiaoce-event-4"));
    assert!(stdout.contains("approved=true"));
    assert!(stdout.contains("source=cli skill approve"));
    assert!(stdout.contains("local_only=true"));
}

#[test]
fn cli_skill_solidify_outputs_refusal_boundary_without_writing() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-5",
            "--task-id",
            "task-5",
            "--summary",
            "本地固化边界回执",
            "--approval-source",
            "cli-review",
            "--json",
        ])
        .output()
        .expect("skill solidify should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("skill solidify should output json");

    assert_eq!(parsed["solidify_requested"], true);
    assert_eq!(parsed["solidify_allowed"], false);
    assert_eq!(parsed["writes_skills"], false);
    assert_eq!(parsed["solidifies_skill"], false);
    assert_eq!(parsed["solidify_receipt_count"], 1);
    assert_eq!(
        parsed["solidify_receipts"][0]["approval_receipt"]["approved"],
        true
    );
    assert_eq!(
        parsed["solidify_receipts"][0]["approval_receipt"]["approval_source"],
        "cli-review"
    );
    assert_eq!(parsed["boundary"]["emits_solidify_receipt"], true);
    assert_eq!(parsed["boundary"]["writes_skill_files"], false);
    assert_eq!(parsed["boundary"]["solidifies_skill"], false);
    assert_eq!(parsed["boundary"]["connects_external_service"], false);
}

#[test]
fn cli_skill_solidify_text_keeps_refusal_boundary_visible() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-6",
            "--task-id",
            "task-6",
            "--summary",
            "本地固化文本输出",
        ])
        .output()
        .expect("skill solidify should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skill_solidify solidify_requested=true"));
    assert!(stdout.contains("solidify_allowed=false"));
    assert!(stdout.contains("solidify_receipts=1"));
    assert!(stdout.contains("solidify_receipt id=approved-solidify-dry-run-xiaoce-event-6"));
    assert!(stdout.contains("approved=true"));
    assert!(stdout.contains("source=cli skill solidify"));
    assert!(stdout.contains("local_only=true"));
}

#[test]
fn cli_skill_solidify_defaults_to_local_only_refusal_receipt() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-7",
            "--task-id",
            "task-7",
            "--summary",
            "默认本地固化回执",
            "--json",
        ])
        .output()
        .expect("skill solidify should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("skill solidify should output json");

    assert_eq!(parsed["solidify_requested"], true);
    assert_eq!(parsed["solidify_allowed"], false);
    assert_eq!(parsed["solidify_receipt_count"], 1);
    assert_eq!(
        parsed["solidify_receipts"][0]["approval_receipt"]["approval_source"],
        "cli skill solidify"
    );
    assert_eq!(
        parsed["solidify_receipts"][0]["approval_receipt"]["approved"],
        true
    );
    assert_eq!(parsed["boundary"]["writes_skill_files"], false);
    assert_eq!(parsed["boundary"]["connects_external_service"], false);
}
