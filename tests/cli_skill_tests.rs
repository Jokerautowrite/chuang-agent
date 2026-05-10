use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

fn test_skills_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-cli-skill-test-{}-{}-{}",
        name,
        std::process::id(),
        nonce
    ));
    fs::create_dir_all(&root).expect("test skills root should be creatable");
    root
}

fn seed_cli_canonical_skill(skills_root: &PathBuf, version: u32) -> PathBuf {
    let path = skills_root.join("dry_run_skill_candidate_for_xiaoce.md");
    fs::write(
        &path,
        format!(
            r#"---
skill_id: dry_run_skill_candidate_for_xiaoce
title: "Dry Run Skill Candidate For Xiaoce"
status: active
version: {version}
---

# Dry Run Skill Candidate For Xiaoce

Existing canonical body kept for lifecycle tests.
"#
        ),
    )
    .expect("seeded canonical skill should be writable");
    path
}

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
    assert_eq!(parsed["self_scored"], true);
    assert_eq!(parsed["approval_policy"], "darwin_style_cli_rubric");
    assert_eq!(parsed["approval_threshold"], 80);
    assert_eq!(parsed["writes_skills"], false);
    assert_eq!(parsed["solidifies_skill"], false);
    assert_eq!(parsed["judgment_count"], 1);
    assert_eq!(parsed["judgments"][0]["approved"], true);
    assert_eq!(
        parsed["judgments"][0]["canonical_skill_id"],
        "dry_run_skill_candidate_for_xiaoce"
    );
    assert!(
        parsed["judgments"][0]["score_total"]
            .as_u64()
            .expect("score_total should be numeric")
            >= 80
    );
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
    assert_eq!(parsed["boundary"]["self_scores_proposal"], true);
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
    assert!(stdout.contains("self_scored=true"));
    assert!(stdout.contains("approval_policy=darwin_style_cli_rubric"));
    assert!(stdout.contains("judgments=1"));
    assert!(stdout.contains("judgment proposal_id=dry-run-xiaoce-event-4"));
    assert!(stdout.contains("skill_id=dry_run_skill_candidate_for_xiaoce"));
    assert!(stdout.contains("writes_skills=false"));
    assert!(stdout.contains("solidifies_skill=false"));
    assert!(stdout.contains("approval_receipts=1"));
    assert!(stdout.contains("approval_receipt id=approved-solidify-dry-run-xiaoce-event-4"));
    assert!(stdout.contains("approved=true"));
    assert!(stdout.contains("source=cli skill approve"));
    assert!(stdout.contains("local_only=true"));
}

#[test]
fn cli_skill_judge_outputs_self_scored_policy_without_writing() {
    let skills_root = test_skills_root("judge-json");
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "judge",
            "--event-id",
            "event-judge-1",
            "--task-id",
            "task-judge-1",
            "--summary",
            "判断候选技能是否应该进入长期资产",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill judge should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("skill judge should output json");

    assert_eq!(parsed["judged"], true);
    assert_eq!(parsed["self_scored"], true);
    assert_eq!(parsed["writes_skills"], false);
    assert_eq!(parsed["solidifies_skill"], false);
    assert_eq!(parsed["approved_count"], 1);
    assert_eq!(
        parsed["judgments"][0]["canonical_skill_id"],
        "dry_run_skill_candidate_for_xiaoce"
    );
    assert_eq!(
        parsed["judgments"][0]["duplicate_state"],
        "new_canonical_skill"
    );
    assert_eq!(parsed["boundary"]["reads_existing_skills"], true);
    assert_eq!(parsed["boundary"]["writes_skill_files"], false);
    assert_eq!(parsed["boundary"]["connects_external_service"], false);
    assert!(!skills_root
        .join("dry_run_skill_candidate_for_xiaoce.md")
        .exists());
}

#[test]
fn cli_skill_judge_detects_preexisting_canonical_skill_file() {
    let skills_root = test_skills_root("judge-existing");
    let path = seed_cli_canonical_skill(&skills_root, 3);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "judge",
            "--event-id",
            "event-judge-existing",
            "--task-id",
            "task-judge-existing",
            "--summary",
            "判断已有 canonical skill 是否应更新",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill judge should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("skill judge should output json");

    assert_eq!(
        parsed["judgments"][0]["canonical_skill_id"],
        "dry_run_skill_candidate_for_xiaoce"
    );
    assert_eq!(
        parsed["judgments"][0]["duplicate_state"],
        "updates_existing"
    );
    assert_eq!(
        parsed["judgments"][0]["target_path"],
        path.display().to_string()
    );
    assert_eq!(parsed["writes_skills"], false);
    assert!(path.exists());
}

#[test]
fn cli_skill_solidify_outputs_write_receipt_and_writes_canonical_file() {
    let skills_root = test_skills_root("solidify-json");
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
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
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
    assert_eq!(parsed["solidify_allowed"], true);
    assert_eq!(parsed["self_scored"], true);
    assert_eq!(parsed["writes_skills"], true);
    assert_eq!(parsed["solidifies_skill"], true);
    assert_eq!(parsed["judgment_count"], 1);
    assert_eq!(parsed["write_count"], 1);
    assert_eq!(
        parsed["write_receipts"][0]["skill_id"],
        "dry_run_skill_candidate_for_xiaoce"
    );
    assert_eq!(parsed["write_receipts"][0]["action"], "created");
    assert_eq!(
        parsed["write_receipts"][0]["duplicate_state"],
        "created_new_canonical_skill"
    );
    assert_eq!(parsed["write_receipts"][0]["status"], "active");
    assert_eq!(parsed["write_receipts"][0]["version"], 1);
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
    assert_eq!(parsed["boundary"]["self_scores_proposal"], true);
    assert_eq!(parsed["boundary"]["reads_existing_skills"], true);
    assert_eq!(parsed["boundary"]["writes_skill_files"], true);
    assert_eq!(parsed["boundary"]["upserts_canonical_skill"], true);
    assert_eq!(parsed["boundary"]["solidifies_skill"], true);
    assert_eq!(parsed["boundary"]["connects_external_service"], false);

    let written_path = skills_root.join("dry_run_skill_candidate_for_xiaoce.md");
    let written = fs::read_to_string(written_path).expect("solidify should write skill file");
    assert!(written.contains("skill_id: dry_run_skill_candidate_for_xiaoce"));
    assert!(written.contains("status: active"));
    assert!(written.contains("approval_policy: darwin_style_cli_rubric"));
    assert!(written.contains("duplicate_policy: `upsert_canonical_skill_id`"));
}

#[test]
fn cli_skill_solidify_text_keeps_write_boundary_visible() {
    let skills_root = test_skills_root("solidify-text");
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
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
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
    assert!(stdout.contains("solidify_allowed=true"));
    assert!(stdout.contains("self_scored=true"));
    assert!(stdout.contains("writes_skills=true"));
    assert!(stdout.contains("solidifies_skill=true"));
    assert!(stdout.contains("writes=1"));
    assert!(stdout.contains("skill_write skill_id=dry_run_skill_candidate_for_xiaoce"));
    assert!(stdout.contains("action=created"));
    assert!(stdout.contains("duplicate_state=created_new_canonical_skill"));
    assert!(stdout.contains("solidify_receipts=1"));
    assert!(stdout.contains("solidify_receipt id=approved-solidify-dry-run-xiaoce-event-6"));
    assert!(stdout.contains("approved=true"));
    assert!(stdout.contains("source=cli skill solidify"));
    assert!(stdout.contains("local_only=true"));
}

#[test]
fn cli_skill_solidify_upserts_existing_canonical_skill_without_duplicate() {
    let skills_root = test_skills_root("solidify-upsert");
    let first = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-7a",
            "--task-id",
            "task-7a",
            "--summary",
            "第一次固化同一 canonical 技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("first skill solidify should execute");

    assert!(
        first.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-7b",
            "--task-id",
            "task-7b",
            "--summary",
            "第二次固化同一 canonical 技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("second skill solidify should execute");

    assert!(
        second.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    let parsed: Value =
        serde_json::from_slice(&second.stdout).expect("skill solidify should output json");

    assert_eq!(
        parsed["judgments"][0]["duplicate_state"],
        "updates_existing"
    );
    assert_eq!(
        parsed["write_receipts"][0]["duplicate_state"],
        "updated_existing_canonical_skill"
    );
    assert_eq!(parsed["write_receipts"][0]["action"], "updated");
    assert_eq!(parsed["write_receipts"][0]["version"], 2);

    let entries = fs::read_dir(&skills_root)
        .expect("skills root should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("skill dir entries should be readable");
    assert_eq!(entries.len(), 1);
}

#[test]
fn cli_skill_solidify_updates_seeded_canonical_file_version() {
    let skills_root = test_skills_root("solidify-seeded-existing");
    let path = seed_cli_canonical_skill(&skills_root, 4);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-seeded-existing",
            "--task-id",
            "task-seeded-existing",
            "--summary",
            "固化时更新已经存在的 canonical 技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
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

    assert_eq!(
        parsed["judgments"][0]["duplicate_state"],
        "updates_existing"
    );
    assert_eq!(parsed["write_receipts"][0]["action"], "updated");
    assert_eq!(
        parsed["write_receipts"][0]["duplicate_state"],
        "updated_existing_canonical_skill"
    );
    assert_eq!(parsed["write_receipts"][0]["version"], 5);
    assert_eq!(
        parsed["write_receipts"][0]["path"],
        path.display().to_string()
    );

    let content = fs::read_to_string(&path).expect("updated canonical skill should be readable");
    assert!(content.contains("version: 5"));
    assert!(content.contains("Previous Version Snapshot"));
}

#[test]
fn cli_skill_retire_marks_skill_in_place_without_deleting() {
    let skills_root = test_skills_root("retire");
    let solidify = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-retire-1",
            "--task-id",
            "task-retire-1",
            "--summary",
            "准备被淘汰的技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill solidify should execute");
    assert!(
        solidify.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&solidify.stderr)
    );

    let retire = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "retire",
            "--skill-id",
            "dry_run_skill_candidate_for_xiaoce",
            "--reason",
            "low usage after maintenance review",
            "--status",
            "retired",
            "--retired-at",
            "2026-05-09T20:00:00+08:00",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill retire should execute");
    assert!(
        retire.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&retire.stderr)
    );
    let parsed: Value = serde_json::from_slice(&retire.stdout).expect("retire json");
    assert_eq!(parsed["lifecycle_updated"], true);
    assert_eq!(parsed["writes_skill_files"], true);
    assert_eq!(parsed["deletes_skill_files"], false);
    assert_eq!(parsed["receipt"]["status"], "retired");
    assert_eq!(parsed["receipt"]["previous_status"], "active");
    assert_eq!(parsed["boundary"]["deletes_skill_files"], false);

    let path = skills_root.join("dry_run_skill_candidate_for_xiaoce.md");
    let content = fs::read_to_string(&path).expect("retired skill should still exist");
    assert!(path.exists());
    assert!(content.contains("status: retired"));
    assert!(content.contains("deletes_skill_file: false"));
    assert!(content.contains("Lifecycle notice"));
}

#[test]
fn cli_skill_retire_text_reports_deprecation_boundary() {
    let skills_root = test_skills_root("retire-text");
    let solidify = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-retire-2",
            "--task-id",
            "task-retire-2",
            "--summary",
            "准备被降级的技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill solidify should execute");
    assert!(
        solidify.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&solidify.stderr)
    );

    let retire = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "deprecate",
            "--skill-id",
            "dry_run_skill_candidate_for_xiaoce",
            "--reason",
            "replaced by broader canonical skill",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
        ])
        .output()
        .expect("skill deprecate should execute");
    assert!(
        retire.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&retire.stderr)
    );
    let stdout = String::from_utf8_lossy(&retire.stdout);
    assert!(stdout.contains("skill_retire lifecycle_updated=true"));
    assert!(stdout.contains("writes_skill_files=true"));
    assert!(stdout.contains("deletes_skill_files=false"));
    assert!(stdout.contains("skill_id=dry_run_skill_candidate_for_xiaoce"));
    assert!(stdout.contains("status=deprecated"));
    assert!(stdout.contains("previous_status=active"));
}

#[test]
fn cli_skill_monitor_reports_decay_and_rollback_candidates() {
    let skills_root = test_skills_root("monitor");
    let solidify = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-monitor-1",
            "--task-id",
            "task-monitor-1",
            "--summary",
            "准备进入监控和淘汰流程的技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill solidify should execute");
    assert!(
        solidify.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&solidify.stderr)
    );

    let retire = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "retire",
            "--skill-id",
            "dry_run_skill_candidate_for_xiaoce",
            "--reason",
            "stale after monitor review",
            "--status",
            "retired",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill retire should execute");
    assert!(
        retire.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&retire.stderr)
    );

    let monitor = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "monitor",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill monitor should execute");
    assert!(
        monitor.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&monitor.stderr)
    );
    let parsed: Value = serde_json::from_slice(&monitor.stdout).expect("monitor json");
    assert_eq!(parsed["monitored"], true);
    assert_eq!(parsed["skill_count"], 1);
    assert_eq!(parsed["active_count"], 0);
    assert_eq!(parsed["retired_count"], 1);
    assert_eq!(parsed["decay_candidate_count"], 1);
    assert_eq!(parsed["rollback_candidate_count"], 1);
    assert_eq!(parsed["skills"][0]["status"], "retired");
    assert_eq!(parsed["skills"][0]["decay_candidate"], true);
    assert_eq!(parsed["skills"][0]["rollback_available"], true);
    assert_eq!(parsed["skills"][0]["has_previous_version_snapshot"], true);
}

#[test]
fn cli_skill_rollback_restores_retired_skill_in_place() {
    let skills_root = test_skills_root("rollback");
    let solidify = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "solidify",
            "--event-id",
            "event-rollback-1",
            "--task-id",
            "task-rollback-1",
            "--summary",
            "准备回滚的技能",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill solidify should execute");
    assert!(
        solidify.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&solidify.stderr)
    );

    let retire = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "retire",
            "--skill-id",
            "dry_run_skill_candidate_for_xiaoce",
            "--reason",
            "rollback test retirement",
            "--status",
            "retired",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill retire should execute");
    assert!(
        retire.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&retire.stderr)
    );

    let rollback = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "rollback",
            "--skill-id",
            "dry_run_skill_candidate_for_xiaoce",
            "--reason",
            "restore after monitor review",
            "--rollback-at",
            "2026-05-09T22:00:00+08:00",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill rollback should execute");
    assert!(
        rollback.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&rollback.stderr)
    );
    let parsed: Value = serde_json::from_slice(&rollback.stdout).expect("rollback json");
    assert_eq!(parsed["lifecycle_updated"], true);
    assert_eq!(parsed["writes_skill_files"], true);
    assert_eq!(parsed["deletes_skill_files"], false);
    assert_eq!(parsed["receipt"]["status"], "active");
    assert_eq!(parsed["receipt"]["previous_status"], "retired");
    assert_eq!(parsed["receipt"]["restored_from_snapshot"], true);
    assert_eq!(parsed["boundary"]["restores_previous_version"], true);

    let path = skills_root.join("dry_run_skill_candidate_for_xiaoce.md");
    let content = fs::read_to_string(&path).expect("rolled back skill should remain readable");
    assert!(path.exists());
    assert!(content.contains("status: active"));
    assert!(content.contains("rollback_reason: restore after monitor review"));
    assert!(content.contains("rollback_from_version: 2"));
    assert!(content.contains("rollback_source_version: 1"));
    assert!(content.contains("<<<CHUANG-SNAPSHOT-BEGIN>>>"));
}

#[test]
fn cli_skill_deprecate_updates_seeded_canonical_file_without_deleting() {
    let skills_root = test_skills_root("deprecate-seeded");
    let path = seed_cli_canonical_skill(&skills_root, 6);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "skill",
            "deprecate",
            "--skill-id",
            "dry_run_skill_candidate_for_xiaoce",
            "--reason",
            "covered by a stronger maintained canonical skill",
            "--retired-at",
            "2026-05-09T21:00:00+08:00",
            "--skills-root",
            skills_root.to_str().expect("utf8 test path"),
            "--json",
        ])
        .output()
        .expect("skill deprecate should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("deprecate json");
    assert_eq!(parsed["receipt"]["status"], "deprecated");
    assert_eq!(parsed["receipt"]["previous_status"], "active");
    assert_eq!(parsed["receipt"]["version"], 7);
    assert_eq!(parsed["receipt"]["path"], path.display().to_string());
    assert_eq!(parsed["deletes_skill_files"], false);

    let content = fs::read_to_string(&path).expect("deprecated skill should remain readable");
    assert!(path.exists());
    assert!(content.contains("status: deprecated"));
    assert!(content.contains("version: 7"));
    assert!(content.contains("deletes_skill_file: false"));
    assert!(content.contains("Existing canonical body kept for lifecycle tests."));
}
