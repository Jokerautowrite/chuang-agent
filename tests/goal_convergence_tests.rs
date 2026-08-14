use std::fs;
use std::path::{Path, PathBuf};

use chuang_agent::goal_mode::{GoalConvergencePolicy, GoalSpec};
use chuang_agent::goal_run::{
    enforce_convergence_gate, ConvergenceStatus, GoalCheckpoint, GoalIntegrationPolicy, GoalRun,
    GoalRunStore, GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
};

fn unique_tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chuang-goal-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn bin_run(bin: &str, args: &[&str]) -> std::process::Output {
    std::process::Command::new(bin)
        .args(args)
        .output()
        .expect("chuang-agent binary should run")
}

/// plan + 3 次相同 blocker（auth-401）→ goal evolve 判定 blocked。
fn seed_blocked_goal(run: &dyn Fn(&[&str]) -> std::process::Output, root: &str, goal_id: &str) {
    let plan = run(&[
        "goal",
        "plan",
        "--goal-id",
        goal_id,
        "--objective",
        "evolve approve smoke",
        "--root",
        root,
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
    ]);
    assert!(plan.status.success(), "goal plan should succeed");
    for i in 1..=3 {
        let cp = run(&[
            "goal",
            "checkpoint",
            "--goal-id",
            goal_id,
            "--root",
            root,
            "--checkpoint-id",
            &format!("c{i}"),
            "--summary",
            &format!("attempt {i}"),
            "--completed-worker-id",
            "w1",
            "--validation-note",
            "failed",
            "--blocker-key",
            "auth-401",
            "--json",
        ]);
        assert!(cp.status.success(), "checkpoint {i} should succeed");
    }
}

fn seed_goal_evolve_benchmark(benchmark_root: &Path, id: &str, best_total: u16) {
    let dir = benchmark_root.join(id);
    fs::create_dir_all(&dir).expect("benchmark dir should be creatable");
    let board = serde_json::json!({
        "benchmark_id": id,
        "version": 1,
        "best": {
            "run_id": "run-baseline",
            "benchmark_id": id,
            "version": 1,
            "tested_at": "2026-08-10T00:00:00Z",
            "case_scores": [],
            "total_score": best_total,
            "max_score": best_total
        },
        "latest": null,
        "history": []
    });
    fs::write(
        dir.join("scoreboard.json"),
        serde_json::to_vec_pretty(&board).expect("scoreboard json"),
    )
    .expect("scoreboard should be writable");
}

#[test]
fn goal_evolve_emits_outer_loop_proposal_when_blocked() {
    // goal evolve 的 CLI 行为：blocked → 外环提案（dry-run）；converging → 不触发。
    // 这里通过 CLI 二进制做端到端验证（bin 测试）。
    let tmp = std::env::temp_dir().join(format!(
        "chuang-goal-evolve-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| {
        std::process::Command::new(bin)
            .args(args)
            .output()
            .expect("chuang-agent binary should run")
    };

    // plan + 3 次相同 blocker
    let plan = run(&[
        "goal",
        "plan",
        "--goal-id",
        "evolve-smoke",
        "--objective",
        "evolve smoke",
        "--root",
        tmp.to_str().unwrap(),
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
    ]);
    assert!(plan.status.success(), "goal plan should succeed");
    for i in 1..=3 {
        let cp = run(&[
            "goal",
            "checkpoint",
            "--goal-id",
            "evolve-smoke",
            "--root",
            tmp.to_str().unwrap(),
            "--checkpoint-id",
            &format!("c{i}"),
            "--summary",
            &format!("attempt {i}"),
            "--completed-worker-id",
            "w1",
            "--validation-note",
            "failed",
            "--blocker-key",
            "auth-401",
            "--json",
        ]);
        assert!(cp.status.success(), "checkpoint {i} should succeed");
    }

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-smoke",
        "--root",
        tmp.to_str().unwrap(),
        "--json",
    ]);
    assert!(evolve.status.success(), "goal evolve should succeed");
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["convergence_status"], "blocked");
    assert_eq!(out["evolved"], true);
    assert_eq!(out["proposal_count"], 1);
    assert_eq!(
        out["proposals"][0]["provenance"][0]["source_kind"],
        "tool_failed"
    );

    // converging → 不触发外环
    let plan2 = run(&[
        "goal",
        "plan",
        "--goal-id",
        "evolve-ok",
        "--objective",
        "converging smoke",
        "--root",
        tmp.to_str().unwrap(),
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
    ]);
    assert!(plan2.status.success());
    let cp = run(&[
        "goal",
        "checkpoint",
        "--goal-id",
        "evolve-ok",
        "--root",
        tmp.to_str().unwrap(),
        "--checkpoint-id",
        "c1",
        "--summary",
        "progress",
        "--completed-worker-id",
        "w1",
        "--validation-note",
        "ok",
        "--json",
    ]);
    assert!(cp.status.success());
    let evolve_ok = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-ok",
        "--root",
        tmp.to_str().unwrap(),
        "--json",
    ]);
    assert!(evolve_ok.status.success());
    let out_ok: serde_json::Value = serde_json::from_slice(&evolve_ok.stdout).expect("valid json");
    assert_eq!(out_ok["convergence_status"], "converging");
    assert_eq!(out_ok["evolved"], false);
    assert_eq!(out_ok["proposal_count"], 0);
}

fn sample_goal_run() -> GoalRun {
    GoalRun::new(
        GoalSpec::mainline_mvp("convergence gate smoke"),
        vec![
            GoalWorkerPlan::new(
                "worker-1",
                "implement convergence gate",
                vec!["scope-src".to_string()],
                vec!["cargo test -q".to_string()],
            ),
            GoalWorkerPlan::new(
                "worker-2",
                "verify convergence gate",
                vec!["scope-tests".to_string()],
                vec!["cargo test -q".to_string()],
            ),
        ],
        vec![
            GoalWriteScope::new("scope-src", vec!["src".to_string()]),
            GoalWriteScope::new("scope-tests", vec!["tests".to_string()]),
        ],
        GoalValidationPlan::new(vec!["cargo test -q".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect("sample goal run should construct")
}

fn checkpoint(
    id: &str,
    worker: &str,
    notes: Vec<&str>,
    blocker_key: Option<&str>,
) -> GoalCheckpoint {
    let notes = notes.into_iter().map(String::from).collect::<Vec<_>>();
    match blocker_key {
        Some(key) => GoalCheckpoint::with_blocker_key(id, id, vec![worker.to_string()], notes, key),
        None => GoalCheckpoint::new(id, id, vec![worker.to_string()], notes),
    }
}

fn default_policy() -> GoalConvergencePolicy {
    GoalConvergencePolicy::default()
}

#[test]
fn empty_log_verdict_is_unknown() {
    let verdict = enforce_convergence_gate(&[], &default_policy());
    assert_eq!(verdict.status, ConvergenceStatus::Unknown);
    assert_eq!(verdict.repeated_count, 0);
    assert_eq!(verdict.repeated_fingerprint, None);
}

#[test]
fn single_checkpoint_without_blocker_is_converging() {
    let log = vec![checkpoint("c1", "worker-1", vec!["tests pass"], None)];
    let verdict = enforce_convergence_gate(&log, &default_policy());
    assert_eq!(verdict.status, ConvergenceStatus::Converging);
    assert_eq!(verdict.repeated_count, 1);
}

#[test]
fn different_blocker_keys_reset_repeat_count() {
    let log = vec![
        checkpoint("c1", "worker-1", vec!["failed"], Some("auth-401")),
        checkpoint("c2", "worker-1", vec!["failed"], Some("dns-timeout")),
        checkpoint("c3", "worker-1", vec!["ok"], None),
    ];
    let verdict = enforce_convergence_gate(&log, &default_policy());
    assert_eq!(verdict.status, ConvergenceStatus::Converging);
    assert_eq!(verdict.repeated_count, 1);
}

#[test]
fn two_same_blockers_is_spinning_not_blocked() {
    let log = vec![
        checkpoint("c1", "worker-1", vec!["failed"], Some("auth-401")),
        checkpoint("c2", "worker-1", vec!["failed"], Some("auth-401")),
    ];
    let verdict = enforce_convergence_gate(&log, &default_policy());
    assert_eq!(verdict.status, ConvergenceStatus::Spinning);
    assert_eq!(verdict.repeated_count, 2);
    assert_eq!(
        verdict.repeated_fingerprint.as_deref(),
        Some("blocker:auth-401")
    );
}

#[test]
fn three_same_blockers_is_blocked() {
    let log = vec![
        checkpoint("c1", "worker-1", vec!["failed"], Some("auth-401")),
        checkpoint("c2", "worker-1", vec!["failed"], Some("auth-401")),
        checkpoint("c3", "worker-1", vec!["failed"], Some("auth-401")),
    ];
    let verdict = enforce_convergence_gate(&log, &default_policy());
    assert_eq!(verdict.status, ConvergenceStatus::Blocked);
    assert_eq!(verdict.repeated_count, 3);
    assert!(verdict.reason.contains("stop retrying"));
}

#[test]
fn identical_validation_notes_count_as_no_progress() {
    let log = vec![
        checkpoint("c1", "worker-1", vec!["same failure line"], None),
        checkpoint("c2", "worker-1", vec!["same failure line"], None),
        checkpoint("c3", "worker-1", vec!["same failure line"], None),
    ];
    let verdict = enforce_convergence_gate(&log, &default_policy());
    assert_eq!(verdict.status, ConvergenceStatus::Blocked);
    assert_eq!(
        verdict.repeated_fingerprint.as_deref(),
        Some("notes:same failure line")
    );
}

#[test]
fn tail_repeat_ignores_older_progress() {
    let log = vec![
        checkpoint("c1", "worker-1", vec!["progress one"], Some("blocker-a")),
        checkpoint("c2", "worker-1", vec!["progress two"], Some("blocker-b")),
        checkpoint("c3", "worker-1", vec!["same"], Some("blocker-b")),
        checkpoint("c4", "worker-1", vec!["same"], Some("blocker-b")),
    ];
    let verdict = enforce_convergence_gate(&log, &default_policy());
    // 最早的 blocker-a 不影响尾部计数；尾部连续 3 个 blocker-b → blocked。
    assert_eq!(verdict.status, ConvergenceStatus::Blocked);
    assert_eq!(verdict.repeated_count, 3);
    assert_eq!(
        verdict.repeated_fingerprint.as_deref(),
        Some("blocker:blocker-b")
    );
}

#[test]
fn zero_max_repeated_disables_blocked_verdict() {
    let policy = GoalConvergencePolicy {
        max_repeated_blockers: 0,
        require_progress_between_checkpoints: true,
    };
    let log = vec![
        checkpoint("c1", "worker-1", vec!["failed"], Some("auth-401")),
        checkpoint("c2", "worker-1", vec!["failed"], Some("auth-401")),
        checkpoint("c3", "worker-1", vec!["failed"], Some("auth-401")),
    ];
    let verdict = enforce_convergence_gate(&log, &policy);
    assert_eq!(verdict.status, ConvergenceStatus::Spinning);
    assert_eq!(verdict.repeated_count, 3);
}

#[test]
fn spec_validate_rejects_max_repeated_blockers_of_one() {
    let mut spec = GoalSpec::mainline_mvp("invalid convergence policy");
    spec.convergence_policy.max_repeated_blockers = 1;
    let err = spec
        .validate()
        .expect_err("max_repeated_blockers=1 must be rejected");
    assert_eq!(err.field, "convergence_policy.max_repeated_blockers");
}

#[test]
fn run_verdict_reflects_recorded_blockers() {
    let mut run = sample_goal_run();
    for id in ["c1", "c2", "c3"] {
        run.record_checkpoint(checkpoint(id, "worker-1", vec!["failed"], Some("auth-401")))
            .expect("checkpoint should record");
    }
    let verdict = run.convergence_verdict();
    assert_eq!(verdict.status, ConvergenceStatus::Blocked);
    let diagnostics = run.diagnostics();
    assert_eq!(diagnostics.convergence_status, "blocked");
    assert_eq!(diagnostics.convergence_repeated_count, 3);
    assert!(diagnostics
        .incomplete_reasons
        .iter()
        .any(|reason| reason.contains("stop retrying")));
}

#[test]
fn store_persists_blocker_key_roundtrip() {
    let tmp = std::env::temp_dir().join(format!(
        "chuang-goal-convergence-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = GoalRunStore::new(&tmp);
    let run = sample_goal_run();
    let goal_id = run.goal_spec.goal_id.clone();
    store.create(&run).expect("goal run should be created");
    store
        .record_checkpoint(
            &goal_id,
            checkpoint("c1", "worker-1", vec!["failed"], Some("auth-401")),
        )
        .expect("checkpoint should persist");
    let reloaded = store.load(&goal_id).expect("goal run should reload");
    assert_eq!(
        reloaded.checkpoint_log[0].blocker_key.as_deref(),
        Some("auth-401")
    );
    let verdict = reloaded.convergence_verdict();
    assert_eq!(verdict.status, ConvergenceStatus::Converging);
}

// ---- 缺口 B：goal evolve 审批固化 + benchmark 验证 auto-revert ----

#[test]
fn goal_evolve_without_approve_stays_dry_run() {
    // 治理边界：没有 --approve 时只产出 dry-run 提案，绝不落盘。
    let tmp = unique_tmp("evolve-dry");
    let skills_root = tmp.join("skills");
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-dry");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-dry",
        "--root",
        tmp.to_str().unwrap(),
        "--json",
    ]);
    assert!(evolve.status.success(), "dry-run evolve should succeed");
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["evolved"], true);
    assert_eq!(out["proposal_count"], 1);
    assert!(
        out.get("approval").is_none(),
        "dry-run output must not carry an approval section"
    );
    assert!(
        !skills_root
            .join("dry_run_skill_candidate_for_chuang_goal.md")
            .exists(),
        "no skill file may be written without --approve"
    );
}

#[test]
fn goal_evolve_approve_solidifies_skill_when_explicitly_approved() {
    // --approve（显式审批参数）→ 自评通过 → 落盘 skill 文件。
    let tmp = unique_tmp("evolve-approve");
    let skills_root = tmp.join("skills");
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-approve");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-approve",
        "--root",
        tmp.to_str().unwrap(),
        "--approve",
        "--approval-source",
        "tiannan-cli",
        "--approval-note",
        "outer-loop approval",
        "--skills-root",
        skills_root.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        evolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&evolve.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["convergence_status"], "blocked");
    assert_eq!(out["evolved"], true);
    assert_eq!(out["approval"]["requested"], true);
    assert_eq!(out["approval"]["approved"], true);
    assert_eq!(out["approval"]["writes_skills"], true);
    assert_eq!(out["approval"]["solidifies_skill"], true);
    assert_eq!(out["approval"]["approval_source"], "tiannan-cli");
    assert_eq!(out["approval"]["judgment_count"], 1);
    assert_eq!(out["approval"]["write_count"], 1);
    assert_eq!(out["approval"]["write_receipts"][0]["action"], "created");
    assert_eq!(out["approval"]["write_receipts"][0]["version"], 1);
    assert!(
        out.get("benchmark_verification").is_none(),
        "no benchmark gate requested -> no verification section"
    );

    let path = skills_root.join("dry_run_skill_candidate_for_chuang_goal.md");
    let content = fs::read_to_string(&path).expect("approved skill should be written");
    assert!(content.contains("skill_id: dry_run_skill_candidate_for_chuang_goal"));
    assert!(content.contains("approval_source: tiannan-cli"));
    assert!(content.contains("version: 1"));
    assert!(content.contains("provenance_event_ids: goal-evolve-evolve-approve-"));
}

#[test]
fn goal_evolve_approve_rejects_when_goal_is_converging() {
    // 没有可固化的 blocker 提案时，--approve 必须拒绝，不落盘。
    let tmp = unique_tmp("evolve-approve-reject");
    let skills_root = tmp.join("skills");
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    let plan = run(&[
        "goal",
        "plan",
        "--goal-id",
        "evolve-ok",
        "--objective",
        "converging approve smoke",
        "--root",
        tmp.to_str().unwrap(),
        "--worker",
        "w1|scope-src|do work",
        "--scope",
        "scope-src=src",
        "--validation",
        "cargo test",
    ]);
    assert!(plan.status.success());
    let cp = run(&[
        "goal",
        "checkpoint",
        "--goal-id",
        "evolve-ok",
        "--root",
        tmp.to_str().unwrap(),
        "--checkpoint-id",
        "c1",
        "--summary",
        "progress",
        "--completed-worker-id",
        "w1",
        "--validation-note",
        "ok",
        "--json",
    ]);
    assert!(cp.status.success());

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-ok",
        "--root",
        tmp.to_str().unwrap(),
        "--approve",
        "--approval-source",
        "tiannan-cli",
        "--skills-root",
        skills_root.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        !evolve.status.success(),
        "approve on converging goal must fail"
    );
    let stderr = String::from_utf8_lossy(&evolve.stderr);
    assert!(
        stderr.contains("no convergence blocker proposal"),
        "stderr={stderr}"
    );
    assert!(
        !skills_root
            .join("dry_run_skill_candidate_for_chuang_goal.md")
            .exists(),
        "no skill file may be written without a blocker proposal"
    );
}

#[test]
fn goal_evolve_benchmark_flags_require_approve() {
    // 治理边界：benchmark 验证是固化后的步骤，未 --approve 时禁止。
    let tmp = unique_tmp("evolve-gate-require-approve");
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-gate-req");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-gate-req",
        "--root",
        tmp.to_str().unwrap(),
        "--benchmark-gate",
        "memory-recall",
        "--benchmark-after-score",
        "5",
        "--json",
    ]);
    assert!(!evolve.status.success());
    let stderr = String::from_utf8_lossy(&evolve.stderr);
    assert!(stderr.contains("require --approve"), "stderr={stderr}");
}

#[test]
fn goal_evolve_approve_benchmark_poor_result_auto_reverts_created_skill() {
    // Penguin 语义：无基线不优化 + 严格提升才接受。
    // 固化后验证 after_score(3) 不严格优于 best(4) → 自动回滚刚创建的规则文件。
    let tmp = unique_tmp("evolve-revert-created");
    let skills_root = tmp.join("skills");
    let benchmark_root = tmp.join("bench");
    seed_goal_evolve_benchmark(&benchmark_root, "memory-recall", 4);
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-revert-created");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-revert-created",
        "--root",
        tmp.to_str().unwrap(),
        "--approve",
        "--approval-source",
        "tiannan-cli",
        "--skills-root",
        skills_root.to_str().unwrap(),
        "--benchmark-gate",
        "memory-recall",
        "--benchmark-after-score",
        "3",
        "--benchmark-root",
        benchmark_root.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        evolve.status.success(),
        "auto-revert is a successful, explicitly marked outcome; stderr={}",
        String::from_utf8_lossy(&evolve.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["approval"]["approved"], true);
    assert_eq!(out["approval"]["write_count"], 1);
    assert_eq!(out["benchmark_verification"]["requested"], true);
    assert_eq!(
        out["benchmark_verification"]["benchmark_gate"],
        "memory-recall"
    );
    assert_eq!(out["benchmark_verification"]["best_score"], 4);
    assert_eq!(out["benchmark_verification"]["after_score"], 3);
    assert_eq!(out["benchmark_verification"]["passed"], false);
    assert_eq!(out["benchmark_verification"]["reverted"], true);
    assert!(
        out["benchmark_verification"]["revert_reason"]
            .as_str()
            .unwrap_or_default()
            .contains("does not strictly exceed"),
        "revert_reason should carry the gate failure"
    );
    assert_eq!(
        out["benchmark_verification"]["revert_receipts"][0]["action"],
        "removed_created"
    );
    assert!(
        !skills_root
            .join("dry_run_skill_candidate_for_chuang_goal.md")
            .exists(),
        "reverted created skill must not remain on disk"
    );
}

#[test]
fn goal_evolve_approve_benchmark_improvement_keeps_skill() {
    // 验证 after_score(5) 严格优于 best(4) → 保留固化，不回滚。
    let tmp = unique_tmp("evolve-keep");
    let skills_root = tmp.join("skills");
    let benchmark_root = tmp.join("bench");
    seed_goal_evolve_benchmark(&benchmark_root, "memory-recall", 4);
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-keep");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-keep",
        "--root",
        tmp.to_str().unwrap(),
        "--approve",
        "--approval-source",
        "tiannan-cli",
        "--skills-root",
        skills_root.to_str().unwrap(),
        "--benchmark-gate",
        "memory-recall",
        "--benchmark-after-score",
        "5",
        "--benchmark-root",
        benchmark_root.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        evolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&evolve.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["benchmark_verification"]["passed"], true);
    assert_eq!(out["benchmark_verification"]["reverted"], false);
    assert_eq!(out["benchmark_verification"]["best_score"], 4);
    assert_eq!(out["benchmark_verification"]["after_score"], 5);
    assert_eq!(
        out["benchmark_verification"]["revert_receipts"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert!(
        skills_root
            .join("dry_run_skill_candidate_for_chuang_goal.md")
            .exists(),
        "improved skill must stay on disk"
    );
}

#[test]
fn goal_evolve_approve_benchmark_revert_restores_existing_skill() {
    // 已存在 canonical skill 时，验证失败应精确还原原内容（不残留新版本）。
    let tmp = unique_tmp("evolve-revert-existing");
    let skills_root = tmp.join("skills");
    let benchmark_root = tmp.join("bench");
    fs::create_dir_all(&skills_root).expect("skills root should be creatable");
    let path = skills_root.join("dry_run_skill_candidate_for_chuang_goal.md");
    fs::write(&path, "ORIGINAL_CANONICAL v1").expect("seed skill should be writable");
    seed_goal_evolve_benchmark(&benchmark_root, "memory-recall", 4);
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-revert-existing");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-revert-existing",
        "--root",
        tmp.to_str().unwrap(),
        "--approve",
        "--approval-source",
        "tiannan-cli",
        "--skills-root",
        skills_root.to_str().unwrap(),
        "--benchmark-gate",
        "memory-recall",
        "--benchmark-after-score",
        "3",
        "--benchmark-root",
        benchmark_root.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        evolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&evolve.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["benchmark_verification"]["reverted"], true);
    assert_eq!(
        out["benchmark_verification"]["revert_receipts"][0]["action"],
        "restored_previous"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("restored skill should be readable"),
        "ORIGINAL_CANONICAL v1"
    );
}

#[test]
fn goal_evolve_approve_benchmark_rejects_when_no_baseline() {
    // 无基线不优化：scoreboard 没有 best 时，固化后验证失败并回滚。
    let tmp = unique_tmp("evolve-revert-nobaseline");
    let skills_root = tmp.join("skills");
    let benchmark_root = tmp.join("bench");
    fs::create_dir_all(benchmark_root.join("memory-recall")).expect("benchmark dir should exist");
    fs::write(
        benchmark_root.join("memory-recall").join("scoreboard.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "benchmark_id": "memory-recall",
            "version": 1,
            "best": null,
            "latest": null,
            "history": []
        }))
        .expect("scoreboard json"),
    )
    .expect("scoreboard should be writable");
    let bin = env!("CARGO_BIN_EXE_chuang-agent");
    let run = |args: &[&str]| bin_run(bin, args);
    seed_blocked_goal(&run, tmp.to_str().unwrap(), "evolve-revert-nobaseline");

    let evolve = run(&[
        "goal",
        "evolve",
        "--goal-id",
        "evolve-revert-nobaseline",
        "--root",
        tmp.to_str().unwrap(),
        "--approve",
        "--approval-source",
        "tiannan-cli",
        "--skills-root",
        skills_root.to_str().unwrap(),
        "--benchmark-gate",
        "memory-recall",
        "--benchmark-after-score",
        "5",
        "--benchmark-root",
        benchmark_root.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        evolve.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&evolve.stderr)
    );
    let out: serde_json::Value = serde_json::from_slice(&evolve.stdout).expect("valid json");
    assert_eq!(out["benchmark_verification"]["passed"], false);
    assert_eq!(out["benchmark_verification"]["reverted"], true);
    assert!(
        out["benchmark_verification"]["revert_reason"]
            .as_str()
            .unwrap_or_default()
            .contains("no best score"),
        "no baseline must refuse optimization"
    );
    assert!(
        !skills_root
            .join("dry_run_skill_candidate_for_chuang_goal.md")
            .exists(),
        "no baseline -> no optimize -> created skill must be reverted"
    );
}
