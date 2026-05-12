use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, TaskId, Timestamp};
use chuang_agent::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{
    ContextIsolation, RunId, SubagentDispatch, SubagentToolPolicy,
};

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
    assert_eq!(
        run["goal_operability"]["goal_dispatch_manifest_state"],
        serde_json::Value::String("missing".to_string())
    );
    assert_eq!(
        run["goal_operability"]["goal_dispatch_ready"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        run["goal_operability"]["goal_step_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        run["goal_operability"]["goal_collect_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        run["goal_operability"]["goal_checkpoint_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        run["goal_operability"]["goal_pipeline_state"],
        serde_json::Value::String("dispatch_pending".to_string())
    );
    assert_eq!(
        run["goal_operability"]["goal_next_command"],
        serde_json::Value::String(format!(
            "cargo run -- goal dispatch --root {} --goal-id multi-worker-goal --subagent-queue-root ./context/subagent-queue",
            root.display()
        ))
    );
    assert_eq!(
        run["goal_operability"]["goal_next_command_reason"],
        serde_json::Value::String("dispatch manifest is missing or invalid".to_string())
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
fn cli_goal_plan_can_override_subtask_budget() {
    let root = temp_goal_root("max-subtasks");
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--objective",
            "allow a small parallel worker budget",
            "--scope",
            "memory=src/memory_recall.rs,tests/memory_recall_tests.rs",
            "--worker",
            "memory-worker|memory|extend memory recall surface",
            "--max-subtasks",
            "2",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

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
    assert_eq!(run["goal_spec"]["budget"]["max_subtasks"], 2);
}

#[test]
fn cli_goal_show_surfaces_next_command_and_stage_readiness() {
    let root = temp_goal_root("show-operability");
    let queue_root = temp_goal_root("show-operability-queue");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-operability-goal",
            "--objective",
            "surface next command and stage readiness",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--scope",
            "goal-tests=tests/goal_dispatch_tests.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
            "--worker",
            "goal-worker-2|goal-tests|stabilize queue metadata",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--validation",
            "cargo test -q --test goal_dispatch_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");
    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let show_before_dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-operability-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal show should execute");
    assert!(
        show_before_dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show_before_dispatch.stderr)
    );
    let before_dispatch: serde_json::Value =
        serde_json::from_slice(&show_before_dispatch.stdout).expect("show should be json");
    assert_eq!(
        before_dispatch["goal_operability"]["goal_dispatch_manifest_state"],
        serde_json::Value::String("missing".to_string())
    );
    assert_eq!(
        before_dispatch["goal_operability"]["queue_root"],
        serde_json::Value::String(queue_root.display().to_string())
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_pipeline_state"],
        serde_json::Value::String("dispatch_pending".to_string())
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_next_command"],
        serde_json::Value::String(format!(
            "cargo run -- goal dispatch --root {} --goal-id show-operability-goal --subagent-queue-root {}",
            root.display(),
            queue_root.display()
        ))
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_next_command_reason"],
        serde_json::Value::String("dispatch manifest is missing or invalid".to_string())
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_dispatch_manifest_error_field"],
        serde_json::Value::String("goal_dispatch_manifest.path".to_string())
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_dispatch_ready"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_step_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_collect_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        before_dispatch["goal_operability"]["goal_checkpoint_ready"],
        serde_json::Value::Bool(false)
    );

    let dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "dispatch",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-operability-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal dispatch should execute");
    assert!(
        dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dispatch.stderr)
    );
    let dispatch_receipt: serde_json::Value =
        serde_json::from_slice(&dispatch.stdout).expect("dispatch should be json");

    let show_after_dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-operability-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal show should execute");
    assert!(
        show_after_dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show_after_dispatch.stderr)
    );
    let after_dispatch: serde_json::Value =
        serde_json::from_slice(&show_after_dispatch.stdout).expect("show should be json");
    assert_eq!(
        after_dispatch["goal_operability"]["goal_dispatch_manifest_state"],
        serde_json::Value::String("ready".to_string())
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_pipeline_state"],
        serde_json::Value::String("step_pending".to_string())
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_next_command"],
        serde_json::Value::String(format!(
            "cargo run -- goal step --root {} --goal-id show-operability-goal --subagent-queue-root {}",
            root.display(),
            queue_root.display()
        ))
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_next_command_reason"],
        serde_json::Value::String(
            "dispatch manifest is present but reports are not yet ready to checkpoint".to_string()
        )
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_step_ready"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_collect_ready"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_checkpoint_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_collect"]["available_report_count"],
        serde_json::Value::Number(0.into())
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_collect"]["missing_run_ids"]
            .as_array()
            .expect("missing run ids")
            .len(),
        2
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_collect"]["blocked_report_run_ids"],
        serde_json::json!([])
    );
    assert_eq!(
        after_dispatch["goal_operability"]["goal_collect"]["blocked_report_reasons"],
        serde_json::json!([])
    );

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    for dispatch in dispatch_receipt["dispatches"]
        .as_array()
        .expect("dispatches")
    {
        let run_id = dispatch["run_id"].as_str().expect("run id");
        let task_id = dispatch["task_id"].as_str().expect("task id");
        let agent_id = dispatch["agent_id"].as_str().expect("agent id");
        let worker_id = dispatch["worker_id"].as_str().expect("worker id");
        queue
            .write_report_for_test(
                &chuang_agent::subagent_spawner::RunId(run_id.to_string()),
                &build_cli_goal_report(run_id, task_id, agent_id, worker_id, "worker completed"),
            )
            .expect("report should write");
    }

    let show_ready_to_checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-operability-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal show should execute");
    assert!(
        show_ready_to_checkpoint.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show_ready_to_checkpoint.stderr)
    );
    let ready_to_checkpoint: serde_json::Value =
        serde_json::from_slice(&show_ready_to_checkpoint.stdout).expect("show should be json");
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_pipeline_state"],
        serde_json::Value::String("checkpoint_ready".to_string())
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_next_command"],
        serde_json::Value::String(format!(
            "cargo run -- goal checkpoint --from-collect --root {} --goal-id show-operability-goal --subagent-queue-root {} --checkpoint-id <checkpoint-id>",
            root.display(),
            queue_root.display()
        ))
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_checkpoint_ready"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_next_command_reason"],
        serde_json::Value::String("dispatch reports are ready to checkpoint".to_string())
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_collect"]["ready_to_checkpoint"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_collect"]["missing_run_ids"],
        serde_json::json!([])
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_collect"]["report_run_ids"]
            .as_array()
            .expect("report run ids")
            .len(),
        2
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_collect"]["blocked_report_run_ids"],
        serde_json::json!([])
    );
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_collect"]["completed_worker_ids"],
        serde_json::json!(["goal-worker-1", "goal-worker-2"])
    );
    let ready_validation_notes = ready_to_checkpoint["goal_operability"]["goal_collect"]
        ["checkpoint_suggestion"]["validation_notes"]
        .as_array()
        .expect("validation notes");
    assert_eq!(ready_validation_notes.len(), 2);
    assert!(ready_validation_notes.iter().all(|note| note
        .as_str()
        .expect("validation note string")
        .contains("worker completed")));
    assert_eq!(
        ready_to_checkpoint["goal_operability"]["goal_collect"]["checkpoint_suggestion"]["summary"],
        serde_json::Value::String(
            "checkpoint ready for goal_id=show-operability-goal workers=goal-worker-1 | goal-worker-2"
                .to_string()
        )
    );

    let show_ready_text = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-operability-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
        ])
        .output()
        .expect("goal show text should execute");
    assert!(
        show_ready_text.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show_ready_text.stderr)
    );
    let stdout = String::from_utf8_lossy(&show_ready_text.stdout);
    assert!(stdout.contains(&format!(
        "goal_operability_queue_root: {}",
        queue_root.display()
    )));
    assert!(stdout.contains("goal_operability_pipeline_state: checkpoint_ready"));
    assert!(stdout
        .contains("goal_operability_next_command: cargo run -- goal checkpoint --from-collect"));
    assert!(stdout.contains(
        "goal_operability_next_command_reason: dispatch reports are ready to checkpoint"
    ));
    assert!(stdout.contains("goal_operability_collect_missing_run_ids: none"));
    assert!(stdout.contains(
        "goal_operability_collect_report_run_ids: goal-show-operability-goal-goal-worker-1-"
    ));
    assert!(stdout.contains("goal_operability_collect_blocked_report_run_ids: none"));
    assert!(stdout.contains("goal_operability_collect_blocked_report_reasons: none"));
    assert!(stdout.contains("goal_operability_collect_ready_to_checkpoint: true"));
    assert!(stdout.contains(
        "goal_operability_checkpoint_completed_worker_ids: goal-worker-1 | goal-worker-2"
    ));
    assert!(stdout.contains("goal_operability_checkpoint_validation_notes:"));
    assert!(stdout.contains("worker completed"));
}

#[test]
fn cli_goal_show_uses_subagent_queue_root_without_creating_it() {
    let root = temp_goal_root("show-readonly-queue-root");
    let dispatch_queue_root = temp_goal_root("show-readonly-dispatch-queue");
    let readonly_queue_root = temp_goal_root("show-readonly-missing-queue");
    plan_dispatch_goal(&root, &dispatch_queue_root, "show-readonly-queue-goal");
    assert!(
        !readonly_queue_root.exists(),
        "precondition: read-only queue root should not exist"
    );

    let show = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "show-readonly-queue-goal",
            "--subagent-queue-root",
            readonly_queue_root
                .to_str()
                .expect("read-only queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal show should execute");

    assert!(
        show.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show.stderr)
    );
    assert!(
        !readonly_queue_root.exists(),
        "goal show must not create the read-only queue root"
    );
    let output: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show json");
    assert_eq!(
        output["goal_operability"]["queue_root"],
        serde_json::Value::String(readonly_queue_root.display().to_string())
    );
    assert_eq!(
        output["goal_operability"]["goal_pipeline_state"],
        serde_json::Value::String("step_pending".to_string())
    );
    assert_eq!(
        output["goal_operability"]["goal_next_command"],
        serde_json::Value::String(format!(
            "cargo run -- goal step --root {} --goal-id show-readonly-queue-goal --subagent-queue-root {}",
            root.display(),
            readonly_queue_root.display()
        ))
    );
    assert_eq!(
        output["goal_operability"]["goal_collect"]["available_report_count"],
        serde_json::Value::Number(0.into())
    );
    assert_eq!(
        output["goal_operability"]["goal_collect"]["missing_run_ids"]
            .as_array()
            .expect("missing run ids")
            .len(),
        2
    );
    assert_eq!(
        output["goal_operability"]["goal_collect"]["blocked_report_run_ids"],
        serde_json::json!([])
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

#[test]
fn cli_goal_dispatch_can_queue_multiple_workers_without_overwriting() {
    let root = temp_goal_root("dispatch");
    let queue_root = temp_goal_root("dispatch-queue");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "dispatch-goal",
            "--objective",
            "dispatch goal work into queue",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--scope",
            "goal-tests=tests/goal_dispatch_tests.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
            "--worker",
            "goal-worker-2|goal-tests|stabilize queue metadata",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--validation",
            "cargo test -q --test goal_dispatch_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "dispatch",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "dispatch-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal dispatch should execute");

    assert!(
        dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dispatch.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&dispatch.stdout).expect("dispatch should be json");
    assert_eq!(receipt["dispatch_count"], 2);
    assert_eq!(
        receipt["dispatch_diagnostics"]["ready_to_dispatch"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        receipt["dispatches"].as_array().expect("dispatches").len(),
        2
    );
    let first_run_id = receipt["dispatches"][0]["run_id"]
        .as_str()
        .expect("run id should exist");
    let first_dispatch_path = queue_root
        .join("dispatch")
        .join(format!("{first_run_id}.json"));
    assert!(first_dispatch_path.exists());

    let manifest_path = root.join("dispatch-goal.dispatch.json");
    assert!(manifest_path.exists());
}

#[test]
fn cli_goal_collect_can_seed_checkpoint_from_ready_reports() {
    let root = temp_goal_root("collect");
    let queue_root = temp_goal_root("collect-queue");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-goal",
            "--objective",
            "collect goal reports from queue",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--scope",
            "goal-tests=tests/goal_dispatch_tests.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
            "--worker",
            "goal-worker-2|goal-tests|stabilize queue metadata",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--validation",
            "cargo test -q --test goal_dispatch_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "dispatch",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal dispatch should execute");

    assert!(
        dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dispatch.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&dispatch.stdout).expect("dispatch should be json");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    for dispatch in receipt["dispatches"].as_array().expect("dispatches") {
        let run_id = dispatch["run_id"].as_str().expect("run id");
        let task_id = dispatch["task_id"].as_str().expect("task id");
        let agent_id = dispatch["agent_id"].as_str().expect("agent id");
        let worker_id = dispatch["worker_id"].as_str().expect("worker id");
        queue
            .write_report_for_test(
                &chuang_agent::subagent_spawner::RunId(run_id.to_string()),
                &build_cli_goal_report(run_id, task_id, agent_id, worker_id, "worker completed"),
            )
            .expect("report should write");
    }

    let collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal collect should execute");

    assert!(
        collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&collect.stdout).expect("collect should be json");
    assert_eq!(receipt["available_report_count"], 2);
    assert_eq!(
        receipt["missing_run_ids"]
            .as_array()
            .expect("missing")
            .len(),
        0
    );
    assert_eq!(
        receipt["completed_worker_ids"]
            .as_array()
            .expect("workers")
            .len(),
        2
    );
    assert_eq!(
        receipt["ready_to_checkpoint"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        receipt["checkpoint_suggestion"]["summary"],
        serde_json::Value::String(
            "checkpoint ready for goal_id=collect-goal workers=goal-worker-1 | goal-worker-2"
                .to_string()
        )
    );
    assert_eq!(
        receipt["checkpoint_suggestion"]["completed_worker_ids"],
        serde_json::json!(["goal-worker-1", "goal-worker-2"])
    );
    assert_eq!(
        receipt["checkpoint_suggestion"]["validation_notes"]
            .as_array()
            .expect("validation notes")
            .len(),
        2
    );

    let checkpoint_args = build_goal_checkpoint_args_from_collect(
        &root,
        "collect-goal",
        &receipt,
        "checkpoint-from-collect",
    )
    .expect("collect receipt should be ready to checkpoint");

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args(checkpoint_args)
        .output()
        .expect("goal checkpoint should execute");

    assert!(
        checkpoint.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&checkpoint.stderr)
    );
    let checkpoint_receipt: serde_json::Value =
        serde_json::from_slice(&checkpoint.stdout).expect("checkpoint should be json");
    assert_eq!(checkpoint_receipt["checkpoint_count"], 1);
    assert_eq!(
        checkpoint_receipt["last_checkpoint_id"],
        "checkpoint-from-collect"
    );
    assert_eq!(
        checkpoint_receipt["last_checkpoint_summary"],
        receipt["checkpoint_suggestion"]["summary"]
    );
    assert_eq!(
        checkpoint_receipt["checkpoint_writeback"]["manual_only"],
        true
    );
    assert_eq!(
        checkpoint_receipt["checkpoint_writeback"]["documentation_targets"],
        serde_json::json!(["docs/progress-log.md", "docs/handoff-current.md"])
    );

    let text_collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
        ])
        .output()
        .expect("goal collect text should execute");

    assert!(
        text_collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&text_collect.stderr)
    );
    let stdout = String::from_utf8_lossy(&text_collect.stdout);
    assert!(stdout.contains("goal_collect_ready_to_checkpoint: true"));
    assert!(stdout.contains("goal_collect_parent_context_handoff_count: 2"));
    assert!(stdout.contains("goal_collect_parent_context_handoff_refs:"));
    assert!(stdout.contains("goal_collect_handoff_query_parent_context_handoff_count: 2"));
    assert!(stdout.contains("goal_collect_handoff_query_report_admission_ref_count: 2"));
    assert!(stdout.contains("goal_collect_handoff_query_report_admission_reason_codes:"));
    assert!(stdout.contains("goal_collect_handoff_query_report_admission_refs:"));
    assert!(stdout.contains("admission_id=goal-report-admission://"));
    assert!(stdout.contains(
        "goal_collect_checkpoint_summary: checkpoint ready for goal_id=collect-goal workers=goal-worker-1 | goal-worker-2"
    ));
    assert!(stdout
        .contains("goal_collect_checkpoint_completed_worker_ids: goal-worker-1 | goal-worker-2"));
    assert!(stdout.contains("goal_collect_checkpoint_validation_notes:"));
}

#[test]
fn cli_goal_collect_surfaces_blocked_report_reasons_for_failed_and_mismatched_reports() {
    let root = temp_goal_root("collect-blocked-reasons");
    let queue_root = temp_goal_root("collect-blocked-reasons-queue");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--objective",
            "collect should expose blocked report reasons",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--scope",
            "goal-tests=tests/goal_dispatch_tests.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
            "--worker",
            "goal-worker-2|goal-tests|stabilize queue metadata",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--validation",
            "cargo test -q --test goal_dispatch_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "dispatch",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal dispatch should execute");

    assert!(
        dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dispatch.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&dispatch.stdout).expect("dispatch should be json");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    let first = &receipt["dispatches"][0];
    let second = &receipt["dispatches"][1];
    let mut failed = build_cli_goal_report(
        first["run_id"].as_str().expect("run id"),
        first["task_id"].as_str().expect("task id"),
        first["agent_id"].as_str().expect("agent id"),
        first["worker_id"].as_str().expect("worker id"),
        "first worker failed",
    );
    failed.status = ExecutionStatus::Failed;
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(
                first["run_id"].as_str().expect("run id").to_string(),
            ),
            &failed,
        )
        .expect("failed report should write");
    let mut mismatched = build_cli_goal_report(
        second["run_id"].as_str().expect("run id"),
        second["task_id"].as_str().expect("task id"),
        second["agent_id"].as_str().expect("agent id"),
        second["worker_id"].as_str().expect("worker id"),
        "second worker mismatched",
    );
    mismatched.agent_id = chuang_agent::common::AgentId("wrong-agent".to_string());
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(
                second["run_id"].as_str().expect("run id").to_string(),
            ),
            &mismatched,
        )
        .expect("mismatched report should write");

    let collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal collect should execute");

    assert!(
        collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&collect.stdout).expect("collect should be json");
    assert_eq!(receipt["available_report_count"], 2);
    assert_eq!(
        receipt["blocked_report_run_ids"]
            .as_array()
            .expect("blocked report run ids")
            .len(),
        2
    );
    assert_eq!(
        receipt["blocked_report_reasons"]
            .as_array()
            .expect("blocked report reasons")
            .len(),
        2
    );
    assert_eq!(
        receipt["completed_worker_ids"]
            .as_array()
            .expect("workers")
            .len(),
        0
    );
    assert_eq!(
        receipt["ready_to_checkpoint"],
        serde_json::Value::Bool(false)
    );
    assert!(receipt["checkpoint_suggestion"].is_null());
    let blocked_reasons = receipt["blocked_report_reasons"]
        .as_array()
        .expect("blocked reasons")
        .iter()
        .map(|value| value.as_str().expect("blocked reason string"))
        .collect::<Vec<_>>();
    assert!(blocked_reasons
        .iter()
        .any(|reason| reason.contains("report status is not success")));
    assert!(blocked_reasons
        .iter()
        .any(|reason| reason.contains("identity does not match")));

    let text_collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
        ])
        .output()
        .expect("goal collect text should execute");

    assert!(
        text_collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&text_collect.stderr)
    );
    let stdout = String::from_utf8_lossy(&text_collect.stdout);
    assert!(stdout.contains("goal_collect_blocked_report_run_ids:"));
    assert!(stdout.contains("goal_collect_blocked_report_reasons:"));
    assert!(stdout.contains("report status is not success"));
    assert!(stdout.contains("identity does not match"));

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--checkpoint-id",
            "checkpoint-from-blocked-collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--from-collect",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(!checkpoint.status.success());
    let stderr = String::from_utf8_lossy(&checkpoint.stderr);
    assert!(stderr.contains("goal_checkpoint_invalid: collect.ready_to_checkpoint"));
    assert!(stderr.contains("blocked_report_run_ids="));
    assert!(stderr.contains("blocked_report_reasons="));
    assert!(stderr.contains("report status is not success"));
    assert!(stderr.contains("identity does not match"));

    let show = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal show should execute");
    assert!(
        show.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show.stderr)
    );
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).expect("show json");
    assert_eq!(
        show_json["goal_operability"]["goal_pipeline_state"],
        serde_json::Value::String("step_pending".to_string())
    );
    assert_eq!(
        show_json["goal_operability"]["goal_next_command"],
        serde_json::Value::String(format!(
            "cargo run -- goal step --root {} --goal-id collect-blocked-reasons-goal --subagent-queue-root {}",
            root.display(),
            queue_root.display()
        ))
    );
    assert_eq!(
        show_json["goal_operability"]["goal_next_command_reason"],
        serde_json::Value::String(
            "dispatch manifest is present but reports are not yet ready to checkpoint".to_string()
        )
    );
    assert_eq!(
        show_json["goal_operability"]["goal_checkpoint_ready"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        show_json["goal_operability"]["goal_collect"]["blocked_report_run_ids"]
            .as_array()
            .expect("blocked ids")
            .len(),
        2
    );
    let show_blocked_reasons = show_json["goal_operability"]["goal_collect"]
        ["blocked_report_reasons"]
        .as_array()
        .expect("blocked reasons")
        .iter()
        .map(|value| value.as_str().expect("blocked reason string"))
        .collect::<Vec<_>>();
    assert!(show_blocked_reasons
        .iter()
        .any(|reason| reason.contains("report status is not success")));
    assert!(show_blocked_reasons
        .iter()
        .any(|reason| reason.contains("identity does not match")));

    let show_text = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-blocked-reasons-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
        ])
        .output()
        .expect("goal show text should execute");
    assert!(
        show_text.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show_text.stderr)
    );
    let stdout = String::from_utf8_lossy(&show_text.stdout);
    assert!(stdout.contains("goal_operability_pipeline_state: step_pending"));
    assert!(stdout.contains("goal_operability_next_command: cargo run -- goal step"));
    assert!(stdout.contains(
        "goal_operability_next_command_reason: dispatch manifest is present but reports are not yet ready to checkpoint"
    ));
    assert!(stdout.contains("goal_operability_checkpoint_ready: false"));
    assert!(stdout.contains("goal_operability_parent_context_handoff_count: 0"));
    assert!(stdout.contains("goal_operability_parent_context_handoff_refs:"));
    assert!(stdout.contains("goal_operability_handoff_query_parent_context_handoff_count: 0"));
    assert!(stdout.contains("goal_operability_handoff_query_report_admission_ref_count: 0"));
    assert!(stdout.contains("goal_operability_handoff_query_report_admission_reason_codes:"));
    assert!(stdout.contains("goal_operability_handoff_query_report_admission_refs:"));
    assert!(stdout.contains("goal_operability_collect_blocked_report_run_ids:"));
    assert!(stdout.contains("goal_operability_collect_blocked_report_reasons:"));
    assert!(stdout.contains("report status is not success"));
    assert!(stdout.contains("identity does not match"));
}

#[test]
fn cli_goal_collect_blocks_malformed_report_from_checkpoint_material() {
    let root = temp_goal_root("collect-malformed");
    let queue_root = temp_goal_root("collect-malformed-queue");
    plan_dispatch_goal(&root, &queue_root, "collect-malformed-goal");
    let manifest_path = root.join("collect-malformed-goal.dispatch.json");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest should exist"))
            .expect("manifest should be json");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    let first = &manifest["dispatches"][0];
    let second = &manifest["dispatches"][1];
    queue
        .write_report_for_test(
            &RunId(first["run_id"].as_str().expect("run id").to_string()),
            &build_cli_goal_report(
                first["run_id"].as_str().expect("run id"),
                first["task_id"].as_str().expect("task id"),
                first["agent_id"].as_str().expect("agent id"),
                first["worker_id"].as_str().expect("worker id"),
                "first worker completed",
            ),
        )
        .expect("valid report should write");
    let malformed_run_id = RunId(second["run_id"].as_str().expect("run id").to_string());
    std::fs::write(queue.report_path(&malformed_run_id), "{bad json")
        .expect("malformed report should write");

    let collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-malformed-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal collect should execute");

    assert!(
        collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&collect.stdout).expect("collect should be json");
    assert_eq!(receipt["available_report_count"], 2);
    assert_eq!(
        receipt["missing_run_ids"]
            .as_array()
            .expect("missing")
            .len(),
        0
    );
    assert_eq!(
        receipt["blocked_report_run_ids"],
        serde_json::json!([malformed_run_id.0.clone()])
    );
    assert_eq!(
        receipt["ready_to_checkpoint"],
        serde_json::Value::Bool(false)
    );
    assert!(receipt["checkpoint_suggestion"].is_null());
    let blocked_reasons = receipt["blocked_report_reasons"]
        .as_array()
        .expect("blocked reasons")
        .iter()
        .map(|value| value.as_str().expect("blocked reason string"))
        .collect::<Vec<_>>();
    assert!(blocked_reasons
        .iter()
        .any(|reason| reason.contains("report parse failed")));

    let text_collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-malformed-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
        ])
        .output()
        .expect("goal collect text should execute");

    assert!(
        text_collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&text_collect.stderr)
    );
    let stdout = String::from_utf8_lossy(&text_collect.stdout);
    assert!(stdout.contains("goal_collect_ready_to_checkpoint: false"));
    assert!(stdout.contains("goal_collect_parent_context_handoff_count: 1"));
    assert!(stdout.contains("goal_collect_parent_context_handoff_refs:"));
    assert!(stdout.contains("goal_collect_handoff_query_parent_context_handoff_count: 1"));
    assert!(stdout.contains("goal_collect_handoff_query_report_admission_ref_count: 1"));
    assert!(stdout.contains("goal_collect_handoff_query_report_admission_reason_codes:"));
    assert!(stdout.contains("goal_collect_handoff_query_report_admission_refs:"));
    assert!(stdout.contains("admission_id=goal-report-admission://"));
    assert!(stdout.contains("goal_collect_blocked_report_run_ids:"));
    assert!(stdout.contains("goal_collect_blocked_report_reasons:"));
    assert!(stdout.contains("report parse failed"));

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-malformed-goal",
            "--checkpoint-id",
            "checkpoint-from-malformed-collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--from-collect",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(!checkpoint.status.success());
    let stderr = String::from_utf8_lossy(&checkpoint.stderr);
    assert!(stderr.contains("goal_checkpoint_invalid: collect.ready_to_checkpoint"));
    assert!(stderr.contains("blocked_report_run_ids="));
    assert!(stderr.contains("blocked_report_reasons="));
    assert!(stderr.contains("report parse failed"));
}

#[test]
fn cli_goal_step_runs_manifest_workers_and_collects_reports_without_checkpointing() {
    let root = temp_goal_root("step");
    let queue_root = temp_goal_root("step-queue");
    plan_dispatch_goal(&root, &queue_root, "step-goal");

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--max-runs",
            "2",
            "--max-concurrency",
            "2",
            "--json",
        ])
        .output()
        .expect("goal step should execute");

    assert!(
        step.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&step.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&step.stdout).expect("step should output json");
    assert_eq!(receipt["manifest"]["dispatch_count"], 2);
    assert_eq!(receipt["run_loop"]["max_runs"], 2);
    assert_eq!(receipt["run_loop"]["max_concurrency"], 2);
    assert_eq!(receipt["run_loop"]["ran_count"], 2);
    let run_ids = receipt["run_loop"]["run_ids"]
        .as_array()
        .expect("run ids")
        .iter()
        .map(|value| value.as_str().expect("run id string").to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_run_ids = receipt["manifest"]["dispatches"]
        .as_array()
        .expect("manifest dispatches")
        .iter()
        .map(|dispatch| {
            dispatch["run_id"]
                .as_str()
                .expect("manifest run id")
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(run_ids, manifest_run_ids);
    assert_eq!(
        receipt["collection"]["ready_to_checkpoint"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(receipt["checkpoint_recorded"], false);
    assert_eq!(receipt["writes_progress_log"], false);
    assert_eq!(receipt["writes_handoff"], false);

    let show = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "show",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-goal",
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
        run["checkpoint_log"].as_array().expect("checkpoints").len(),
        0
    );
}

#[test]
fn cli_goal_step_text_exposes_handoff_query_summary() {
    let root = temp_goal_root("step-text-handoff");
    let queue_root = temp_goal_root("step-text-handoff-queue");
    plan_dispatch_goal(&root, &queue_root, "step-text-handoff-goal");

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-text-handoff-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--max-runs",
            "2",
            "--max-concurrency",
            "2",
        ])
        .output()
        .expect("goal step should execute");

    assert!(
        step.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&step.stderr)
    );
    let stdout = String::from_utf8_lossy(&step.stdout);
    assert!(stdout.contains("goal_step_handoff_query_parent_context_handoff_count: 2"));
    assert!(stdout.contains("goal_step_handoff_query_report_admission_ref_count: 2"));
    assert!(stdout.contains("goal_step_handoff_query_report_admission_reason_codes:"));
    assert!(stdout.contains("goal_step_handoff_query_report_admission_refs:"));
}

#[test]
fn cli_goal_step_only_runs_manifest_dispatches() {
    let root = temp_goal_root("step-allowlist");
    let queue_root = temp_goal_root("step-allowlist-queue");
    plan_dispatch_goal(&root, &queue_root, "step-allowlist-goal");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    queue
        .write_dispatch(&sample_unrelated_dispatch())
        .expect("unrelated dispatch should write");

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-allowlist-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--max-concurrency",
            "3",
            "--json",
        ])
        .output()
        .expect("goal step should execute");

    assert!(
        step.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&step.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&step.stdout).expect("step should output json");
    assert_eq!(receipt["run_loop"]["max_runs"], 2);
    assert_eq!(receipt["run_loop"]["max_concurrency"], 3);
    assert_eq!(receipt["run_loop"]["ran_count"], 2);
    let run_ids = receipt["run_loop"]["run_ids"]
        .as_array()
        .expect("run ids")
        .iter()
        .map(|value| value.as_str().expect("run id string").to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_run_ids = receipt["manifest"]["dispatches"]
        .as_array()
        .expect("manifest dispatches")
        .iter()
        .map(|dispatch| {
            dispatch["run_id"]
                .as_str()
                .expect("manifest run id")
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(run_ids, manifest_run_ids);
    assert!(!queue
        .report_path(&RunId("unrelated-run".to_string()))
        .exists());
}

#[test]
fn cli_goal_step_reports_not_ready_when_max_runs_is_partial() {
    let root = temp_goal_root("step-partial");
    let queue_root = temp_goal_root("step-partial-queue");
    plan_dispatch_goal(&root, &queue_root, "step-partial-goal");

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-partial-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--max-runs",
            "1",
            "--max-concurrency",
            "2",
            "--json",
        ])
        .output()
        .expect("goal step should execute");

    assert!(
        step.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&step.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&step.stdout).expect("step should output json");
    assert_eq!(receipt["run_loop"]["max_runs"], 1);
    assert_eq!(receipt["run_loop"]["max_concurrency"], 2);
    assert_eq!(receipt["run_loop"]["ran_count"], 1);
    let run_ids = receipt["run_loop"]["run_ids"]
        .as_array()
        .expect("run ids")
        .iter()
        .map(|value| value.as_str().expect("run id string").to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_run_ids = receipt["manifest"]["dispatches"]
        .as_array()
        .expect("manifest dispatches")
        .iter()
        .map(|dispatch| {
            dispatch["run_id"]
                .as_str()
                .expect("manifest run id")
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(run_ids.is_subset(&manifest_run_ids));
    assert_eq!(
        receipt["collection"]["ready_to_checkpoint"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        receipt["collection"]["missing_run_ids"]
            .as_array()
            .expect("missing")
            .len(),
        1
    );
}

#[test]
fn cli_goal_step_rejects_zero_max_runs() {
    let root = temp_goal_root("step-zero-max-runs");
    let queue_root = temp_goal_root("step-zero-max-runs-queue");
    plan_dispatch_goal(&root, &queue_root, "step-zero-max-runs-goal");

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-zero-max-runs-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--max-runs",
            "0",
        ])
        .output()
        .expect("goal step should execute");

    assert!(!step.status.success());
    let stderr = String::from_utf8_lossy(&step.stderr);
    assert!(stderr.contains("--max-runs must be greater than zero"));
}

#[test]
fn cli_goal_step_rejects_unbounded_parallel_concurrency() {
    let root = temp_goal_root("step-max-concurrency");
    let queue_root = temp_goal_root("step-max-concurrency-queue");
    plan_dispatch_goal(&root, &queue_root, "step-max-concurrency-goal");

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-max-concurrency-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--max-concurrency",
            "9",
        ])
        .output()
        .expect("goal step should execute");

    assert!(!step.status.success());
    let stderr = String::from_utf8_lossy(&step.stderr);
    assert!(stderr.contains("--max-concurrency above 8 is not supported"));
}

#[test]
fn cli_goal_step_requires_existing_dispatch_manifest() {
    let root = temp_goal_root("step-missing-manifest");
    let queue_root = temp_goal_root("step-missing-manifest-queue");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-missing-manifest-goal",
            "--objective",
            "step should require existing dispatch manifest",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
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

    let step = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "step",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "step-missing-manifest-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
        ])
        .output()
        .expect("goal step should execute");

    assert!(!step.status.success());
    let stderr = String::from_utf8_lossy(&step.stderr);
    assert!(stderr.contains("goal_dispatch_invalid: goal_dispatch_manifest.path"));
}

#[test]
fn cli_goal_collect_blocks_checkpoint_handoff_when_reports_are_missing() {
    let root = temp_goal_root("collect-missing");
    let queue_root = temp_goal_root("collect-missing-queue");
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-missing-goal",
            "--objective",
            "collect goal reports with a missing worker",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--scope",
            "goal-tests=tests/goal_dispatch_tests.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
            "--worker",
            "goal-worker-2|goal-tests|stabilize queue metadata",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--validation",
            "cargo test -q --test goal_dispatch_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");

    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "dispatch",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-missing-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal dispatch should execute");

    assert!(
        dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dispatch.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&dispatch.stdout).expect("dispatch should be json");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    let first = &receipt["dispatches"][0];
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(
                first["run_id"].as_str().expect("run id").to_string(),
            ),
            &build_cli_goal_report(
                first["run_id"].as_str().expect("run id"),
                first["task_id"].as_str().expect("task id"),
                first["agent_id"].as_str().expect("agent id"),
                first["worker_id"].as_str().expect("worker id"),
                "first worker completed",
            ),
        )
        .expect("report should write");

    let collect = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "collect",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-missing-goal",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal collect should execute");

    assert!(
        collect.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&collect.stdout).expect("collect should be json");
    assert_eq!(receipt["available_report_count"], 1);
    assert_eq!(
        receipt["missing_run_ids"]
            .as_array()
            .expect("missing")
            .len(),
        1
    );
    assert_eq!(
        receipt["ready_to_checkpoint"],
        serde_json::Value::Bool(false)
    );
    assert!(receipt["checkpoint_suggestion"].is_null());

    let err = build_goal_checkpoint_args_from_collect(
        &root,
        "collect-missing-goal",
        &receipt,
        "checkpoint-from-missing-collect",
    )
    .expect_err("partial collect should not seed checkpoint");
    assert!(err.contains("goal collect is not ready to checkpoint"));

    let checkpoint = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "checkpoint",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            "collect-missing-goal",
            "--checkpoint-id",
            "checkpoint-from-missing-collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--from-collect",
        ])
        .output()
        .expect("goal checkpoint should execute");

    assert!(!checkpoint.status.success());
    let stderr = String::from_utf8_lossy(&checkpoint.stderr);
    assert!(stderr.contains("goal_checkpoint_invalid: collect.ready_to_checkpoint"));
    assert!(stderr.contains("missing_run_ids="));
    assert!(stderr.contains("report_run_ids="));
    assert!(stderr.contains("blocked_report_run_ids=none"));
    assert!(stderr.contains("blocked_report_reasons=none"));
}

fn assert_rfc3339_timestamp(value: &str) {
    chrono::DateTime::parse_from_rfc3339(value).expect("created_at should be RFC3339");
}

fn plan_dispatch_goal(root: &PathBuf, queue_root: &PathBuf, goal_id: &str) {
    let planned = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "plan",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            goal_id,
            "--objective",
            "step goal reports from queue",
            "--scope",
            "goal-main=src/goal_dispatch.rs",
            "--scope",
            "goal-tests=tests/goal_dispatch_tests.rs",
            "--worker",
            "goal-worker-1|goal-main|tighten dispatch bridge",
            "--worker",
            "goal-worker-2|goal-tests|stabilize queue metadata",
            "--validation",
            "cargo test -q --test cli_goal_tests",
            "--validation",
            "cargo test -q --test goal_dispatch_tests",
            "--json",
        ])
        .output()
        .expect("goal plan should execute");
    assert!(
        planned.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let dispatch = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "goal",
            "dispatch",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--goal-id",
            goal_id,
            "--subagent-queue-root",
            queue_root.to_str().expect("queue root should be utf8"),
            "--json",
        ])
        .output()
        .expect("goal dispatch should execute");
    assert!(
        dispatch.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dispatch.stderr)
    );
}

fn sample_unrelated_dispatch() -> SubagentDispatch {
    SubagentDispatch {
        run_id: RunId("unrelated-run".to_string()),
        agent_id: AgentId("unrelated-agent".to_string()),
        task_id: TaskId("unrelated-task".to_string()),
        parent_agent_id: AgentId("unrelated-parent".to_string()),
        agent_name: "unrelated-worker".to_string(),
        task: "unrelated queued task".to_string(),
        tool_policy: SubagentToolPolicy::Analyze,
        context_isolation: ContextIsolation::Isolated,
        token_budget: 512,
        idle_timeout_ms: 30_000,
        recursive_spawn: false,
        metadata: Default::default(),
    }
}

fn build_cli_goal_report(
    run_id: &str,
    task_id: &str,
    agent_id: &str,
    worker_id: &str,
    summary: &str,
) -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{run_id}")),
        task_id: TaskId(task_id.to_string()),
        agent_id: AgentId(agent_id.to_string()),
        parent_agent_id: Some(AgentId("chuang-goal".to_string())),
        status: ExecutionStatus::Success,
        started_at: Timestamp("2026-05-08T00:00:00Z".to_string()),
        finished_at: Timestamp("2026-05-08T00:00:01Z".to_string()),
        summary: format!("{worker_id}: {summary}"),
        exit_code: Some(0),
        stdout_preview: Some("ok".to_string()),
        stderr_preview: None,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: None,
        context_debug: None,
        governance_decision: None,
        truncated: false,
    }
}

fn build_goal_checkpoint_args_from_collect(
    root: &PathBuf,
    goal_id: &str,
    receipt: &serde_json::Value,
    checkpoint_id: &str,
) -> Result<Vec<String>, String> {
    if !receipt
        .get("ready_to_checkpoint")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Err("goal collect is not ready to checkpoint".to_string());
    }
    let suggestion = receipt
        .get("checkpoint_suggestion")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())?;
    let summary = suggestion
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())?;
    let completed_worker_ids = suggestion
        .get("completed_worker_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())?;
    let validation_notes = suggestion
        .get("validation_notes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())?;

    let mut args = vec![
        "goal".to_string(),
        "checkpoint".to_string(),
        "--root".to_string(),
        root.to_str().expect("root should be utf8").to_string(),
        "--goal-id".to_string(),
        goal_id.to_string(),
        "--checkpoint-id".to_string(),
        checkpoint_id.to_string(),
        "--summary".to_string(),
        summary.to_string(),
    ];
    for worker_id in completed_worker_ids {
        args.push("--completed-worker-id".to_string());
        args.push(
            worker_id
                .as_str()
                .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())?
                .to_string(),
        );
    }
    for validation_note in validation_notes {
        args.push("--validation-note".to_string());
        args.push(
            validation_note
                .as_str()
                .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())?
                .to_string(),
        );
    }
    args.push("--json".to_string());
    Ok(args)
}
