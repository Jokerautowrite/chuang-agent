use chuang_agent::common::{AgentId, ReportId, TaskId, Timestamp};
use chuang_agent::subagent_report::{
    ArtifactKind, ArtifactRef, ExecutionStatus, ReportBuilder, ReportRejectReason, ReportValidator,
    ResourceUsage, SubagentReport, SubagentReportBuilder, SubagentReportValidator,
};

fn sample_report() -> SubagentReport {
    SubagentReport {
        schema_version: "1.0.0".to_string(),
        report_id: ReportId("report-1".to_string()),
        task_id: TaskId("task-1".to_string()),
        agent_id: AgentId("agent-1".to_string()),
        parent_agent_id: None,
        status: ExecutionStatus::Success,
        started_at: Timestamp("2026-04-30T10:30:00.123Z".to_string()),
        finished_at: Timestamp("2026-04-30T10:31:00.123Z".to_string()),
        summary: "ok".to_string(),
        exit_code: Some(0),
        stdout_preview: Some("stdout".to_string()),
        stderr_preview: None,
        resource_usage: ResourceUsage::default(),
        artifacts: vec![ArtifactRef {
            kind: ArtifactKind::Log,
            locator: "logs/run.log".to_string(),
            description: None,
        }],
        replay_ref: None,
        context_debug: Some(chuang_agent::subagent_report::ContextDebugSummary {
            dropped_segment_ids: vec!["working-user-input".to_string()],
            drop_reasons: vec![chuang_agent::subagent_report::ContextDropReasonSummary {
                segment_id: "working-user-input".to_string(),
                reason: "budget_limit".to_string(),
            }],
            budget_exceeded: true,
            budget_exceeded_reasons: vec!["min_working_tokens_unmet".to_string()],
            working_reservation: Some(chuang_agent::subagent_report::WorkingReservationDebug {
                reserved_segment_id: "working-user-input".to_string(),
                reserved_tokens: 20,
                dropped_segment_ids: vec!["mem-1".to_string()],
                reason: "minimum_working_tokens".to_string(),
            }),
        }),
        truncated: false,
    }
}

#[test]
fn report_can_carry_context_debug_summary() {
    let report = sample_report();
    let debug = report.context_debug.expect("context debug should exist");

    assert_eq!(
        debug.dropped_segment_ids,
        vec!["working-user-input".to_string()]
    );
    assert_eq!(debug.drop_reasons.len(), 1);
    assert_eq!(debug.drop_reasons[0].segment_id, "working-user-input");
    assert_eq!(debug.drop_reasons[0].reason, "budget_limit");
    assert!(debug.budget_exceeded);
    assert_eq!(
        debug.budget_exceeded_reasons,
        vec!["min_working_tokens_unmet".to_string()]
    );
    let reservation = debug
        .working_reservation
        .expect("working reservation should exist");
    assert_eq!(reservation.reserved_segment_id, "working-user-input");
    assert_eq!(reservation.reason, "minimum_working_tokens");
}

#[test]
fn report_skeleton_keeps_required_fields() {
    let report = sample_report();

    assert_eq!(report.schema_version, "1.0.0");
    assert_eq!(report.summary, "ok");
    assert!(!report.truncated);
}

#[test]
fn reject_reason_can_describe_missing_field() {
    let reason = ReportRejectReason::MissingRequiredField { field: "summary" };

    assert_eq!(
        reason,
        ReportRejectReason::MissingRequiredField { field: "summary" }
    );
}

#[test]
fn validator_accepts_valid_report_bytes() {
    let validator = SubagentReportValidator::default();
    let raw = br#"{
        "schema_version":"1.0.0",
        "report_id":"report-1",
        "task_id":"task-1",
        "agent_id":"agent-1",
        "status":"Success",
        "started_at":"2026-04-30T10:30:00.123Z",
        "finished_at":"2026-04-30T10:31:00.123Z",
        "summary":"ok",
        "resource_usage":{},
        "artifacts":[],
        "context_debug":{
            "dropped_segment_ids":["working-user-input"],
            "drop_reasons":[{"segment_id":"working-user-input","reason":"budget_limit"}],
            "budget_exceeded":true,
            "budget_exceeded_reasons":["min_working_tokens_unmet"]
        },
        "truncated":false
    }"#;

    let result = validator.validate(raw);

    assert!(result.is_ok());
}

#[test]
fn subagent_report_can_roundtrip_as_json() {
    let report = sample_report();

    let encoded = serde_json::to_string(&report).expect("report should serialize");
    let decoded: SubagentReport =
        serde_json::from_str(&encoded).expect("report should deserialize");

    assert_eq!(decoded, report);
    assert!(encoded.contains("\"status\":\"Success\""));
    assert!(encoded.contains("\"report_id\":\"report-1\""));
}

#[test]
fn validator_rejects_unsupported_schema_major() {
    let validator = SubagentReportValidator::default();
    let raw = br#"{
        "schema_version":"2.0.0",
        "report_id":"report-1",
        "task_id":"task-1",
        "agent_id":"agent-1",
        "status":"Success",
        "started_at":"2026-04-30T10:30:00.123Z",
        "finished_at":"2026-04-30T10:31:00.123Z",
        "summary":"ok",
        "resource_usage":{},
        "artifacts":[],
        "truncated":false
    }"#;

    let result = validator.validate(raw);

    assert_eq!(
        result,
        Err(ReportRejectReason::UnsupportedSchemaVersion {
            required_major: 1,
            current: "2.0.0".to_string(),
        })
    );
}

#[test]
fn validator_rejects_missing_required_summary() {
    let validator = SubagentReportValidator::default();
    let raw = br#"{
        "schema_version":"1.0.0",
        "report_id":"report-1",
        "task_id":"task-1",
        "agent_id":"agent-1",
        "status":"Success",
        "started_at":"2026-04-30T10:30:00.123Z",
        "finished_at":"2026-04-30T10:31:00.123Z",
        "resource_usage":{},
        "artifacts":[],
        "truncated":false
    }"#;

    let result = validator.validate(raw);

    assert_eq!(
        result,
        Err(ReportRejectReason::MissingRequiredField { field: "summary" })
    );
}

#[test]
fn validator_rejects_invalid_timestamp() {
    let validator = SubagentReportValidator::default();
    let raw = br#"{
        "schema_version":"1.0.0",
        "report_id":"report-1",
        "task_id":"task-1",
        "agent_id":"agent-1",
        "status":"Success",
        "started_at":"bad-timestamp",
        "finished_at":"2026-04-30T10:31:00.123Z",
        "summary":"ok",
        "resource_usage":{},
        "artifacts":[],
        "truncated":false
    }"#;

    let result = validator.validate(raw);

    assert_eq!(
        result,
        Err(ReportRejectReason::InvalidTimestampFormat {
            field: "started_at",
            found: "bad-timestamp".to_string(),
        })
    );
}

#[test]
fn validator_rejects_payload_over_size_limit() {
    let validator = SubagentReportValidator::new(32);
    let raw = br#"{
        "schema_version":"1.0.0",
        "report_id":"report-1",
        "task_id":"task-1",
        "agent_id":"agent-1",
        "status":"Success",
        "started_at":"2026-04-30T10:30:00.123Z",
        "finished_at":"2026-04-30T10:31:00.123Z",
        "summary":"ok",
        "resource_usage":{},
        "artifacts":[],
        "truncated":false
    }"#;

    let result = validator.validate(raw);

    assert_eq!(
        result,
        Err(ReportRejectReason::SizeLimitExceeded {
            limit_bytes: 32,
            actual: raw.len(),
        })
    );
}

#[test]
fn builder_truncates_previews_and_marks_report() {
    let report = sample_report();
    let builder = SubagentReportBuilder::new(report);

    let built = builder.truncate_previews(4).build();

    assert_eq!(built.stdout_preview, Some("stdo".to_string()));
    assert_eq!(built.stderr_preview, None);
    assert!(built.truncated);
}

#[test]
fn builder_can_build_report_from_runtime_input() {
    let built =
        SubagentReportBuilder::from_runtime(chuang_agent::subagent_report::RuntimeReportInput {
            report_id: "report-runtime-1".to_string(),
            task_id: "task-runtime-1".to_string(),
            agent_id: "agent-runtime-1".to_string(),
            parent_agent_id: Some("agent-parent-1".to_string()),
            summary: "runtime summary ok".to_string(),
            response_body: "runtime body".to_string(),
            response_trace: "runtime trace".to_string(),
            dropped_segment_ids: vec!["working-user-input".to_string()],
            drop_reasons: vec![("working-user-input".to_string(), "budget_limit".to_string())],
            budget_exceeded: true,
            budget_exceeded_reasons: vec!["min_working_tokens_unmet".to_string()],
            working_reservation: Some(chuang_agent::subagent_report::WorkingReservationDebug {
                reserved_segment_id: "working-user-input".to_string(),
                reserved_tokens: 20,
                dropped_segment_ids: vec!["mem-1".to_string()],
                reason: "minimum_working_tokens".to_string(),
            }),
        })
        .build();

    assert_eq!(built.report_id.0, "report-runtime-1");
    assert_eq!(built.task_id.0, "task-runtime-1");
    assert_eq!(built.agent_id.0, "agent-runtime-1");
    assert_eq!(built.parent_agent_id.expect("parent").0, "agent-parent-1");
    assert_eq!(built.summary, "runtime summary ok");
    assert_eq!(built.stdout_preview, Some("runtime body".to_string()));
    let debug = built.context_debug.expect("context debug should exist");
    assert_eq!(
        debug.dropped_segment_ids,
        vec!["working-user-input".to_string()]
    );
    assert!(debug.budget_exceeded);
    let reservation = debug
        .working_reservation
        .expect("working reservation should exist");
    assert_eq!(reservation.reserved_segment_id, "working-user-input");
    assert_eq!(reservation.reason, "minimum_working_tokens");
}
