use chuang_agent::runtime_event_ledger::RuntimeEventKind;
use chuang_agent::subagent_tree_events::{
    subagent_closed_event, subagent_message_sent_event, subagent_reported_event,
    subagent_spawned_event, subagent_wait_started_event, SubagentTreeBridgeEventKind,
    SubagentTreeEventBuilder,
};
use chuang_agent::subagent_tree_ledger::{
    AgentRole, InMemorySubagentTreeLedger, ReportAdmissionRef, SpawnRequest, SubagentTreeLedger,
    SubagentTreePolicy, SubagentTreeStatus,
};

const CREATED_AT: &str = "2026-05-11T00:00:00Z";

fn ledger() -> InMemorySubagentTreeLedger {
    InMemorySubagentTreeLedger::new("root-thread", SubagentTreePolicy::new(3, 4))
        .expect("ledger should initialize")
}

fn spawn_request(child_thread_id: &str, nickname: &str) -> SpawnRequest {
    SpawnRequest {
        parent_thread_id: "root-thread".to_string(),
        child_thread_id: child_thread_id.to_string(),
        agent_role: AgentRole::Analyze,
        nickname: nickname.to_string(),
    }
}

fn admission(status: &str, reason_code: &str, evidence_ref: &str) -> ReportAdmissionRef {
    ReportAdmissionRef {
        admission_id: format!("admission-{reason_code}"),
        report_id: Some(format!("report-{reason_code}")),
        status: status.to_string(),
        reason_code: reason_code.to_string(),
        evidence_ref: Some(evidence_ref.to_string()),
    }
}

#[test]
fn spawn_event_serializes_runtime_event_and_tree_identity() {
    let mut ledger = ledger();
    let edge = ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");

    let event = SubagentTreeEventBuilder::new(CREATED_AT)
        .with_turn_id("turn-1")
        .spawn_event("root-thread", &edge);
    let helper_event = subagent_spawned_event(CREATED_AT, "root-thread", &edge);

    assert_eq!(event.schema_version, 1);
    assert_eq!(
        helper_event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentSpawned
    );
    assert_eq!(helper_event.runtime_event.turn_id, None);
    assert_eq!(
        event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentSpawned
    );
    assert_eq!(event.runtime_event.schema_version, 1);
    assert_eq!(
        event.runtime_event.event_type,
        RuntimeEventKind::SubagentSpawned
    );
    assert_eq!(event.runtime_event.thread_id, "child-thread-1");
    assert_eq!(event.runtime_event.turn_id.as_deref(), Some("turn-1"));
    assert_eq!(
        event.runtime_event.call_id.as_deref(),
        Some("subagent-spawn:child-thread-1")
    );
    assert_eq!(event.root_thread_id, "root-thread");
    assert_eq!(event.parent_thread_id.as_deref(), Some("root-thread"));
    assert_eq!(event.child_thread_id, "child-thread-1");
    assert_eq!(event.agent_role, AgentRole::Analyze);
    assert_eq!(event.nickname, "reader");
    assert_eq!(event.status, SubagentTreeStatus::Spawned);
    assert!(event.admission_id.is_none());
    assert_eq!(
        event.evidence_ref,
        "subagent-tree://root-thread/spawn/root-thread/child-thread-1"
    );
    assert_eq!(
        event.runtime_event.evidence_ref.as_deref(),
        Some("subagent-tree://root-thread/spawn/root-thread/child-thread-1")
    );

    let encoded = serde_json::to_string(&event).expect("event should serialize");
    assert!(encoded.contains("\"bridge_event_kind\":\"subagent_spawned\""));
    assert!(encoded.contains("\"event_type\":\"subagent_spawned\""));
    assert!(encoded.contains("\"root_thread_id\":\"root-thread\""));
    assert!(encoded.contains("\"child_thread_id\":\"child-thread-1\""));
    assert!(encoded.contains("\"nickname\":\"reader\""));
}

#[test]
fn report_event_uses_admission_evidence_ref_and_report_identity() {
    let mut ledger = ledger();
    ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");
    let record = ledger
        .register_report(
            "child-thread-1",
            admission(
                "Accepted",
                "report_validated",
                "queue://reports/report_validated",
            ),
        )
        .expect("report should register");

    let event = subagent_reported_event(CREATED_AT, &record);

    assert_eq!(
        event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentReported
    );
    assert_eq!(
        event.runtime_event.event_type,
        RuntimeEventKind::SubagentReported
    );
    assert_eq!(
        event.runtime_event.call_id.as_deref(),
        Some("subagent-report:child-thread-1")
    );
    assert_eq!(event.status, SubagentTreeStatus::Reported);
    assert_eq!(
        event.admission_id.as_deref(),
        Some("admission-report_validated")
    );
    assert_eq!(event.report_id.as_deref(), Some("report-report_validated"));
    assert_eq!(event.admission_status.as_deref(), Some("Accepted"));
    assert_eq!(
        event.admission_reason_code.as_deref(),
        Some("report_validated")
    );
    assert_eq!(event.evidence_ref, "queue://reports/report_validated");
    assert_eq!(
        event.runtime_event.evidence_ref.as_deref(),
        Some("queue://reports/report_validated")
    );

    let encoded = serde_json::to_string(&event).expect("event should serialize");
    assert!(encoded.contains("\"bridge_event_kind\":\"subagent_reported\""));
    assert!(encoded.contains("\"admission_status\":\"Accepted\""));
    assert!(encoded.contains("\"evidence_ref\":\"queue://reports/report_validated\""));
}

#[test]
fn rejected_admission_event_preserves_rejected_status_reason_and_evidence() {
    let mut ledger = ledger();
    ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");
    let record = ledger
        .register_report(
            "child-thread-1",
            admission(
                "Rejected",
                "command_protocol_report_rejected",
                "queue://reports/rejected",
            ),
        )
        .expect("rejected report should register");

    let event = subagent_reported_event(CREATED_AT, &record);

    assert_eq!(
        event.runtime_event.event_type,
        RuntimeEventKind::SubagentReported
    );
    assert_eq!(event.admission_status.as_deref(), Some("Rejected"));
    assert_eq!(
        event.admission_reason_code.as_deref(),
        Some("command_protocol_report_rejected")
    );
    assert_eq!(event.evidence_ref, "queue://reports/rejected");
    assert_eq!(
        event.runtime_event.evidence_ref.as_deref(),
        Some("queue://reports/rejected")
    );

    let encoded = serde_json::to_string(&event).expect("event should serialize");
    assert!(encoded.contains("\"admission_status\":\"Rejected\""));
    assert!(encoded.contains("\"admission_reason_code\":\"command_protocol_report_rejected\""));
    assert!(encoded.contains("\"evidence_ref\":\"queue://reports/rejected\""));
}

#[test]
fn close_event_serializes_as_bridge_event_without_new_runtime_event_kind() {
    let mut ledger = ledger();
    ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");
    let record = ledger
        .close("child-thread-1")
        .expect("close should not delete records");

    let event = subagent_closed_event(CREATED_AT, &record);

    assert_eq!(
        event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentClosed
    );
    assert_eq!(
        event.runtime_event.event_type,
        RuntimeEventKind::ToolFinished
    );
    assert_eq!(
        event.runtime_event.call_id.as_deref(),
        Some("subagent-close:child-thread-1")
    );
    assert_eq!(event.status, SubagentTreeStatus::Closed);
    assert_eq!(
        event.evidence_ref,
        "subagent-tree://root-thread/record/close/child-thread-1"
    );
    assert_eq!(
        event.runtime_event.evidence_ref.as_deref(),
        Some("subagent-tree://root-thread/record/close/child-thread-1")
    );

    let encoded = serde_json::to_string(&event).expect("event should serialize");
    assert!(encoded.contains("\"bridge_event_kind\":\"subagent_closed\""));
    assert!(encoded.contains("\"event_type\":\"tool_finished\""));
    assert!(encoded.contains("\"status\":\"Closed\""));
}

#[test]
fn message_and_wait_events_record_audited_subagent_activity_without_secret_values() {
    let mut ledger = ledger();
    ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");
    let record = ledger
        .register_report(
            "child-thread-1",
            admission("Accepted", "report_validated", "queue://reports/report-1"),
        )
        .expect("report should register");

    let message_event = subagent_message_sent_event(CREATED_AT, &record, "msg-1");
    let wait_event = subagent_wait_started_event(CREATED_AT, &record, "wait-1");

    assert_eq!(
        message_event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentMessageSent
    );
    assert_eq!(
        message_event.runtime_event.event_type,
        RuntimeEventKind::ToolStarted
    );
    assert_eq!(
        message_event.runtime_event.call_id.as_deref(),
        Some("subagent-message:child-thread-1:msg-1")
    );
    assert!(message_event.evidence_ref.contains("message/msg-1"));
    assert_eq!(
        wait_event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentWaitStarted
    );
    assert_eq!(
        wait_event.runtime_event.event_type,
        RuntimeEventKind::ToolStarted
    );
    assert_eq!(
        wait_event.runtime_event.call_id.as_deref(),
        Some("subagent-wait:child-thread-1:wait-1")
    );
    assert!(wait_event.evidence_ref.contains("wait/wait-1"));

    let encoded =
        serde_json::to_string(&[message_event, wait_event]).expect("events should serialize");
    assert!(encoded.contains("subagent_message_sent"));
    assert!(encoded.contains("subagent_wait_started"));
    assert!(!encoded.contains("message-body-secret-value"));
    assert!(!encoded.contains("wait-result-secret-value"));
}

#[test]
fn list_event_snapshots_children_and_their_evidence_refs() {
    let mut ledger = ledger();
    ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");
    ledger
        .register_report(
            "child-thread-1",
            admission("Accepted", "report_validated", "queue://reports/report-1"),
        )
        .expect("report should register");
    ledger
        .spawn(spawn_request("child-thread-2", "reviewer"))
        .expect("second spawn should succeed");
    let children = ledger.list_children("root-thread");

    let event = SubagentTreeEventBuilder::new(CREATED_AT).list_event(
        "root-thread",
        "root-thread",
        &children,
    );

    assert_eq!(
        event.bridge_event_kind,
        SubagentTreeBridgeEventKind::SubagentChildrenListed
    );
    assert_eq!(
        event.runtime_event.event_type,
        RuntimeEventKind::ToolFinished
    );
    assert_eq!(event.runtime_event.thread_id, "root-thread");
    assert_eq!(
        event.runtime_event.call_id.as_deref(),
        Some("subagent-list-children:root-thread")
    );
    assert_eq!(event.child_count, 2);
    assert!(event.consistency_warnings.is_empty());
    assert_eq!(
        event.evidence_ref,
        "subagent-tree://root-thread/children/root-thread"
    );
    assert_eq!(event.children[0].child_thread_id, "child-thread-1");
    assert_eq!(event.children[0].root_thread_id, "root-thread");
    assert_eq!(
        event.children[0].parent_thread_id.as_deref(),
        Some("root-thread")
    );
    assert_eq!(
        event.children[0].admission_id.as_deref(),
        Some("admission-report_validated")
    );
    assert_eq!(
        event.children[0].admission_status.as_deref(),
        Some("Accepted")
    );
    assert_eq!(
        event.children[0].admission_reason_code.as_deref(),
        Some("report_validated")
    );
    assert_eq!(
        event.children[0].evidence_ref.as_deref(),
        Some("queue://reports/report-1")
    );
    assert_eq!(event.children[1].child_thread_id, "child-thread-2");
    assert_eq!(event.children[1].root_thread_id, "root-thread");
    assert_eq!(
        event.children[1].parent_thread_id.as_deref(),
        Some("root-thread")
    );
    assert!(event.children[1].admission_id.is_none());
    assert!(event.children[1].admission_status.is_none());
    assert!(event.children[1].admission_reason_code.is_none());
    assert!(event.children[1].evidence_ref.is_none());

    let encoded = serde_json::to_string(&event).expect("event should serialize");
    assert!(encoded.contains("\"bridge_event_kind\":\"subagent_children_listed\""));
    assert!(encoded.contains("\"child_count\":2"));
    assert!(encoded.contains("\"root_thread_id\":\"root-thread\""));
    assert!(encoded.contains("\"parent_thread_id\":\"root-thread\""));
    assert!(encoded.contains("\"admission_id\":\"admission-report_validated\""));
    assert!(encoded.contains("\"admission_status\":\"Accepted\""));
    assert!(encoded.contains("\"admission_reason_code\":\"report_validated\""));
    assert!(encoded.contains("\"evidence_ref\":\"queue://reports/report-1\""));
}

#[test]
fn list_event_records_consistency_warnings_when_caller_context_mismatches_children() {
    let mut ledger = ledger();
    ledger
        .spawn(spawn_request("child-thread-1", "reader"))
        .expect("spawn should succeed");
    let children = ledger.list_children("root-thread");

    let event = SubagentTreeEventBuilder::new(CREATED_AT).list_event(
        "wrong-root",
        "wrong-parent",
        &children,
    );

    assert_eq!(event.consistency_warnings.len(), 2);
    assert!(event.consistency_warnings[0]
        .contains("child_root_mismatch child_thread_id=child-thread-1"));
    assert!(event.consistency_warnings[1]
        .contains("child_parent_mismatch child_thread_id=child-thread-1"));

    let encoded = serde_json::to_string(&event).expect("event should serialize");
    assert!(encoded.contains("\"consistency_warnings\""));
    assert!(encoded.contains("child_root_mismatch"));
    assert!(encoded.contains("child_parent_mismatch"));
}
