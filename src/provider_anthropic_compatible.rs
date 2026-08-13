//! `provider_anthropic_compatible` 模块。公开接口：struct AnthropicCompatibleRequestEnvelope,
//! AnthropicCompatibleProviderAdapter；fn new, with_transport, with_endpoint,
//! with_reasoning_effort, with_max_output_tokens, with_request_timeout_ms,
//! with_tls_ca_cert_path, build_request_envelope, build_http_request_preview,
//! execute_stub_post_call, execute_http_post_call, execute_native_post_call,
//! execute_curl_post_call。
//!
//! 对接 Anthropic Messages API（`POST {base_url}/v1/messages`）：
//! - 认证走 `x-api-key: <key>` + `anthropic-version: 2023-06-01`（不是 Bearer）；
//! - system prompt 是顶层 `system` 字段，不放进 `messages`；
//! - `messages` 只含 user/assistant 轮次。
//!
//! Transport 层（stub/http/native/curl）与响应/重试核心（`run_provider_respond`）
//! 复用 `provider_openai_compatible` 的共享实现，保证两种方言走同一套传输、
//! 退避、看门狗与错误语义。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;

use crate::provider_openai_compatible::{
    execute_curl_transport, execute_http_transport, execute_native_transport,
    run_provider_respond, DEFAULT_PROVIDER_REQUEST_TIMEOUT_MS, HttpCallResult, HttpRequestPreview,
    ProviderConfigError, ProviderRespondContext, ProviderTransport, ReasoningEffort,
    StubHttpCallResult,
};
use crate::responder::{ProviderAdapterResponder, ProviderIdentity, ResponderRequest};
use crate::runtime_config::AnthropicApiEndpoint;

const ANTHROPIC_VERSION_HEADER: &str = "2023-06-01";
const DEFAULT_ANTHROPIC_MAX_TOKENS: u32 = 4096;
const DEFAULT_ANTHROPIC_TEMPERATURE: f64 = 0.7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicCompatibleRequestEnvelope {
    pub provider_id: String,
    pub base_url: String,
    pub model: String,
    /// Anthropic Messages API：system prompt 是顶层字段，不进入 messages。
    pub system: String,
    pub input: String,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicCompatibleProviderAdapter {
    identity: ProviderIdentity,
    base_url: String,
    api_key: String,
    transport: ProviderTransport,
    endpoint: AnthropicApiEndpoint,
    reasoning_effort: Option<ReasoningEffort>,
    max_output_tokens: Option<u32>,
    request_timeout_ms: u64,
    tls_ca_cert_path: Option<PathBuf>,
}

impl AnthropicCompatibleProviderAdapter {
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
            endpoint: AnthropicApiEndpoint::default(),
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

    pub fn with_endpoint(mut self, endpoint: AnthropicApiEndpoint) -> Self {
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
    ) -> Result<AnthropicCompatibleRequestEnvelope, ProviderConfigError> {
        self.validate_config()?;
        Ok(AnthropicCompatibleRequestEnvelope {
            provider_id: self.identity.provider_id.clone(),
            base_url: self.base_url.clone(),
            model: self.identity.model_name.clone(),
            system: request.prompt.clone(),
            input: request.user_input.clone(),
            max_output_tokens: self.max_output_tokens,
        })
    }

    /// 构造 `POST {base_url}/v1/messages` 的请求预览：
    /// `x-api-key` + `anthropic-version` 认证头，system 顶层字段 + messages 轮次。
    pub fn build_http_request_preview(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpRequestPreview, ProviderConfigError> {
        let envelope = self.build_request_envelope(request)?;
        let url = match self.endpoint {
            AnthropicApiEndpoint::Messages => {
                format!("{}/v1/messages", envelope.base_url.trim_end_matches('/'))
            }
        };
        let body = json!({
            "model": envelope.model,
            "system": envelope.system,
            "messages": [
                {
                    "role": "user",
                    "content": envelope.input,
                }
            ],
            "max_tokens": envelope
                .max_output_tokens
                .unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS),
            "temperature": DEFAULT_ANTHROPIC_TEMPERATURE,
        });
        let body_json = body.to_string();

        Ok(HttpRequestPreview {
            method: "POST".to_string(),
            url,
            headers: BTreeMap::from([
                ("x-api-key".to_string(), self.api_key.clone()),
                (
                    "anthropic-version".to_string(),
                    ANTHROPIC_VERSION_HEADER.to_string(),
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
            response_body_json: build_anthropic_stub_response_body(&self.identity, request),
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
        execute_native_transport(&preview, self.request_timeout_ms, self.tls_ca_cert_path.as_ref())
    }

    pub fn execute_curl_post_call(
        &self,
        request: &ResponderRequest,
    ) -> Result<HttpCallResult, ProviderConfigError> {
        let preview = self.build_http_request_preview(request)?;
        execute_curl_transport(&preview, self.request_timeout_ms)
    }
}

/// Anthropic Messages API 风格的 stub 响应体（`type: message` + content 数组 +
/// 顶层 `stop_reason`）。
fn build_anthropic_stub_response_body(
    identity: &ProviderIdentity,
    request: &ResponderRequest,
) -> String {
    json!({
        "id": "msg_stub_001",
        "type": "message",
        "role": "assistant",
        "model": identity.model_name,
        "stubbed": true,
        "provider_id": identity.provider_id,
        "content": [
            {
                "type": "text",
                "text": format!(
                    "stubbed_post_ok: provider={} model={} user_input=《{}》",
                    identity.provider_id,
                    identity.model_name,
                    request.user_input
                ),
            }
        ],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0
        }
    })
    .to_string()
}

fn default_finish_reason_for_transport(transport: ProviderTransport) -> &'static str {
    match transport {
        ProviderTransport::Stub => "stubbed-anthropic-compatible",
        ProviderTransport::Http => "http-anthropic-compatible",
        ProviderTransport::Native => "native-anthropic-compatible",
        ProviderTransport::Curl => "curl-anthropic-compatible",
    }
}

impl ProviderAdapterResponder for AnthropicCompatibleProviderAdapter {
    fn identity(&self) -> ProviderIdentity {
        self.identity.clone()
    }

    fn respond(&self, request: &ResponderRequest) -> crate::responder::ProviderAdapterResponse {
        let ctx = ProviderRespondContext {
            identity: &self.identity,
            base_url: &self.base_url,
            api_key: &self.api_key,
            transport: self.transport.clone(),
            request_timeout_ms: self.request_timeout_ms,
            tls_ca_cert_path: self.tls_ca_cert_path.as_ref(),
            transport_label: "anthropic-compatible",
            default_finish_reason: default_finish_reason_for_transport,
        };
        run_provider_respond(
            &ctx,
            request,
            |req| self.build_http_request_preview(req),
            |identity, req, _preview| build_anthropic_stub_response_body(identity, req),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_openai_compatible::{ProviderTransport, ReasoningEffort};
    use crate::responder::Responder;
    use serde_json::Value;

    fn request() -> ResponderRequest {
        ResponderRequest {
            prompt: "你是系统提示".to_string(),
            user_input: "用户问题".to_string(),
            recall_hit_count: 0,
        }
    }

    #[test]
    fn build_http_request_preview_uses_v1_messages_url_and_anthropic_auth_headers() {
        let adapter = AnthropicCompatibleProviderAdapter::new(
            "anthropic-main",
            "https://api.anthropic.com",
            "secret-key-123",
            "claude-opus-4-1",
        )
        .with_transport(ProviderTransport::Stub)
        .with_endpoint(AnthropicApiEndpoint::Messages)
        .with_reasoning_effort(Some(ReasoningEffort::High));

        let preview = adapter.build_http_request_preview(&request()).unwrap();
        assert_eq!(preview.method, "POST");
        assert_eq!(preview.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(preview.headers.get("x-api-key").unwrap(), "secret-key-123");
        assert_eq!(
            preview.headers.get("anthropic-version").unwrap(),
            "2023-06-01"
        );
        assert_eq!(
            preview.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert!(
            !preview
                .headers
                .contains_key("authorization"),
            "anthropic must not use Bearer authorization"
        );
    }

    #[test]
    fn build_http_request_preview_separates_system_from_messages() {
        let adapter = AnthropicCompatibleProviderAdapter::new(
            "anthropic-main",
            "https://api.anthropic.com/",
            "secret-key-123",
            "claude-opus-4-1",
        )
        .with_max_output_tokens(Some(2048));

        let preview = adapter.build_http_request_preview(&request()).unwrap();
        let body: Value = serde_json::from_str(&preview.body_json).unwrap();

        assert_eq!(body["model"], "claude-opus-4-1");
        assert_eq!(body["system"], "你是系统提示");
        assert_eq!(body["max_tokens"], 2048);
        assert_eq!(body["temperature"], 0.7);

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "用户问题");
        for message in messages {
            assert_ne!(
                message["role"], "system",
                "system prompt must stay at top-level `system`, not in messages"
            );
        }
    }

    #[test]
    fn stub_post_call_returns_anthropic_message_response() {
        let adapter = AnthropicCompatibleProviderAdapter::new(
            "anthropic-main",
            "https://api.anthropic.com",
            "secret-key-123",
            "claude-opus-4-1",
        )
        .with_max_output_tokens(Some(1024));

        let result = adapter.execute_stub_post_call(&request()).unwrap();
        assert_eq!(result.status_code, 200);
        assert_eq!(result.url, "https://api.anthropic.com/v1/messages");

        let response: Value = serde_json::from_str(&result.response_body_json).unwrap();
        assert_eq!(response["type"], "message");
        assert_eq!(response["role"], "assistant");
        assert_eq!(response["stop_reason"], "end_turn");
        let text = &response["content"][0]["text"];
        assert!(
            text.as_str().unwrap().contains("stubbed_post_ok"),
            "unexpected stub text: {text}"
        );
    }

    #[test]
    fn respond_via_slot_interface_extracts_anthropic_content() {
        let adapter = AnthropicCompatibleProviderAdapter::new(
            "anthropic-main",
            "https://api.anthropic.com",
            "secret-key-123",
            "claude-opus-4-1",
        )
        .with_transport(ProviderTransport::Stub);

        let output = adapter.generate(&request());
        assert!(output.body.contains("stubbed_post_ok"));
        assert_eq!(
            output.meta.extra.get("transport").unwrap(),
            "anthropic-compatible"
        );
        assert_eq!(
            output.meta.extra.get("response_kind").unwrap(),
            "message"
        );
        assert_eq!(
            output.meta.extra.get("response_finish_reason").unwrap(),
            "end_turn"
        );
    }
}
