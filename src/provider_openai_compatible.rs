use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rustls::{ClientConfig, RootCertStore};
use rustls_pemfile::certs;
use serde_json::json;
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::time::timeout;

use crate::responder::{
    ProviderAdapterResponder, ProviderAdapterResponse, ProviderIdentity, ResponderRequest,
};

const DEFAULT_PROVIDER_REQUEST_TIMEOUT_MS: u64 = 60_000;

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
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
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
            other => Err(format!(
                "unsupported reasoning effort: {other} (supported: low, medium, high, xhigh)"
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
    reasoning_effort: Option<ReasoningEffort>,
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
            reasoning_effort: None,
            request_timeout_ms: DEFAULT_PROVIDER_REQUEST_TIMEOUT_MS,
            tls_ca_cert_path: None,
        }
    }

    pub fn with_transport(mut self, transport: ProviderTransport) -> Self {
        self.transport = transport;
        self
    }

    pub fn with_reasoning_effort(mut self, reasoning_effort: Option<ReasoningEffort>) -> Self {
        self.reasoning_effort = reasoning_effort;
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
        })
    }

    pub fn build_http_request_preview(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpRequestPreview, ProviderConfigError> {
        let envelope = self.build_request_envelope(request)?;
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
        let response_body_json = json!({
            "id": "resp-stub-001",
            "object": "response",
            "status": "completed",
            "completed_at": 0,
            "provider_id": self.identity.provider_id,
            "model": self.identity.model_name,
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
                                self.identity.provider_id,
                                self.identity.model_name,
                                request.user_input
                            ),
                            "annotations": []
                        }
                    ]
                }
            ],
            "output_text": format!(
                "stubbed_post_ok: provider={} model={} user_input=《{}》",
                self.identity.provider_id,
                self.identity.model_name,
                request.user_input
            ),
            "store": false,
            "usage": {
                "input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": 0
            }
        })
        .to_string();

        Ok(StubHttpCallResult {
            status_code: 200,
            url: preview.url,
            request_body_json: preview.body_json,
            response_body_json,
        })
    }

    pub fn execute_http_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
        let (host, port, path) = parse_http_target(&preview.url)?;
        let timeout_duration = Duration::from_millis(self.request_timeout_ms);
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
                    format!(
                        "request timed out after {}ms: {error}",
                        self.request_timeout_ms
                    )
                } else {
                    error.to_string()
                },
            })?;

        let (status_code, response_body_json) = parse_http_response(&raw_response)?;

        Ok(HttpCallResult {
            status_code,
            url: preview.url,
            request_body_json: preview.body_json,
            response_body_json,
        })
    }

    pub fn execute_native_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
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
            let connector = self.build_native_https_connector()?;
            let client: Client<_, Full<Bytes>> =
                Client::builder(TokioExecutor::new()).build(connector);
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
            let response = timeout(
                Duration::from_millis(self.request_timeout_ms),
                client.request(req),
            )
            .await
            .map_err(|_| ProviderConfigError {
                field: "native_http_timeout".to_string(),
                message: format!("request timed out after {}ms", self.request_timeout_ms),
            })?
            .map_err(|error| ProviderConfigError {
                field: "native_http_send".to_string(),
                message: error.to_string(),
            })?;
            let status_code = response.status().as_u16();
            let response_body_json = timeout(
                Duration::from_millis(self.request_timeout_ms),
                response.into_body().collect(),
            )
            .await
            .map_err(|_| ProviderConfigError {
                field: "native_http_timeout".to_string(),
                message: format!(
                    "response body timed out after {}ms",
                    self.request_timeout_ms
                ),
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

    pub fn execute_curl_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
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
            curl_max_time_seconds(self.request_timeout_ms).to_string(),
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

        let output = wait_with_timeout(child, self.request_timeout_ms).map_err(|error| {
            ProviderConfigError {
                field: "curl_wait".to_string(),
                message: error.to_string(),
            }
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
            url: preview.url,
            request_body_json: preview.body_json,
            response_body_json: response_body_json.to_string(),
        })
    }

    fn execute_transport(
        &self,
        request: &ResponderRequest,
    ) -> Result<TransportCallResult, ProviderConfigError> {
        match self.transport {
            ProviderTransport::Stub => Ok(TransportCallResult::Stub(
                self.execute_stub_post_call(request)?,
            )),
            ProviderTransport::Http => self
                .execute_http_post_call(request)
                .map(TransportCallResult::Http),
            ProviderTransport::Native => self
                .execute_native_post_call(request)
                .map(TransportCallResult::Native),
            ProviderTransport::Curl => self
                .execute_curl_post_call(request)
                .map(TransportCallResult::Curl),
        }
    }

    fn build_native_https_connector(
        &self,
    ) -> Result<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        ProviderConfigError,
    > {
        if let Some(path) = &self.tls_ca_cert_path {
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
            Ok(HttpsConnectorBuilder::new()
                .with_native_roots()
                .map_err(|error| ProviderConfigError {
                    field: "native_http_tls".to_string(),
                    message: error.to_string(),
                })?
                .https_or_http()
                .enable_http1()
                .build())
        }
    }
}

impl ProviderAdapterResponder for OpenAICompatibleProviderAdapter {
    fn identity(&self) -> ProviderIdentity {
        self.identity.clone()
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        let masked_key = if self.api_key.is_empty() {
            "missing".to_string()
        } else {
            format!("len:{}", self.api_key.len())
        };

        match self.execute_transport(request) {
            Ok(call) => {
                let response_body = call.response_body_json();
                let status_code = call.status_code();
                if !http_status_is_success(status_code) {
                    let error_message = extract_provider_error_message(response_body)
                        .unwrap_or_else(|| format!("status_code={status_code}"));
                    return ProviderAdapterResponse {
                        body: format!(
                            "PROVIDER_HTTP_ERROR: provider={} model={} transport={} status_code={} error={}",
                            self.identity.provider_id,
                            self.identity.model_name,
                            call.transport().as_str(),
                            status_code,
                            error_message
                        ),
                        trace: format!(
                            "transport=openai-compatible provider={} model={} base_url={} api_key={} recall_hits={} message_count={} request_url={} status_code={} transport_mode={} provider_http_error={}",
                            self.identity.provider_id,
                            self.identity.model_name,
                            self.base_url,
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
                            let mut meta = build_success_meta(&call);
                            meta.insert("provider_response_ok".to_string(), "false".to_string());
                            meta.insert("provider_error_class".to_string(), "http_status".to_string());
                            meta.insert("provider_error_message".to_string(), error_message.clone());
                            insert_provider_failure_meta(
                                &mut meta,
                                Some(status_code),
                                "http_status",
                                Some(&error_message),
                            );
                            insert_provider_timeout_meta(&mut meta, Some(status_code), "http_status", Some(&error_message));
                            meta
                        },
                    };
                }

                let assistant_content = extract_assistant_content(response_body);
                let mut extra_meta = build_success_meta(&call);
                let (body, finish_reason) = if let Some(content) = assistant_content {
                    extra_meta.insert("provider_response_ok".to_string(), "true".to_string());
                    (
                        content,
                        extract_finish_reason(response_body).or_else(|| {
                            Some(default_finish_reason_for_transport(call.transport()).to_string())
                        }),
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
                    extra_meta.insert("provider_retryable".to_string(), "false".to_string());
                    insert_provider_failure_meta(
                        &mut extra_meta,
                        Some(call.status_code()),
                        "missing_content",
                        Some("missing assistant content in successful provider response"),
                    );
                    (
                        format!(
                            "PROVIDER_MISSING_CONTENT: provider={} model={} transport={} status_code={} response_kind={}",
                            self.identity.provider_id,
                            self.identity.model_name,
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
                        "transport=openai-compatible provider={} model={} base_url={} api_key={} recall_hits={} message_count={} request_url={} status_code={} transport_mode={}",
                        self.identity.provider_id,
                        self.identity.model_name,
                        self.base_url,
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
            Err(error) => {
                let preview = self.build_http_request_preview(request).ok();

                ProviderAdapterResponse {
                    body: format!(
                        "CONFIG_ERROR: openai-compatible provider invalid field={} reason={}",
                        error.field, error.message
                    ),
                    trace: format!(
                        "transport=openai-compatible provider={} model={} config_error_field={} reason={}",
                        self.identity.provider_id,
                        self.identity.model_name,
                        error.field,
                        error.message
                    ),
                    finish_reason: Some("invalid-config".to_string()),
                    extra_meta: build_config_error_meta(
                        &error,
                        preview.as_ref(),
                        self.transport.as_str(),
                        self.request_timeout_ms,
                    ),
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TransportCallResult {
    Stub(StubHttpCallResult),
    Http(HttpCallResult),
    Native(HttpCallResult),
    Curl(HttpCallResult),
}

impl TransportCallResult {
    fn transport(&self) -> ProviderTransport {
        match self {
            Self::Stub(_) => ProviderTransport::Stub,
            Self::Http(_) => ProviderTransport::Http,
            Self::Native(_) => ProviderTransport::Native,
            Self::Curl(_) => ProviderTransport::Curl,
        }
    }

    fn status_code(&self) -> u16 {
        match self {
            Self::Stub(result) => result.status_code,
            Self::Http(result) => result.status_code,
            Self::Native(result) => result.status_code,
            Self::Curl(result) => result.status_code,
        }
    }

    fn url(&self) -> &str {
        match self {
            Self::Stub(result) => &result.url,
            Self::Http(result) => &result.url,
            Self::Native(result) => &result.url,
            Self::Curl(result) => &result.url,
        }
    }

    fn request_body_json(&self) -> &str {
        match self {
            Self::Stub(result) => &result.request_body_json,
            Self::Http(result) => &result.request_body_json,
            Self::Native(result) => &result.request_body_json,
            Self::Curl(result) => &result.request_body_json,
        }
    }

    fn response_body_json(&self) -> &str {
        match self {
            Self::Stub(result) => &result.response_body_json,
            Self::Http(result) => &result.response_body_json,
            Self::Native(result) => &result.response_body_json,
            Self::Curl(result) => &result.response_body_json,
        }
    }
}

fn build_success_meta(call: &TransportCallResult) -> BTreeMap<String, String> {
    let usage_meta = extract_usage_meta(call.response_body_json());
    let response_kind_value =
        response_kind(call.response_body_json()).unwrap_or_else(|| "unknown".to_string());
    let response_finish_reason = extract_finish_reason(call.response_body_json())
        .unwrap_or_else(|| default_finish_reason_for_transport(call.transport()).to_string());

    let mut meta = BTreeMap::from([
        ("transport".to_string(), "openai-compatible".to_string()),
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
) -> BTreeMap<String, String> {
    let error_class = provider_error_class(error);
    let mut meta = BTreeMap::from([
        ("transport".to_string(), "openai-compatible".to_string()),
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

fn http_status_retryable(status_code: u16) -> bool {
    status_code == 408 || status_code == 429 || (500..=599).contains(&status_code)
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
    let mut cursor = std::io::Cursor::new(pem);
    let cert_chain = certs(&mut cursor)
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
        .and_then(|value| value.get("object")?.as_str().map(str::to_string))
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
    use serde_json::json;

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
