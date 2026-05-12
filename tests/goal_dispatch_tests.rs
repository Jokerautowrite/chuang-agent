use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, Timestamp};
use chuang_agent::goal_dispatch::{
    collect_goal_dispatch_reports, collect_goal_dispatch_reports_read_only, dispatch_goal_run,
    load_goal_dispatch_manifest, GoalCheckpointSuggestion, GoalDispatchCollectionReceipt,
};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::goal_run::{
    GoalIntegrationPolicy, GoalRun, GoalRunStore, GoalValidationPlan, GoalWorkerPlan,
    GoalWriteScope,
};
use chuang_agent::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};

fn temp_goal_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-goal-dispatch-{name}-{nanos}"))
}

#[test]
fn goal_dispatch_writes_parallel_worker_dispatches_with_metadata() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("store");
    let queue_root = temp_goal_root("queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");

    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("goal dispatch should succeed");

    assert_eq!(receipt.goal_id, "goal-dispatch");
    assert_eq!(receipt.dispatch_count, 2);
    assert!(receipt.dispatch_diagnostics.ready_to_dispatch);
    assert_eq!(receipt.dispatches.len(), 2);

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    let dispatches = queue.list_dispatches().expect("dispatches should list");
    assert_eq!(dispatches.len(), 2);

    let first = queue
        .read_dispatch(&chuang_agent::subagent_spawner::RunId(
            receipt.dispatches[0].run_id.clone(),
        ))
        .expect("dispatch should read")
        .expect("dispatch should exist");
    assert_eq!(
        first.metadata.get("goal_id").map(String::as_str),
        Some("goal-dispatch")
    );
    assert_eq!(
        first.metadata.get("worker_id").map(String::as_str),
        Some("worker-1")
    );
    assert_eq!(
        first.metadata.get("source").map(String::as_str),
        Some("goal-dispatch")
    );
    assert!(first.task.contains("WORKER"));
    assert!(first.task.contains("validation_checks"));
    let manifest =
        load_goal_dispatch_manifest(&store_root, "goal-dispatch").expect("manifest should load");
    assert_eq!(manifest.goal_id, "goal-dispatch");
    assert_eq!(manifest.dispatch_count, 2);
    assert_eq!(manifest.dispatches.len(), 2);
}

#[test]
fn goal_dispatch_rejects_over_budget_worker_plan() {
    let mut run = GoalRun::new(
        {
            let mut goal = GoalSpec::mainline_mvp("budgeted dispatch should fail");
            goal.goal_id = "goal-dispatch-budget".to_string();
            goal
        },
        vec![
            GoalWorkerPlan::new(
                "worker-1",
                "first task",
                vec!["scope-a".to_string()],
                vec!["cargo test -q".to_string()],
            ),
            GoalWorkerPlan::new(
                "worker-2",
                "second task",
                vec!["scope-b".to_string()],
                vec!["cargo test -q".to_string()],
            ),
        ],
        vec![
            GoalWriteScope::new("scope-a", vec!["src/goal_run.rs".to_string()]),
            GoalWriteScope::new("scope-b", vec!["tests/goal_run_tests.rs".to_string()]),
        ],
        GoalValidationPlan::new(vec!["cargo test -q".to_string()]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect("goal run should build");
    run.goal_spec.budget.max_subtasks = Some(1);

    let store_root = temp_goal_root("budget-store");
    let queue_root = temp_goal_root("budget");
    let err = dispatch_goal_run(&run, &store_root, &queue_root, "goal-controller")
        .expect_err("dispatch should reject over-budget worker plan");

    assert_eq!(err.field, "budget.max_subtasks");
    assert!(err
        .message
        .contains("worker plan exceeds goal subtask budget"));
}

#[test]
fn goal_dispatch_repeated_runs_do_not_overwrite_existing_dispatches() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("repeat-store");
    let queue_root = temp_goal_root("repeat-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");

    let first = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("first dispatch should succeed");
    std::thread::sleep(Duration::from_millis(1));
    let second = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("second dispatch should succeed");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    let dispatches = queue.list_dispatches().expect("dispatches should list");
    assert_eq!(dispatches.len(), 4);
    assert_ne!(first.dispatches[0].run_id, second.dispatches[0].run_id);
    assert!(queue
        .dispatch_path(&chuang_agent::subagent_spawner::RunId(
            first.dispatches[0].run_id.clone()
        ))
        .exists());
    assert!(queue
        .dispatch_path(&chuang_agent::subagent_spawner::RunId(
            second.dispatches[0].run_id.clone()
        ))
        .exists());
    assert!(load_goal_dispatch_manifest(&store_root, "goal-dispatch").is_ok());
}

#[test]
fn goal_collect_keeps_checkpoint_handoff_unavailable_when_reports_are_missing() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("collect-store");
    let queue_root = temp_goal_root("collect-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("dispatch should succeed");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    let first_dispatch = &receipt.dispatches[0];
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(first_dispatch.run_id.clone()),
            &sample_report(first_dispatch, "worker-1 finished"),
        )
        .expect("report should write");

    let collection = collect_goal_dispatch_reports(&store_root, &queue_root, "goal-dispatch")
        .expect("collection should succeed");
    assert_eq!(collection.available_report_count, 1);
    assert_eq!(collection.missing_run_ids.len(), 1);
    assert_eq!(collection.completed_worker_ids, vec!["worker-1"]);
    assert!(!collection.ready_to_checkpoint);
    assert!(collection.checkpoint_suggestion.is_none());
    assert_eq!(collection.report_summaries, vec!["worker-1 finished"]);

    let err = checkpoint_suggestion_from_collection(&collection)
        .expect_err("partial collect should not produce checkpoint handoff");
    assert_eq!(err, "goal collect is not ready to checkpoint");
}

#[test]
fn goal_collect_exposes_checkpoint_handoff_when_all_reports_exist() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("collect-ready-store");
    let queue_root = temp_goal_root("collect-ready-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("dispatch should succeed");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    for dispatch in &receipt.dispatches {
        queue
            .write_report_for_test(
                &chuang_agent::subagent_spawner::RunId(dispatch.run_id.clone()),
                &sample_report(dispatch, &format!("{} finished", dispatch.worker_id)),
            )
            .expect("report should write");
    }

    let collection = collect_goal_dispatch_reports(&store_root, &queue_root, "goal-dispatch")
        .expect("collection should succeed");
    assert_eq!(collection.available_report_count, 2);
    assert!(collection.missing_run_ids.is_empty());
    assert_eq!(
        collection.completed_worker_ids,
        vec!["worker-1".to_string(), "worker-2".to_string()]
    );
    assert_eq!(collection.parent_context_handoffs.len(), 2);
    assert!(collection.parent_context_handoffs[0].accepted);
    assert_eq!(
        collection
            .handoff_query_summary
            .parent_context_handoff_count,
        2
    );
    assert_eq!(
        collection.handoff_query_summary.parent_context_handoff_refs,
        vec![
            format!(
                "report://{}/report-{}",
                receipt.dispatches[0].agent_id, receipt.dispatches[0].run_id
            ),
            format!(
                "report://{}/report-{}",
                receipt.dispatches[1].agent_id, receipt.dispatches[1].run_id
            ),
        ]
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_ref_count,
        2
    );
    assert_eq!(
        collection
            .handoff_query_summary
            .report_admission_reason_codes
            .get("report_validated"),
        Some(&2)
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs.len(),
        2
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0].report_id,
        format!("report-{}", receipt.dispatches[0].run_id)
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0]
            .admission_id
            .as_deref(),
        Some(
            format!(
                "goal-report-admission://{}/report-{}",
                receipt.dispatches[0].agent_id, receipt.dispatches[0].run_id
            )
            .as_str()
        )
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0].task_id,
        receipt.dispatches[0].task_id
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0].agent_id,
        receipt.dispatches[0].agent_id
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0].admission_status,
        "Accepted"
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0].reason_code,
        "report_validated"
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs[0]
            .evidence_ref
            .as_deref(),
        Some(format!(
            "report://{}/report-{}",
            receipt.dispatches[0].agent_id, receipt.dispatches[0].run_id
        ))
        .as_deref()
    );
    assert_eq!(
        collection.parent_context_handoffs[0].provenance_ref,
        Some(format!(
            "report://{}/report-{}",
            receipt.dispatches[0].agent_id, receipt.dispatches[0].run_id
        ))
    );
    assert!(collection.ready_to_checkpoint);
    let suggestion = checkpoint_suggestion_from_collection(&collection)
        .expect("collection should expose checkpoint handoff");
    assert_eq!(
        suggestion.summary,
        "checkpoint ready for goal_id=goal-dispatch workers=worker-1 | worker-2"
    );
    assert_eq!(
        suggestion.completed_worker_ids,
        vec!["worker-1".to_string(), "worker-2".to_string()]
    );
    assert_eq!(
        suggestion.validation_notes,
        vec![
            format!(
                "report available for worker_id=worker-1 run_id={} summary=worker-1 finished",
                receipt.dispatches[0].run_id
            ),
            format!(
                "report available for worker_id=worker-2 run_id={} summary=worker-2 finished",
                receipt.dispatches[1].run_id
            ),
        ]
    );
    assert!(collection
        .manifest_path
        .ends_with("goal-dispatch.dispatch.json"));
    assert_eq!(
        collection.report_run_ids,
        vec![
            receipt.dispatches[0].run_id.clone(),
            receipt.dispatches[1].run_id.clone()
        ]
    );
}

#[test]
fn goal_collect_blocks_failed_worker_reports_from_checkpoint_handoff() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("collect-failed-store");
    let queue_root = temp_goal_root("collect-failed-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("dispatch should succeed");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(receipt.dispatches[0].run_id.clone()),
            &sample_report(&receipt.dispatches[0], "worker-1 finished"),
        )
        .expect("first report should write");
    let mut failed = sample_report(&receipt.dispatches[1], "worker-2 failed");
    failed.status = ExecutionStatus::Failed;
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(receipt.dispatches[1].run_id.clone()),
            &failed,
        )
        .expect("failed report should write");

    let collection = collect_goal_dispatch_reports(&store_root, &queue_root, "goal-dispatch")
        .expect("collection should succeed");
    assert_eq!(collection.available_report_count, 2);
    assert!(collection.missing_run_ids.is_empty());
    assert_eq!(collection.completed_worker_ids, vec!["worker-1"]);
    assert_eq!(collection.parent_context_handoffs.len(), 1);
    assert_eq!(
        collection
            .handoff_query_summary
            .parent_context_handoff_count,
        1
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_ref_count,
        1
    );
    assert_eq!(
        collection
            .handoff_query_summary
            .report_admission_reason_codes
            .get("report_validated"),
        Some(&1)
    );
    assert_eq!(
        collection.blocked_report_run_ids,
        vec![receipt.dispatches[1].run_id.clone()]
    );
    assert!(collection.blocked_report_reasons[0].contains("report status is not success"));
    assert!(!collection.ready_to_checkpoint);
    assert!(collection.checkpoint_suggestion.is_none());
}

#[test]
fn goal_collect_blocks_identity_mismatch_reports_from_checkpoint_handoff() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("collect-mismatch-store");
    let queue_root = temp_goal_root("collect-mismatch-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("dispatch should succeed");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    for dispatch in &receipt.dispatches {
        let mut report = sample_report(dispatch, &format!("{} finished", dispatch.worker_id));
        if dispatch.worker_id == "worker-2" {
            report.agent_id = AgentId("wrong-agent".to_string());
        }
        queue
            .write_report_for_test(
                &chuang_agent::subagent_spawner::RunId(dispatch.run_id.clone()),
                &report,
            )
            .expect("report should write");
    }

    let collection = collect_goal_dispatch_reports(&store_root, &queue_root, "goal-dispatch")
        .expect("collection should succeed");
    assert_eq!(collection.available_report_count, 2);
    assert_eq!(collection.completed_worker_ids, vec!["worker-1"]);
    assert_eq!(collection.parent_context_handoffs.len(), 1);
    assert_eq!(
        collection
            .handoff_query_summary
            .parent_context_handoff_count,
        1
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_ref_count,
        1
    );
    assert_eq!(
        collection.blocked_report_run_ids,
        vec![receipt.dispatches[1].run_id.clone()]
    );
    assert!(collection.blocked_report_reasons[0].contains("identity does not match"));
    assert!(!collection.ready_to_checkpoint);
    assert!(collection.checkpoint_suggestion.is_none());
}

#[test]
fn goal_collect_excludes_rejected_admissions_from_handoff_query_summary() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("collect-rejected-admission-store");
    let queue_root = temp_goal_root("collect-rejected-admission-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &queue_root, "goal-controller")
        .expect("dispatch should succeed");

    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .expect("queue should open");
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(receipt.dispatches[0].run_id.clone()),
            &sample_report(&receipt.dispatches[0], "worker-1 finished"),
        )
        .expect("accepted report should write");
    let mut rejected = sample_report(&receipt.dispatches[1], "worker-2 has unsupported schema");
    rejected.schema_version = "2.0.0".to_string();
    queue
        .write_report_for_test(
            &chuang_agent::subagent_spawner::RunId(receipt.dispatches[1].run_id.clone()),
            &rejected,
        )
        .expect("rejected admission report should write");

    let collection = collect_goal_dispatch_reports(&store_root, &queue_root, "goal-dispatch")
        .expect("collection should succeed");

    assert_eq!(collection.available_report_count, 2);
    assert_eq!(collection.completed_worker_ids, vec!["worker-1"]);
    assert_eq!(collection.parent_context_handoffs.len(), 1);
    assert_eq!(
        collection
            .handoff_query_summary
            .parent_context_handoff_count,
        1
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_ref_count,
        1
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_refs.len(),
        1
    );
    assert_eq!(
        collection
            .handoff_query_summary
            .report_admission_reason_codes
            .get("report_validated"),
        Some(&1)
    );
    assert!(!collection
        .handoff_query_summary
        .report_admission_reason_codes
        .contains_key("unsupported_schema_version"));
    assert_eq!(
        collection.blocked_report_run_ids,
        vec![receipt.dispatches[1].run_id.clone()]
    );
    assert!(collection.blocked_report_reasons[0].contains("report admission rejected"));
    assert!(collection.blocked_report_reasons[0].contains("unsupported_schema_version"));
    assert!(!collection.ready_to_checkpoint);
    assert!(collection.checkpoint_suggestion.is_none());
}

#[test]
fn goal_collect_read_only_reports_missing_state_without_touching_queue_dirs() {
    let goal = sample_goal_run();
    let store_root = temp_goal_root("collect-readonly-store");
    let active_queue_root = temp_goal_root("collect-readonly-active-queue");
    let readonly_queue_root = temp_goal_root("collect-readonly-ro-queue");
    let store = GoalRunStore::new(&store_root);
    store.create(&goal).expect("goal should store");
    let loaded = store.load("goal-dispatch").expect("goal should load");
    let receipt = dispatch_goal_run(&loaded, &store_root, &active_queue_root, "goal-controller")
        .expect("dispatch should succeed");

    assert!(!readonly_queue_root.exists());

    let collection =
        collect_goal_dispatch_reports_read_only(&store_root, &readonly_queue_root, "goal-dispatch")
            .expect("readonly collection should succeed");
    assert!(!readonly_queue_root.exists());
    assert_eq!(collection.available_report_count, 0);
    assert_eq!(collection.missing_run_ids.len(), receipt.dispatches.len());
    assert!(collection.parent_context_handoffs.is_empty());
    assert_eq!(
        collection
            .handoff_query_summary
            .parent_context_handoff_count,
        0
    );
    assert_eq!(
        collection.handoff_query_summary.report_admission_ref_count,
        0
    );
    assert!(!collection.ready_to_checkpoint);
    assert!(collection.checkpoint_suggestion.is_none());
    assert!(collection
        .manifest_path
        .ends_with("goal-dispatch.dispatch.json"));
}

#[test]
fn goal_collect_handoff_query_summary_deserializes_legacy_json_without_admission_refs() {
    let summary: chuang_agent::goal_dispatch::GoalDispatchHandoffSummary = serde_json::from_str(
        r#"{
            "parent_context_handoff_count":1,
            "parent_context_handoff_refs":["report://agent-1/report-1"],
            "report_admission_ref_count":1,
            "report_admission_reason_codes":{"report_validated":1}
        }"#,
    )
    .expect("legacy summary should deserialize");

    assert_eq!(summary.parent_context_handoff_count, 1);
    assert!(summary.report_admission_refs.is_empty());
}

#[test]
fn goal_collect_receipt_deserializes_legacy_json_without_handoff_query_summary() {
    let receipt: GoalDispatchCollectionReceipt = serde_json::from_str(
        r#"{
            "goal_id":"legacy-goal",
            "goal_objective":"legacy objective",
            "goal_root":"/tmp/legacy-goal-root",
            "queue_root":"/tmp/legacy-queue-root",
            "dispatch_count":1,
            "available_report_count":1,
            "missing_run_ids":[],
            "report_run_ids":["run-1"],
            "blocked_report_run_ids":[],
            "blocked_report_reasons":[],
            "completed_worker_ids":["worker-1"],
            "report_summaries":["worker-1 finished"],
            "parent_context_handoffs":[],
            "ready_to_checkpoint":true,
            "checkpoint_suggestion":null,
            "manifest_path":"/tmp/legacy-goal-root/goal-dispatch.dispatch.json"
        }"#,
    )
    .expect("legacy receipt should deserialize");

    assert_eq!(receipt.goal_id, "legacy-goal");
    assert!(receipt
        .handoff_query_summary
        .parent_context_handoff_refs
        .is_empty());
    assert_eq!(
        receipt.handoff_query_summary.parent_context_handoff_count,
        0
    );
    assert_eq!(receipt.handoff_query_summary.report_admission_ref_count, 0);
    assert!(receipt
        .handoff_query_summary
        .report_admission_refs
        .is_empty());
}

#[test]
fn goal_report_admission_ref_deserializes_legacy_json_without_admission_id() {
    let admission_ref: chuang_agent::goal_dispatch::GoalReportAdmissionRef = serde_json::from_str(
        r#"{
                "report_id":"report-1",
                "task_id":"task-1",
                "agent_id":"agent-1",
                "admission_status":"Accepted",
                "reason_code":"report_validated",
                "evidence_ref":"report://agent-1/report-1"
            }"#,
    )
    .expect("legacy admission ref should deserialize");

    assert_eq!(admission_ref.admission_id, None);
    assert_eq!(admission_ref.report_id, "report-1");
}

fn checkpoint_suggestion_from_collection(
    collection: &GoalDispatchCollectionReceipt,
) -> Result<GoalCheckpointSuggestion, String> {
    collection
        .checkpoint_suggestion
        .clone()
        .ok_or_else(|| "goal collect is not ready to checkpoint".to_string())
}

fn sample_goal_run() -> GoalRun {
    GoalRun::new(
        {
            let mut goal = GoalSpec::mainline_mvp("dispatch worker plans into queue");
            goal.goal_id = "goal-dispatch".to_string();
            goal
        },
        vec![
            GoalWorkerPlan::new(
                "worker-1",
                "tighten goal dispatch bridge",
                vec!["scope-a".to_string()],
                vec!["cargo test -q --test goal_dispatch_tests".to_string()],
            ),
            GoalWorkerPlan::new(
                "worker-2",
                "extend queue evidence",
                vec!["scope-b".to_string()],
                vec!["cargo test -q --test goal_dispatch_tests".to_string()],
            ),
        ],
        vec![
            GoalWriteScope::new("scope-a", vec!["src/goal_dispatch.rs".to_string()]),
            GoalWriteScope::new("scope-b", vec!["tests/goal_dispatch_tests.rs".to_string()]),
        ],
        GoalValidationPlan::new(vec![
            "cargo fmt --all".to_string(),
            "cargo test -q --test goal_dispatch_tests".to_string(),
        ]),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .expect("sample goal run should build")
}

fn sample_report(
    dispatch: &chuang_agent::goal_dispatch::GoalWorkerDispatchReceipt,
    summary: &str,
) -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", dispatch.run_id)),
        task_id: chuang_agent::common::TaskId(dispatch.task_id.clone()),
        agent_id: AgentId(dispatch.agent_id.clone()),
        parent_agent_id: Some(AgentId("goal-controller".to_string())),
        status: ExecutionStatus::Success,
        started_at: Timestamp("2026-05-08T00:00:00Z".to_string()),
        finished_at: Timestamp("2026-05-08T00:00:01Z".to_string()),
        summary: summary.to_string(),
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
