use chuang_agent::goal_mode::{GoalConvergencePolicy, GoalSpec};
use chuang_agent::goal_run::{
    enforce_convergence_gate, ConvergenceStatus, GoalCheckpoint, GoalIntegrationPolicy, GoalRun,
    GoalRunStore, GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
};

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
