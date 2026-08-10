//! `external_ai_dispatch` 模块。公开接口：struct ExternalAiDispatchRequest, ExternalAiDispatchOutput, UnifiedIdentityEngineRequest, UnifiedIdentityEngineResult, ExternalAiStructuredResult, ExternalAiDispatchError；fn new, build_external_ai_dispatch。

use serde::Serialize;

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
    let engine_request = UnifiedIdentityEngineRequest {
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
    };
    let audit_id = format!(
        "external-ai-{}-{:016x}",
        sanitize_id(&engine_request.platform),
        stable_hash(&format!(
            "{}\n{}\n{}",
            engine_request.platform, engine_request.task, engine_request.context
        ))
    );
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

fn validate_request(request: &ExternalAiDispatchRequest) -> Result<(), ExternalAiDispatchError> {
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
        return Err(ExternalAiDispatchError::new(
            "dry_run",
            "live external AI dispatch is not enabled by this adapter",
        ));
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
