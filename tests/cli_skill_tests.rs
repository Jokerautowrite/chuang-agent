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
    assert_eq!(parsed["proposal_count"], 1);
    assert_eq!(parsed["proposals"][0]["dry_run"], true);
    assert_eq!(parsed["proposals"][0]["writes_skills"], false);
    assert_eq!(parsed["proposals"][0]["requires_approval"], true);
    assert_eq!(
        parsed["proposals"][0]["provenance"][0]["source_event_id"],
        "event-1"
    );
    assert_eq!(parsed["boundary"]["writes_skill_files"], false);
    assert_eq!(parsed["boundary"]["solidifies_skill"], false);
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
    assert!(stdout.contains("writes_skill_files=false"));
    assert!(stdout.contains("solidifies_skill=false"));
}
