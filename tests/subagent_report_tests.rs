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
        truncated: false,
    }
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
        "truncated":false
    }"#;

    let result = validator.validate(raw);

    assert!(result.is_ok());
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
