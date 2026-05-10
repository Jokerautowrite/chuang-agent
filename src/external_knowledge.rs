use serde::Serialize;

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
    pub endpoint: Option<String>,
    pub token_env: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl ExternalKnowledgeSourceConfig {
    pub fn disabled() -> Self {
        Self {
            endpoint: None,
            token_env: None,
            timeout_ms: None,
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
}

impl ExternalKnowledgeRead for LiveExternalKnowledgeReader {
    fn preflight(&self) -> ExternalKnowledgePreflightStatus {
        preflight_for_source(self.source, &self.config, &self.token_state)
    }

    fn read(&self, _request: ExternalKnowledgeReadRequest) -> ExternalKnowledgeReadStatus {
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

        let adapter_kind = preflight.adapter_kind.clone();
        unavailable_read_status(
            self.source,
            &adapter_kind,
            "live wiki/GBrain query execution is not wired in this build",
            "wire an audited HTTP adapter before treating live read as available",
            preflight,
        )
    }
}

pub fn preflight_for_source(
    source: ExternalKnowledgeSource,
    config: &ExternalKnowledgeSourceConfig,
    token_state: &str,
) -> ExternalKnowledgePreflightStatus {
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
    let live_adapter_configured = false;
    let available = false;
    let reason_code = if endpoint_state == "missing" {
        "endpoint_missing"
    } else if token_env_state == "missing" {
        "token_env_missing"
    } else if token_state != "<set>" {
        "token_missing"
    } else if read_configured && token_available {
        "real_adapter_missing"
    } else {
        "unavailable"
    };
    let reason = if endpoint_state == "missing" {
        format!(
            "{} endpoint is missing; live external knowledge read cannot be claimed",
            source.as_str()
        )
    } else if token_env_state == "missing" {
        format!(
            "{} token env is missing; live external knowledge read cannot be claimed",
            source.as_str()
        )
    } else if token_state != "<set>" {
        format!(
            "{} token is unavailable; live external knowledge read cannot be claimed",
            source.as_str()
        )
    } else {
        format!(
            "{} endpoint and token env are configured, but no audited live adapter is wired; live external knowledge read cannot be claimed",
            source.as_str()
        )
    };
    let next_action = if endpoint_state == "missing" {
        format!(
            "set external_knowledge.{}.endpoint and the token env before enabling live read",
            source.as_str()
        )
    } else if token_env_state == "missing" {
        format!(
            "set external_knowledge.{}.token_env before enabling live read",
            source.as_str()
        )
    } else if token_state != "<set>" {
        format!(
            "export the token env named by external_knowledge.{}.token_env before enabling live read",
            source.as_str()
        )
    } else {
        format!(
            "wire an audited wiki/GBrain HTTP adapter for {} before marking live read available",
            source.as_str()
        )
    };

    ExternalKnowledgePreflightStatus {
        source: source.as_str().to_string(),
        adapter_kind: "preflight_only".to_string(),
        live_adapter_configured,
        endpoint_state: endpoint_state.to_string(),
        token_state: token_env_state.to_string(),
        available,
        reason_code: reason_code.to_string(),
        reason,
        next_action,
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
