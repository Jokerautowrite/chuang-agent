//! verifier-first（验收先行）接入 goal 的测试：
//! 目标完成必须有文件系统证据，不能只靠模型自述。

use chuang_agent::goal_mode::{
    AcceptanceCheck, AcceptanceCheckContract, GoalAcceptancePlan, GoalEvidence,
};
use chuang_agent::goal_run::{
    check_evidence_at, check_evidence_items, check_evidence_plan, evaluate_acceptance_check,
    evaluate_acceptance_plan, GoalCheckpoint, GoalIntegrationPolicy, GoalRun, GoalRunStore,
    GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
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

// ---------------------------------------------------------------------------
// verifier-first 类型化验收检查契约（AcceptanceCheck / GoalAcceptancePlan）
// ---------------------------------------------------------------------------

#[test]
fn acceptance_check_evidence_validate_rejects_empty_path() {
    let check = AcceptanceCheck::Evidence(GoalEvidence::new(""));
    let error = check.validate().expect_err("empty path must be rejected");
    assert_eq!(error.field, "acceptance_plan.checks[].path");
    assert!(error.message.contains("must not be empty"));
}

#[test]
fn acceptance_check_evidence_validate_rejects_zero_min_lines() {
    let check = AcceptanceCheck::Evidence(GoalEvidence::new("done.md").with_min_lines(0));
    let error = check.validate().expect_err("min_lines=0 must be rejected");
    assert_eq!(error.field, "acceptance_plan.checks[].min_lines");
    assert!(error.message.contains("greater than zero"));
}

#[test]
fn acceptance_check_command_validate_rejects_empty_command() {
    let check = AcceptanceCheck::Command("   ".to_string());
    let error = check
        .validate()
        .expect_err("empty command must be rejected");
    assert_eq!(error.field, "acceptance_plan.checks[].command");
}

#[test]
fn acceptance_check_validate_passes_for_valid_declarations() {
    let evidence = AcceptanceCheck::Evidence(
        GoalEvidence::new("done.md")
            .with_min_lines(3)
            .with_description("进度摘要"),
    );
    let command = AcceptanceCheck::Command("cargo test -q".to_string());
    assert!(evidence.validate().is_ok());
    assert!(command.validate().is_ok());
    assert_eq!(evidence.evaluator(), "evidence");
    assert_eq!(command.evaluator(), "command");
}

#[test]
fn acceptance_plan_validate_reports_first_invalid_check() {
    let plan = GoalAcceptancePlan::new(vec![
        AcceptanceCheck::Command("cargo test -q".to_string()),
        AcceptanceCheck::Evidence(GoalEvidence::new("")),
    ]);
    let error = plan.validate().expect_err("invalid check must surface");
    assert_eq!(error.field, "acceptance_plan.checks[].path");
    assert_eq!(plan.len(), 2);
    assert!(!plan.is_empty());
}

#[test]
fn acceptance_check_contract_is_implemented_for_acceptance_check() {
    // 接口先行：goal 生命周期只依赖 trait，不依赖具体实现。
    let check: &dyn AcceptanceCheckContract = &AcceptanceCheck::Command("true".to_string());
    assert!(check.validate_contract().is_ok());
    assert_eq!(check.evaluator(), "command");
    assert_eq!(check.description(), "true");
}

#[test]
fn evaluate_acceptance_check_evidence_passes_when_file_ready() {
    let root = temp_root("accept-evidence-pass");
    fs::write(root.join("done.md"), "# done\n\nbody\n").expect("write evidence");
    let check = AcceptanceCheck::Evidence(
        GoalEvidence::new("done.md")
            .with_min_lines(3)
            .with_min_content("# done"),
    );
    let verdict = evaluate_acceptance_check(&root, &check, 7);
    assert!(verdict.passed, "evidence should pass: {}", verdict.reason);
    assert_eq!(verdict.check_index, 7);
    assert_eq!(verdict.evaluator, "evidence");
    assert_eq!(verdict.exit_code, None);
}

#[test]
fn evaluate_acceptance_check_evidence_fails_when_file_missing() {
    let root = temp_root("accept-evidence-missing");
    let check = AcceptanceCheck::Evidence(GoalEvidence::new("never-written.md"));
    let verdict = evaluate_acceptance_check(&root, &check, 0);
    assert!(!verdict.passed);
    assert_eq!(verdict.check_index, 0);
    assert!(verdict.reason.contains("file not found"));
    assert!(verdict.reason.contains("never-written.md"));
}

#[test]
fn evaluate_acceptance_check_command_passes_on_zero_exit() {
    let root = temp_root("accept-command-pass");
    let check = AcceptanceCheck::Command("true".to_string());
    let verdict = evaluate_acceptance_check(&root, &check, 1);
    assert!(verdict.passed, "true must pass: {}", verdict.reason);
    assert_eq!(verdict.check_index, 1);
    assert_eq!(verdict.evaluator, "command");
    assert_eq!(verdict.exit_code, Some(0));
}

#[test]
fn evaluate_acceptance_check_command_fails_on_nonzero_exit() {
    let root = temp_root("accept-command-fail");
    let check = AcceptanceCheck::Command("false".to_string());
    let verdict = evaluate_acceptance_check(&root, &check, 2);
    assert!(!verdict.passed);
    assert_eq!(verdict.check_index, 2);
    assert_eq!(verdict.exit_code, Some(1));
    assert!(verdict.reason.contains("exited with code 1"));
}

#[test]
fn evaluate_acceptance_plan_reports_each_check_by_index() {
    let root = temp_root("accept-plan-mixed");
    fs::write(root.join("report.md"), "RESULT=PASS\n").expect("write report");
    let plan = GoalAcceptancePlan::new(vec![
        AcceptanceCheck::Evidence(GoalEvidence::new("report.md")),
        AcceptanceCheck::Command("true".to_string()),
        AcceptanceCheck::Command("false".to_string()),
    ]);
    let verdicts = evaluate_acceptance_plan(&root, &plan);
    assert_eq!(verdicts.len(), 3);
    assert_eq!(verdicts[0].check_index, 0);
    assert!(verdicts[0].passed);
    assert_eq!(verdicts[0].evaluator, "evidence");
    assert_eq!(verdicts[1].check_index, 1);
    assert!(verdicts[1].passed);
    assert_eq!(verdicts[1].evaluator, "command");
    assert_eq!(verdicts[2].check_index, 2);
    assert!(!verdicts[2].passed);
    assert_eq!(verdicts[2].exit_code, Some(1));
}

#[test]
fn check_evidence_items_reports_each_item_by_index() {
    let root = temp_root("evidence-items");
    fs::write(root.join("ok.txt"), "fine\n").expect("write ok");
    let verdicts = check_evidence_items(
        &root,
        &[
            GoalEvidence::new("ok.txt"),
            GoalEvidence::new("missing.txt"),
        ],
    );
    assert_eq!(verdicts.len(), 2);
    assert!(verdicts[0].passed);
    assert_eq!(verdicts[0].evidence_index, 0);
    assert_eq!(verdicts[1].evidence_index, 1);
    assert!(!verdicts[1].passed);
}

#[test]
fn goal_spec_with_acceptance_plan_validates_and_renders_context_block() {
    let plan = GoalAcceptancePlan::new(vec![
        AcceptanceCheck::Evidence(
            GoalEvidence::new("done.md")
                .with_min_lines(3)
                .with_description("进度摘要"),
        ),
        AcceptanceCheck::Command("cargo test -q".to_string()),
    ]);
    let spec = chuang_agent::goal_mode::GoalSpec::mainline_mvp("acceptance plan spec")
        .with_acceptance_plan(plan);
    spec.validate().expect("valid plan must validate");
    let block = spec.render_context_block().expect("render must succeed");
    assert!(block.contains("acceptance_plan:"));
    assert!(block.contains("- [evidence] 进度摘要"));
    assert!(block.contains("- [command] cargo test -q"));
}

#[test]
fn goal_spec_with_invalid_acceptance_plan_fails_validate() {
    let plan = GoalAcceptancePlan::new(vec![AcceptanceCheck::Evidence(GoalEvidence::new(""))]);
    let spec = chuang_agent::goal_mode::GoalSpec::mainline_mvp("invalid plan spec")
        .with_acceptance_plan(plan);
    let error = spec
        .validate()
        .expect_err("invalid plan must fail validate");
    assert_eq!(error.field, "acceptance_plan.checks[].path");
}

#[test]
fn goal_spec_serde_roundtrips_acceptance_plan() {
    let plan = GoalAcceptancePlan::new(vec![
        AcceptanceCheck::Evidence(
            GoalEvidence::new("done.md")
                .with_min_lines(3)
                .with_description("进度摘要"),
        ),
        AcceptanceCheck::Command("cargo test -q".to_string()),
    ]);
    let spec =
        chuang_agent::goal_mode::GoalSpec::mainline_mvp("serde plan").with_acceptance_plan(plan);
    let json = serde_json::to_string(&spec).expect("serialize");
    let decoded: chuang_agent::goal_mode::GoalSpec =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.acceptance_plan.len(), 2);
    assert_eq!(decoded.acceptance_plan.checks[1].evaluator(), "command");
    assert!(json.contains("\"acceptance_plan\""));
}

#[test]
fn goal_spec_legacy_json_without_acceptance_plan_deserializes() {
    // 向后兼容：旧 on-disk goal JSON 无 acceptance_plan 字段仍可反序列化，
    // 且默认空计划不破坏现有 goal 流程。
    let json = r#"{
        "goal_id":"legacy-plan",
        "objective":"legacy objective",
        "acceptance_checks":["cargo test -q"],
        "acceptance_evidence":[],
        "budget":{"max_minutes":60,"max_tool_rounds":null,"max_subtasks":2},
        "allowed_slots":["worker"],
        "checkpoint_policy":{"update_progress_log":true,"update_handoff":true,"commit_checkpoint":true},
        "final_report_policy":{"include_validation":true,"include_next_steps":true},
        "convergence_policy":{"max_repeated_blockers":2,"require_progress":true}
    }"#;
    let spec: chuang_agent::goal_mode::GoalSpec =
        serde_json::from_str(json).expect("legacy spec should deserialize");
    assert_eq!(spec.goal_id, "legacy-plan");
    assert!(spec.acceptance_plan.is_empty());
    assert!(spec.validate().is_ok());
}

#[test]
fn cli_goal_verify_judges_acceptance_plan_by_evidence_and_command() {
    let root = temp_root("verify-cli");
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
        "verify-cli",
        "--objective",
        "produce verifiable evidence",
        "--root",
        root.to_str().unwrap(),
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
        "--acceptance",
        "evidence:done.md|3|RESULT=PASS",
        "--acceptance",
        "command:true",
    ]);
    assert!(plan.status.success(), "goal plan should succeed");

    // 证据缺失 → verify 判定失败，command 检查仍可见 exit_code。
    let verify = run(&[
        "goal",
        "verify",
        "--goal-id",
        "verify-cli",
        "--root",
        root.to_str().unwrap(),
        "--json",
    ]);
    assert!(verify.status.success(), "goal verify should succeed");
    let out: serde_json::Value = serde_json::from_slice(&verify.stdout).expect("valid json");
    assert_eq!(out["goal_id"], "verify-cli");
    assert_eq!(out["acceptance_checks"], 2);
    assert_eq!(out["passed"], false);
    assert_eq!(out["missing"].as_array().unwrap().len(), 1);
    assert!(out["missing"][0].as_str().unwrap().contains("done.md"));
    assert_eq!(out["verdicts"][0]["evaluator"], "evidence");
    assert_eq!(out["verdicts"][0]["passed"], false);
    assert_eq!(out["verdicts"][1]["evaluator"], "command");
    assert_eq!(out["verdicts"][1]["passed"], true);
    assert_eq!(out["verdicts"][1]["exit_code"], 0);

    // 补上证据 → verify 全部通过。
    fs::write(root.join("done.md"), "RESULT=PASS\na\nb\n").expect("write evidence");
    let verify2 = run(&[
        "goal",
        "verify",
        "--goal-id",
        "verify-cli",
        "--root",
        root.to_str().unwrap(),
        "--json",
    ]);
    let out2: serde_json::Value = serde_json::from_slice(&verify2.stdout).expect("valid json");
    assert_eq!(out2["passed"], true);
    assert!(out2["missing"].as_array().unwrap().is_empty());
    assert_eq!(out2["verdicts"][0]["passed"], true);
}

#[test]
fn cli_goal_verify_command_check_failure_shows_exit_code() {
    let root = temp_root("verify-cli-fail");
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
        "verify-cli-fail",
        "--objective",
        "fail verification",
        "--root",
        root.to_str().unwrap(),
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
        "--acceptance",
        "command:false",
    ]);
    assert!(plan.status.success(), "goal plan should succeed");
    let verify = run(&[
        "goal",
        "verify",
        "--goal-id",
        "verify-cli-fail",
        "--root",
        root.to_str().unwrap(),
        "--json",
    ]);
    let out: serde_json::Value = serde_json::from_slice(&verify.stdout).expect("valid json");
    assert_eq!(out["passed"], false);
    assert_eq!(out["verdicts"][0]["exit_code"], 1);
    assert!(out["missing"][0]
        .as_str()
        .unwrap()
        .contains("exited with code 1"));
}
