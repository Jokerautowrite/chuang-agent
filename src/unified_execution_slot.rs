//! `unified_execution_slot` 模块。公开接口：trait UnifiedExecutionOrchestrator；struct ExecutionRequest, ExecutionResult, ExecutionFailure, ExecutionOutputPreview, ExecutionEnvironmentSnapshot, ExecutionEnvVarSnapshot, FakeUnifiedExecutionOrchestrator；enum ExecutionFailureKind, EnvValueState, FakeExecutionOutcome；fn new, with_environment, code, capture, empty, from_pairs_redacted, with_preview_limit, with_timestamps；const UNIFIED_EXECUTION_SCHEMA_VERSION, DEFAULT_OUTPUT_PREVIEW_LIMIT, REDACTED_SECRET_LIKE_PREVIEW, REDACTED_SECRET_LIKE_ENV, REDACTED_SECRET_LIKE_REASON。

use std::path::PathBuf;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub const UNIFIED_EXECUTION_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_OUTPUT_PREVIEW_LIMIT: usize = 8_000;
pub const REDACTED_SECRET_LIKE_PREVIEW: &str = "[redacted: secret-like execution output]";
pub const REDACTED_SECRET_LIKE_ENV: &str = "<redacted: secret-like env>";
pub const REDACTED_SECRET_LIKE_REASON: &str = "[redacted: execution failure reason]";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub schema_version: u16,
    pub tool_name: String,
    pub call_id: String,
    pub cwd: PathBuf,
    pub audit_label: String,
    pub sandbox_summary: String,
    pub adapter_available: bool,
    pub environment: ExecutionEnvironmentSnapshot,
}

impl ExecutionRequest {
    pub fn new(
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        cwd: impl Into<PathBuf>,
        audit_label: impl Into<String>,
        sandbox_summary: impl Into<String>,
        adapter_available: bool,
    ) -> Self {
        Self {
            schema_version: UNIFIED_EXECUTION_SCHEMA_VERSION,
            tool_name: tool_name.into(),
            call_id: call_id.into(),
            cwd: cwd.into(),
            audit_label: audit_label.into(),
            sandbox_summary: sandbox_summary.into(),
            adapter_available,
            environment: ExecutionEnvironmentSnapshot::empty(),
        }
    }

    pub fn with_environment(mut self, environment: ExecutionEnvironmentSnapshot) -> Self {
        self.environment = environment;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub schema_version: u16,
    pub tool_name: String,
    pub call_id: String,
    pub cwd: PathBuf,
    pub audit_label: String,
    pub sandbox_summary: String,
    pub adapter_available: bool,
    pub started_at: String,
    pub completed_at: String,
    pub success: bool,
    pub failure: Option<ExecutionFailure>,
    pub stdout: ExecutionOutputPreview,
    pub stderr: ExecutionOutputPreview,
    pub environment: ExecutionEnvironmentSnapshot,
}

impl ExecutionResult {
    fn from_request(
        request: &ExecutionRequest,
        started_at: impl Into<String>,
        completed_at: impl Into<String>,
        success: bool,
        failure: Option<ExecutionFailure>,
        stdout: ExecutionOutputPreview,
        stderr: ExecutionOutputPreview,
    ) -> Self {
        Self {
            schema_version: UNIFIED_EXECUTION_SCHEMA_VERSION,
            tool_name: request.tool_name.clone(),
            call_id: request.call_id.clone(),
            cwd: request.cwd.clone(),
            audit_label: request.audit_label.clone(),
            sandbox_summary: request.sandbox_summary.clone(),
            adapter_available: request.adapter_available,
            started_at: started_at.into(),
            completed_at: completed_at.into(),
            success,
            failure,
            stdout,
            stderr,
            environment: request.environment.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionFailure {
    pub kind: ExecutionFailureKind,
    pub code: String,
    pub reason: String,
    pub reason_redacted: bool,
    pub retryable: bool,
}

impl ExecutionFailure {
    pub fn new(kind: ExecutionFailureKind, reason: impl Into<String>, retryable: bool) -> Self {
        let (reason, reason_redacted) = redact_secret_like_reason(reason.into());
        Self {
            code: kind.code().to_string(),
            kind,
            reason,
            reason_redacted,
            retryable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionFailureKind {
    AdapterUnavailable,
    PermissionDenied,
    Timeout,
    InvalidOutput,
}

impl ExecutionFailureKind {
    pub fn code(self) -> &'static str {
        match self {
            ExecutionFailureKind::AdapterUnavailable => "adapter_unavailable",
            ExecutionFailureKind::PermissionDenied => "permission_denied",
            ExecutionFailureKind::Timeout => "timeout",
            ExecutionFailureKind::InvalidOutput => "invalid_output",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionOutputPreview {
    pub text: String,
    pub original_bytes: usize,
    pub preview_bytes: usize,
    pub limit_bytes: usize,
    pub truncated: bool,
    pub redacted: bool,
}

impl ExecutionOutputPreview {
    pub fn capture(raw: &str, limit_bytes: usize) -> Self {
        let original_bytes = raw.len();
        if is_secret_like_text(raw) {
            return Self {
                text: REDACTED_SECRET_LIKE_PREVIEW.to_string(),
                original_bytes,
                preview_bytes: REDACTED_SECRET_LIKE_PREVIEW.len(),
                limit_bytes,
                truncated: false,
                redacted: true,
            };
        }

        let text = truncate_utf8(raw, limit_bytes);
        let preview_bytes = text.len();
        Self {
            text,
            original_bytes,
            preview_bytes,
            limit_bytes,
            truncated: original_bytes > preview_bytes,
            redacted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEnvironmentSnapshot {
    pub vars: Vec<ExecutionEnvVarSnapshot>,
    pub rejected_secret_like_env: bool,
}

impl ExecutionEnvironmentSnapshot {
    pub fn empty() -> Self {
        Self {
            vars: Vec::new(),
            rejected_secret_like_env: false,
        }
    }

    pub fn from_pairs_redacted<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut rejected_secret_like_env = false;
        let vars = pairs
            .into_iter()
            .map(|(key, value)| {
                let key = key.into();
                let value = value.into();
                if is_secret_like_env(&key, &value) {
                    rejected_secret_like_env = true;
                    ExecutionEnvVarSnapshot {
                        name: key,
                        value_state: EnvValueState::Redacted,
                        value_preview: Some(REDACTED_SECRET_LIKE_ENV.to_string()),
                    }
                } else if value.is_empty() {
                    ExecutionEnvVarSnapshot {
                        name: key,
                        value_state: EnvValueState::Missing,
                        value_preview: None,
                    }
                } else {
                    ExecutionEnvVarSnapshot {
                        name: key,
                        value_state: EnvValueState::Set,
                        value_preview: Some("<set>".to_string()),
                    }
                }
            })
            .collect();
        Self {
            vars,
            rejected_secret_like_env,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEnvVarSnapshot {
    pub name: String,
    pub value_state: EnvValueState,
    pub value_preview: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvValueState {
    Set,
    Missing,
    Redacted,
}

pub trait UnifiedExecutionOrchestrator {
    fn execute(&self, request: ExecutionRequest) -> ExecutionResult;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeUnifiedExecutionOrchestrator {
    outcome: FakeExecutionOutcome,
    preview_limit_bytes: usize,
    started_at: String,
    completed_at: String,
}

impl FakeUnifiedExecutionOrchestrator {
    pub fn new(outcome: FakeExecutionOutcome) -> Self {
        Self {
            outcome,
            preview_limit_bytes: DEFAULT_OUTPUT_PREVIEW_LIMIT,
            started_at: fixed_now(),
            completed_at: fixed_now(),
        }
    }

    pub fn with_preview_limit(mut self, preview_limit_bytes: usize) -> Self {
        self.preview_limit_bytes = preview_limit_bytes;
        self
    }

    pub fn with_timestamps(
        mut self,
        started_at: impl Into<String>,
        completed_at: impl Into<String>,
    ) -> Self {
        self.started_at = started_at.into();
        self.completed_at = completed_at.into();
        self
    }
}

impl UnifiedExecutionOrchestrator for FakeUnifiedExecutionOrchestrator {
    fn execute(&self, request: ExecutionRequest) -> ExecutionResult {
        match &self.outcome {
            FakeExecutionOutcome::Success { stdout, stderr } => ExecutionResult::from_request(
                &request,
                &self.started_at,
                &self.completed_at,
                true,
                None,
                ExecutionOutputPreview::capture(stdout, self.preview_limit_bytes),
                ExecutionOutputPreview::capture(stderr, self.preview_limit_bytes),
            ),
            FakeExecutionOutcome::AdapterUnavailable { reason } => failed_result(
                &request,
                &self.started_at,
                &self.completed_at,
                self.preview_limit_bytes,
                ExecutionFailure::new(ExecutionFailureKind::AdapterUnavailable, reason, true),
            ),
            FakeExecutionOutcome::PermissionDenied { reason } => failed_result(
                &request,
                &self.started_at,
                &self.completed_at,
                self.preview_limit_bytes,
                ExecutionFailure::new(ExecutionFailureKind::PermissionDenied, reason, false),
            ),
            FakeExecutionOutcome::Timeout { reason } => failed_result(
                &request,
                &self.started_at,
                &self.completed_at,
                self.preview_limit_bytes,
                ExecutionFailure::new(ExecutionFailureKind::Timeout, reason, true),
            ),
            FakeExecutionOutcome::InvalidOutput { reason, stdout } => {
                ExecutionResult::from_request(
                    &request,
                    &self.started_at,
                    &self.completed_at,
                    false,
                    Some(ExecutionFailure::new(
                        ExecutionFailureKind::InvalidOutput,
                        reason,
                        false,
                    )),
                    ExecutionOutputPreview::capture(stdout, self.preview_limit_bytes),
                    ExecutionOutputPreview::capture("", self.preview_limit_bytes),
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeExecutionOutcome {
    Success { stdout: String, stderr: String },
    AdapterUnavailable { reason: String },
    PermissionDenied { reason: String },
    Timeout { reason: String },
    InvalidOutput { reason: String, stdout: String },
}

impl FakeExecutionOutcome {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self::Success {
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub fn adapter_unavailable(reason: impl Into<String>) -> Self {
        Self::AdapterUnavailable {
            reason: reason.into(),
        }
    }

    pub fn permission_denied(reason: impl Into<String>) -> Self {
        Self::PermissionDenied {
            reason: reason.into(),
        }
    }

    pub fn timeout(reason: impl Into<String>) -> Self {
        Self::Timeout {
            reason: reason.into(),
        }
    }

    pub fn invalid_output(reason: impl Into<String>, stdout: impl Into<String>) -> Self {
        Self::InvalidOutput {
            reason: reason.into(),
            stdout: stdout.into(),
        }
    }
}

fn failed_result(
    request: &ExecutionRequest,
    started_at: &str,
    completed_at: &str,
    preview_limit_bytes: usize,
    failure: ExecutionFailure,
) -> ExecutionResult {
    let stderr = failure.reason.clone();
    ExecutionResult::from_request(
        request,
        started_at,
        completed_at,
        false,
        Some(failure),
        ExecutionOutputPreview::capture("", preview_limit_bytes),
        ExecutionOutputPreview::capture(&stderr, preview_limit_bytes),
    )
}

pub fn redact_secret_like(raw: &str) -> ExecutionOutputPreview {
    ExecutionOutputPreview::capture(raw, DEFAULT_OUTPUT_PREVIEW_LIMIT)
}

pub fn redact_secret_like_reason(reason: String) -> (String, bool) {
    if is_secret_like_text(&reason) {
        (REDACTED_SECRET_LIKE_REASON.to_string(), true)
    } else {
        (reason, false)
    }
}

pub fn is_secret_like_env(name: &str, value: &str) -> bool {
    is_secret_like_text(name) || is_secret_like_text(value)
}

pub fn is_secret_like_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|marker| lower.contains(marker))
}

const SECRET_MARKERS: &[&str] = &[
    ".env",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "authorization:",
    "bearer ",
    "secret",
    "password",
    "passwd",
    "private_key",
    "client_secret",
    "id_rsa",
    "id_ed25519",
];

fn truncate_utf8(raw: &str, limit_bytes: usize) -> String {
    if raw.len() <= limit_bytes {
        return raw.to_string();
    }
    let mut end = 0;
    for (index, _) in raw.char_indices() {
        if index > limit_bytes {
            break;
        }
        end = index;
    }
    if end == 0 {
        return String::new();
    }
    raw[..end].to_string()
}

fn fixed_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
