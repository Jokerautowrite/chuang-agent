//! `external_knowledge` 模块。公开接口：trait ExternalKnowledgeRead；struct ExternalKnowledgeSourceConfig, ExternalKnowledgeConfig, ExternalKnowledgeReadRequest, ExternalKnowledgeReadHit, ExternalKnowledgePreflightStatus, ExternalKnowledgeReadStatus, FakeExternalKnowledgeReader, LiveExternalKnowledgeReader；enum ExternalKnowledgeSource；fn as_str, disabled, source_config, new, from_runtime, preflight_for_source, unavailable_preflight, unavailable_read_status。
//!
//! knowledge_context（GBrain 直连 API 通道）在这里启用：开关走
//! `RuntimeConfig.metadata.knowledge_context=1`（与 emotion_brain 同款显式开关），
//! endpoint/token_env/timeout 走 `runtime.external_knowledge.gbrain`；
//! 真实 token 由 `token_env` 命名的环境变量在读取时解析，绝不写入结构体/日志/回执。
//! 预检失败返回结构化状态（reason_code/reason），live 查询失败同样返回结构化
//! 不可用状态，绝不静默吞掉，也绝不阻断主对话（调用方把 knowledge 段标记 unavailable）。

use serde::Serialize;

use crate::knowledge_read::{
    KnowledgeReadAdapter, KnowledgeReadQuery, KnowledgeReadSourceConfig,
    ReadonlyHttpKnowledgeReadAdapter,
};
use crate::runtime_config::RuntimeConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalKnowledgeSource {
    Wiki,
    GBrain,
}

impl ExternalKnowledgeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wiki => "wiki",
            Self::GBrain => "gbrain",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalKnowledgeSourceConfig {
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub token_env: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl ExternalKnowledgeSourceConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            endpoint: None,
            token_env: None,
            timeout_ms: None,
        }
    }

    /// From a `KnowledgeReadSourceConfig` (runtime storage shape) plus the
    /// channel enable switch. The switch comes from the caller so this module
    /// stays decoupled from metadata key conventions.
    pub fn from_runtime_source(
        enabled: bool,
        source: &KnowledgeReadSourceConfig,
    ) -> Self {
        Self {
            enabled,
            endpoint: source.endpoint.clone(),
            token_env: source.token_env.clone(),
            timeout_ms: source.timeout_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalKnowledgeConfig {
    pub wiki: ExternalKnowledgeSourceConfig,
    pub gbrain: ExternalKnowledgeSourceConfig,
}

impl ExternalKnowledgeConfig {
    pub fn disabled() -> Self {
        Self {
            wiki: ExternalKnowledgeSourceConfig::disabled(),
            gbrain: ExternalKnowledgeSourceConfig::disabled(),
        }
    }

    pub fn source_config(&self, source: ExternalKnowledgeSource) -> &ExternalKnowledgeSourceConfig {
        match source {
            ExternalKnowledgeSource::Wiki => &self.wiki,
            ExternalKnowledgeSource::GBrain => &self.gbrain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalKnowledgeReadRequest {
    pub source: ExternalKnowledgeSource,
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalKnowledgeReadHit {
    pub source: String,
    pub path: String,
    pub preview: String,
    pub score: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalKnowledgePreflightStatus {
    pub source: String,
    pub adapter_kind: String,
    pub live_adapter_configured: bool,
    pub endpoint_state: String,
    pub token_state: String,
    pub available: bool,
    pub reason_code: String,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalKnowledgeReadStatus {
    pub source: String,
    pub adapter_kind: String,
    pub live_adapter_configured: bool,
    pub available: bool,
    pub endpoint_state: String,
    pub token_state: String,
    pub reason_code: String,
    pub reason: String,
    pub preflight: ExternalKnowledgePreflightStatus,
    pub hit_count: usize,
    pub hits: Vec<ExternalKnowledgeReadHit>,
}

pub trait ExternalKnowledgeRead {
    fn preflight(&self) -> ExternalKnowledgePreflightStatus;
    fn read(&self, request: ExternalKnowledgeReadRequest) -> ExternalKnowledgeReadStatus;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeExternalKnowledgeReader {
    source: ExternalKnowledgeSource,
}

impl FakeExternalKnowledgeReader {
    pub fn new(source: ExternalKnowledgeSource) -> Self {
        Self { source }
    }
}

impl ExternalKnowledgeRead for FakeExternalKnowledgeReader {
    fn preflight(&self) -> ExternalKnowledgePreflightStatus {
        unavailable_preflight(
            self.source,
            "fake_adapter",
            "fake adapter configured for external knowledge read",
            "configure a live wiki/GBrain endpoint and token env before claiming live read",
        )
    }

    fn read(&self, _request: ExternalKnowledgeReadRequest) -> ExternalKnowledgeReadStatus {
        unavailable_read_status(
            self.source,
            "fake_adapter",
            "fake adapter configured for external knowledge read",
            "configure a live wiki/GBrain endpoint and token env before claiming live read",
            self.preflight(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveExternalKnowledgeReader {
    source: ExternalKnowledgeSource,
    config: ExternalKnowledgeSourceConfig,
    token_state: String,
}

impl LiveExternalKnowledgeReader {
    pub fn new(
        source: ExternalKnowledgeSource,
        config: ExternalKnowledgeSourceConfig,
        token_state: String,
    ) -> Self {
        Self {
            source,
            config,
            token_state,
        }
    }

    /// Build the knowledge_context channel reader (GBrain direct API) from the
    /// active runtime config. The channel is enabled by
    /// `metadata.knowledge_context=1`; endpoint/token_env/timeout come from
    /// `external_knowledge.gbrain`. The real token is resolved from the
    /// `token_env`-named environment variable at read time, never stored here.
    pub fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self::new(
            ExternalKnowledgeSource::GBrain,
            ExternalKnowledgeSourceConfig::from_runtime_source(
                runtime.knowledge_context_enabled(),
                &runtime.external_knowledge.gbrain,
            ),
            resolve_token_state(runtime.external_knowledge.gbrain.token_env.as_deref()),
        )
    }
}

impl ExternalKnowledgeRead for LiveExternalKnowledgeReader {
    fn preflight(&self) -> ExternalKnowledgePreflightStatus {
        preflight_for_source(self.source, &self.config, &self.token_state)
    }

    fn read(&self, request: ExternalKnowledgeReadRequest) -> ExternalKnowledgeReadStatus {
        let preflight = self.preflight();
        if !preflight.available {
            let adapter_kind = preflight.adapter_kind.clone();
            let reason = preflight.reason.clone();
            let next_action = preflight.next_action.clone();
            return unavailable_read_status(
                self.source,
                &adapter_kind,
                &reason,
                &next_action,
                preflight,
            );
        }

        // Live adapter is wired: ReadonlyHttpKnowledgeReadAdapter performs the
        // audited read-only POST. Token is resolved fresh from the env named by
        // token_env; it never appears in statuses, receipts, or logs.
        let endpoint = self
            .config
            .endpoint
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        let Some(token_env) = self
            .config
            .token_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return unavailable_read_status(
                self.source,
                "readonly_http",
                "token env is missing at read time",
                "configure external_knowledge.gbrain.token_env before live read",
                preflight,
            );
        };
        let Some(token) = std::env::var(token_env)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            return unavailable_read_status(
                self.source,
                "readonly_http",
                "token is unavailable at read time",
                &format!("export the token env {token_env} before live read"),
                preflight,
            );
        };
        let timeout_ms = self.config.timeout_ms.unwrap_or(30_000);
        let adapter = match self.source {
            ExternalKnowledgeSource::GBrain => {
                ReadonlyHttpKnowledgeReadAdapter::new_gbrain(endpoint, token, timeout_ms)
            }
            ExternalKnowledgeSource::Wiki => {
                ReadonlyHttpKnowledgeReadAdapter::new_wiki(endpoint, token, timeout_ms)
            }
        };
        match adapter.query(KnowledgeReadQuery {
            source: self.source.as_str().to_string(),
            query: request.query,
            limit: request.limit,
        }) {
            Ok(result) => {
                let hits = result
                    .hits
                    .into_iter()
                    .map(|hit| ExternalKnowledgeReadHit {
                        source: hit.source,
                        path: hit.uri,
                        preview: hit.preview,
                        score: 0,
                    })
                    .collect::<Vec<_>>();
                ExternalKnowledgeReadStatus {
                    source: self.source.as_str().to_string(),
                    adapter_kind: "readonly_http".to_string(),
                    live_adapter_configured: true,
                    available: true,
                    endpoint_state: "set".to_string(),
                    token_state: "<set>".to_string(),
                    reason_code: format!("{}_readonly_http_read", self.source.as_str()),
                    reason: format!(
                        "{} live read succeeded via the read-only HTTP adapter",
                        self.source.as_str()
                    ),
                    preflight,
                    hit_count: hits.len(),
                    hits,
                }
            }
            Err(error) => {
                // 结构化降级：live 查询失败不 panic、不静默吞掉，返回带
                // reason_code/reason 的不可用状态；主对话不受阻断。
                ExternalKnowledgeReadStatus {
                    source: self.source.as_str().to_string(),
                    adapter_kind: error.adapter_kind,
                    live_adapter_configured: true,
                    available: false,
                    endpoint_state: "set".to_string(),
                    token_state: "<set>".to_string(),
                    reason_code: error.code,
                    reason: error.message,
                    preflight,
                    hit_count: 0,
                    hits: Vec::new(),
                }
            }
        }
    }
}

pub fn preflight_for_source(
    source: ExternalKnowledgeSource,
    config: &ExternalKnowledgeSourceConfig,
    token_state: &str,
) -> ExternalKnowledgePreflightStatus {
    let source_name = source.as_str();
    if !config.enabled {
        return ExternalKnowledgePreflightStatus {
            source: source_name.to_string(),
            adapter_kind: "preflight_only".to_string(),
            live_adapter_configured: false,
            endpoint_state: if config
                .endpoint
                .as_ref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            {
                "set"
            } else {
                "missing"
            }
            .to_string(),
            token_state: if config
                .token_env
                .as_ref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            {
                token_state.to_string()
            } else {
                "missing".to_string()
            },
            available: false,
            reason_code: "disabled".to_string(),
            reason: format!(
                "{source_name} knowledge_context channel is disabled; live external knowledge read is not claimed"
            ),
            next_action: format!(
                "enable {source_name} knowledge_context (e.g. metadata knowledge_context=1) before live read"
            ),
        };
    }
    let endpoint_state = if config
        .endpoint
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        "set"
    } else {
        "missing"
    };
    let token_env_state = if config
        .token_env
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        token_state
    } else {
        "missing"
    };
    let read_configured = endpoint_state == "set" && token_env_state != "missing";
    let token_available = token_state == "<set>";
    let live_adapter_configured = read_configured && token_available;
    let available = live_adapter_configured;
    let reason_code: String = if endpoint_state == "missing" {
        "endpoint_missing".to_string()
    } else if token_env_state == "missing" {
        "token_env_missing".to_string()
    } else if token_state != "<set>" {
        "token_missing".to_string()
    } else {
        format!("{}_readonly_http_configured", source_name)
    };
    let reason = if endpoint_state == "missing" {
        format!(
            "{} endpoint is missing; live external knowledge read cannot be claimed",
            source_name
        )
    } else if token_env_state == "missing" {
        format!(
            "{} token env is missing; live external knowledge read cannot be claimed",
            source_name
        )
    } else if token_state != "<set>" {
        format!(
            "{} token is unavailable; live external knowledge read cannot be claimed",
            source_name
        )
    } else {
        format!(
            "{} live read is available via the read-only HTTP adapter (endpoint, token env, and token are configured)",
            source_name
        )
    };
    let next_action = if endpoint_state == "missing" {
        format!(
            "set external_knowledge.{}.endpoint and the token env before enabling live read",
            source_name
        )
    } else if token_env_state == "missing" {
        format!(
            "set external_knowledge.{}.token_env before enabling live read",
            source_name
        )
    } else if token_state != "<set>" {
        format!(
            "export the token env named by external_knowledge.{}.token_env before enabling live read",
            source_name
        )
    } else {
        format!(
            "run read queries; the {} read-only HTTP adapter only performs operator-configured read queries and never writes core memory",
            source_name
        )
    };

    ExternalKnowledgePreflightStatus {
        source: source_name.to_string(),
        adapter_kind: if available {
            "readonly_http".to_string()
        } else {
            "preflight_only".to_string()
        },
        live_adapter_configured,
        endpoint_state: endpoint_state.to_string(),
        token_state: token_env_state.to_string(),
        available,
        reason_code,
        reason,
        next_action,
    }
}

fn resolve_token_state(token_env: Option<&str>) -> String {
    let Some(token_env) = token_env.map(str::trim).filter(|value| !value.is_empty()) else {
        return "<missing>".to_string();
    };
    match std::env::var(token_env) {
        Ok(value) if !value.trim().is_empty() => "<set>".to_string(),
        _ => "<missing>".to_string(),
    }
}

pub fn unavailable_preflight(
    source: ExternalKnowledgeSource,
    adapter_kind: &str,
    reason: &str,
    next_action: &str,
) -> ExternalKnowledgePreflightStatus {
    ExternalKnowledgePreflightStatus {
        source: source.as_str().to_string(),
        adapter_kind: adapter_kind.to_string(),
        live_adapter_configured: false,
        endpoint_state: "missing".to_string(),
        token_state: "missing".to_string(),
        available: false,
        reason_code: "adapter_unavailable".to_string(),
        reason: reason.to_string(),
        next_action: next_action.to_string(),
    }
}

pub fn unavailable_read_status(
    source: ExternalKnowledgeSource,
    adapter_kind: &str,
    reason: &str,
    _next_action: &str,
    preflight: ExternalKnowledgePreflightStatus,
) -> ExternalKnowledgeReadStatus {
    ExternalKnowledgeReadStatus {
        source: source.as_str().to_string(),
        adapter_kind: adapter_kind.to_string(),
        live_adapter_configured: false,
        available: false,
        endpoint_state: preflight.endpoint_state.clone(),
        token_state: preflight.token_state.clone(),
        reason_code: preflight.reason_code.clone(),
        reason: reason.to_string(),
        preflight,
        hit_count: 0,
        hits: Vec::new(),
    }
}
