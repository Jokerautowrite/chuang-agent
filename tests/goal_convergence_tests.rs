use chuang_agent::goal_mode::{GoalConvergencePolicy, GoalSpec};
use chuang_agent::goal_run::{
    enforce_convergence_gate, ConvergenceStatus, GoalCheckpoint, GoalIntegrationPolicy, GoalRun,
    GoalRunStore, GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
};

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
