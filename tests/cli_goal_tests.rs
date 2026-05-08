use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_goal_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-goal-{name}-{nanos}"))
}

#[test]
fn cli_goal_plan_can_record_disjoint_multi_worker_plan() {
    let root = temp_goal_root("multi-worker");
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "multi-worker-goal",
            "--objective",
            "split mainline work without overlapping writes",
            "--scope",
            "memory=src/memory_recall.rs,tests/memory_recall_tests.rs",
            "--scope",
            "governance=src/governance.rs,tests/governance_tests.rs",
            "--worker",
            "memory-worker|memory|extend memory recall surface",
            "--worker",
            "governance-worker|governance|tighten governance readiness",
            "--validation",
            "cargo test -q --test memory_recall_tests",
            "--validation",
            "cargo test -q --test governance_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("receipt should be json");
    assert_eq!(receipt["goal_id"], "multi-worker-goal");
    assert_eq!(receipt["checkpoint_count"], 0);

    let show = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "multi-worker-goal",
            "--json",
        ])
        .output()
        .expect("goal show should execute");

    assert!(
        show.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show.stderr)
    );
    let run: serde_json::Value = serde_json::from_slice(&show.stdout).expect("run should be json");
    assert_eq!(run["worker_plan"].as_array().expect("workers").len(), 2);
    assert_eq!(
        run["disjoint_write_scopes"]
            .as_array()
            .expect("scopes")
            .len(),
        2
    );
    assert_eq!(
        run["worker_plan"][0]["write_scope_ids"][0],
        serde_json::Value::String("memory".to_string())
    );
    assert_eq!(
        run["worker_plan"][0]["validation_checks"]
            .as_array()
            .expect("worker validation")
            .len(),
        2
    );
    assert_eq!(
        run["goal_run_diagnostics"]["worker_scope_complete"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        run["goal_run_diagnostics"]["worker_validation_complete"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        run["goal_run_diagnostics"]["executes_automatically"],
        serde_json::Value::Bool(false)
    );
}

#[test]
fn cli_goal_plan_rejects_overlapping_scope_paths() {
    let root = temp_goal_root("overlap");
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--objective",
            "reject unsafe split",
            "--scope",
            "a=src",
            "--scope",
            "b=src/goal_run.rs",
            "--worker",
            "worker-a|a|edit src",
            "--worker",
            "worker-b|b|edit goal run",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("goal_run_invalid: disjoint_write_scopes.paths"));
}

#[test]
fn cli_goal_plan_defaults_worker_to_declared_scopes() {
    let root = temp_goal_root("default-worker-scopes");
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--objective",
            "default worker should follow declared scopes",
            "--scope",
            "memory=src/memory_recall.rs,tests/memory_recall_tests.rs",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("receipt should be json");
    assert_eq!(receipt["goal_id"], "mainline-mvp");

    let show = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "mainline-mvp",
            "--json",
        ])
        .output()
        .expect("goal show should execute");

    assert!(
        show.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show.stderr)
    );
    let run: serde_json::Value = serde_json::from_slice(&show.stdout).expect("run should be json");
    assert_eq!(run["worker_plan"].as_array().expect("workers").len(), 1);
    assert_eq!(
        run["worker_plan"][0]["write_scope_ids"]
            .as_array()
            .expect("scope ids"),
        &vec![serde_json::Value::String("memory".to_string())]
    );
    assert_eq!(
        run["disjoint_write_scopes"][0]["scope_id"],
        serde_json::Value::String("memory".to_string())
    );
}

#[test]
fn cli_goal_checkpoint_surfaces_last_checkpoint_diagnostics() {
    let root = temp_goal_root("checkpoint-diagnostics");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-goal",
            "--objective",
            "make last checkpoint visible",
            "--scope",
            "goal=src/goal_run.rs,src/cli_goal.rs",
            "--worker",
            "goal-worker|goal|tighten checkpoint diagnostics",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-goal",
            "--checkpoint-id",
            "checkpoint-1",
            "--summary",
            "diagnostics landed",
            "--completed-worker-id",
            "goal-worker",
            "--validation-note",
            "cargo test -q --test cli_goal_tests",
            "--json",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(
        checkpoint.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&checkpoint.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&checkpoint.stdout).expect("receipt should be json");
    assert_eq!(receipt["checkpoint_count"], 1);
    assert_eq!(receipt["last_checkpoint_id"], "checkpoint-1");
    assert_eq!(receipt["checkpoint_writeback"]["manual_only"], true);
    assert_eq!(
        receipt["checkpoint_writeback"]["documentation_targets"]
            .as_array()
            .expect("writeback targets")
            .iter()
            .map(|value| value.as_str().expect("target string"))
            .collect::<Vec<_>>(),
        vec!["docs/progress-log.md", "docs/handoff-current.md"]
    );

    let show = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-goal",
            "--json",
        ])
        .output()
        .expect("goal show should execute");

    assert!(
        show.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show.stderr)
    );
    let run: serde_json::Value = serde_json::from_slice(&show.stdout).expect("run should be json");
    assert_eq!(
        run["goal_run_diagnostics"]["last_checkpoint_id"],
        serde_json::Value::String("checkpoint-1".to_string())
    );
    assert_eq!(
        run["goal_run_diagnostics"]["last_checkpoint_summary"],
        serde_json::Value::String("diagnostics landed".to_string())
    );
    assert_rfc3339_timestamp(
        run["checkpoint_log"][0]["created_at"]
            .as_str()
            .expect("checkpoint created_at should be present"),
    );
    assert_eq!(
        run["goal_run_diagnostics"]["checkpoint_writeback"]["manual_only"],
        true
    );
    assert_eq!(
        run["goal_run_diagnostics"]["checkpoint_writeback"]["documentation_targets"],
        serde_json::json!(["docs/progress-log.md", "docs/handoff-current.md"])
    );
    assert_eq!(
        run["goal_run_diagnostics"]["checkpoint_log_complete"],
        serde_json::Value::Bool(true)
    );

    let show_text = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-goal",
        ])
        .output()
        .expect("goal show text should execute");

    assert!(
        show_text.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show_text.stderr)
    );
    let stdout = String::from_utf8_lossy(&show_text.stdout);
    assert!(stdout.contains("goal_checkpoint_log_complete: true"));
    assert!(stdout.contains("goal_last_checkpoint: checkpoint-1"));
    assert!(stdout.contains("goal_last_summary: diagnostics landed"));
    assert!(stdout.contains("goal_checkpoint_writeback_manual_only: true"));
    assert!(stdout.contains(
        "goal_checkpoint_writeback_targets: docs/progress-log.md | docs/handoff-current.md"
    ));
    assert!(stdout.contains("goal_incomplete_reasons: none"));
}

#[test]
fn cli_goal_checkpoint_requires_completed_worker_id() {
    let root = temp_goal_root("checkpoint-requires-worker");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-worker-required",
            "--objective",
            "checkpoint must identify completed worker",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-worker-required",
            "--checkpoint-id",
            "checkpoint-empty-worker",
            "--summary",
            "missing completed worker",
            "--validation-note",
            "cargo test -q --test cli_goal_tests",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(!checkpoint.status.success());
    let stderr = String::from_utf8_lossy(&checkpoint.stderr);
    assert!(stderr.contains("goal_run_invalid: checkpoint_log.completed_worker_ids"));
}

#[test]
fn cli_goal_checkpoint_requires_validation_note() {
    let root = temp_goal_root("checkpoint-requires-validation");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-validation-required",
            "--objective",
            "checkpoint must include validation evidence",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-validation-required",
            "--checkpoint-id",
            "checkpoint-empty-validation",
            "--summary",
            "missing validation note",
            "--completed-worker-id",
            "main-process",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(!checkpoint.status.success());
    let stderr = String::from_utf8_lossy(&checkpoint.stderr);
    assert!(stderr.contains("goal_run_invalid: checkpoint_log.validation_notes"));
}

#[test]
fn cli_goal_checkpoint_rejects_duplicate_completed_worker_id() {
    let root = temp_goal_root("checkpoint-duplicate-worker");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-duplicate-worker",
            "--objective",
            "checkpoint should not double count worker",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-duplicate-worker",
            "--checkpoint-id",
            "checkpoint-duplicate-worker",
            "--summary",
            "duplicate completed worker",
            "--completed-worker-id",
            "main-process",
            "--completed-worker-id",
            "main-process",
            "--validation-note",
            "cargo test -q --test cli_goal_tests",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(!checkpoint.status.success());
    let stderr = String::from_utf8_lossy(&checkpoint.stderr);
    assert!(stderr.contains("goal_run_invalid: checkpoint_log.completed_worker_ids"));
    assert!(stderr.contains("completed worker ids must be unique"));
}

#[test]
fn cli_goal_checkpoint_text_output_includes_writeback_hints() {
    let root = temp_goal_root("checkpoint-text-writeback");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-text-writeback",
            "--objective",
            "checkpoint text should show manual writeback hints",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "checkpoint-text-writeback",
            "--checkpoint-id",
            "checkpoint-text-1",
            "--summary",
            "text output should name writeback targets",
            "--completed-worker-id",
            "main-process",
            "--validation-note",
            "cargo test -q --test cli_goal_tests",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(
        checkpoint.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&checkpoint.stderr)
    );
    let stdout = String::from_utf8_lossy(&checkpoint.stdout);
    assert!(stdout.contains("goal_checkpoint_summary: text output should name writeback targets"));
    assert!(stdout.contains("goal_checkpoint_writeback_manual_only: true"));
    assert!(stdout.contains(
        "goal_checkpoint_writeback_targets: docs/progress-log.md | docs/handoff-current.md"
    ));
}

fn assert_rfc3339_timestamp(value: &str) {
    chrono::DateTime::parse_from_rfc3339(value).expect("created_at should be RFC3339");
}
