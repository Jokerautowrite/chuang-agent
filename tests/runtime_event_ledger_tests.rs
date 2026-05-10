use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../src/runtime_event_ledger.rs"]
mod runtime_event_ledger;

use runtime_event_ledger::{
    InMemoryRuntimeEventLedger, JsonlRuntimeEventLedger, RuntimeEvent, RuntimeEventKind,
    RuntimeEventLedger, RuntimeEventLedgerError, RuntimeRiskDecision,
};

fn fixed_at() -> &'static str {
    "2026-05-11T00:00:00Z"
}

fn event(kind: RuntimeEventKind) -> RuntimeEvent {
    RuntimeEvent::at(kind, "thread-1", fixed_at()).with_turn_id("turn-1")
}

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "chuang-runtime-event-ledger-{name}-{nanos}-{}.jsonl",
        std::process::id()
    ))
}

#[test]
fn runtime_event_serializes_supported_m1_event_types() {
    let cases = [
        (RuntimeEventKind::ThreadStarted, "thread_started"),
        (RuntimeEventKind::TurnStarted, "turn_started"),
        (RuntimeEventKind::ToolStarted, "tool_started"),
        (RuntimeEventKind::ToolFinished, "tool_finished"),
        (RuntimeEventKind::ApprovalRequested, "approval_requested"),
        (RuntimeEventKind::ApprovalResolved, "approval_resolved"),
        (RuntimeEventKind::SubagentSpawned, "subagent_spawned"),
        (RuntimeEventKind::SubagentReported, "subagent_reported"),
        (RuntimeEventKind::TurnCompleted, "turn_completed"),
        (RuntimeEventKind::TurnFailed, "turn_failed"),
    ];

    for (kind, expected_type) in cases {
        let value = serde_json::to_value(RuntimeEvent::at(kind, "thread-1", fixed_at()))
            .expect("event should serialize");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["event_type"], expected_type);
        assert_eq!(value["thread_id"], "thread-1");
        assert_eq!(value["created_at"], fixed_at());
        assert_eq!(value["turn_id"], serde_json::Value::Null);
        assert_eq!(value["call_id"], serde_json::Value::Null);
        assert_eq!(value["risk_decision"], serde_json::Value::Null);
        assert_eq!(value["evidence_ref"], serde_json::Value::Null);
    }
}

#[test]
fn runtime_event_carries_turn_call_risk_and_evidence_fields() {
    let event = RuntimeEvent::at(RuntimeEventKind::ToolStarted, "thread-a", fixed_at())
        .with_turn_id("turn-a")
        .with_call_id("call-a")
        .with_risk_decision(
            RuntimeRiskDecision::new("allow", "read-only tool")
                .with_policy_ref("policy://local-ga/read-only"),
        )
        .with_evidence_ref("ledger://tool/call-a/start");

    let value = serde_json::to_value(&event).expect("event should serialize");
    assert_eq!(value["event_type"], "tool_started");
    assert_eq!(value["thread_id"], "thread-a");
    assert_eq!(value["turn_id"], "turn-a");
    assert_eq!(value["call_id"], "call-a");
    assert_eq!(value["risk_decision"]["decision"], "allow");
    assert_eq!(value["risk_decision"]["reason"], "read-only tool");
    assert_eq!(
        value["risk_decision"]["policy_ref"],
        "policy://local-ga/read-only"
    );
    assert_eq!(value["evidence_ref"], "ledger://tool/call-a/start");

    let decoded: RuntimeEvent = serde_json::from_value(value).expect("event should deserialize");
    assert_eq!(decoded, event);
}

#[test]
fn runtime_event_new_stamps_current_time_and_default_schema() {
    let event = RuntimeEvent::new(RuntimeEventKind::ThreadStarted, "thread-now");

    assert_eq!(event.schema_version, 1);
    assert_eq!(event.event_type, RuntimeEventKind::ThreadStarted);
    assert_eq!(event.thread_id, "thread-now");
    assert!(event.created_at.ends_with('Z'));
}

#[test]
fn in_memory_runtime_event_ledger_appends_and_lists_events_in_order() {
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let started = event(RuntimeEventKind::TurnStarted);
    let completed = event(RuntimeEventKind::TurnCompleted).with_evidence_ref("turn://done");

    ledger
        .append(started.clone())
        .expect("append started should succeed");
    ledger
        .append(completed.clone())
        .expect("append completed should succeed");

    assert_eq!(
        ledger.list().expect("list should succeed"),
        vec![started, completed]
    );
    assert_eq!(ledger.into_events().len(), 2);
}

#[test]
fn jsonl_runtime_event_ledger_appends_one_event_per_line_and_replays() {
    let path = temp_path("roundtrip");
    let mut ledger = JsonlRuntimeEventLedger::new(&path);
    assert_eq!(ledger.path(), path.as_path());
    let thread_started =
        RuntimeEvent::at(RuntimeEventKind::ThreadStarted, "thread-jsonl", fixed_at());
    let approval_requested = RuntimeEvent::at(
        RuntimeEventKind::ApprovalRequested,
        "thread-jsonl",
        "2026-05-11T00:00:01Z",
    )
    .with_turn_id("turn-jsonl")
    .with_call_id("approval-1")
    .with_risk_decision(RuntimeRiskDecision::new(
        "prompt",
        "external send requires approval",
    ))
    .with_evidence_ref("approval://request/approval-1");

    ledger
        .append(thread_started.clone())
        .expect("append thread should succeed");
    ledger
        .append(approval_requested.clone())
        .expect("append approval should succeed");

    let raw = fs::read_to_string(&path).expect("jsonl file should read");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"event_type\":\"thread_started\""));
    assert!(lines[1].contains("\"event_type\":\"approval_requested\""));

    let replay = JsonlRuntimeEventLedger::new(&path)
        .list()
        .expect("jsonl replay should succeed");
    assert_eq!(replay, vec![thread_started, approval_requested]);
}

#[test]
fn jsonl_runtime_event_ledger_missing_file_lists_empty_without_fallback_runtime() {
    let path = temp_path("missing");
    let ledger = JsonlRuntimeEventLedger::new(&path);

    assert_eq!(
        ledger.list().expect("missing file should be empty"),
        Vec::new()
    );
}

#[test]
fn jsonl_runtime_event_ledger_reports_bad_line_with_path_and_line_number() {
    let path = temp_path("bad-line");
    let valid = serde_json::to_string(&RuntimeEvent::at(
        RuntimeEventKind::ThreadStarted,
        "thread-bad-line",
        fixed_at(),
    ))
    .expect("valid event should serialize");
    fs::write(&path, format!("{valid}\nnot-json\n")).expect("bad jsonl fixture should write");
    let ledger = JsonlRuntimeEventLedger::new(&path);

    let error = ledger.list().expect_err("bad jsonl should fail");
    match error {
        RuntimeEventLedgerError::DeserializeEvent {
            path: error_path,
            line,
            ..
        } => {
            assert_eq!(error_path, path);
            assert_eq!(line, 2);
        }
        other => panic!("expected deserialize error, got {other:?}"),
    }
}
