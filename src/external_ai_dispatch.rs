//! `external_ai_dispatch` 模块。公开接口：struct ExternalAiDispatchRequest, ExternalAiDispatchOutput, UnifiedIdentityEngineRequest, UnifiedIdentityEngineResult, ExternalAiStructuredResult, ExternalAiDispatchError；fn new, build_external_ai_dispatch。

use serde::Serialize;
use std::time::Instant;

use crate::responder::{ProviderAdapterResponder, ResponderRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAiDispatchRequest {
    pub platform: String,
    pub task: String,
    pub context: String,
    pub session_hint: Option<String>,
    pub timeout_ms: u64,
    pub audit: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalAiDispatchOutput {
    pub adapter: String,
    pub dry_run: bool,
    pub connects_real_service: bool,
    pub writes_memory: bool,
    pub request: UnifiedIdentityEngineRequest,
    pub result: UnifiedIdentityEngineResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnifiedIdentityEngineRequest {
    pub platform: String,
    pub task: String,
    pub context: String,
    pub session_hint: Option<String>,
    pub timeout_ms: u64,
    pub audit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnifiedIdentityEngineResult {
    pub success: bool,
    pub platform: String,
    pub audit_id: String,
    pub quality: String,
    pub result: ExternalAiStructuredResult,
    pub duration_ms: u64,
    pub failure_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalAiStructuredResult {
    pub summary: String,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
    pub follow_up_needed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAiDispatchError {
    pub field: String,
    pub message: String,
}

impl ExternalAiDispatchRequest {
    pub fn new(
        platform: impl Into<String>,
        task: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self {
            platform: platform.into(),
            task: task.into(),
            context: context.into(),
            session_hint: None,
            timeout_ms: 60_000,
            audit: true,
            dry_run: true,
        }
    }
}

pub fn build_external_ai_dispatch(
    request: ExternalAiDispatchRequest,
) -> Result<ExternalAiDispatchOutput, ExternalAiDispatchError> {
    validate_request(&request)?;
    let engine_request = build_engine_request(&request);
    let audit_id = build_audit_id(&engine_request);
    Ok(ExternalAiDispatchOutput {
        adapter: "unified_identity_engine".to_string(),
        dry_run: request.dry_run,
        connects_real_service: false,
        writes_memory: false,
        result: UnifiedIdentityEngineResult {
            success: true,
            platform: engine_request.platform.clone(),
            audit_id,
            quality: "acceptable".to_string(),
            result: ExternalAiStructuredResult {
                summary: "dry-run external AI dispatch request prepared for subagent review"
                    .to_string(),
                evidence: vec![
                    "no external platform connection was attempted".to_string(),
                    "request is bounded for unified identity engine adapter".to_string(),
                ],
                risks: vec![
                    "live platform execution remains disabled until an audited adapter is configured"
                        .to_string(),
                ],
                follow_up_needed: false,
            },
            duration_ms: 0,
            failure_class: None,
        },
        request: engine_request,
    })
}

/// Execute a live request through the configured OpenAI-compatible adapter.
/// Credentials and transport details remain inside the provider adapter.
pub fn execute_external_ai_dispatch(
    request: ExternalAiDispatchRequest,
    provider: &dyn ProviderAdapterResponder,
) -> Result<ExternalAiDispatchOutput, ExternalAiDispatchError> {
    validate_request(&request)?;
    if request.dry_run {
        return build_external_ai_dispatch(request);
    }
    let platform = parse_live_platform(&request.platform)?;
    let identity = provider.identity();
    if platform
        .model
        .as_deref()
        .is_some_and(|model| model != identity.model_name)
    {
        return Err(ExternalAiDispatchError::new(
            "platform",
            "requested model does not match configured live provider",
        ));
    }
    let engine_request = build_engine_request(&request);
    let audit_id = build_audit_id(&engine_request);
    let provider_request = build_live_responder_request(&engine_request);
    let started = Instant::now();
    let response = provider.respond(&provider_request);
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let success = response
        .extra_meta
        .get("provider_response_ok")
        .is_some_and(|value| value == "true");
    let failure_class = (!success).then(|| {
        response
            .extra_meta
            .get("provider_error_class")
            .cloned()
            .unwrap_or_else(|| "provider_error".to_string())
    });
    Ok(ExternalAiDispatchOutput {
        adapter: "opencodex_openai_compatible".to_string(),
        dry_run: false,
        connects_real_service: true,
        writes_memory: false,
        request: engine_request.clone(),
        result: UnifiedIdentityEngineResult {
            success,
            platform: engine_request.platform,
            audit_id,
            quality: if success { "acceptable" } else { "failed" }.to_string(),
            result: ExternalAiStructuredResult {
                summary: response.body,
                evidence: vec![
                    format!(
                        "provider={} model={}",
                        identity.provider_id, identity.model_name
                    ),
                    "request completed through the configured OpenAI-compatible adapter"
                        .to_string(),
                ],
                risks: if success {
                    Vec::new()
                } else {
                    vec!["external provider did not return a successful model response".to_string()]
                },
                follow_up_needed: !success,
            },
            duration_ms,
            failure_class,
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveExternalAiPlatform {
    pub kind: String,
    pub model: Option<String>,
}

pub fn parse_live_platform(raw: &str) -> Result<LiveExternalAiPlatform, ExternalAiDispatchError> {
    let trimmed = raw.trim();
    let (kind, model) = match trimmed.split_once(':') {
        Some((kind, model)) => (kind, Some(model.trim())),
        None => (trimmed, None),
    };
    if !matches!(kind, "opencodex" | "openai-compatible") {
        return Err(ExternalAiDispatchError::new(
            "platform",
            "live platform must be opencodex or openai-compatible",
        ));
    }
    if model.is_some_and(str::is_empty) {
        return Err(ExternalAiDispatchError::new(
            "platform",
            "platform model override must not be empty",
        ));
    }
    Ok(LiveExternalAiPlatform {
        kind: kind.to_string(),
        model: model.map(str::to_string),
    })
}

pub fn build_live_responder_request(request: &UnifiedIdentityEngineRequest) -> ResponderRequest {
    let session_hint = request
        .session_hint
        .as_deref()
        .map(|value| format!("\nSession hint: {value}"))
        .unwrap_or_default();
    ResponderRequest {
        prompt: "You are an external worker dispatched by Chuang. Complete only the bounded task. Return a concise result with factual evidence, risks, and required follow-up. Do not claim actions you did not perform.".to_string(),
        user_input: format!(
            "Task:\n{}\n\nContext:\n{}{}",
            request.task, request.context, session_hint
        ),
        recall_hit_count: 0,
    }
}

fn build_engine_request(request: &ExternalAiDispatchRequest) -> UnifiedIdentityEngineRequest {
    UnifiedIdentityEngineRequest {
        platform: request.platform.trim().to_string(),
        task: request.task.trim().to_string(),
        context: request.context.trim().to_string(),
        session_hint: request
            .session_hint
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        timeout_ms: request.timeout_ms,
        audit: request.audit,
    }
}

fn build_audit_id(request: &UnifiedIdentityEngineRequest) -> String {
    format!(
        "external-ai-{}-{:016x}",
        sanitize_id(&request.platform),
        stable_hash(&format!(
            "{}\n{}\n{}",
            request.platform, request.task, request.context
        ))
    )
}

pub fn validate_request(
    request: &ExternalAiDispatchRequest,
) -> Result<(), ExternalAiDispatchError> {
    require_non_empty("platform", &request.platform)?;
    require_non_empty("task", &request.task)?;
    require_non_empty("context", &request.context)?;
    if request.timeout_ms == 0 {
        return Err(ExternalAiDispatchError::new(
            "timeout_ms",
            "timeout must be greater than zero",
        ));
    }
    if !request.dry_run {
        parse_live_platform(&request.platform)?;
    }
    for (field, value) in [
        ("platform", request.platform.as_str()),
        ("task", request.task.as_str()),
        ("context", request.context.as_str()),
    ] {
        reject_secret_like(field, value)?;
    }
    Ok(())
}

fn require_non_empty(field: &str, value: &str) -> Result<(), ExternalAiDispatchError> {
    if value.trim().is_empty() {
        Err(ExternalAiDispatchError::new(
            field,
            "field must not be empty",
        ))
    } else {
        Ok(())
    }
}

fn reject_secret_like(field: &str, value: &str) -> Result<(), ExternalAiDispatchError> {
    let lower = value.to_ascii_lowercase();
    if [
        "secret",
        "token",
        "password",
        "private_key",
        "cookie",
        ".env",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        Err(ExternalAiDispatchError::new(
            field,
            "external AI dispatch must not include secret-like content",
        ))
    } else {
        Ok(())
    }
}

fn sanitize_id(raw: &str) -> String {
    let value = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if value.is_empty() {
        "platform".to_string()
    } else {
        value
    }
}

fn stable_hash(raw: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in raw.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

impl ExternalAiDispatchError {
    fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
        }
    }
}
