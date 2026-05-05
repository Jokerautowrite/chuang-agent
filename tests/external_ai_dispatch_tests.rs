use chuang_agent::external_ai_dispatch::{build_external_ai_dispatch, ExternalAiDispatchRequest};

#[test]
fn external_ai_dispatch_builds_dry_run_unified_identity_request() {
    let mut request = ExternalAiDispatchRequest::new(
        "kimi",
        "review memory architecture",
        "bounded project context",
    );
    request.session_hint = Some("session-1".to_string());

    let output = build_external_ai_dispatch(request).expect("dispatch should build");

    assert_eq!(output.adapter, "unified_identity_engine");
    assert!(output.dry_run);
    assert!(!output.connects_real_service);
    assert!(!output.writes_memory);
    assert_eq!(output.request.platform, "kimi");
    assert_eq!(output.request.session_hint.as_deref(), Some("session-1"));
    assert_eq!(output.request.timeout_ms, 60_000);
    assert!(output.request.audit);
    assert!(output.result.success);
    assert_eq!(output.result.quality, "acceptable");
    assert!(output.result.audit_id.starts_with("external-ai-kimi-"));
    assert!(output.result.failure_class.is_none());
}

#[test]
fn external_ai_dispatch_rejects_secret_like_context() {
    let err = build_external_ai_dispatch(ExternalAiDispatchRequest::new(
        "kimi",
        "review",
        "contains token value",
    ))
    .expect_err("secret-like content should fail");

    assert_eq!(err.field, "context");
}
