//! verifier-first（验收先行）接入 goal 的测试：
//! 目标完成必须有文件系统证据，不能只靠模型自述。

use chuang_agent::goal_mode::GoalEvidence;
use chuang_agent::goal_run::{
    check_evidence_at, check_evidence_plan, GoalCheckpoint, GoalIntegrationPolicy, GoalRun,
    GoalRunStore, GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-goal-verifier-{name}-{nanos}"));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

#[test]
fn check_evidence_at_passes_when_file_exists_with_lines_and_content() {
    let root = temp_root("pass");
    fs::write(root.join("evidence.md"), "# summary\n\nbody line\n").expect("write evidence");
    let evidence = GoalEvidence::new("evidence.md")
        .with_min_lines(3)
        .with_min_content("# summary")
        .with_description("进度摘要");
    let verdict = check_evidence_at(&root, &evidence, 0);
    assert!(verdict.passed, "evidence should pass: {}", verdict.reason);
    assert_eq!(verdict.evidence_index, 0);
    assert_eq!(verdict.reason, "ok");
}

#[test]
fn check_evidence_at_fails_when_file_missing() {
    let root = temp_root("missing-file");
    let evidence = GoalEvidence::new("never-written.md").with_description("必须落盘的证据");
    let verdict = check_evidence_at(&root, &evidence, 0);
    assert!(!verdict.passed);
    assert!(verdict.reason.contains("file not found"));
    assert!(verdict.reason.contains("必须落盘的证据"));
}

#[test]
fn check_evidence_at_fails_when_too_few_lines() {
    let root = temp_root("short");
    fs::write(root.join("stub.md"), "one line\n").expect("write stub");
    let evidence = GoalEvidence::new("stub.md").with_min_lines(5);
    let verdict = check_evidence_at(&root, &evidence, 0);
    assert!(!verdict.passed);
    assert!(verdict.reason.contains("too few lines"));
}

#[test]
fn check_evidence_at_fails_when_required_content_missing() {
    let root = temp_root("no-content");
    fs::write(root.join("report.md"), "some text without the marker\n").expect("write report");
    let evidence = GoalEvidence::new("report.md").with_min_content("RESULT=PASS");
    let verdict = check_evidence_at(&root, &evidence, 0);
    assert!(!verdict.passed);
    assert!(verdict.reason.contains("missing required content"));
}

#[test]
fn check_evidence_plan_reports_each_item_by_index() {
    let root = temp_root("plan");
    fs::write(root.join("ok.txt"), "fine\n").expect("write ok");
    let spec =
        chuang_agent::goal_mode::GoalSpec::mainline_mvp("evidence plan").with_evidence(vec![
            GoalEvidence::new("ok.txt"),
            GoalEvidence::new("missing.txt"),
        ]);
    let verdicts = check_evidence_plan(&root, &spec);
    assert_eq!(verdicts.len(), 2);
    assert!(verdicts[0].passed);
    assert_eq!(verdicts[1].evidence_index, 1);
    assert!(!verdicts[1].passed);
}

fn sample_run_with_evidence(evidence: Vec<GoalEvidence>) -> GoalRun {
    let mut spec = chuang_agent::goal_mode::GoalSpec::mainline_mvp("verifier goal");
    spec.acceptance_evidence = evidence;
    GoalRun::new(
        spec,
        vec![GoalWorkerPlan::new(
            "w1",
            "produce evidence",
            vec!["scope-a".to_string()],
            vec!["cargo test".to_string()],
        )],
        vec![GoalWriteScope::new("scope-a", vec!["src".to_string()])],
        GoalValidationPlan::new(vec!["cargo test".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect("run should build")
}

#[test]
fn diagnostics_report_evidence_complete_when_all_pass() {
    let root = temp_root("diag-pass");
    fs::write(root.join("done.md"), "x\nx\nx\nx\nx\n").expect("write evidence");
    let mut run = sample_run_with_evidence(vec![GoalEvidence::new("done.md").with_min_lines(5)]);
    let verdicts = check_evidence_plan(&root, &run.goal_spec);
    run.record_checkpoint(GoalCheckpoint::with_evidence(
        "c1",
        "first checkpoint",
        vec!["w1".to_string()],
        vec!["cargo test".to_string()],
        verdicts,
    ))
    .expect("checkpoint should record");
    let diagnostics = run.diagnostics();
    assert!(diagnostics.evidence_expected);
    assert!(diagnostics.evidence_complete);
    assert!(diagnostics.evidence_missing.is_empty());
    assert_eq!(
        diagnostics.evidence_checked_at_checkpoint.as_deref(),
        Some("c1")
    );
    assert!(!diagnostics
        .incomplete_reasons
        .iter()
        .any(|reason| reason.contains("acceptance evidence")));
}

#[test]
fn diagnostics_report_evidence_missing_when_checkpoint_fails_evidence() {
    let root = temp_root("diag-missing");
    let mut run = sample_run_with_evidence(vec![GoalEvidence::new("never-written.md")]);
    let verdicts = check_evidence_plan(&root, &run.goal_spec);
    assert!(!verdicts[0].passed);
    run.record_checkpoint(GoalCheckpoint::with_evidence(
        "c1",
        "claimed done but no evidence",
        vec!["w1".to_string()],
        vec!["cargo test".to_string()],
        verdicts,
    ))
    .expect("checkpoint should record");
    let diagnostics = run.diagnostics();
    assert!(diagnostics.evidence_expected);
    assert!(!diagnostics.evidence_complete);
    assert_eq!(diagnostics.evidence_missing.len(), 1);
    assert!(diagnostics.evidence_missing[0].contains("never-written.md"));
    assert!(diagnostics
        .incomplete_reasons
        .iter()
        .any(|reason| reason.contains("acceptance evidence")));
}

#[test]
fn diagnostics_report_evidence_not_checked_when_checkpoint_has_no_verdicts() {
    let _root = temp_root("diag-unchecked");
    let mut run = sample_run_with_evidence(vec![GoalEvidence::new("done.md")]);
    run.record_checkpoint(GoalCheckpoint::new(
        "c1",
        "checkpoint without evidence run",
        vec!["w1".to_string()],
        vec!["cargo test".to_string()],
    ))
    .expect("checkpoint should record");
    let diagnostics = run.diagnostics();
    assert!(diagnostics.evidence_expected);
    assert!(!diagnostics.evidence_complete);
    assert!(diagnostics.evidence_missing[0].contains("not checked"));
}

#[test]
fn store_roundtrips_evidence_verdicts() {
    let root = temp_root("store");
    let store = GoalRunStore::new(&root);
    let mut run = sample_run_with_evidence(vec![GoalEvidence::new("done.md")]);
    run.record_checkpoint(GoalCheckpoint::with_evidence(
        "c1",
        "with evidence",
        vec!["w1".to_string()],
        vec!["cargo test".to_string()],
        vec![chuang_agent::goal_run::EvidenceVerdict {
            evidence_index: 0,
            path: "done.md".to_string(),
            passed: true,
            reason: "ok".to_string(),
        }],
    ))
    .expect("checkpoint should record");
    store.create(&run).expect("store should create");
    let loaded = store.load("mainline-mvp").expect("store should load");
    let last = loaded.checkpoint_log.last().expect("one checkpoint");
    assert_eq!(last.evidence_verdicts.len(), 1);
    assert!(last.evidence_verdicts[0].passed);
    assert_eq!(last.evidence_verdicts[0].path, "done.md");
}

#[test]
fn cli_checkpoint_auto_checks_evidence_files() {
    let root = temp_root("cli");
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| {
        std::process::Command::new(bin)
            .args(args)
            .output()
            .expect("chuang-agent binary should run")
    };
    let plan = run(&[
        "goal",
        "plan",
        "--goal-id",
        "verifier-cli",
        "--objective",
        "produce evidence file",
        "--root",
        root.to_str().unwrap(),
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
        "--evidence",
        "done.md|3||验收文件",
    ]);
    assert!(plan.status.success(), "goal plan should succeed");

    // 证据文件不存在 → checkpoint 记录，但 show 显示 evidence 缺失
    let cp = run(&[
        "goal",
        "checkpoint",
        "--goal-id",
        "verifier-cli",
        "--root",
        root.to_str().unwrap(),
        "--checkpoint-id",
        "c1",
        "--summary",
        "claim done",
        "--completed-worker-id",
        "w1",
        "--validation-note",
        "passed",
        "--json",
    ]);
    assert!(cp.status.success(), "checkpoint should succeed");
    let show = run(&[
        "goal",
        "show",
        "--goal-id",
        "verifier-cli",
        "--root",
        root.to_str().unwrap(),
        "--json",
    ]);
    assert!(show.status.success(), "goal show should succeed");
    let out: serde_json::Value = serde_json::from_slice(&show.stdout).expect("valid json");
    assert_eq!(out["goal_run_diagnostics"]["evidence_expected"], true);
    assert_eq!(out["goal_run_diagnostics"]["evidence_complete"], false);
    assert!(out["goal_run_diagnostics"]["evidence_missing"][0]
        .as_str()
        .unwrap()
        .contains("done.md"));

    // 补上证据文件 → 新 checkpoint 后证据通过
    fs::write(root.join("done.md"), "a\nb\nc\n").expect("write evidence");
    let cp2 = run(&[
        "goal",
        "checkpoint",
        "--goal-id",
        "verifier-cli",
        "--root",
        root.to_str().unwrap(),
        "--checkpoint-id",
        "c2",
        "--summary",
        "evidence ready",
        "--completed-worker-id",
        "w1",
        "--validation-note",
        "passed",
        "--json",
    ]);
    assert!(cp2.status.success(), "second checkpoint should succeed");
    let show2 = run(&[
        "goal",
        "show",
        "--goal-id",
        "verifier-cli",
        "--root",
        root.to_str().unwrap(),
        "--json",
    ]);
    let out2: serde_json::Value = serde_json::from_slice(&show2.stdout).expect("valid json");
    assert_eq!(out2["goal_run_diagnostics"]["evidence_complete"], true);
    assert!(out2["goal_run_diagnostics"]["evidence_missing"]
        .as_array()
        .unwrap()
        .is_empty());
}
