use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::goal_run::{
    GoalCheckpoint, GoalIntegrationPolicy, GoalRun, GoalRunStore, GoalValidationPlan,
    GoalWorkerPlan, GoalWriteScope,
};

#[test]
fn goal_run_constructs_lightweight_plan_shell() {
    let run = sample_goal_run();

    assert_eq!(run.goal_spec.goal_id, "mainline-mvp");
    assert_eq!(run.worker_plan.len(), 2);
    assert_eq!(run.disjoint_write_scopes.len(), 2);
    assert_eq!(
        run.validation_plan.commands,
        vec!["cargo fmt --all", "cargo test -q"]
    );
    assert!(run.integration_policy.main_process_owns_integration);
    assert!(!run.integration_policy.workers_may_commit);
    assert!(run.checkpoint_log.is_empty());
    let diagnostics = run.diagnostics();
    assert!(!diagnostics.executes_automatically);
    assert!(!diagnostics.bypasses_governance);
    assert!(diagnostics.worker_scope_complete);
    assert!(diagnostics.worker_validation_complete);
    assert!(diagnostics.validation_plan_complete);
    assert!(!diagnostics.checkpoint_log_complete);
    assert_eq!(diagnostics.last_checkpoint_id, None);
}

#[test]
fn goal_run_rejects_overlapping_write_scopes() {
    let err = GoalRun::new(
        GoalSpec::mainline_mvp("split work safely"),
        vec![GoalWorkerPlan::new(
            "worker-1",
            "edit goal files",
            vec!["scope-a".to_string()],
            vec![],
        )],
        vec![
            GoalWriteScope::new("scope-a", vec!["src".to_string()]),
            GoalWriteScope::new("scope-b", vec!["src/goal_run.rs".to_string()]),
        ],
        GoalValidationPlan::new(vec!["cargo test -q".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect_err("nested paths should not be accepted as disjoint scopes");

    assert_eq!(err.field, "disjoint_write_scopes.paths");
}

#[test]
fn goal_run_records_checkpoints_in_order() {
    let mut run = sample_goal_run();

    run.record_checkpoint(GoalCheckpoint::new(
        "checkpoint-1",
        "worker one reported a focused patch",
        vec!["worker-1".to_string()],
        vec!["cargo test goal_run --test goal_run_tests".to_string()],
    ))
    .expect("checkpoint should record");

    assert_eq!(run.checkpoint_log.len(), 1);
    assert_eq!(run.checkpoint_log[0].checkpoint_id, "checkpoint-1");
    assert_rfc3339_timestamp(run.checkpoint_log[0].created_at.as_deref());
    assert_eq!(run.checkpoint_log[0].completed_worker_ids, vec!["worker-1"]);
    let diagnostics = run.diagnostics();
    assert!(diagnostics.checkpoint_log_complete);
    assert_eq!(
        diagnostics.last_checkpoint_id,
        Some("checkpoint-1".to_string())
    );
}

#[test]
fn goal_run_rejects_duplicate_checkpoint_ids() {
    let mut run = sample_goal_run();
    let checkpoint = GoalCheckpoint::new(
        "checkpoint-1",
        "first checkpoint",
        vec!["worker-1".to_string()],
        vec!["cargo test -q --test goal_run_tests".to_string()],
    );

    run.record_checkpoint(checkpoint.clone())
        .expect("first checkpoint should record");
    let err = run
        .record_checkpoint(checkpoint)
        .expect_err("duplicate checkpoint id should fail");

    assert_eq!(err.field, "checkpoint_log.checkpoint_id");
}

#[test]
fn goal_run_rejects_checkpoint_without_completed_worker() {
    let mut run = sample_goal_run();
    let err = run
        .record_checkpoint(GoalCheckpoint::new(
            "checkpoint-empty-worker",
            "checkpoint needs ownership",
            Vec::new(),
            vec!["cargo test -q --test goal_run_tests".to_string()],
        ))
        .expect_err("checkpoint should identify completed worker");

    assert_eq!(err.field, "checkpoint_log.completed_worker_ids");
}

#[test]
fn goal_run_rejects_checkpoint_without_validation_note() {
    let mut run = sample_goal_run();
    let err = run
        .record_checkpoint(GoalCheckpoint::new(
            "checkpoint-empty-validation",
            "checkpoint needs validation evidence",
            vec!["worker-1".to_string()],
            Vec::new(),
        ))
        .expect_err("checkpoint should include validation note");

    assert_eq!(err.field, "checkpoint_log.validation_notes");
}

#[test]
fn goal_run_rejects_duplicate_completed_worker_ids_in_checkpoint() {
    let mut run = sample_goal_run();
    let err = run
        .record_checkpoint(GoalCheckpoint::new(
            "checkpoint-duplicate-worker",
            "checkpoint should not double count workers",
            vec!["worker-1".to_string(), "worker-1".to_string()],
            vec!["cargo test -q --test goal_run_tests".to_string()],
        ))
        .expect_err("duplicate completed workers should fail");

    assert_eq!(err.field, "checkpoint_log.completed_worker_ids");
    assert_eq!(
        err.message,
        "completed worker ids must be unique within a checkpoint"
    );
}

#[test]
fn goal_run_rejects_checkpoint_with_invalid_created_at() {
    let mut run = sample_goal_run();
    let err = run
        .record_checkpoint(GoalCheckpoint {
            checkpoint_id: "checkpoint-invalid-time".to_string(),
            summary: "checkpoint timestamps must be parseable".to_string(),
            created_at: Some("not-a-timestamp".to_string()),
            completed_worker_ids: vec!["worker-1".to_string()],
            validation_notes: vec!["cargo test -q --test goal_run_tests".to_string()],
        })
        .expect_err("invalid timestamp should fail");

    assert_eq!(err.field, "checkpoint_log.created_at");
}

#[test]
fn goal_run_rejects_workers_without_validation_checks() {
    let err = GoalRun::new(
        GoalSpec::mainline_mvp("workers need verifiable acceptance"),
        vec![GoalWorkerPlan::new(
            "worker-1",
            "edit goal files",
            vec!["scope-a".to_string()],
            vec![],
        )],
        vec![GoalWriteScope::new(
            "scope-a",
            vec!["src/goal_run.rs".to_string()],
        )],
        GoalValidationPlan::new(vec!["cargo test -q".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect_err("worker validation checks should be required");

    assert_eq!(err.field, "worker_plan.validation_checks");
}

#[test]
fn goal_run_rejects_scope_owned_by_multiple_workers() {
    let err = GoalRun::new(
        GoalSpec::mainline_mvp("workers need disjoint ownership"),
        vec![
            GoalWorkerPlan::new(
                "worker-1",
                "edit goal files",
                vec!["scope-a".to_string()],
                vec!["cargo test -q --test goal_run_tests".to_string()],
            ),
            GoalWorkerPlan::new(
                "worker-2",
                "also edit goal files",
                vec!["scope-a".to_string()],
                vec!["cargo test -q --test goal_run_tests".to_string()],
            ),
        ],
        vec![GoalWriteScope::new(
            "scope-a",
            vec!["src/goal_run.rs".to_string()],
        )],
        GoalValidationPlan::new(vec!["cargo test -q".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect_err("write scope should have one owner");

    assert_eq!(err.field, "worker_plan.write_scope_ids");
    assert_eq!(err.message, "write scope must be owned by only one worker");
}

#[test]
fn goal_run_rejects_worker_plan_over_subtask_budget() {
    let mut goal = GoalSpec::mainline_mvp("workers need a parallel budget");
    goal.budget.max_subtasks = Some(1);

    let err = GoalRun::new(
        goal,
        vec![
            GoalWorkerPlan::new(
                "worker-1",
                "edit goal files",
                vec!["scope-a".to_string()],
                vec!["cargo test -q --test goal_run_tests".to_string()],
            ),
            GoalWorkerPlan::new(
                "worker-2",
                "also edit goal files",
                vec!["scope-b".to_string()],
                vec!["cargo test -q --test goal_run_tests".to_string()],
            ),
        ],
        vec![
            GoalWriteScope::new("scope-a", vec!["src/goal_run.rs".to_string()]),
            GoalWriteScope::new("scope-b", vec!["tests/goal_run_tests.rs".to_string()]),
        ],
        GoalValidationPlan::new(vec!["cargo test -q".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect_err("worker plan should not exceed subtask budget");

    assert_eq!(err.field, "budget.max_subtasks");
}

#[test]
fn goal_run_rejects_checkpoint_for_unknown_worker() {
    let mut run = sample_goal_run();
    let err = run
        .record_checkpoint(GoalCheckpoint::new(
            "checkpoint-unknown",
            "unknown worker should not be accepted",
            vec!["missing-worker".to_string()],
            vec!["cargo test -q --test goal_run_tests".to_string()],
        ))
        .expect_err("unknown worker should fail");

    assert_eq!(err.field, "checkpoint_log.completed_worker_ids");
}

#[test]
fn goal_run_round_trips_through_json() {
    let mut run = sample_goal_run();
    run.record_checkpoint(GoalCheckpoint::new(
        "checkpoint-1",
        "construction and validation plan landed",
        vec!["worker-1".to_string(), "worker-2".to_string()],
        vec!["cargo test -q goal_run --test goal_run_tests".to_string()],
    ))
    .expect("checkpoint should record");

    let encoded = serde_json::to_string(&run).expect("goal run should serialize");
    let decoded: GoalRun = serde_json::from_str(&encoded).expect("goal run should deserialize");

    assert_eq!(decoded, run);
}

#[test]
fn goal_run_store_creates_loads_and_records_checkpoint() {
    let root = temp_goal_root("store-roundtrip");
    let store = GoalRunStore::new(&root);
    let run = sample_goal_run();

    let created = store.create(&run).expect("goal run should be stored");
    assert_eq!(created.goal_id, "mainline-mvp");
    assert_eq!(created.checkpoint_count, 0);
    assert!(std::path::Path::new(&created.path).exists());

    let loaded = store
        .load("mainline-mvp")
        .expect("goal run should load from disk");
    assert_eq!(loaded.goal_spec.objective, run.goal_spec.objective);

    let checkpointed = store
        .record_checkpoint(
            "mainline-mvp",
            GoalCheckpoint::new(
                "checkpoint-1",
                "stored checkpoint can resume work",
                vec!["worker-1".to_string()],
                vec!["cargo test -q --test goal_run_tests".to_string()],
            ),
        )
        .expect("checkpoint should append");
    assert_eq!(checkpointed.checkpoint_count, 1);

    let resumed = store
        .load("mainline-mvp")
        .expect("checkpointed goal run should load");
    assert_eq!(resumed.checkpoint_log.len(), 1);
    assert_rfc3339_timestamp(resumed.checkpoint_log[0].created_at.as_deref());
    assert_eq!(
        resumed.checkpoint_log[0].summary,
        "stored checkpoint can resume work"
    );
}

#[test]
fn goal_run_store_loads_legacy_persisted_checkpoint_without_created_at() {
    let root = temp_goal_root("load-legacy-without-created-at");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");

    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(
        &path,
        r#"{
  "goal_spec": {
    "goal_id": "mainline-mvp",
    "objective": "split a goal run into owned worker scopes",
    "acceptance_checks": ["cargo fmt --all", "cargo test -q"],
    "budget": {
      "max_minutes": 60,
      "max_tool_rounds": 8,
      "max_subtasks": 4
    },
    "allowed_slots": ["context"],
    "checkpoint_policy": {
      "update_progress_log": true,
      "update_handoff": true,
      "commit_checkpoint": true
    },
    "final_report_policy": {
      "include_validation": true,
      "include_next_steps": true
    }
  },
  "worker_plan": [
    {
      "worker_id": "worker-1",
      "objective": "implement library primitive",
      "write_scope_ids": ["goal-run-lib"],
      "validation_checks": ["cargo test -q goal_run --test goal_run_tests"]
    }
  ],
  "disjoint_write_scopes": [
    {
      "scope_id": "goal-run-lib",
      "paths": ["src/goal_run.rs"]
    }
  ],
  "validation_plan": {
    "commands": ["cargo fmt --all"]
  },
  "integration_policy": {
    "main_process_owns_integration": true,
    "workers_may_commit": false,
    "workers_may_touch_secrets": false,
    "require_worker_reports": true
  },
  "checkpoint_log": [
    {
      "checkpoint_id": "checkpoint-legacy-without-created-at",
      "summary": "legacy checkpoint records remain loadable",
      "completed_worker_ids": ["worker-1"],
      "validation_notes": ["cargo test -q --test goal_run_tests"]
    }
  ]
}"#,
    )
    .expect("legacy run should be written");

    let loaded = store
        .load("mainline-mvp")
        .expect("legacy persisted checkpoint should load");

    assert_eq!(loaded.checkpoint_log.len(), 1);
    assert_eq!(loaded.checkpoint_log[0].created_at, None);
    assert_eq!(
        loaded.checkpoint_log[0].checkpoint_id,
        "checkpoint-legacy-without-created-at"
    );
}

#[test]
fn goal_run_store_rejects_persisted_checkpoint_with_invalid_created_at() {
    let root = temp_goal_root("load-invalid-created-at");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");

    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(
        &path,
        r#"{
  "goal_spec": {
    "goal_id": "mainline-mvp",
    "objective": "split a goal run into owned worker scopes",
    "acceptance_checks": ["cargo fmt --all", "cargo test -q"],
    "budget": {
      "max_minutes": 60,
      "max_tool_rounds": 8,
      "max_subtasks": 4
    },
    "allowed_slots": ["context"],
    "checkpoint_policy": {
      "update_progress_log": true,
      "update_handoff": true,
      "commit_checkpoint": true
    },
    "final_report_policy": {
      "include_validation": true,
      "include_next_steps": true
    }
  },
  "worker_plan": [
    {
      "worker_id": "worker-1",
      "objective": "implement library primitive",
      "write_scope_ids": ["goal-run-lib"],
      "validation_checks": ["cargo test -q goal_run --test goal_run_tests"]
    }
  ],
  "disjoint_write_scopes": [
    {
      "scope_id": "goal-run-lib",
      "paths": ["src/goal_run.rs"]
    }
  ],
  "validation_plan": {
    "commands": ["cargo fmt --all"]
  },
  "integration_policy": {
    "main_process_owns_integration": true,
    "workers_may_commit": false,
    "workers_may_touch_secrets": false,
    "require_worker_reports": true
  },
  "checkpoint_log": [
    {
      "checkpoint_id": "checkpoint-invalid-created-at",
      "summary": "invalid created_at should not be accepted",
      "created_at": "",
      "completed_worker_ids": ["worker-1"],
      "validation_notes": ["cargo test -q --test goal_run_tests"]
    }
  ]
}"#,
    )
    .expect("invalid run should be written");

    let err = store
        .load("mainline-mvp")
        .expect_err("invalid persisted checkpoint should fail on load");

    assert_eq!(err.field, "checkpoint_log.created_at");
}

#[test]
fn goal_run_store_rejects_invalid_persisted_checkpoint_on_load() {
    let root = temp_goal_root("load-invalid");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");

    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(
        &path,
        r#"{
  "goal_spec": {
    "goal_id": "mainline-mvp",
    "objective": "split a goal run into owned worker scopes",
    "acceptance_checks": ["cargo fmt --all", "cargo test -q"],
    "budget": {
      "max_minutes": 60,
      "max_tool_rounds": 8,
      "max_subtasks": 4
    },
    "allowed_slots": ["context"],
    "checkpoint_policy": {
      "update_progress_log": true,
      "update_handoff": true,
      "commit_checkpoint": true
    },
    "final_report_policy": {
      "include_validation": true,
      "include_next_steps": true
    }
  },
  "worker_plan": [
    {
      "worker_id": "worker-1",
      "objective": "implement library primitive",
      "write_scope_ids": ["goal-run-lib"],
      "validation_checks": ["cargo test -q goal_run --test goal_run_tests"]
    }
  ],
  "disjoint_write_scopes": [
    {
      "scope_id": "goal-run-lib",
      "paths": ["src/goal_run.rs"]
    }
  ],
  "validation_plan": {
    "commands": ["cargo fmt --all"]
  },
  "integration_policy": {
    "main_process_owns_integration": true,
    "workers_may_commit": false,
    "workers_may_touch_secrets": false,
    "require_worker_reports": true
  },
  "checkpoint_log": [
    {
      "checkpoint_id": "checkpoint-1",
      "summary": "invalid persisted checkpoint",
      "completed_worker_ids": ["worker-1"],
      "validation_notes": ["cargo test -q --test goal_run_tests"]
    },
    {
      "checkpoint_id": "checkpoint-1",
      "summary": "duplicate checkpoint id",
      "completed_worker_ids": ["worker-1"],
      "validation_notes": ["cargo test -q --test goal_run_tests"]
    }
  ]
}"#,
    )
    .expect("invalid run should be written");

    let err = store
        .load("mainline-mvp")
        .expect_err("invalid persisted checkpoint should fail on load");

    assert_eq!(err.field, "checkpoint_log.checkpoint_id");
}

#[test]
fn goal_run_store_loads_legacy_persisted_checkpoint_with_empty_completed_workers() {
    let root = temp_goal_root("load-legacy-checkpoint");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");

    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(
        &path,
        r#"{
  "goal_spec": {
    "goal_id": "mainline-mvp",
    "objective": "split a goal run into owned worker scopes",
    "acceptance_checks": ["cargo fmt --all", "cargo test -q"],
    "budget": {
      "max_minutes": 60,
      "max_tool_rounds": 8,
      "max_subtasks": 4
    },
    "allowed_slots": ["context"],
    "checkpoint_policy": {
      "update_progress_log": true,
      "update_handoff": true,
      "commit_checkpoint": true
    },
    "final_report_policy": {
      "include_validation": true,
      "include_next_steps": true
    }
  },
  "worker_plan": [
    {
      "worker_id": "worker-1",
      "objective": "implement library primitive",
      "write_scope_ids": ["goal-run-lib"],
      "validation_checks": ["cargo test -q goal_run --test goal_run_tests"]
    }
  ],
  "disjoint_write_scopes": [
    {
      "scope_id": "goal-run-lib",
      "paths": ["src/goal_run.rs"]
    }
  ],
  "validation_plan": {
    "commands": ["cargo fmt --all"]
  },
  "integration_policy": {
    "main_process_owns_integration": true,
    "workers_may_commit": false,
    "workers_may_touch_secrets": false,
    "require_worker_reports": true
  },
  "checkpoint_log": [
    {
      "checkpoint_id": "checkpoint-legacy-1",
      "summary": "legacy checkpoint records remain loadable",
      "completed_worker_ids": [],
      "validation_notes": ["cargo test -q --test goal_run_tests"]
    }
  ]
}"#,
    )
    .expect("legacy run should be written");

    let loaded = store
        .load("mainline-mvp")
        .expect("legacy persisted checkpoint should load");

    assert_eq!(loaded.checkpoint_log.len(), 1);
    assert!(loaded.checkpoint_log[0].completed_worker_ids.is_empty());
    assert_eq!(
        loaded.checkpoint_log[0].checkpoint_id,
        "checkpoint-legacy-1"
    );
    assert_eq!(
        loaded.diagnostics().last_checkpoint_id,
        Some("checkpoint-legacy-1".to_string())
    );
}

#[test]
fn goal_run_store_loads_legacy_persisted_checkpoint_with_empty_validation_notes() {
    let root = temp_goal_root("load-legacy-validation-notes");
    let store = GoalRunStore::new(&root);
    let path = store
        .goal_path("mainline-mvp")
        .expect("goal path should resolve");

    std::fs::create_dir_all(&root).expect("goal root should exist");
    std::fs::write(
        &path,
        r#"{
  "goal_spec": {
    "goal_id": "mainline-mvp",
    "objective": "split a goal run into owned worker scopes",
    "acceptance_checks": ["cargo fmt --all", "cargo test -q"],
    "budget": {
      "max_minutes": 60,
      "max_tool_rounds": 8,
      "max_subtasks": 4
    },
    "allowed_slots": ["context"],
    "checkpoint_policy": {
      "update_progress_log": true,
      "update_handoff": true,
      "commit_checkpoint": true
    },
    "final_report_policy": {
      "include_validation": true,
      "include_next_steps": true
    }
  },
  "worker_plan": [
    {
      "worker_id": "worker-1",
      "objective": "implement library primitive",
      "write_scope_ids": ["goal-run-lib"],
      "validation_checks": ["cargo test -q goal_run --test goal_run_tests"]
    }
  ],
  "disjoint_write_scopes": [
    {
      "scope_id": "goal-run-lib",
      "paths": ["src/goal_run.rs"]
    }
  ],
  "validation_plan": {
    "commands": ["cargo fmt --all"]
  },
  "integration_policy": {
    "main_process_owns_integration": true,
    "workers_may_commit": false,
    "workers_may_touch_secrets": false,
    "require_worker_reports": true
  },
  "checkpoint_log": [
    {
      "checkpoint_id": "checkpoint-legacy-validation",
      "summary": "legacy checkpoint notes remain loadable",
      "completed_worker_ids": ["worker-1"],
      "validation_notes": []
    }
  ]
}"#,
    )
    .expect("legacy run should be written");

    let loaded = store
        .load("mainline-mvp")
        .expect("legacy persisted checkpoint should load");

    assert_eq!(loaded.checkpoint_log.len(), 1);
    assert!(loaded.checkpoint_log[0].validation_notes.is_empty());
    assert_eq!(
        loaded.checkpoint_log[0].checkpoint_id,
        "checkpoint-legacy-validation"
    );
    assert_eq!(
        loaded.diagnostics().last_checkpoint_id,
        Some("checkpoint-legacy-validation".to_string())
    );
}

#[test]
fn goal_run_store_rejects_unsafe_goal_id_path() {
    let root = temp_goal_root("unsafe-id");
    let store = GoalRunStore::new(&root);
    let err = store
        .goal_path("../escape")
        .expect_err("path traversal goal id should fail");

    assert_eq!(err.field, "goal_id");
}

fn sample_goal_run() -> GoalRun {
    GoalRun::new(
        GoalSpec::mainline_mvp("split a goal run into owned worker scopes"),
        vec![
            GoalWorkerPlan::new(
                "worker-1",
                "implement library primitive",
                vec!["goal-run-lib".to_string()],
                vec!["cargo test -q goal_run --test goal_run_tests".to_string()],
            ),
            GoalWorkerPlan::new(
                "worker-2",
                "add focused tests",
                vec!["goal-run-tests".to_string()],
                vec!["cargo test -q goal_run --test goal_run_tests".to_string()],
            ),
        ],
        vec![
            GoalWriteScope::new("goal-run-lib", vec!["src/goal_run.rs".to_string()]),
            GoalWriteScope::new(
                "goal-run-tests",
                vec!["tests/goal_run_tests.rs".to_string()],
            ),
        ],
        GoalValidationPlan::new(vec![
            "cargo fmt --all".to_string(),
            "cargo test -q".to_string(),
        ]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect("sample goal run should construct")
}

fn temp_goal_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "chuang-goal-run-test-{name}-{}",
        std::process::id()
    ))
}

fn assert_rfc3339_timestamp(value: Option<&str>) {
    let value = value.expect("checkpoint should include created_at");
    chrono::DateTime::parse_from_rfc3339(value).expect("created_at should be RFC3339");
}

#[test]
fn goal_run_sets_started_at_and_caps_step_runs() {
    let run = sample_goal_run();
    assert!(run.started_at.is_some());
    assert_eq!(run.step_run_cap(100), 8); // mainline_mvp max_tool_rounds=8
    assert_eq!(run.step_run_cap(3), 3);
    run.assert_time_budget_allows_continue()
        .expect("fresh run should be within 60 minutes");
}

#[test]
fn goal_run_time_budget_blocks_when_elapsed() {
    let mut run = sample_goal_run();
    run.goal_spec.budget.max_minutes = Some(1);
    // two hours ago
    run.started_at = Some(
        (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339(),
    );
    let err = run
        .assert_time_budget_allows_continue()
        .expect_err("should exhaust time budget");
    assert_eq!(err.field, "budget.max_minutes");
}

#[test]
fn goal_run_time_budget_skips_legacy_without_started_at() {
    let mut run = sample_goal_run();
    run.started_at = None;
    run.goal_spec.budget.max_minutes = Some(1);
    run.assert_time_budget_allows_continue()
        .expect("legacy runs without started_at must not hard-block");
}
