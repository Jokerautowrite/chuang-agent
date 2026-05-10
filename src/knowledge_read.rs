use serde::{Deserialize, Serialize};

pub const KNOWLEDGE_READ_CONTRACT_VERSION: u16 = 1;
pub const KNOWLEDGE_READ_SOURCES: &[&str] = &["wiki", "gbrain"];
const KNOWLEDGE_READ_BOUNDARY: &str = "knowledge_read_wiki_gbrain_live_contract";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadSourceConfig {
    pub endpoint: Option<String>,
    pub token_env: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl KnowledgeReadSourceConfig {
    pub fn disabled() -> Self {
        Self {
            endpoint: None,
            token_env: None,
            timeout_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadConfig {
    pub wiki: KnowledgeReadSourceConfig,
    pub gbrain: KnowledgeReadSourceConfig,
}

impl KnowledgeReadConfig {
    pub fn disabled() -> Self {
        Self {
            wiki: KnowledgeReadSourceConfig::disabled(),
            gbrain: KnowledgeReadSourceConfig::disabled(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadStatus {
    pub contract_version: u16,
    pub adapter_kind: String,
    pub available: bool,
    pub state: String,
    pub sources: Vec<String>,
    pub boundary: String,
    pub reason_code: String,
    pub reason: String,
    pub local_preview_is_separate: bool,
    pub connects_real_service: bool,
    pub writes_automatically: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadQuery {
    pub source: String,
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadHit {
    pub source: String,
    pub title: String,
    pub uri: String,
    pub preview: String,
    pub provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadResult {
    pub source: String,
    pub query: String,
    pub hits: Vec<KnowledgeReadHit>,
    pub read_only: bool,
    pub receipt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadError {
    pub code: String,
    pub message: String,
    pub adapter_kind: String,
    pub retryable: bool,
}

pub trait KnowledgeReadAdapter {
    fn status(&self) -> KnowledgeReadStatus;
    fn query(&self, request: KnowledgeReadQuery)
        -> Result<KnowledgeReadResult, KnowledgeReadError>;
}

pub fn preflight_knowledge_read_status(
    config: &KnowledgeReadConfig,
    source: &str,
    token_state: &str,
) -> KnowledgeReadStatus {
    let source_config = match source {
        "wiki" => &config.wiki,
        "gbrain" => &config.gbrain,
        _ => return preflight_unknown_source_status(source),
    };
    let endpoint_state = if source_config
        .endpoint
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        "set"
    } else {
        "missing"
    };
    let token_env_state = if source_config
        .token_env
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        "set"
    } else {
        "missing"
    };
    let preflight_ready =
        endpoint_state == "set" && token_env_state == "set" && token_state == "<set>";
    let reason_code = if endpoint_state == "missing" {
        "endpoint_missing"
    } else if token_env_state == "missing" {
        "token_env_missing"
    } else if token_state != "<set>" {
        "token_missing"
    } else {
        "real_adapter_missing"
    };
    KnowledgeReadStatus {
        contract_version: KNOWLEDGE_READ_CONTRACT_VERSION,
        adapter_kind: "preflight_only".to_string(),
        available: false,
        state: if preflight_ready {
            "preflight_ready_adapter_missing".to_string()
        } else {
            "unavailable".to_string()
        },
        sources: knowledge_read_sources(),
        boundary: KNOWLEDGE_READ_BOUNDARY.to_string(),
        reason_code: reason_code.to_string(),
        reason: if endpoint_state == "missing" {
            format!(
                "{} endpoint is missing; live wiki/GBrain read cannot be claimed",
                source
            )
        } else if token_env_state == "missing" {
            format!(
                "{} token env is missing; live wiki/GBrain read cannot be claimed",
                source
            )
        } else if token_state != "<set>" {
            format!(
                "{} token is unavailable; live wiki/GBrain read cannot be claimed",
                source
            )
        } else {
            format!(
                "{} endpoint and token env are configured, but no audited live adapter is wired; live wiki/GBrain read cannot be claimed",
                source
            )
        },
        local_preview_is_separate: true,
        connects_real_service: false,
        writes_automatically: false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeKnowledgeReadAdapter {
    hits: Vec<KnowledgeReadHit>,
}

impl FakeKnowledgeReadAdapter {
    pub fn new(hits: Vec<KnowledgeReadHit>) -> Self {
        Self { hits }
    }
}

impl KnowledgeReadAdapter for FakeKnowledgeReadAdapter {
    fn status(&self) -> KnowledgeReadStatus {
        KnowledgeReadStatus {
            contract_version: KNOWLEDGE_READ_CONTRACT_VERSION,
            adapter_kind: "fake".to_string(),
            available: false,
            state: "fake_contract_only".to_string(),
            sources: knowledge_read_sources(),
            boundary: KNOWLEDGE_READ_BOUNDARY.to_string(),
            reason_code: "fake_contract_only".to_string(),
            reason: "fake knowledge_read adapter returns injected local-preview hits for contract tests only; it is not a real wiki/GBrain live-read adapter".to_string(),
            local_preview_is_separate: true,
            connects_real_service: false,
            writes_automatically: false,
        }
    }

    fn query(
        &self,
        request: KnowledgeReadQuery,
    ) -> Result<KnowledgeReadResult, KnowledgeReadError> {
        validate_knowledge_read_source(&request.source, "fake")?;
        let limit = request.limit.max(1);
        let hits = self
            .hits
            .iter()
            .filter(|hit| hit.source == request.source)
            .take(limit)
            .cloned()
            .collect();
        Ok(KnowledgeReadResult {
            source: request.source,
            query: request.query,
            hits,
            read_only: true,
            receipt: "fake_knowledge_read_contract_receipt".to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnavailableKnowledgeReadAdapter;

impl KnowledgeReadAdapter for UnavailableKnowledgeReadAdapter {
    fn status(&self) -> KnowledgeReadStatus {
        unavailable_knowledge_read_status("real_adapter_missing")
    }

    fn query(
        &self,
        request: KnowledgeReadQuery,
    ) -> Result<KnowledgeReadResult, KnowledgeReadError> {
        validate_knowledge_read_source(&request.source, "unavailable")?;
        Err(KnowledgeReadError {
            code: "knowledge_read_unavailable".to_string(),
            message: format!(
                "knowledge_read live adapter is not configured; cannot query real {} service",
                request.source
            ),
            adapter_kind: "unavailable".to_string(),
            retryable: false,
        })
    }
}

pub fn unavailable_knowledge_read_status(reason_code: &str) -> KnowledgeReadStatus {
    KnowledgeReadStatus {
        contract_version: KNOWLEDGE_READ_CONTRACT_VERSION,
        adapter_kind: "unavailable".to_string(),
        available: false,
        state: "unavailable".to_string(),
        sources: knowledge_read_sources(),
        boundary: KNOWLEDGE_READ_BOUNDARY.to_string(),
        reason_code: reason_code.to_string(),
        reason: "no audited wiki/GBrain live adapter is configured; local preview/source-contract results must not be reported as real wiki/GBrain live reads".to_string(),
        local_preview_is_separate: true,
        connects_real_service: false,
        writes_automatically: false,
    }
}

fn preflight_unknown_source_status(source: &str) -> KnowledgeReadStatus {
    KnowledgeReadStatus {
        contract_version: KNOWLEDGE_READ_CONTRACT_VERSION,
        adapter_kind: "preflight_only".to_string(),
        available: false,
        state: "unavailable".to_string(),
        sources: knowledge_read_sources(),
        boundary: KNOWLEDGE_READ_BOUNDARY.to_string(),
        reason_code: "unknown_source".to_string(),
        reason: format!(
            "{} is not a configured knowledge_read source; only wiki and gbrain are valid live-read sources",
            source
        ),
        local_preview_is_separate: true,
        connects_real_service: false,
        writes_automatically: false,
    }
}

fn validate_knowledge_read_source(
    source: &str,
    adapter_kind: &str,
) -> Result<(), KnowledgeReadError> {
    if KNOWLEDGE_READ_SOURCES.contains(&source) {
        return Ok(());
    }

    Err(KnowledgeReadError {
        code: "knowledge_read_unknown_source".to_string(),
        message: format!(
            "{} is not a valid knowledge_read source; expected wiki or gbrain",
            source
        ),
        adapter_kind: adapter_kind.to_string(),
        retryable: false,
    })
}

fn knowledge_read_sources() -> Vec<String> {
    KNOWLEDGE_READ_SOURCES
        .iter()
        .map(|source| source.to_string())
        .collect()
}
