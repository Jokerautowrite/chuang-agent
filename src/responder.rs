use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::str::FromStr;

use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderRequest {
    pub prompt: String,
    pub user_input: String,
    pub recall_hit_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub provider_id: String,
    pub model_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderProvider {
    pub provider_id: String,
    pub model_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderMeta {
    pub provider: Option<String>,
    pub recall_hit_count: Option<usize>,
    pub finish_reason: Option<String>,
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponderOutput {
    pub model_name: String,
    pub body: String,
    pub trace: String,
    pub meta: ResponderMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapterResponse {
    pub body: String,
    pub trace: String,
    pub finish_reason: Option<String>,
    pub extra_meta: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfigError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAICompatibleMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAICompatibleRequestEnvelope {
    pub provider_id: String,
    pub base_url: String,
    pub model: String,
    pub messages: Vec<OpenAICompatibleMessage>,
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

pub trait Responder {
    fn generate(&self, request: &ResponderRequest) -> ResponderOutput;
    fn provider(&self) -> ResponderProvider;
}

pub trait ProviderAdapterResponder {
    fn identity(&self) -> ProviderIdentity;
    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse;
}

impl<T: ProviderAdapterResponder> Responder for T {
    fn generate(&self, request: &ResponderRequest) -> ResponderOutput {
        let identity = self.identity();
        let response = self.respond(request);

        ResponderOutput {
            model_name: identity.model_name.clone(),
            body: response.body,
            trace: response.trace,
            meta: ResponderMeta {
                provider: Some(identity.provider_id),
                recall_hit_count: Some(request.recall_hit_count),
                finish_reason: response.finish_reason,
                extra: response.extra_meta,
            },
        }
    }

    fn provider(&self) -> ResponderProvider {
        let identity = self.identity();
        ResponderProvider {
            provider_id: identity.provider_id,
            model_name: identity.model_name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTransport {
    Stub,
    Http,
}

impl ProviderTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stub => "stub",
            Self::Http => "http",
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
            other => Err(format!(
                "unsupported provider transport: {other} (supported: stub, http)"
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
        }
    }

    pub fn with_transport(mut self, transport: ProviderTransport) -> Self {
        self.transport = transport;
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
            messages: vec![
                OpenAICompatibleMessage {
                    role: "system".to_string(),
                    content: request.prompt.clone(),
                },
                OpenAICompatibleMessage {
                    role: "user".to_string(),
                    content: request.user_input.clone(),
                },
            ],
        })
    }

    pub fn build_http_request_preview(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpRequestPreview, ProviderConfigError> {
        let envelope = self.build_request_envelope(request)?;
        let url = format!(
            "{}/chat/completions",
            envelope.base_url.trim_end_matches('/'),
        );
        let body_json = json!({
            "model": envelope.model,
            "messages": envelope
                .messages
                .iter()
                .map(|message| json!({
                    "role": message.role,
                    "content": message.content,
                }))
                .collect::<Vec<_>>(),
        })
        .to_string();

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
            "id": "stub-chatcmpl-001",
            "object": "chat.completion",
            "provider_id": self.identity.provider_id,
            "model": self.identity.model_name,
            "stubbed": true,
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": format!(
                            "stubbed_post_ok: provider={} model={} user_input=《{}》",
                            self.identity.provider_id,
                            self.identity.model_name,
                            request.user_input
                        )
                    },
                    "finish_reason": "stop"
                }
            ]
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
        let mut stream =
            TcpStream::connect((host.as_str(), port)).map_err(|error| ProviderConfigError {
                field: "http_connect".to_string(),
                message: format!("target={} error={error}", preview.url),
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
            preview.body_json.as_bytes().len(),
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
                field: "http_read".to_string(),
                message: error.to_string(),
            })?;

        let (status_code, response_body_json) = parse_http_response(&raw_response)?;

        Ok(HttpCallResult {
            status_code,
            url: preview.url,
            request_body_json: preview.body_json,
            response_body_json,
        })
    }

    fn execute_transport(
        &self,
        request: &ResponderRequest,
    ) -> Result<TransportCallResult, ProviderConfigError> {
        match self.transport {
            ProviderTransport::Stub => self
                .execute_stub_post_call(request)
                .map(TransportCallResult::Stub),
            ProviderTransport::Http => self
                .execute_http_post_call(request)
                .map(TransportCallResult::Http),
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
                let finish_reason = extract_finish_reason(response_body).or_else(|| {
                    Some(default_finish_reason_for_transport(call.transport()).to_string())
                });
                ProviderAdapterResponse {
                    body: extract_assistant_content(response_body).unwrap_or_else(|| {
                        format!(
                            "provider_response_missing_content: provider={} model={} transport={} user_input=《{}》",
                            self.identity.provider_id,
                            self.identity.model_name,
                            call.transport().as_str(),
                            request.user_input
                        )
                    }),
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
                    extra_meta: build_success_meta(&call),
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
}

impl TransportCallResult {
    fn transport(&self) -> ProviderTransport {
        match self {
            Self::Stub(_) => ProviderTransport::Stub,
            Self::Http(_) => ProviderTransport::Http,
        }
    }

    fn status_code(&self) -> u16 {
        match self {
            Self::Stub(result) => result.status_code,
            Self::Http(result) => result.status_code,
        }
    }

    fn url(&self) -> &str {
        match self {
            Self::Stub(result) => &result.url,
            Self::Http(result) => &result.url,
        }
    }

    fn request_body_json(&self) -> &str {
        match self {
            Self::Stub(result) => &result.request_body_json,
            Self::Http(result) => &result.request_body_json,
        }
    }

    fn response_body_json(&self) -> &str {
        match self {
            Self::Stub(result) => &result.response_body_json,
            Self::Http(result) => &result.response_body_json,
        }
    }
}

fn build_success_meta(call: &TransportCallResult) -> BTreeMap<String, String> {
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
        ("response_kind".to_string(), response_kind_value.clone()),
        ("response_finish_reason".to_string(), response_finish_reason),
    ]);

    if matches!(call.transport(), ProviderTransport::Stub) {
        meta.insert(
            "stub_status_code".to_string(),
            call.status_code().to_string(),
        );
        meta.insert("stub_response_kind".to_string(), response_kind_value);
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
) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::from([
        ("transport".to_string(), "openai-compatible".to_string()),
        ("transport_mode".to_string(), transport_mode.to_string()),
        ("config_error_field".to_string(), error.field.clone()),
    ]);

    if let Some(preview) = preview {
        meta.insert("request_url".to_string(), preview.url.clone());
        meta.insert("request_method".to_string(), preview.method.clone());
        meta.insert(
            "request_message_count".to_string(),
            request_message_count(&preview.body_json).to_string(),
        );
    }

    meta
}

fn request_message_count(body_json: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(body_json)
        .ok()
        .and_then(|value| {
            value
                .get("messages")
                .and_then(|messages| messages.as_array().cloned())
        })
        .map(|messages| messages.len())
        .unwrap_or(0)
}

fn extract_assistant_content(response_body_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(response_body_json)
        .ok()
        .and_then(|value| value.get("choices")?.as_array()?.first().cloned())
        .and_then(|choice| {
            choice
                .get("message")?
                .get("content")?
                .as_str()
                .map(str::to_string)
        })
}

fn extract_finish_reason(response_body_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(response_body_json)
        .ok()
        .and_then(|value| value.get("choices")?.as_array()?.first().cloned())
        .and_then(|choice| choice.get("finish_reason")?.as_str().map(str::to_string))
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
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeResponder {
    model_name: String,
}

impl FakeResponder {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
        }
    }
}

impl ProviderAdapterResponder for FakeResponder {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider_id: "fake-responder".to_string(),
            model_name: self.model_name.clone(),
        }
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        ProviderAdapterResponse {
            body: format!(
                "fake-responder[{}]: user_input=《{}》 recall_hits={}",
                self.model_name, request.user_input, request.recall_hit_count
            ),
            trace: format!(
                "provider={} model={} user_input=《{}》 recall_hits={}",
                self.identity().provider_id,
                self.model_name,
                request.user_input,
                request.recall_hit_count
            ),
            finish_reason: Some("stubbed".to_string()),
            extra_meta: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedResponder {
    model_name: String,
    scripted_output: String,
    extra_meta: BTreeMap<String, String>,
}

impl ScriptedResponder {
    pub fn new(model_name: impl Into<String>, scripted_output: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            scripted_output: scripted_output.into(),
            extra_meta: BTreeMap::new(),
        }
    }

    pub fn with_extra_meta(mut self, extra_meta: BTreeMap<String, String>) -> Self {
        self.extra_meta = extra_meta;
        self
    }
}

impl ProviderAdapterResponder for ScriptedResponder {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider_id: "scripted-responder".to_string(),
            model_name: self.model_name.clone(),
        }
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        ProviderAdapterResponse {
            body: self.scripted_output.clone(),
            trace: format!(
                "provider={} model={} scripted=true user_input=《{}》 recall_hits={}",
                self.identity().provider_id,
                self.model_name,
                request.user_input,
                request.recall_hit_count
            ),
            finish_reason: Some("scripted".to_string()),
            extra_meta: self.extra_meta.clone(),
        }
    }
}
