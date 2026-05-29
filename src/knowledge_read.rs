use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::time::timeout;

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

#[derive(Clone, PartialEq, Eq)]
pub struct ReadonlyHttpKnowledgeReadAdapter {
    source: String,
    endpoint: String,
    token: String,
    timeout_ms: u64,
}

impl ReadonlyHttpKnowledgeReadAdapter {
    pub fn new_wiki(
        endpoint: impl Into<String>,
        token: impl Into<String>,
        timeout_ms: u64,
    ) -> Self {
        Self {
            source: "wiki".to_string(),
            endpoint: endpoint.into(),
            token: token.into(),
            timeout_ms: timeout_ms.max(1),
        }
    }

    fn is_configured(&self) -> bool {
        self.source == "wiki" && !self.endpoint.trim().is_empty() && !self.token.trim().is_empty()
    }

    fn query_http(
        &self,
        request: &KnowledgeReadQuery,
    ) -> Result<(u16, String), KnowledgeReadError> {
        let endpoint = self.endpoint.clone();
        let token = self.token.clone();
        let timeout_ms = self.timeout_ms;
        let body_json = json!({
            "source": request.source,
            "query": request.query,
            "limit": request.limit.max(1),
            "read_only": true,
        })
        .to_string();
        let runtime = TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| {
                knowledge_read_error(
                    "knowledge_read_http_runtime",
                    "cannot initialize knowledge_read HTTP runtime",
                    true,
                    "readonly_http",
                )
            })?;

        runtime.block_on(async move {
            let connector = HttpsConnectorBuilder::new()
                .with_native_roots()
                .map_err(|_| {
                    knowledge_read_error(
                        "knowledge_read_http_tls",
                        "cannot initialize knowledge_read TLS roots",
                        true,
                        "readonly_http",
                    )
                })?
                .https_or_http()
                .enable_http1()
                .build();
            let client: Client<_, Full<Bytes>> =
                Client::builder(TokioExecutor::new()).build(connector);
            let req = Request::builder()
                .method(Method::POST)
                .uri(&endpoint)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(body_json)))
                .map_err(|_| {
                    knowledge_read_error(
                        "knowledge_read_http_request",
                        "cannot build knowledge_read HTTP request",
                        false,
                        "readonly_http",
                    )
                })?;
            let response = timeout(Duration::from_millis(timeout_ms), client.request(req))
                .await
                .map_err(|_| {
                    knowledge_read_error(
                        "knowledge_read_http_timeout",
                        "knowledge_read HTTP request timed out",
                        true,
                        "readonly_http",
                    )
                })?
                .map_err(|_| {
                    knowledge_read_error(
                        "knowledge_read_http_send",
                        "knowledge_read HTTP request failed",
                        true,
                        "readonly_http",
                    )
                })?;
            let status_code = response.status().as_u16();
            let body = timeout(
                Duration::from_millis(timeout_ms),
                response.into_body().collect(),
            )
            .await
            .map_err(|_| {
                knowledge_read_error(
                    "knowledge_read_http_timeout",
                    "knowledge_read HTTP response timed out",
                    true,
                    "readonly_http",
                )
            })?
            .map_err(|_| {
                knowledge_read_error(
                    "knowledge_read_http_body",
                    "knowledge_read HTTP response body failed",
                    true,
                    "readonly_http",
                )
            })?
            .to_bytes();

            Ok((status_code, String::from_utf8_lossy(&body).to_string()))
        })
    }
}

impl KnowledgeReadAdapter for ReadonlyHttpKnowledgeReadAdapter {
    fn status(&self) -> KnowledgeReadStatus {
        let configured = self.is_configured();
        KnowledgeReadStatus {
            contract_version: KNOWLEDGE_READ_CONTRACT_VERSION,
            adapter_kind: "readonly_http".to_string(),
            available: configured,
            state: if configured { "ready" } else { "unavailable" }.to_string(),
            sources: vec!["wiki".to_string()],
            boundary: KNOWLEDGE_READ_BOUNDARY.to_string(),
            reason_code: if configured {
                "wiki_readonly_http_configured"
            } else {
                "wiki_readonly_http_config_missing"
            }
            .to_string(),
            reason: if configured {
                "wiki read-only HTTP adapter is configured; it only performs operator-configured read queries and never writes core memory"
                    .to_string()
            } else {
                "wiki read-only HTTP adapter requires endpoint and token before live read can be attempted"
                    .to_string()
            },
            local_preview_is_separate: true,
            connects_real_service: configured,
            writes_automatically: false,
        }
    }

    fn query(
        &self,
        request: KnowledgeReadQuery,
    ) -> Result<KnowledgeReadResult, KnowledgeReadError> {
        validate_knowledge_read_source(&request.source, "readonly_http")?;
        if request.source != self.source {
            return Err(knowledge_read_error(
                "knowledge_read_source_unavailable",
                format!(
                    "{} read-only HTTP adapter is not wired; only wiki is available in this slice",
                    request.source
                ),
                false,
                "readonly_http",
            ));
        }
        if request.query.trim().is_empty() {
            return Err(knowledge_read_error(
                "knowledge_read_empty_query",
                "knowledge_read query must not be empty",
                false,
                "readonly_http",
            ));
        }
        if !self.is_configured() {
            return Err(knowledge_read_error(
                "knowledge_read_unavailable",
                "wiki read-only HTTP adapter is missing endpoint or token",
                false,
                "readonly_http",
            ));
        }

        let (status_code, body) = self.query_http(&request)?;
        if !(200..300).contains(&status_code) {
            return Err(knowledge_read_error(
                "knowledge_read_http_status",
                format!("wiki read-only HTTP adapter returned status_code={status_code}"),
                status_code >= 500 || status_code == 429,
                "readonly_http",
            ));
        }

        let hits = parse_knowledge_read_hits(&body, &request.source)?;
        let limited_hits = hits
            .into_iter()
            .take(request.limit.max(1))
            .collect::<Vec<_>>();
        let receipt = json!({
            "adapter": "readonly_http",
            "source": request.source,
            "status_code": status_code,
            "hit_count": limited_hits.len(),
            "read_only": true,
            "writes_automatically": false,
            "token": "<redacted>",
        })
        .to_string();

        Ok(KnowledgeReadResult {
            source: request.source,
            query: request.query,
            hits: limited_hits,
            read_only: true,
            receipt,
        })
    }
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

fn parse_knowledge_read_hits(
    body: &str,
    requested_source: &str,
) -> Result<Vec<KnowledgeReadHit>, KnowledgeReadError> {
    let value = serde_json::from_str::<serde_json::Value>(body).map_err(|_| {
        knowledge_read_error(
            "knowledge_read_response_decode",
            "wiki read-only HTTP adapter returned invalid JSON",
            false,
            "readonly_http",
        )
    })?;
    let hits_value = value
        .get("hits")
        .or_else(|| value.get("results"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            knowledge_read_error(
                "knowledge_read_response_decode",
                "wiki read-only HTTP adapter response must contain hits or results array",
                false,
                "readonly_http",
            )
        })?;

    let mut hits = Vec::with_capacity(hits_value.len());
    for hit in hits_value {
        let title = hit
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let uri = hit
            .get("uri")
            .or_else(|| hit.get("url"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let preview = hit
            .get("preview")
            .or_else(|| hit.get("snippet"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let provenance = hit
            .get("provenance")
            .and_then(|value| value.as_str())
            .unwrap_or("wiki_readonly_http")
            .to_string();
        hits.push(KnowledgeReadHit {
            source: hit
                .get("source")
                .and_then(|value| value.as_str())
                .unwrap_or(requested_source)
                .to_string(),
            title,
            uri,
            preview,
            provenance,
        });
    }

    Ok(hits)
}

fn knowledge_read_error(
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
    adapter_kind: impl Into<String>,
) -> KnowledgeReadError {
    KnowledgeReadError {
        code: code.into(),
        message: message.into(),
        adapter_kind: adapter_kind.into(),
        retryable,
    }
}
