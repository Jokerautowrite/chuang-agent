use chuang_agent::subagent_tree_ledger::{
    AgentRole, InMemorySubagentTreeLedger, ReportAdmissionRef, SpawnRequest, SubagentTreeLedger,
    SubagentTreeLedgerError, SubagentTreePolicy, SubagentTreeStatus,
};

fn ledger(max_depth: u16, max_concurrent_children: u16) -> InMemorySubagentTreeLedger {
    InMemorySubagentTreeLedger::new(
        "root-thread",
        SubagentTreePolicy::new(max_depth, max_concurrent_children),
    )
    .expect("ledger should initialize")
}

fn spawn_request(parent: &str, child: &str, role: AgentRole, nickname: &str) -> SpawnRequest {
    SpawnRequest {
        parent_thread_id: parent.to_string(),
        child_thread_id: child.to_string(),
        agent_role: role,
        nickname: nickname.to_string(),
    }
}

fn admission_ref(id: &str) -> ReportAdmissionRef {
    ReportAdmissionRef {
        admission_id: id.to_string(),
        report_id: Some(format!("report-{id}")),
        status: "Accepted".to_string(),
        reason_code: "report_validated".to_string(),
        evidence_ref: Some(format!("queue://reports/{id}")),
    }
}

#[test]
fn summarize_children_counts_statuses_and_report_reasons_without_payload_leakage() {
    let mut ledger = ledger(3, 4);
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-1",
            AgentRole::Analyze,
            "reader",
        ))
        .expect("first spawn should succeed");
    ledger
        .register_report("child-thread-1", admission_ref("admission-1"))
        .expect("report should register");
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-2",
            AgentRole::Execute,
            "builder",
        ))
        .expect("second spawn should succeed");
    ledger
        .close("child-thread-2")
        .expect("close should succeed");
    let summary = ledger.summarize_children("root-thread");

    assert_eq!(summary.parent_thread_id, "root-thread");
    assert_eq!(summary.child_count, 2);
    assert_eq!(summary.open_child_count, 1);
    assert_eq!(summary.reported_child_count, 1);
    assert_eq!(summary.closed_child_count, 1);
    assert_eq!(summary.accepted_report_count, 1);
    assert_eq!(summary.rejected_report_count, 0);
    assert_eq!(summary.missing_report_count, 1);
    assert_eq!(
        summary.child_thread_ids,
        vec!["child-thread-1", "child-thread-2"]
    );
    assert_eq!(
        summary.report_reason_codes.get("report_validated"),
        Some(&1)
    );
    assert_eq!(summary.report_admission_refs.len(), 1);
    assert_eq!(summary.report_admission_refs[0].admission_id, "admission-1");
    assert_eq!(
        summary.report_admission_refs[0].report_id.as_deref(),
        Some("report-admission-1")
    );
    assert_eq!(summary.report_admission_refs[0].status, "Accepted");
    assert_eq!(
        summary.report_admission_refs[0].reason_code,
        "report_validated"
    );
    assert_eq!(
        summary.report_admission_refs[0].evidence_ref.as_deref(),
        Some("queue://reports/admission-1")
    );

    let encoded = serde_json::to_string(&summary).expect("summary should serialize");
    assert!(encoded.contains("child-thread-1"));
    assert!(encoded.contains("\"report_admission_refs\""));
    assert!(encoded.contains("\"admission_id\":\"admission-1\""));
    assert!(encoded.contains("queue://reports/admission-1"));
    assert!(!encoded.contains("report-admission-secret-value"));
}

#[test]
fn ledger_records_root_child_relation_spawn_edge_and_report_ref() {
    let mut ledger = ledger(3, 4);

    let edge = ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-1",
            AgentRole::Analyze,
            "code-reviewer",
        ))
        .expect("spawn should succeed");

    assert_eq!(ledger.root_thread_id(), "root-thread");
    assert_eq!(ledger.policy(), &SubagentTreePolicy::new(3, 4));
    assert_eq!(edge.parent_thread_id, "root-thread");
    assert_eq!(edge.child_thread_id, "child-thread-1");
    assert_eq!(edge.depth, 1);
    assert_eq!(edge.agent_role, AgentRole::Analyze);
    assert_eq!(edge.nickname, "code-reviewer");
    assert_eq!(edge.status, SubagentTreeStatus::Spawned);
    assert_eq!(edge.report_admission_ref, None);

    let report = admission_ref("admission-1");
    let record = ledger
        .register_report("child-thread-1", report.clone())
        .expect("report registration should succeed");

    assert_eq!(record.relation.root_thread_id, "root-thread");
    assert_eq!(
        record.relation.parent_thread_id.as_deref(),
        Some("root-thread")
    );
    assert_eq!(record.relation.thread_id, "child-thread-1");
    assert_eq!(record.relation.depth, 1);
    assert_eq!(record.agent_role, AgentRole::Analyze);
    assert_eq!(record.nickname, "code-reviewer");
    assert_eq!(record.status, SubagentTreeStatus::Reported);
    assert_eq!(record.report_admission_ref, Some(report.clone()));
    let spawn_edge = record.spawn_edge.expect("spawn edge should exist");
    assert_eq!(spawn_edge.status, SubagentTreeStatus::Reported);
    assert_eq!(spawn_edge.report_admission_ref, Some(report));
}

#[test]
fn ledger_lists_children_in_spawn_order_and_close_frees_concurrency_slot() {
    let mut ledger = ledger(2, 2);
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-1",
            AgentRole::Analyze,
            "reader",
        ))
        .expect("first spawn should succeed");
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-2",
            AgentRole::Execute,
            "builder",
        ))
        .expect("second spawn should succeed");

    let blocked = ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-3",
            AgentRole::Reviewer,
            "reviewer",
        ))
        .expect_err("third open child should exceed max_concurrent_children");
    assert_eq!(
        blocked,
        SubagentTreeLedgerError::ConcurrentLimitExceeded {
            parent_thread_id: "root-thread".to_string(),
            open_child_count: 2,
            max_concurrent_children: 2,
        }
    );

    let children = ledger.list_children("root-thread");
    assert_eq!(
        children
            .iter()
            .map(|record| record.relation.thread_id.as_str())
            .collect::<Vec<_>>(),
        vec!["child-thread-1", "child-thread-2"]
    );

    let closed = ledger
        .close("child-thread-1")
        .expect("close should succeed");
    assert_eq!(closed.status, SubagentTreeStatus::Closed);

    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-3",
            AgentRole::Reviewer,
            "reviewer",
        ))
        .expect("closed child should free one concurrent slot");
    let children = ledger.list_children("root-thread");
    assert_eq!(
        children
            .iter()
            .map(|record| record.relation.thread_id.as_str())
            .collect::<Vec<_>>(),
        vec!["child-thread-1", "child-thread-2", "child-thread-3"]
    );
}

#[test]
fn ledger_rejects_depth_limit_without_mutating_tree() {
    let mut ledger = ledger(1, 4);
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-1",
            AgentRole::Orchestrate,
            "planner",
        ))
        .expect("depth one spawn should succeed");

    let before = ledger.records();
    let err = ledger
        .spawn(spawn_request(
            "child-thread-1",
            "grandchild-thread-1",
            AgentRole::Analyze,
            "nested-reader",
        ))
        .expect_err("depth two should be rejected");

    assert_eq!(
        err,
        SubagentTreeLedgerError::DepthLimitExceeded {
            parent_thread_id: "child-thread-1".to_string(),
            child_thread_id: "grandchild-thread-1".to_string(),
            attempted_depth: 2,
            max_depth: 1,
        }
    );
    assert_eq!(ledger.records(), before);
    assert!(ledger.get("grandchild-thread-1").is_none());
}

#[test]
fn ledger_spawn_validation_is_pure_and_rejects_duplicate_unknown_and_empty_fields() {
    let mut ledger = ledger(2, 4);
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-1",
            AgentRole::Analyze,
            "reader",
        ))
        .expect("spawn should succeed");
    let before = ledger.records();

    let duplicate = spawn_request(
        "root-thread",
        "child-thread-1",
        AgentRole::Execute,
        "duplicate",
    );
    assert_eq!(
        ledger
            .validate_spawn(&duplicate)
            .expect_err("duplicate should fail validation"),
        SubagentTreeLedgerError::DuplicateThread {
            thread_id: "child-thread-1".to_string(),
        }
    );
    assert_eq!(ledger.records(), before);

    let unknown_parent = spawn_request("missing-parent", "child-thread-2", AgentRole::Analyze, "x");
    assert_eq!(
        ledger
            .validate_spawn(&unknown_parent)
            .expect_err("unknown parent should fail validation"),
        SubagentTreeLedgerError::UnknownParent {
            parent_thread_id: "missing-parent".to_string(),
        }
    );
    assert_eq!(ledger.records(), before);

    let empty_nickname = spawn_request("root-thread", "child-thread-2", AgentRole::Analyze, " ");
    assert_eq!(
        ledger
            .validate_spawn(&empty_nickname)
            .expect_err("empty nickname should fail validation"),
        SubagentTreeLedgerError::EmptyNickname
    );
    assert_eq!(ledger.records(), before);
}

#[test]
fn ledger_contract_structs_roundtrip_as_json() {
    let mut ledger = ledger(3, 2);
    ledger
        .spawn(spawn_request(
            "root-thread",
            "child-thread-1",
            AgentRole::Custom("researcher".to_string()),
            "wiki-reader",
        ))
        .expect("spawn should succeed");
    ledger
        .register_report("child-thread-1", admission_ref("admission-1"))
        .expect("report should register");

    let encoded = serde_json::to_string(&ledger).expect("ledger should serialize");
    let decoded: InMemorySubagentTreeLedger =
        serde_json::from_str(&encoded).expect("ledger should deserialize");

    assert_eq!(decoded, ledger);
    assert!(encoded.contains("\"root_thread_id\":\"root-thread\""));
    assert!(encoded.contains("\"parent_thread_id\":\"root-thread\""));
    assert!(encoded.contains("\"child_thread_id\":\"child-thread-1\""));
    assert!(encoded.contains("\"agent_role\":{\"Custom\":\"researcher\"}"));
    assert!(encoded.contains("\"nickname\":\"wiki-reader\""));
    assert!(encoded.contains("\"status\":\"Reported\""));
    assert!(encoded.contains("\"report_admission_ref\""));
}

#[test]
fn children_summary_deserializes_legacy_json_without_report_admission_refs() {
    let summary: chuang_agent::subagent_tree_ledger::SubagentChildrenSummary =
        serde_json::from_str(
            r#"{
                "parent_thread_id":"root-thread",
                "child_count":1,
                "open_child_count":1,
                "reported_child_count":0,
                "closed_child_count":0,
                "accepted_report_count":0,
                "rejected_report_count":0,
                "missing_report_count":1,
                "child_thread_ids":["child-thread-1"],
                "report_reason_codes":{}
            }"#,
        )
        .expect("legacy summary should deserialize");

    assert_eq!(summary.parent_thread_id, "root-thread");
    assert_eq!(summary.child_count, 1);
    assert!(summary.report_admission_refs.is_empty());
}

#[test]
fn ledger_close_rejects_root_and_unknown_thread() {
    let mut ledger = ledger(2, 2);

    assert_eq!(
        ledger
            .close("root-thread")
            .expect_err("root close should be rejected"),
        SubagentTreeLedgerError::CannotCloseRoot {
            thread_id: "root-thread".to_string(),
        }
    );
    assert_eq!(
        ledger
            .register_report("missing-thread", admission_ref("admission-1"))
            .expect_err("missing thread should be rejected"),
        SubagentTreeLedgerError::UnknownThread {
            thread_id: "missing-thread".to_string(),
        }
    );
}
