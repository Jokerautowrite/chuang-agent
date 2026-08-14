//! `provider_openai_compatible` 模块。公开接口：struct ProviderConfigError, OpenAICompatibleRequestEnvelope, HttpRequestPreview, StubHttpCallResult, HttpCallResult, OpenAICompatibleProviderAdapter；enum ProviderTransport, ReasoningEffort；fn as_str, new, with_transport, with_endpoint, with_reasoning_effort, with_max_output_tokens, with_request_timeout_ms, with_tls_ca_cert_path。
//!
//! Transport 层（stub/http/native/curl）与响应/重试核心（`run_provider_respond`）
//! 以 `pub(crate)` 共享给 `provider_anthropic_compatible`，保证 Anthropic 与
//! OpenAI 走同一套传输、退避、看门狗与错误语义。

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::{pem::PemObject, CertificateDer};
use serde_json::json;
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::time::timeout;

use crate::responder::{
    ProviderAdapterResponder, ProviderAdapterResponse, ProviderIdentity, ResponderRequest,
};
use crate::runtime_config::ProviderApiEndpoint;

pub(crate) const DEFAULT_PROVIDER_REQUEST_TIMEOUT_MS: u64 = 60_000;

// 错误恢复级联（蓝本 docs/reference-dig-20260810.md §2.5 withRetry）：
// - 退避：base 500ms，指数 ×2，上限 32s，jitter ±25%。
// - 重试类目：[429,529]（限流/上游不可达）+ ECONNRESET/ETIMEDOUT 类传输错误；
//   保留 408/5xx 既有网关语义（fallback 与既有测试依赖）。
const RETRY_BACKOFF_BASE_MS: u64 = 500;
const RETRY_BACKOFF_MAX_MS: u64 = 32_000;
const RETRY_BACKOFF_JITTER_RATIO: f64 = 0.25;

// 空闲看门狗（蓝本 §2.5）：单次 provider 调用（含重试序列）45s 告警、90s 中断。
// 单次 transport 尝试仍受 request_timeout_ms（provider_timeout_ms 配置）约束。
const PROVIDER_IDLE_WATCHDOG_WARN_MS: u64 = 45_000;
const PROVIDER_IDLE_WATCHDOG_KILL_MS: u64 = 90_000;

const RETRYABLE_STATUS_CODES: &[u16] = &[408, 429, 500, 502, 503, 504, 529];
const MAX_PROVIDER_ATTEMPTS: usize = 3;

fn backoff_ms(attempt: usize) -> u64 {
    // withRetry（蓝本 §2.5）：base 500ms，指数 ×2，上限 32s，jitter ±25%。
    let exponent = attempt.min(6) as u32; // 500 * 2^6 = 32000 = 上限
    let base = RETRY_BACKOFF_BASE_MS
        .saturating_mul(1u64 << exponent)
        .min(RETRY_BACKOFF_MAX_MS);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    // jitter 在 cap 之后施加：最终值不得超过 32s 上限。
    jittered_backoff(base, seed).min(RETRY_BACKOFF_MAX_MS)
}

/// ±RETRY_BACKOFF_JITTER_RATIO 抖动；确定性：同 seed 同结果（便于测试）。
fn jittered_backoff(base_ms: u64, seed: u64) -> u64 {
    let span = ((base_ms as f64) * RETRY_BACKOFF_JITTER_RATIO) as u64;
    let offset = (seed % (span * 2 + 1)) as i64 - span as i64;
    (base_ms as i64 + offset).max(1) as u64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfigError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAICompatibleRequestEnvelope {
    pub provider_id: String,
    pub base_url: String,
    pub model: String,
    pub instructions: String,
    pub input: String,
    pub store: bool,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequestPreview {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubHttpCallResult {
    pub status_code: u16,
    pub url: String,
    pub request_body_json: String,
    pub response_body_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCallResult {
    pub status_code: u16,
    pub url: String,
    pub request_body_json: String,
    pub response_body_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTransport {
    Stub,
    Http,
    Native,
    Curl,
}

impl ProviderTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stub => "stub",
            Self::Http => "http",
            Self::Native => "native",
            Self::Curl => "curl",
        }
    }
}

impl fmt::Display for ProviderTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderTransport {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "stub" => Ok(Self::Stub),
            "http" => Ok(Self::Http),
            "native" => Ok(Self::Native),
            "curl" => Ok(Self::Curl),
            other => Err(format!(
                "unsupported provider transport: {other} (supported: stub, http, native, curl)"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    XHigh,
    /// Highest effort for providers that accept OpenAI-style `"max"` (e.g. gpt-5.6-terra).
    Max,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReasoningEffort {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::XHigh),
            "max" => Ok(Self::Max),
            other => Err(format!(
                "unsupported reasoning effort: {other} (supported: low, medium, high, xhigh, max)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAICompatibleProviderAdapter {
    identity: ProviderIdentity,
    base_url: String,
    api_key: String,
    transport: ProviderTransport,
    endpoint: ProviderApiEndpoint,
    reasoning_effort: Option<ReasoningEffort>,
    max_output_tokens: Option<u32>,
    request_timeout_ms: u64,
    tls_ca_cert_path: Option<PathBuf>,
}

impl OpenAICompatibleProviderAdapter {
    pub fn new(
        provider_id: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model_name: impl Into<String>,
    ) -> Self {
        Self {
            identity: ProviderIdentity {
                provider_id: provider_id.into(),
                model_name: model_name.into(),
            },
            base_url: base_url.into(),
            api_key: api_key.into(),
            transport: ProviderTransport::Stub,
            endpoint: ProviderApiEndpoint::default(),
            reasoning_effort: None,
            max_output_tokens: None,
            request_timeout_ms: DEFAULT_PROVIDER_REQUEST_TIMEOUT_MS,
            tls_ca_cert_path: None,
        }
    }

    pub fn with_transport(mut self, transport: ProviderTransport) -> Self {
        self.transport = transport;
        self
    }

    pub fn with_endpoint(mut self, endpoint: ProviderApiEndpoint) -> Self {
        self.endpoint = endpoint;
        self
    }

    pub fn with_reasoning_effort(mut self, reasoning_effort: Option<ReasoningEffort>) -> Self {
        self.reasoning_effort = reasoning_effort;
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: Option<u32>) -> Self {
        self.max_output_tokens = max_output_tokens;
        self
    }

    pub fn with_request_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.request_timeout_ms = timeout_ms.max(1);
        self
    }

    pub fn with_tls_ca_cert_path(mut self, tls_ca_cert_path: Option<PathBuf>) -> Self {
        self.tls_ca_cert_path = tls_ca_cert_path;
        self
    }

    pub fn validate_config(&self) -> Result<(), ProviderConfigError> {
        if self.base_url.trim().is_empty() {
            return Err(ProviderConfigError {
                field: "base_url".to_string(),
                message: "base_url must not be empty".to_string(),
            });
        }

        if self.identity.model_name.trim().is_empty() {
            return Err(ProviderConfigError {
                field: "model_name".to_string(),
                message: "model_name must not be empty".to_string(),
            });
        }

        Ok(())
    }

    pub fn build_request_envelope(
        &self,
        request: &ResponderRequest,
    ) -> Result<OpenAICompatibleRequestEnvelope, ProviderConfigError> {
        self.validate_config()?;
        Ok(OpenAICompatibleRequestEnvelope {
            provider_id: self.identity.provider_id.clone(),
            base_url: self.base_url.clone(),
            model: self.identity.model_name.clone(),
            instructions: request.prompt.clone(),
            input: request.user_input.clone(),
            store: false,
            max_output_tokens: self.max_output_tokens,
        })
    }

    pub fn build_http_request_preview(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpRequestPreview, ProviderConfigError> {
        let envelope = self.build_request_envelope(request)?;
        let (url, body) = match self.endpoint {
            ProviderApiEndpoint::Responses => {
                let url = format!("{}/responses", envelope.base_url.trim_end_matches('/'),);
                let mut body = json!({
                    "model": envelope.model,
                    "instructions": envelope.instructions,
                    "input": envelope.input,
                    "store": envelope.store,
                });
                if let Some(reasoning_effort) = self.reasoning_effort {
                    body["reasoning"] = json!({ "effort": reasoning_effort.as_str() });
                }
                if let Some(max_output_tokens) = envelope.max_output_tokens {
                    body["max_output_tokens"] = json!(max_output_tokens);
                }
                (url, body)
            }
            ProviderApiEndpoint::ChatCompletions => {
                let url = format!(
                    "{}/chat/completions",
                    envelope.base_url.trim_end_matches('/'),
                );
                let mut body = json!({
                    "model": envelope.model,
                    "messages": [
                        {
                            "role": "system",
                            "content": envelope.instructions,
                        },
                        {
                            "role": "user",
                            "content": envelope.input,
                        }
                    ],
                });
                if let Some(reasoning_effort) = self.reasoning_effort {
                    body["reasoning"] = json!({ "effort": reasoning_effort.as_str() });
                }
                if let Some(max_output_tokens) = envelope.max_output_tokens {
                    body["max_tokens"] = json!(max_output_tokens);
                }
                (url, body)
            }
        };
        let body_json = body.to_string();

        Ok(HttpRequestPreview {
            method: "POST".to_string(),
            url,
            headers: BTreeMap::from([
                (
                    "authorization".to_string(),
                    format!("Bearer {}", self.api_key),
                ),
                ("content-type".to_string(), "application/json".to_string()),
            ]),
            body_json,
        })
    }

    pub fn execute_stub_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<StubHttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
        Ok(StubHttpCallResult {
            status_code: 200,
            url: preview.url,
            request_body_json: preview.body_json,
            response_body_json: build_openai_stub_response_body(&self.identity, request),
        })
    }

    pub fn execute_http_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
        execute_http_transport(&preview, self.request_timeout_ms)
    }

    pub fn execute_native_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
        execute_native_transport(
            &preview,
            self.request_timeout_ms,
            self.tls_ca_cert_path.as_ref(),
        )
    }

    pub fn execute_curl_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
        execute_curl_transport(&preview, self.request_timeout_ms)
    }
}

/// OpenAI Responses 风格的 stub 响应体（保持既有测试断言不变）。
fn build_openai_stub_response_body(
    identity: &ProviderIdentity,
    request: &ResponderRequest,
) -> String {
    json!({
        "id": "resp-stub-001",
        "object": "response",
        "status": "completed",
        "completed_at": 0,
        "provider_id": identity.provider_id,
        "model": identity.model_name,
        "stubbed": true,
        "instructions": request.prompt,
        "output": [
            {
                "id": "msg-stub-001",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": format!(
                            "stubbed_post_ok: provider={} model={} user_input=《{}》",
                            identity.provider_id,
                            identity.model_name,
                            request.user_input
                        ),
                        "annotations": []
                    }
                ]
            }
        ],
        "output_text": format!(
            "stubbed_post_ok: provider={} model={} user_input=《{}》",
            identity.provider_id,
            identity.model_name,
            request.user_input
        ),
        "store": false,
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0,
            "total_tokens": 0
        }
    })
    .to_string()
}

/// 纯 HTTP/1.1 transport（provider_openai_compatible 与 anthropic 共享）。
pub(crate) fn execute_http_transport(
    preview: &HttpRequestPreview,
    timeout_ms: u64,
) -> Result<HttpCallResult, ProviderConfigError> {
    let (host, port, path) = parse_http_target(&preview.url)?;
    let timeout_duration = Duration::from_millis(timeout_ms);
    let target_addr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| ProviderConfigError {
            field: "http_connect".to_string(),
            message: format!("target={} error={error}", preview.url),
        })?
        .next()
        .ok_or_else(|| ProviderConfigError {
            field: "http_connect".to_string(),
            message: format!("target={} error=no_resolved_address", preview.url),
        })?;
    let mut stream =
        TcpStream::connect_timeout(&target_addr, timeout_duration).map_err(|error| {
            ProviderConfigError {
                field: "http_connect".to_string(),
                message: format!("target={} error={error}", preview.url),
            }
        })?;
    stream
        .set_read_timeout(Some(timeout_duration))
        .map_err(|error| ProviderConfigError {
            field: "http_timeout".to_string(),
            message: format!("set_read_timeout failed: {error}"),
        })?;
    stream
        .set_write_timeout(Some(timeout_duration))
        .map_err(|error| ProviderConfigError {
            field: "http_timeout".to_string(),
            message: format!("set_write_timeout failed: {error}"),
        })?;

    let request_text = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        host,
        preview
            .headers
            .get("authorization")
            .map(String::as_str)
            .unwrap_or(""),
        preview
            .headers
            .get("content-type")
            .map(String::as_str)
            .unwrap_or("application/json"),
        preview.body_json.len(),
        preview.body_json,
    );
    stream
        .write_all(request_text.as_bytes())
        .map_err(|error| ProviderConfigError {
            field: "http_write".to_string(),
            message: error.to_string(),
        })?;
    stream.flush().map_err(|error| ProviderConfigError {
        field: "http_flush".to_string(),
        message: error.to_string(),
    })?;

    let mut raw_response = String::new();
    stream
        .read_to_string(&mut raw_response)
        .map_err(|error| ProviderConfigError {
            field: if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                "http_timeout".to_string()
            } else {
                "http_read".to_string()
            },
            message: if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                format!("request timed out after {}ms: {error}", timeout_ms)
            } else {
                error.to_string()
            },
        })?;

    let (status_code, response_body_json) = parse_http_response(&raw_response)?;

    Ok(HttpCallResult {
        status_code,
        url: preview.url.clone(),
        request_body_json: preview.body_json.clone(),
        response_body_json,
    })
}

/// hyper-native transport（provider_openai_compatible 与 anthropic 共享）。
pub(crate) fn execute_native_transport(
    preview: &HttpRequestPreview,
    timeout_ms: u64,
    tls_ca_cert_path: Option<&PathBuf>,
) -> Result<HttpCallResult, ProviderConfigError> {
    let url = preview.url.clone();
    let request_body_json = preview.body_json.clone();
    let runtime = TokioRuntimeBuilder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| ProviderConfigError {
            field: "native_http_runtime".to_string(),
            message: error.to_string(),
        })?;

    runtime.block_on(async move {
        let connector = build_native_https_connector(tls_ca_cert_path)?;
        let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(connector);
        let authorization = preview
            .headers
            .get("authorization")
            .cloned()
            .unwrap_or_else(|| "Bearer".to_string());
        let content_type = preview
            .headers
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "application/json".to_string());
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Authorization", authorization)
            .header("Content-Type", content_type)
            .body(Full::new(Bytes::from(request_body_json.clone())))
            .map_err(|error| ProviderConfigError {
                field: "native_http_request".to_string(),
                message: error.to_string(),
            })?;
        let response = timeout(Duration::from_millis(timeout_ms), client.request(req))
            .await
            .map_err(|_| ProviderConfigError {
                field: "native_http_timeout".to_string(),
                message: format!("request timed out after {}ms", timeout_ms),
            })?
            .map_err(|error| ProviderConfigError {
                field: "native_http_send".to_string(),
                message: error.to_string(),
            })?;
        let status_code = response.status().as_u16();
        let response_body_json = timeout(
            Duration::from_millis(timeout_ms),
            response.into_body().collect(),
        )
        .await
        .map_err(|_| ProviderConfigError {
            field: "native_http_timeout".to_string(),
            message: format!("response body timed out after {}ms", timeout_ms),
        })?
        .map_err(|error| ProviderConfigError {
            field: "native_http_response_body".to_string(),
            message: error.to_string(),
        })?
        .to_bytes();

        Ok(HttpCallResult {
            status_code,
            url,
            request_body_json,
            response_body_json: String::from_utf8_lossy(&response_body_json).to_string(),
        })
    })
}

/// curl transport（provider_openai_compatible 与 anthropic 共享）。
pub(crate) fn execute_curl_transport(
    preview: &HttpRequestPreview,
    timeout_ms: u64,
) -> Result<HttpCallResult, ProviderConfigError> {
    let authorization_header = preview
        .headers
        .get("authorization")
        .map(|value| format!("Authorization: {value}"))
        .unwrap_or_else(|| "Authorization:".to_string());
    let content_type_header = preview
        .headers
        .get("content-type")
        .map(|value| format!("Content-Type: {value}"))
        .unwrap_or_else(|| "Content-Type: application/json".to_string());
    let args = vec![
        "--silent".to_string(),
        "--show-error".to_string(),
        "--location".to_string(),
        "--max-time".to_string(),
        curl_max_time_seconds(timeout_ms).to_string(),
        "--request".to_string(),
        "POST".to_string(),
        "--header".to_string(),
        authorization_header,
        "--header".to_string(),
        content_type_header,
        "--data-binary".to_string(),
        "@-".to_string(),
        "--write-out".to_string(),
        "\n__CHUANG_CURL_STATUS__:%{http_code}".to_string(),
        preview.url.clone(),
    ];
    let mut child = Command::new("curl")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ProviderConfigError {
            field: "curl_spawn".to_string(),
            message: error.to_string(),
        })?;

    let mut stdin = child.stdin.take().ok_or_else(|| ProviderConfigError {
        field: "curl_stdin".to_string(),
        message: "stdin_unavailable".to_string(),
    })?;
    stdin
        .write_all(preview.body_json.as_bytes())
        .map_err(|error| ProviderConfigError {
            field: "curl_write".to_string(),
            message: error.to_string(),
        })?;
    stdin.flush().map_err(|error| ProviderConfigError {
        field: "curl_write".to_string(),
        message: error.to_string(),
    })?;
    drop(stdin);

    let output = wait_with_timeout(child, timeout_ms).map_err(|error| ProviderConfigError {
        field: "curl_wait".to_string(),
        message: error.to_string(),
    })?;
    if !output.status.success() {
        return Err(ProviderConfigError {
            field: "curl_exit".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let (response_body_json, status_raw) = stdout
        .rsplit_once("\n__CHUANG_CURL_STATUS__:")
        .ok_or_else(|| ProviderConfigError {
            field: "curl_response".to_string(),
            message: "missing_status_marker".to_string(),
        })?;
    let status_code = status_raw
        .trim()
        .parse::<u16>()
        .map_err(|_| ProviderConfigError {
            field: "curl_response".to_string(),
            message: format!("invalid_status_code:{status_raw}"),
        })?;

    Ok(HttpCallResult {
        status_code,
        url: preview.url.clone(),
        request_body_json: preview.body_json.clone(),
        response_body_json: response_body_json.to_string(),
    })
}

fn build_native_https_connector(
    tls_ca_cert_path: Option<&PathBuf>,
) -> Result<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    ProviderConfigError,
> {
    if let Some(path) = tls_ca_cert_path {
        let roots = load_root_store_from_pem(path)?;
        let client_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(HttpsConnectorBuilder::new()
            .with_tls_config(client_config)
            .https_or_http()
            .enable_http1()
            .build())
    } else {
        let tls_builder = match HttpsConnectorBuilder::new().with_native_roots() {
            Ok(builder) => builder,
            Err(_) => HttpsConnectorBuilder::new().with_webpki_roots(),
        };
        Ok(tls_builder.https_or_http().enable_http1().build())
    }
}

impl ProviderAdapterResponder for OpenAICompatibleProviderAdapter {
    fn identity(&self) -> ProviderIdentity {
        self.identity.clone()
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        let ctx = ProviderRespondContext {
            identity: &self.identity,
            base_url: &self.base_url,
            api_key: &self.api_key,
            transport: self.transport.clone(),
            request_timeout_ms: self.request_timeout_ms,
            tls_ca_cert_path: self.tls_ca_cert_path.as_ref(),
            transport_label: "openai-compatible",
            default_finish_reason: default_finish_reason_for_transport,
        };
        run_provider_respond(
            &ctx,
            request,
            |req| self.build_http_request_preview(req),
            |identity, req, _preview| build_openai_stub_response_body(identity, req),
        )
    }
}

/// 通用 provider 响应核心（OpenAI / Anthropic 共享）：重试、扣留、看门狗、
/// success/http-error/config-error 响应构造。`build_preview` 按 provider 方言
/// 构造请求；`stub_response_body` 负责 stub transport 的响应体。
pub(crate) struct ProviderRespondContext<'a> {
    pub identity: &'a ProviderIdentity,
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub transport: ProviderTransport,
    pub request_timeout_ms: u64,
    pub tls_ca_cert_path: Option<&'a PathBuf>,
    /// 日志/元数据里的 transport 标签，如 "openai-compatible" / "anthropic-compatible"。
    pub transport_label: &'static str,
    /// 成功响应缺失 finish_reason 时的默认值（按 transport 区分）。
    pub default_finish_reason: fn(ProviderTransport) -> &'static str,
}

pub(crate) fn run_provider_respond(
    ctx: &ProviderRespondContext,
    request: &ResponderRequest,
    build_preview: impl Fn(&ResponderRequest) -> Result<HttpRequestPreview, ProviderConfigError>,
    stub_response_body: impl Fn(&ProviderIdentity, &ResponderRequest, &HttpRequestPreview) -> String,
) -> ProviderAdapterResponse {
    let masked_key = if ctx.api_key.is_empty() {
        "missing".to_string()
    } else {
        format!("len:{}", ctx.api_key.len())
    };
    let started = Instant::now();
    let mut retry_attempts = 0usize;

    for attempt in 0..MAX_PROVIDER_ATTEMPTS {
        // 空闲看门狗：单次调用（含重试序列）总时长超过 90s 即中断，不再发起新尝试。
        // 单次 transport 尝试本身仍受 request_timeout_ms（provider_timeout_ms 配置）约束。
        if started.elapsed() >= Duration::from_millis(PROVIDER_IDLE_WATCHDOG_KILL_MS) {
            return build_watchdog_kill_response(
                ctx,
                request,
                &masked_key,
                started.elapsed(),
                &build_preview,
            );
        }
        match execute_transport_for(ctx, request, &build_preview, &stub_response_body) {
            Ok(call) => {
                let status_code = call.status_code();
                if !http_status_is_success(status_code) {
                    // 扣留机制：可恢复错误先扣留（重试），全部恢复失败才释放冒泡。
                    // Transient gateway/limit errors are retried with
                    // backoff; auth and other hard errors are not.
                    if attempt + 1 < MAX_PROVIDER_ATTEMPTS
                        && RETRYABLE_STATUS_CODES.contains(&status_code)
                    {
                        retry_attempts += 1;
                        std::thread::sleep(Duration::from_millis(backoff_ms(attempt)));
                        continue;
                    }
                    let mut response = build_http_error_response(ctx, request, call, &masked_key);
                    annotate_retry_and_watchdog(&mut response, started, retry_attempts, "released");
                    return response;
                }
                let mut response = build_success_response(ctx, request, call, &masked_key);
                annotate_retry_and_watchdog(&mut response, started, retry_attempts, "recovered");
                return response;
            }
            Err(error) => {
                let preview = build_preview(request).ok();
                let error_class = provider_error_class(&error);
                // ECONNRESET / ETIMEDOUT 类传输错误与 429/529 一样先扣留重试，
                // 重试耗尽才释放；配置/协议等硬错误不重试直接释放。
                if attempt + 1 < MAX_PROVIDER_ATTEMPTS && transport_retryable(&error, error_class) {
                    retry_attempts += 1;
                    std::thread::sleep(Duration::from_millis(backoff_ms(attempt)));
                    continue;
                }
                let mut response = ProviderAdapterResponse {
                    body: format!(
                        "CONFIG_ERROR: {} provider invalid field={} reason={}",
                        ctx.transport_label, error.field, error.message
                    ),
                    trace: format!(
                        "transport={} provider={} model={} config_error_field={} reason={}",
                        ctx.transport_label,
                        ctx.identity.provider_id,
                        ctx.identity.model_name,
                        error.field,
                        error.message
                    ),
                    finish_reason: Some("invalid-config".to_string()),
                    extra_meta: build_config_error_meta(
                        &error,
                        preview.as_ref(),
                        ctx.transport.as_str(),
                        ctx.request_timeout_ms,
                        ctx.transport_label,
                    ),
                };
                annotate_retry_and_watchdog(&mut response, started, retry_attempts, "released");
                return response;
            }
        }
    }
    unreachable!("provider respond loop always returns")
}

fn execute_transport_for(
    ctx: &ProviderRespondContext,
    request: &ResponderRequest,
    build_preview: &impl Fn(&ResponderRequest) -> Result<HttpRequestPreview, ProviderConfigError>,
    stub_response_body: &impl Fn(&ProviderIdentity, &ResponderRequest, &HttpRequestPreview) -> String,
) -> Result<TransportCallResult, ProviderConfigError> {
    let preview = build_preview(request)?;
    match ctx.transport {
        ProviderTransport::Stub => Ok(TransportCallResult::Stub(StubHttpCallResult {
            status_code: 200,
            url: preview.url.clone(),
            request_body_json: preview.body_json.clone(),
            response_body_json: stub_response_body(ctx.identity, request, &preview),
        })),
        ProviderTransport::Http => {
            execute_http_transport(&preview, ctx.request_timeout_ms).map(TransportCallResult::Http)
        }
        ProviderTransport::Native => {
            execute_native_transport(&preview, ctx.request_timeout_ms, ctx.tls_ca_cert_path)
                .map(TransportCallResult::Native)
        }
        ProviderTransport::Curl => {
            execute_curl_transport(&preview, ctx.request_timeout_ms).map(TransportCallResult::Curl)
        }
    }
}

fn annotate_retry_and_watchdog(
    response: &mut ProviderAdapterResponse,
    started: Instant,
    retry_attempts: usize,
    retry_outcome: &str,
) {
    if retry_attempts > 0 {
        response.extra_meta.insert(
            "provider_retry_attempts".to_string(),
            retry_attempts.to_string(),
        );
        response.extra_meta.insert(
            "provider_retry_outcome".to_string(),
            retry_outcome.to_string(),
        );
    }
    let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    if elapsed_ms >= PROVIDER_IDLE_WATCHDOG_WARN_MS {
        response
            .extra_meta
            .insert("provider_watchdog_warned".to_string(), "true".to_string());
        response.extra_meta.insert(
            "provider_watchdog_elapsed_ms".to_string(),
            elapsed_ms.to_string(),
        );
    }
}

fn build_watchdog_kill_response(
    ctx: &ProviderRespondContext,
    request: &ResponderRequest,
    masked_key: &str,
    elapsed: Duration,
    build_preview: &impl Fn(&ResponderRequest) -> Result<HttpRequestPreview, ProviderConfigError>,
) -> ProviderAdapterResponse {
    let elapsed_ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
    let message = format!(
        "request timed out after {elapsed_ms}ms (idle watchdog kill, limit {PROVIDER_IDLE_WATCHDOG_KILL_MS}ms)"
    );
    let preview = build_preview(request).ok();
    let mut extra_meta = BTreeMap::from([
        ("transport".to_string(), ctx.transport_label.to_string()),
        ("provider_response_ok".to_string(), "false".to_string()),
        ("provider_error_class".to_string(), "transport".to_string()),
        ("provider_error_message".to_string(), message.clone()),
        // 看门狗中断即扣留释放：本调用已耗尽恢复预算，交给上层（fallback/循环层）。
        ("provider_retryable".to_string(), "false".to_string()),
        ("provider_watchdog_killed".to_string(), "true".to_string()),
        (
            "provider_watchdog_elapsed_ms".to_string(),
            elapsed_ms.to_string(),
        ),
    ]);
    if let Some(preview) = preview {
        extra_meta.insert("request_url".to_string(), preview.url.clone());
        extra_meta.insert("request_method".to_string(), preview.method.clone());
        extra_meta.insert(
            "request_message_count".to_string(),
            request_message_count(&preview.body_json).to_string(),
        );
    }
    insert_provider_failure_meta(&mut extra_meta, None, "transport", Some(&message));
    insert_provider_timeout_meta(&mut extra_meta, None, "transport", Some(&message));
    ProviderAdapterResponse {
        body: format!(
            "PROVIDER_IDLE_TIMEOUT: provider={} model={} transport={} elapsed_ms={elapsed_ms}",
            ctx.identity.provider_id,
            ctx.identity.model_name,
            ctx.transport.as_str(),
        ),
        trace: format!(
            "transport={} provider={} model={} base_url={} api_key={} recall_hits={} transport_mode={} provider_idle_timeout_elapsed_ms={elapsed_ms}",
            ctx.transport_label,
            ctx.identity.provider_id,
            ctx.identity.model_name,
            ctx.base_url,
            masked_key,
            request.recall_hit_count,
            ctx.transport.as_str(),
        ),
        finish_reason: Some("provider-idle-timeout".to_string()),
        extra_meta,
    }
}

fn build_success_response(
    ctx: &ProviderRespondContext,
    request: &ResponderRequest,
    call: TransportCallResult,
    masked_key: &str,
) -> ProviderAdapterResponse {
    let response_body = call.response_body_json();
    let assistant_content = extract_assistant_content(response_body);
    let mut extra_meta = build_success_meta(&call, ctx.transport_label);
    let (body, finish_reason) = if let Some(content) = assistant_content {
        extra_meta.insert("provider_response_ok".to_string(), "true".to_string());
        (
            content,
            extract_finish_reason(response_body)
                .or_else(|| Some((ctx.default_finish_reason)(call.transport()).to_string())),
        )
    } else {
        extra_meta.insert("provider_response_ok".to_string(), "false".to_string());
        extra_meta.insert(
            "provider_error_class".to_string(),
            "missing_content".to_string(),
        );
        extra_meta.insert(
            "provider_error_message".to_string(),
            "missing assistant content in successful provider response".to_string(),
        );
        // deepseek 等推理模型偶发 200+空 content，重试一次通常即恢复，
        // 循环层按 provider_retryable=true 走自动重试。
        extra_meta.insert("provider_retryable".to_string(), "true".to_string());
        insert_provider_failure_meta(
            &mut extra_meta,
            Some(call.status_code()),
            "missing_content",
            Some("missing assistant content in successful provider response"),
        );
        (
            format!(
                "PROVIDER_MISSING_CONTENT: provider={} model={} transport={} status_code={} response_kind={}",
                ctx.identity.provider_id,
                ctx.identity.model_name,
                call.transport().as_str(),
                call.status_code(),
                extra_meta
                    .get("response_kind")
                    .map(String::as_str)
                    .unwrap_or("unknown")
            ),
            Some("provider-error-missing-content".to_string()),
        )
    };
    ProviderAdapterResponse {
        body,
        trace: format!(
            "transport={} provider={} model={} base_url={} api_key={} recall_hits={} message_count={} request_url={} status_code={} transport_mode={}",
            ctx.transport_label,
            ctx.identity.provider_id,
            ctx.identity.model_name,
            ctx.base_url,
            masked_key,
            request.recall_hit_count,
            request_message_count(call.request_body_json()),
            call.url(),
            call.status_code(),
            call.transport().as_str(),
        ),
        finish_reason,
        extra_meta,
    }
}

fn build_http_error_response(
    ctx: &ProviderRespondContext,
    request: &ResponderRequest,
    call: TransportCallResult,
    masked_key: &str,
) -> ProviderAdapterResponse {
    let response_body = call.response_body_json();
    let status_code = call.status_code();
    let error_message = extract_provider_error_message(response_body)
        .unwrap_or_else(|| format!("status_code={status_code}"));
    ProviderAdapterResponse {
        body: format!(
            "PROVIDER_HTTP_ERROR: provider={} model={} transport={} status_code={} error={}",
            ctx.identity.provider_id,
            ctx.identity.model_name,
            call.transport().as_str(),
            status_code,
            error_message
        ),
        trace: format!(
            "transport={} provider={} model={} base_url={} api_key={} recall_hits={} message_count={} request_url={} status_code={} transport_mode={} provider_http_error={}",
            ctx.transport_label,
            ctx.identity.provider_id,
            ctx.identity.model_name,
            ctx.base_url,
            masked_key,
            request.recall_hit_count,
            request_message_count(call.request_body_json()),
            call.url(),
            status_code,
            call.transport().as_str(),
            error_message,
        ),
        finish_reason: Some(format!("http-error-{status_code}")),
        extra_meta: {
            let mut meta = build_success_meta(&call, ctx.transport_label);
            meta.insert("provider_response_ok".to_string(), "false".to_string());
            meta.insert("provider_error_class".to_string(), "http_status".to_string());
            meta.insert("provider_error_message".to_string(), error_message.clone());
            insert_provider_failure_meta(
                &mut meta,
                Some(status_code),
                "http_status",
                Some(&error_message),
            );
            insert_provider_timeout_meta(
                &mut meta,
                Some(status_code),
                "http_status",
                Some(&error_message),
            );
            meta
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransportCallResult {
    Stub(StubHttpCallResult),
    Http(HttpCallResult),
    Native(HttpCallResult),
    Curl(HttpCallResult),
}

impl TransportCallResult {
    pub(crate) fn transport(&self) -> ProviderTransport {
        match self {
            Self::Stub(_) => ProviderTransport::Stub,
            Self::Http(_) => ProviderTransport::Http,
            Self::Native(_) => ProviderTransport::Native,
            Self::Curl(_) => ProviderTransport::Curl,
        }
    }

    pub(crate) fn status_code(&self) -> u16 {
        match self {
            Self::Stub(result) => result.status_code,
            Self::Http(result) => result.status_code,
            Self::Native(result) => result.status_code,
            Self::Curl(result) => result.status_code,
        }
    }

    pub(crate) fn url(&self) -> &str {
        match self {
            Self::Stub(result) => &result.url,
            Self::Http(result) => &result.url,
            Self::Native(result) => &result.url,
            Self::Curl(result) => &result.url,
        }
    }

    pub(crate) fn request_body_json(&self) -> &str {
        match self {
            Self::Stub(result) => &result.request_body_json,
            Self::Http(result) => &result.request_body_json,
            Self::Native(result) => &result.request_body_json,
            Self::Curl(result) => &result.request_body_json,
        }
    }

    pub(crate) fn response_body_json(&self) -> &str {
        match self {
            Self::Stub(result) => &result.response_body_json,
            Self::Http(result) => &result.response_body_json,
            Self::Native(result) => &result.response_body_json,
            Self::Curl(result) => &result.response_body_json,
        }
    }
}

fn build_success_meta(
    call: &TransportCallResult,
    transport_label: &'static str,
) -> BTreeMap<String, String> {
    let usage_meta = extract_usage_meta(call.response_body_json());
    let response_kind_value =
        response_kind(call.response_body_json()).unwrap_or_else(|| "unknown".to_string());
    let response_finish_reason = extract_finish_reason(call.response_body_json())
        .unwrap_or_else(|| default_finish_reason_for_transport(call.transport()).to_string());

    let mut meta = BTreeMap::from([
        ("transport".to_string(), transport_label.to_string()),
        (
            "transport_mode".to_string(),
            call.transport().as_str().to_string(),
        ),
        ("request_url".to_string(), call.url().to_string()),
        ("request_method".to_string(), "POST".to_string()),
        (
            "request_message_count".to_string(),
            request_message_count(call.request_body_json()).to_string(),
        ),
        ("status_code".to_string(), call.status_code().to_string()),
        (
            "provider_retryable".to_string(),
            http_status_retryable(call.status_code()).to_string(),
        ),
        ("response_kind".to_string(), response_kind_value.clone()),
        ("response_finish_reason".to_string(), response_finish_reason),
    ]);
    meta.extend(usage_meta);

    if matches!(call.transport(), ProviderTransport::Stub) {
        meta.insert(
            "stub_status_code".to_string(),
            call.status_code().to_string(),
        );
        meta.insert("stub_response_kind".to_string(), response_kind_value);
    }

    meta
}

fn extract_usage_meta(response_body_json: &str) -> BTreeMap<String, String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response_body_json) else {
        return BTreeMap::new();
    };
    let Some(usage) = value.get("usage") else {
        return BTreeMap::new();
    };

    let mut meta = BTreeMap::new();
    if let Some(value) = usage.get("prompt_tokens").and_then(|value| value.as_u64()) {
        meta.insert("prompt_tokens".to_string(), value.to_string());
    } else if let Some(value) = usage.get("input_tokens").and_then(|value| value.as_u64()) {
        meta.insert("prompt_tokens".to_string(), value.to_string());
    }

    if let Some(value) = usage
        .get("completion_tokens")
        .and_then(|value| value.as_u64())
    {
        meta.insert("completion_tokens".to_string(), value.to_string());
    } else if let Some(value) = usage.get("output_tokens").and_then(|value| value.as_u64()) {
        meta.insert("completion_tokens".to_string(), value.to_string());
    }

    if let Some(value) = usage.get("total_tokens").and_then(|value| value.as_u64()) {
        meta.insert("total_tokens".to_string(), value.to_string());
    }

    meta
}

fn parse_http_target(url: &str) -> Result<(String, u16, String), ProviderConfigError> {
    let without_scheme = url
        .strip_prefix("http://")
        .ok_or_else(|| ProviderConfigError {
            field: "base_url".to_string(),
            message: format!("unsupported_http_scheme:{url}"),
        })?;
    let (host_port, path_suffix) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));
    let path = format!("/{}", path_suffix);
    let (host, port) = match host_port.split_once(':') {
        Some((host, port_raw)) => {
            let port = port_raw.parse::<u16>().map_err(|_| ProviderConfigError {
                field: "base_url".to_string(),
                message: format!("invalid_port:{url}"),
            })?;
            (host.to_string(), port)
        }
        None => (host_port.to_string(), 80),
    };

    Ok((host, port, path))
}

fn parse_http_response(raw_response: &str) -> Result<(u16, String), ProviderConfigError> {
    let (head, body) = raw_response
        .split_once("\r\n\r\n")
        .ok_or_else(|| ProviderConfigError {
            field: "http_response".to_string(),
            message: "missing_header_separator".to_string(),
        })?;
    let status_line = head.lines().next().ok_or_else(|| ProviderConfigError {
        field: "http_response".to_string(),
        message: "missing_status_line".to_string(),
    })?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| ProviderConfigError {
            field: "http_response".to_string(),
            message: format!("missing_status_code:{status_line}"),
        })?
        .parse::<u16>()
        .map_err(|_| ProviderConfigError {
            field: "http_response".to_string(),
            message: format!("invalid_status_code:{status_line}"),
        })?;

    Ok((status_code, body.to_string()))
}

fn build_config_error_meta(
    error: &ProviderConfigError,
    preview: Option<&HttpRequestPreview>,
    transport_mode: &str,
    timeout_ms: u64,
    transport_label: &'static str,
) -> BTreeMap<String, String> {
    let error_class = provider_error_class(error);
    let mut meta = BTreeMap::from([
        ("transport".to_string(), transport_label.to_string()),
        ("transport_mode".to_string(), transport_mode.to_string()),
        ("config_error_field".to_string(), error.field.clone()),
        ("provider_error_class".to_string(), error_class.to_string()),
        (
            "provider_retryable".to_string(),
            provider_error_retryable(error, error_class).to_string(),
        ),
        ("provider_timeout_ms".to_string(), timeout_ms.to_string()),
    ]);

    if let Some(preview) = preview {
        meta.insert("request_url".to_string(), preview.url.clone());
        meta.insert("request_method".to_string(), preview.method.clone());
        meta.insert(
            "request_message_count".to_string(),
            request_message_count(&preview.body_json).to_string(),
        );
    }

    insert_provider_failure_meta(&mut meta, None, error_class, Some(&error.message));
    insert_provider_timeout_meta(&mut meta, None, error_class, Some(&error.message));

    meta
}

fn insert_provider_failure_meta(
    meta: &mut BTreeMap<String, String>,
    status_code: Option<u16>,
    error_class: &str,
    error_message: Option<&str>,
) {
    let (reason_code, category) = provider_failure_reason(status_code, error_class, error_message);
    meta.insert(
        "provider_failure_reason_code".to_string(),
        reason_code.to_string(),
    );
    meta.insert(
        "provider_failure_category".to_string(),
        category.to_string(),
    );
}

fn insert_provider_timeout_meta(
    meta: &mut BTreeMap<String, String>,
    status_code: Option<u16>,
    error_class: &str,
    error_message: Option<&str>,
) {
    if !is_timeout_error(
        status_code,
        error_class,
        error_message.unwrap_or("").to_ascii_lowercase().as_str(),
    ) {
        return;
    }

    meta.insert(
        "provider_timeout_reason_code".to_string(),
        "request_timeout".to_string(),
    );
    meta.insert(
        "provider_timeout_category".to_string(),
        "timeout".to_string(),
    );
}

fn provider_failure_reason(
    status_code: Option<u16>,
    error_class: &str,
    error_message: Option<&str>,
) -> (&'static str, &'static str) {
    let message = error_message.unwrap_or("").to_ascii_lowercase();
    if is_timeout_error(status_code, error_class, &message) {
        return ("request_timeout", "timeout");
    }
    if message.contains("capacity") || message.contains("overloaded") {
        return ("model_capacity", "capacity");
    }
    if message.contains("rate limit") || message.contains("rate_limit") {
        return ("rate_limited", "rate_limit");
    }
    if message.contains("quota") || message.contains("billing") {
        return ("quota_or_billing", "quota");
    }
    if error_class == "missing_content" {
        return ("missing_content", "response");
    }

    if let Some(status_code) = status_code {
        return match status_code {
            401 => ("auth_failed", "auth"),
            402 => ("quota_or_billing", "quota"),
            403 => ("permission_denied", "auth"),
            408 => ("request_timeout", "timeout"),
            429 => ("rate_limited", "rate_limit"),
            500..=599 => ("upstream_unavailable", "upstream"),
            _ => ("http_status_error", "http_status"),
        };
    }

    match error_class {
        "transport" => ("transport_failure", "transport"),
        "tls" => ("tls_failure", "transport"),
        "protocol" => ("provider_protocol_failure", "protocol"),
        "config" => ("provider_config_invalid", "config"),
        "missing_content" => ("missing_content", "response"),
        _ => ("provider_failure_unknown", "unknown"),
    }
}

fn is_timeout_error(status_code: Option<u16>, error_class: &str, message: &str) -> bool {
    if status_code == Some(408) {
        return true;
    }

    error_class == "transport" && message.contains("timed out")
}

fn provider_error_class(error: &ProviderConfigError) -> &'static str {
    match error.field.as_str() {
        "base_url" | "model_name" => "config",
        "http_connect"
        | "http_write"
        | "http_flush"
        | "http_read"
        | "http_timeout"
        | "native_http_timeout"
        | "native_http_send"
        | "native_http_response_body"
        | "curl_spawn"
        | "curl_stdin"
        | "curl_write"
        | "curl_wait"
        | "curl_exit" => "transport",
        "native_http_tls" => "tls",
        "http_response" | "curl_response" | "native_http_request" | "native_http_runtime" => {
            "protocol"
        }
        _ => "unknown",
    }
}

fn provider_error_retryable(error: &ProviderConfigError, error_class: &str) -> bool {
    if error.field == "curl_spawn" {
        return false;
    }

    if matches!(error_class, "transport") {
        return true;
    }

    matches!(
        error.field.as_str(),
        "native_http_timeout" | "http_timeout" | "curl_wait"
    )
}

/// withRetry 传输层重试判定（蓝本 §2.5）：只重试 ECONNRESET / ETIMEDOUT 类错误
/// （connection reset / broken pipe / timed out / would block）。配置/协议等
/// 硬错误不重试。比 provider_error_retryable 更窄，供 respond 重试循环使用。
fn transport_retryable(error: &ProviderConfigError, error_class: &str) -> bool {
    if error.field == "curl_spawn" {
        return false;
    }
    if matches!(
        error.field.as_str(),
        "native_http_timeout" | "http_timeout" | "curl_wait"
    ) {
        return true;
    }
    if error_class == "transport" {
        let message = error.message.to_ascii_lowercase();
        return message.contains("timed out")
            || message.contains("connection reset")
            || message.contains("broken pipe")
            || message.contains("reset by peer")
            || message.contains("would block");
    }
    false
}

fn http_status_retryable(status_code: u16) -> bool {
    // 蓝本 withRetry 关键类目 429（限流）/529（上游不可达）；保留 408/5xx 网关语义。
    matches!(status_code, 408 | 429 | 529) || (500..=599).contains(&status_code)
}

fn http_status_is_success(status_code: u16) -> bool {
    (200..=299).contains(&status_code)
}

fn load_root_store_from_pem(path: &PathBuf) -> Result<RootCertStore, ProviderConfigError> {
    let pem = fs::read(path).map_err(|error| ProviderConfigError {
        field: "native_http_tls".to_string(),
        message: format!(
            "failed_to_read_tls_ca_path={} error={error}",
            path.display()
        ),
    })?;
    let cert_chain = CertificateDer::pem_slice_iter(&pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ProviderConfigError {
            field: "native_http_tls".to_string(),
            message: format!(
                "failed_to_parse_tls_ca_path={} error={error}",
                path.display()
            ),
        })?;
    if cert_chain.is_empty() {
        return Err(ProviderConfigError {
            field: "native_http_tls".to_string(),
            message: format!("no_certificates_found_in_tls_ca_path={}", path.display()),
        });
    }
    let mut roots = RootCertStore::empty();
    let _ = roots.add_parsable_certificates(cert_chain);
    Ok(roots)
}

fn curl_max_time_seconds(timeout_ms: u64) -> u64 {
    timeout_ms.saturating_add(999).max(1) / 1000
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout_ms: u64,
) -> std::io::Result<std::process::Output> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "curl command timed out after {timeout_ms}ms status={:?}",
                    output.status.code()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn request_message_count(body_json: &str) -> usize {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body_json) else {
        return 0;
    };

    if let Some(messages) = value
        .get("messages")
        .and_then(|messages| messages.as_array())
    {
        return messages.len();
    }

    count_request_item(value.get("instructions")) + count_request_item(value.get("input"))
}

fn count_request_item(value: Option<&serde_json::Value>) -> usize {
    match value {
        Some(serde_json::Value::Array(items)) => items.len(),
        Some(serde_json::Value::Null) | None => 0,
        Some(_) => 1,
    }
}

fn extract_assistant_content(response_body_json: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(response_body_json).ok()?;
    extract_assistant_content_from_value(&value)
}

fn extract_assistant_content_from_value(value: &serde_json::Value) -> Option<String> {
    if let Some(content) = value.get("output_text").and_then(|value| value.as_str()) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(output) = value.get("output").and_then(|value| value.as_array()) {
        for item in output {
            if let Some(text) = extract_text_field(item) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(content) = item.get("content") {
                if let Some(text) = extract_text_like_value(content) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    if let Some(choices) = value.get("choices").and_then(|value| value.as_array()) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                if let Some(text) = extract_text_field(message) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                if let Some(content) = message.get("content") {
                    if let Some(text) = extract_text_like_value(content) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }

            if let Some(delta) = choice.get("delta") {
                if let Some(text) = extract_text_field(delta) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                if let Some(content) = delta.get("content") {
                    if let Some(text) = extract_text_like_value(content) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }

            if let Some(text) = extract_text_field(choice) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    if let Some(text) = extract_text_field(value) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn extract_text_field(value: &serde_json::Value) -> Option<String> {
    value
        .get("text")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| value.get("content").and_then(extract_text_like_value))
        .or_else(|| {
            value
                .get("value")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
}

fn extract_text_like_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(extract_text_like_value)
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(""))
            }
        }
        serde_json::Value::Object(map) => map
            .get("text")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| map.get("content").and_then(extract_text_like_value))
            .or_else(|| {
                map.get("value")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            }),
        _ => None,
    }
}

fn extract_finish_reason(response_body_json: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response_body_json) else {
        return None;
    };

    // Anthropic Messages API：顶层 `stop_reason`（如 end_turn / max_tokens / stop_sequence）。
    if let Some(reason) = value.get("stop_reason").and_then(|reason| reason.as_str()) {
        return Some(reason.to_string());
    }

    if let Some(reason) = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(|finish_reason| finish_reason.as_str())
    {
        return Some(reason.to_string());
    }

    if let Some(status) = value.get("status").and_then(|status| status.as_str()) {
        match status {
            "completed" => return Some("stop".to_string()),
            "incomplete" => {
                if let Some(reason) = value
                    .get("incomplete_details")
                    .and_then(|details| details.get("reason"))
                    .and_then(|reason| reason.as_str())
                {
                    return Some(reason.to_string());
                }
                return Some("incomplete".to_string());
            }
            "failed" => return Some("failed".to_string()),
            other => return Some(other.to_string()),
        }
    }

    None
}

fn extract_provider_error_message(response_body_json: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response_body_json) else {
        let trimmed = response_body_json.trim();
        return if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    };
    if let Some(message) = value.get("error").and_then(|value| {
        if let Some(text) = value.as_str() {
            Some(text.to_string())
        } else {
            value
                .get("message")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    value
                        .get("detail")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .or_else(|| {
                    value
                        .get("code")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
        }
    }) {
        return Some(message);
    }

    if let Some(message) = value.as_str() {
        return Some(message.to_string());
    }

    value
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("detail")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
}

fn response_kind(response_body_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(response_body_json)
        .ok()
        .and_then(|value| {
            // OpenAI 系：`object` 字段（如 "response" / "chat.completion"）。
            if let Some(kind) = value.get("object").and_then(|value| value.as_str()) {
                return Some(kind.to_string());
            }
            // Anthropic Messages API：`type` 字段（如 "message"）。
            value
                .get("type")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
}

fn default_finish_reason_for_transport(transport: ProviderTransport) -> &'static str {
    match transport {
        ProviderTransport::Stub => "stubbed-openai-compatible",
        ProviderTransport::Http => "http-openai-compatible",
        ProviderTransport::Native => "native-openai-compatible",
        ProviderTransport::Curl => "curl-openai-compatible",
    }
}

#[cfg(test)]
mod tests {
    use super::extract_assistant_content_from_value;
    use super::extract_finish_reason;
    use super::{
        backoff_ms, http_status_retryable, jittered_backoff, provider_error_class,
        provider_error_retryable, transport_retryable, ProviderConfigError, MAX_PROVIDER_ATTEMPTS,
        PROVIDER_IDLE_WATCHDOG_KILL_MS, PROVIDER_IDLE_WATCHDOG_WARN_MS, RETRYABLE_STATUS_CODES,
        RETRY_BACKOFF_BASE_MS, RETRY_BACKOFF_JITTER_RATIO, RETRY_BACKOFF_MAX_MS,
    };
    use serde_json::json;

    #[test]
    fn retry_policy_covers_transient_http_errors() {
        assert!(RETRYABLE_STATUS_CODES.contains(&502));
        assert!(RETRYABLE_STATUS_CODES.contains(&503));
        assert!(RETRYABLE_STATUS_CODES.contains(&429));
        // 蓝本 withRetry 关键类目：429 限流 / 529 上游不可达。
        assert!(RETRYABLE_STATUS_CODES.contains(&529));
        assert!(http_status_retryable(429));
        assert!(http_status_retryable(529));
        const _: () = assert!(MAX_PROVIDER_ATTEMPTS >= 2);
        assert!(backoff_ms(0) < backoff_ms(1));
    }

    #[test]
    fn backoff_matches_blueprint_bounds() {
        assert_eq!(RETRY_BACKOFF_BASE_MS, 500);
        assert_eq!(RETRY_BACKOFF_MAX_MS, 32_000);
        for attempt in 0..10 {
            let value = backoff_ms(attempt);
            assert!(
                value <= RETRY_BACKOFF_MAX_MS,
                "attempt={attempt} value={value} must cap at max"
            );
            let base = RETRY_BACKOFF_BASE_MS
                .saturating_mul(1u64 << attempt.min(6))
                .min(RETRY_BACKOFF_MAX_MS);
            let span = (base as f64 * RETRY_BACKOFF_JITTER_RATIO) as u64;
            assert!(
                value >= base.saturating_sub(span) && value <= base + span,
                "attempt={attempt} value={value} base={base} span={span}"
            );
        }
        // jitter ±25% 下指数退避仍严格单调（下界 0.75*2^n > 上界 1.25*2^(n-1)）。
        for attempt in 1..6 {
            assert!(
                backoff_ms(attempt - 1) < backoff_ms(attempt),
                "attempt {attempt} should back off monotonically"
            );
        }
    }

    #[test]
    fn jittered_backoff_is_deterministic_and_bounded() {
        assert_eq!(jittered_backoff(500, 42), jittered_backoff(500, 42));
        for seed in [0u64, 1, 7, 123_456_789] {
            let value = jittered_backoff(500, seed);
            assert!((375..=625).contains(&value), "seed={seed} value={value}");
        }
    }

    #[test]
    fn retryable_categories_cover_blueprint_transport_errors() {
        // ETIMEDOUT：超时字段（native_http_timeout / http_timeout / curl_wait）必重试。
        let timeout = ProviderConfigError {
            field: "native_http_timeout".to_string(),
            message: "request timed out after 60000ms".to_string(),
        };
        assert_eq!(provider_error_class(&timeout), "transport");
        assert!(provider_error_retryable(&timeout, "transport"));
        assert!(transport_retryable(&timeout, "transport"));

        // ECONNRESET：connection reset / broken pipe / reset by peer。
        for message in [
            "Connection reset by peer",
            "broken pipe",
            "read error: connection reset",
            "operation would block",
        ] {
            let reset = ProviderConfigError {
                field: "http_read".to_string(),
                message: message.to_string(),
            };
            assert!(
                transport_retryable(&reset, "transport"),
                "message={message} should be retryable"
            );
        }

        // 硬错误不重试：配置错误、curl 无法 spawn、非重置类传输错误。
        let config_error = ProviderConfigError {
            field: "base_url".to_string(),
            message: "base_url must not be empty".to_string(),
        };
        assert_eq!(provider_error_class(&config_error), "config");
        assert!(!provider_error_retryable(&config_error, "config"));
        assert!(!transport_retryable(&config_error, "config"));

        let spawn_error = ProviderConfigError {
            field: "curl_spawn".to_string(),
            message: "No such file or directory".to_string(),
        };
        assert!(!transport_retryable(&spawn_error, "transport"));

        let generic_transport = ProviderConfigError {
            field: "http_read".to_string(),
            message: "unexpected EOF".to_string(),
        };
        assert!(!transport_retryable(&generic_transport, "transport"));
    }

    #[test]
    fn watchdog_constants_follow_blueprint() {
        assert_eq!(PROVIDER_IDLE_WATCHDOG_WARN_MS, 45_000);
        assert_eq!(PROVIDER_IDLE_WATCHDOG_KILL_MS, 90_000);
    }

    #[test]
    fn extracts_openai_chat_completion_content() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "  你好，老爸  "
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        assert_eq!(
            extract_assistant_content_from_value(&value).as_deref(),
            Some("你好，老爸")
        );
    }

    #[test]
    fn extracts_output_text_fallback() {
        let value = json!({
            "id": "resp-1",
            "object": "response",
            "output_text": "  我在。  "
        });

        assert_eq!(
            extract_assistant_content_from_value(&value).as_deref(),
            Some("我在。")
        );
    }

    #[test]
    fn extracts_array_content_fallback() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": [
                            { "type": "text", "text": "创" },
                            { "type": "text", "text": "项目" }
                        ]
                    }
                }
            ]
        });

        assert_eq!(
            extract_assistant_content_from_value(&value).as_deref(),
            Some("创项目")
        );
    }

    #[test]
    fn extracts_responses_finish_reason_from_completed_status() {
        let value = json!({
            "id": "resp-1",
            "object": "response",
            "status": "completed",
            "output": []
        });

        assert_eq!(
            extract_finish_reason(&value.to_string()).as_deref(),
            Some("stop")
        );
    }

    #[test]
    fn extracts_responses_finish_reason_from_incomplete_details() {
        let value = json!({
            "id": "resp-2",
            "object": "response",
            "status": "incomplete",
            "incomplete_details": { "reason": "max_output_tokens" },
            "output": []
        });

        assert_eq!(
            extract_finish_reason(&value.to_string()).as_deref(),
            Some("max_output_tokens")
        );
    }
}
