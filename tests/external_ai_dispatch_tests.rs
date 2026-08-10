use std::collections::BTreeMap;

use chuang_agent::external_ai_dispatch::{
    build_external_ai_dispatch, build_live_responder_request, execute_external_ai_dispatch,
    parse_live_platform, validate_request, ExternalAiDispatchRequest,
};
use chuang_agent::responder::{
    ProviderAdapterResponder, ProviderAdapterResponse, ProviderIdentity, ResponderRequest,
};

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

#[test]
fn external_ai_dispatch_accepts_supported_live_platforms() {
    for platform in ["opencodex", "openai-compatible", "opencodex:sub2/gpt-5.5"] {
        let mut request =
            ExternalAiDispatchRequest::new(platform, "bounded task", "bounded context");
        request.dry_run = false;
        validate_request(&request).expect("supported live platform should validate");
    }
    assert_eq!(
        parse_live_platform("opencodex:sub2/gpt-5.5")
            .unwrap()
            .model
            .as_deref(),
        Some("sub2/gpt-5.5")
    );
}

#[test]
fn external_ai_dispatch_builds_live_request_for_fake_provider() {
    struct FakeProvider;
    impl ProviderAdapterResponder for FakeProvider {
        fn identity(&self) -> ProviderIdentity {
            ProviderIdentity {
                provider_id: "opencodex".into(),
                model_name: "sub2/test".into(),
            }
        }

        fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
            assert!(request.user_input.contains("Task:\nbounded task"));
            ProviderAdapterResponse {
                body: "model answer".into(),
                trace: "fake".into(),
                finish_reason: Some("stop".into()),
                extra_meta: BTreeMap::from([(
                    String::from("provider_response_ok"),
                    String::from("true"),
                )]),
            }
        }
    }

    let mut request =
        ExternalAiDispatchRequest::new("opencodex", "bounded task", "bounded context");
    request.dry_run = false;
    let output =
        execute_external_ai_dispatch(request, &FakeProvider).expect("live dispatch should execute");
    assert!(output.connects_real_service);
    assert!(!output.dry_run);
    assert_eq!(output.adapter, "opencodex_openai_compatible");
    assert_eq!(output.result.result.summary, "model answer");
    assert!(!output.result.result.follow_up_needed);
}

#[test]
fn external_ai_dispatch_prompt_contains_task_context_and_session_hint() {
    let request = ExternalAiDispatchRequest::new("opencodex", "task", "context");
    let mut request = request;
    request.session_hint = Some("session-1".into());
    let prompt = build_live_responder_request(
        &chuang_agent::external_ai_dispatch::UnifiedIdentityEngineRequest {
            platform: request.platform,
            task: request.task,
            context: request.context,
            session_hint: request.session_hint,
            timeout_ms: request.timeout_ms,
            audit: request.audit,
        },
    );
    assert!(prompt.user_input.contains("Context:\ncontext"));
    assert!(prompt.user_input.contains("Session hint: session-1"));
}
