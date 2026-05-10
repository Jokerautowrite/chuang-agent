use std::path::PathBuf;

use chuang_agent::unified_execution_slot::{
    redact_secret_like, EnvValueState, ExecutionEnvironmentSnapshot, ExecutionFailureKind,
    ExecutionOutputPreview, ExecutionRequest, FakeExecutionOutcome,
    FakeUnifiedExecutionOrchestrator, UnifiedExecutionOrchestrator, REDACTED_SECRET_LIKE_ENV,
    REDACTED_SECRET_LIKE_PREVIEW, REDACTED_SECRET_LIKE_REASON,
};

fn request() -> ExecutionRequest {
    ExecutionRequest::new(
        "code_execute",
        "call-1",
        "/workspace/project",
        "tool.code_execute.local",
        "workspace-write-network-denied",
        true,
    )
    .with_environment(ExecutionEnvironmentSnapshot::from_pairs_redacted([
        ("PATH", "/usr/bin"),
        ("EMPTY_VALUE", ""),
    ]))
}

#[test]
fn execution_request_and_success_receipt_are_serializable_and_complete() {
    let orchestrator = FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::success("ok\n"))
        .with_timestamps("2026-05-11T00:00:00Z", "2026-05-11T00:00:01Z");
    let result = orchestrator.execute(request());

    assert!(result.success);
    assert_eq!(result.schema_version, 1);
    assert_eq!(result.tool_name, "code_execute");
    assert_eq!(result.call_id, "call-1");
    assert_eq!(result.cwd, PathBuf::from("/workspace/project"));
    assert_eq!(result.audit_label, "tool.code_execute.local");
    assert_eq!(result.sandbox_summary, "workspace-write-network-denied");
    assert!(result.adapter_available);
    assert_eq!(result.started_at, "2026-05-11T00:00:00Z");
    assert_eq!(result.completed_at, "2026-05-11T00:00:01Z");
    assert!(result.failure.is_none());
    assert_eq!(result.stdout.text, "ok\n");
    assert_eq!(result.environment.vars.len(), 2);
    assert_eq!(result.environment.vars[0].name, "PATH");
    assert_eq!(result.environment.vars[0].value_state, EnvValueState::Set);
    assert_eq!(
        result.environment.vars[0].value_preview.as_deref(),
        Some("<set>")
    );
    assert_eq!(
        result.environment.vars[1].value_state,
        EnvValueState::Missing
    );

    let value = serde_json::to_value(&result).expect("result should serialize");
    assert_eq!(value["success"], true);
    assert_eq!(value["stdout"]["text"], "ok\n");
    assert_eq!(value["environment"]["vars"][0]["value_state"], "set");
}

#[test]
fn execution_request_is_serializable() {
    let req = request();
    let value = serde_json::to_value(&req).expect("request should serialize");

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool_name"], "code_execute");
    assert_eq!(value["call_id"], "call-1");
    assert_eq!(value["environment"]["vars"][0]["name"], "PATH");
}

#[test]
fn output_preview_limits_text_without_splitting_secret_state() {
    let preview = ExecutionOutputPreview::capture("abcdef", 3);

    assert_eq!(preview.text, "abc");
    assert_eq!(preview.original_bytes, 6);
    assert_eq!(preview.preview_bytes, 3);
    assert_eq!(preview.limit_bytes, 3);
    assert!(preview.truncated);
    assert!(!preview.redacted);
}

#[test]
fn output_preview_with_zero_limit_is_structured_and_truncated() {
    let preview = ExecutionOutputPreview::capture("abcdef", 0);

    assert_eq!(preview.text, "");
    assert_eq!(preview.original_bytes, 6);
    assert_eq!(preview.preview_bytes, 0);
    assert_eq!(preview.limit_bytes, 0);
    assert!(preview.truncated);
    assert!(!preview.redacted);
}

#[test]
fn output_preview_redacts_secret_like_text() {
    let preview = redact_secret_like("Authorization: Bearer live-token-value");

    assert_eq!(preview.text, REDACTED_SECRET_LIKE_PREVIEW);
    assert!(preview.redacted);
    assert!(!preview.truncated);
    assert!(!preview.text.contains("live-token-value"));
}

#[test]
fn environment_snapshot_redacts_secret_like_values_and_never_stores_raw_secret() {
    let snapshot = ExecutionEnvironmentSnapshot::from_pairs_redacted([
        ("OPENAI_API_KEY", "sk-live-secret"),
        ("NORMAL_FLAG", "enabled"),
    ]);

    assert!(snapshot.rejected_secret_like_env);
    assert_eq!(snapshot.vars[0].name, "OPENAI_API_KEY");
    assert_eq!(snapshot.vars[0].value_state, EnvValueState::Redacted);
    assert_eq!(
        snapshot.vars[0].value_preview.as_deref(),
        Some(REDACTED_SECRET_LIKE_ENV)
    );
    assert_eq!(snapshot.vars[1].name, "NORMAL_FLAG");
    assert_eq!(snapshot.vars[1].value_state, EnvValueState::Set);

    let rendered = serde_json::to_string(&snapshot).expect("snapshot should serialize");
    assert!(!rendered.contains("sk-live-secret"));
    assert!(!rendered.contains("enabled"));
}

#[test]
fn fake_orchestrator_returns_adapter_unavailable_failure() {
    let result = FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::adapter_unavailable(
        "actuator adapter unavailable",
    ))
    .execute(ExecutionRequest::new(
        "open_app",
        "call-open",
        "/workspace/project",
        "actuator.open_app.local",
        "desktop-live-gated",
        false,
    ));

    let failure = result.failure.expect("failure should be present");
    assert!(!result.success);
    assert_eq!(failure.kind, ExecutionFailureKind::AdapterUnavailable);
    assert_eq!(failure.code, "adapter_unavailable");
    assert!(failure.retryable);
    assert_eq!(failure.reason, "actuator adapter unavailable");
    assert!(!result.adapter_available);
}

#[test]
fn fake_orchestrator_returns_permission_denied_failure() {
    let result = FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::permission_denied(
        "policy denied external send",
    ))
    .execute(request());

    let failure = result.failure.expect("failure should be present");
    assert!(!result.success);
    assert_eq!(failure.kind, ExecutionFailureKind::PermissionDenied);
    assert_eq!(failure.code, "permission_denied");
    assert!(!failure.retryable);
    assert_eq!(result.stderr.text, "policy denied external send");
}

#[test]
fn fake_orchestrator_returns_timeout_failure() {
    let result =
        FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::timeout("execution timed out"))
            .execute(request());

    let failure = result.failure.expect("failure should be present");
    assert!(!result.success);
    assert_eq!(failure.kind, ExecutionFailureKind::Timeout);
    assert_eq!(failure.code, "timeout");
    assert!(failure.retryable);
}

#[test]
fn fake_orchestrator_returns_invalid_output_with_redacted_preview() {
    let result = FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::invalid_output(
        "tool output was not valid json",
        "password=secret-value",
    ))
    .execute(request());

    let failure = result.failure.expect("failure should be present");
    assert!(!result.success);
    assert_eq!(failure.kind, ExecutionFailureKind::InvalidOutput);
    assert_eq!(failure.code, "invalid_output");
    assert!(!failure.retryable);
    assert_eq!(result.stdout.text, REDACTED_SECRET_LIKE_PREVIEW);
    assert!(!result.stdout.text.contains("secret-value"));
}

#[test]
fn failure_stderr_is_redacted_when_reason_looks_secret_like() {
    let result = FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::permission_denied(
        "Authorization: Bearer very-secret-token",
    ))
    .execute(request());

    let failure = result.failure.expect("failure should be present");
    assert_eq!(failure.kind, ExecutionFailureKind::PermissionDenied);
    assert_eq!(failure.code, "permission_denied");
    assert_eq!(failure.reason, REDACTED_SECRET_LIKE_REASON);
    assert!(failure.reason_redacted);
    assert!(!failure.reason.contains("very-secret-token"));
    assert_eq!(result.stderr.text, REDACTED_SECRET_LIKE_REASON);
    assert!(!result.stderr.redacted);
    assert!(!result.stderr.text.contains("very-secret-token"));
}

#[test]
fn fake_orchestrator_applies_preview_limit_to_success_output() {
    let result = FakeUnifiedExecutionOrchestrator::new(FakeExecutionOutcome::success("abcdef"))
        .with_preview_limit(4)
        .execute(request());

    assert!(result.success);
    assert_eq!(result.stdout.text, "abcd");
    assert!(result.stdout.truncated);
    assert_eq!(result.stdout.original_bytes, 6);
    assert_eq!(result.stdout.preview_bytes, 4);
}
