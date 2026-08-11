//! knowledge_context（GBrain 直连 API 通道）集成测试。
//!
//! 覆盖：启用（metadata.knowledge_context=1 + gbrain endpoint/token_env +
//! env token → preflight.available）、禁用（结构化 reason_code="disabled"）、
//! 预检缺失（endpoint/token_env/token 缺失各给结构化原因）、live 查询成功
//! （readonly_http 返回 hits，token 不泄漏）、live 查询降级（HTTP 5xx →
//! 结构化不可用、不 panic、不阻断）。

use chuang_agent::external_knowledge::{
    ExternalKnowledgeRead, ExternalKnowledgeReadRequest, ExternalKnowledgeSource,
    LiveExternalKnowledgeReader,
};
use chuang_agent::knowledge_read::KnowledgeReadSourceConfig;
use chuang_agent::runtime_config::RuntimeConfig;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

/// 每个测试用独立 env 变量名，避免并行测试互相污染；Drop 时清理。
struct EnvGuard {
    names: Vec<String>,
}

impl EnvGuard {
    fn set(name: &str, value: &str) -> Self {
        std::env::set_var(name, value);
        Self {
            names: vec![name.to_string()],
        }
    }

    fn clear(name: &str) -> Self {
        std::env::remove_var(name);
        Self {
            names: vec![name.to_string()],
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for name in &self.names {
            std::env::remove_var(name);
        }
    }
}

fn runtime_with_gbrain(endpoint: Option<&str>, token_env: Option<&str>, enabled: bool) -> RuntimeConfig {
    let mut runtime = RuntimeConfig::new(PathBuf::from("./tmp/chuang-kc-test.db"));
    if enabled {
        runtime
            .metadata
            .insert("knowledge_context".to_string(), "1".to_string());
    }
    runtime.external_knowledge.gbrain = KnowledgeReadSourceConfig {
        endpoint: endpoint.map(str::to_string),
        token_env: token_env.map(str::to_string),
        timeout_ms: Some(5_000),
    };
    runtime
}

fn spawn_knowledge_read_server(
    status_code: u16,
    response_body: &'static str,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind localhost");
    let port = listener
        .local_addr()
        .expect("test server local addr should be available")
        .port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("test server should accept request");
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let read = stream
                .read(&mut buf)
                .expect("test server should read request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buf[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n")
                && request_contains_full_body(&request)
            {
                break;
            }
        }
        tx.send(String::from_utf8_lossy(&request).to_string())
            .expect("test server should send captured request");
        let response = format!(
            "HTTP/1.1 {status_code} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .expect("test server should write response");
    });
    (format!("http://127.0.0.1:{port}/query"), rx)
}

fn request_contains_full_body(request: &[u8]) -> bool {
    let request_text = String::from_utf8_lossy(request);
    let body_start = request_text
        .find("\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(request_text.len());
    let body = &request_text[body_start..];
    let content_length = request_text
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    body.len() >= content_length && content_length > 0
}

#[test]
fn knowledge_context_enabled_preflight_claims_readonly_http_adapter() {
    let _guard = EnvGuard::set("CHUANG_TEST_KC_ENABLE_TOKEN", "test-kc-enable-token");
    let runtime = runtime_with_gbrain(
        Some("https://gbrain.example.invalid/query"),
        Some("CHUANG_TEST_KC_ENABLE_TOKEN"),
        true,
    );

    let reader = LiveExternalKnowledgeReader::from_runtime(&runtime);
    let preflight = reader.preflight();
    assert!(preflight.available);
    assert_eq!(preflight.adapter_kind, "readonly_http");
    assert!(preflight.live_adapter_configured);
    assert_eq!(preflight.endpoint_state, "set");
    assert_eq!(preflight.token_state, "<set>");
    assert_eq!(preflight.reason_code, "gbrain_readonly_http_configured");
    assert!(preflight.reason.contains("read-only HTTP adapter"));
}

#[test]
fn knowledge_context_disabled_preflight_returns_structured_disabled() {
    let _guard = EnvGuard::clear("CHUANG_TEST_KC_DISABLED_TOKEN");
    let runtime = runtime_with_gbrain(
        Some("https://gbrain.example.invalid/query"),
        Some("CHUANG_TEST_KC_DISABLED_TOKEN"),
        false,
    );

    let reader = LiveExternalKnowledgeReader::from_runtime(&runtime);
    let preflight = reader.preflight();
    assert!(!preflight.available);
    assert!(!preflight.live_adapter_configured);
    assert_eq!(preflight.reason_code, "disabled");
    assert!(preflight.reason.contains("knowledge_context channel is disabled"));
    assert!(preflight
        .next_action
        .contains("metadata knowledge_context=1"));
}

#[test]
fn knowledge_context_enabled_without_token_preflight_returns_structured_unavailable() {
    let _guard = EnvGuard::clear("CHUANG_TEST_KC_MISSING_TOKEN");
    let runtime = runtime_with_gbrain(
        Some("https://gbrain.example.invalid/query"),
        Some("CHUANG_TEST_KC_MISSING_TOKEN"),
        true,
    );

    let reader = LiveExternalKnowledgeReader::from_runtime(&runtime);
    let preflight = reader.preflight();
    assert!(!preflight.available);
    assert_eq!(preflight.adapter_kind, "preflight_only");
    assert_eq!(preflight.reason_code, "token_missing");
    assert!(preflight.reason.contains("token is unavailable"));
}

#[test]
fn knowledge_context_enabled_without_endpoint_preflight_returns_structured_unavailable() {
    let _guard = EnvGuard::clear("CHUANG_TEST_KC_NO_ENDPOINT_TOKEN");
    let runtime = runtime_with_gbrain(
        None,
        Some("CHUANG_TEST_KC_NO_ENDPOINT_TOKEN"),
        true,
    );

    let reader = LiveExternalKnowledgeReader::from_runtime(&runtime);
    let preflight = reader.preflight();
    assert!(!preflight.available);
    assert_eq!(preflight.reason_code, "endpoint_missing");
    assert!(preflight.reason.contains("endpoint is missing"));
}

#[test]
fn knowledge_context_live_read_returns_hits_without_leaking_token() {
    let _guard = EnvGuard::set("CHUANG_TEST_KC_LIVE_TOKEN", "kc-live-secret-token");
    let (endpoint, request_rx) = spawn_knowledge_read_server(
        200,
        r#"{"hits":[{"source":"gbrain","title":"Ops Playbook","uri":"gbrain://ops/1","preview":"Readonly GBrain hit","provenance":"gbrain_fixture"}]}"#,
    );
    let runtime = runtime_with_gbrain(
        Some(endpoint.as_str()),
        Some("CHUANG_TEST_KC_LIVE_TOKEN"),
        true,
    );

    let reader = LiveExternalKnowledgeReader::from_runtime(&runtime);
    let status = reader.read(ExternalKnowledgeReadRequest {
        source: ExternalKnowledgeSource::GBrain,
        query: "ops".to_string(),
        limit: 1,
    });
    assert!(status.available);
    assert_eq!(status.adapter_kind, "readonly_http");
    assert_eq!(status.hit_count, 1);
    assert_eq!(status.reason_code, "gbrain_readonly_http_read");
    assert_eq!(status.hits[0].path, "gbrain://ops/1");
    assert_eq!(status.hits[0].preview, "Readonly GBrain hit");

    let request = request_rx
        .recv()
        .expect("test server should capture request");
    assert!(request.contains("\"source\":\"gbrain\""));
    assert!(request.contains("\"read_only\":true"));
    // token 用于鉴权（Authorization 头），但绝不进入状态/回执/日志。
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer kc-live-secret-token"));
    assert!(!format!("{status:?}").contains("kc-live-secret-token"));
}

#[test]
fn knowledge_context_live_read_degrades_structured_on_http_error_without_panic() {
    let _guard = EnvGuard::set("CHUANG_TEST_KC_DEGRADE_TOKEN", "kc-degrade-secret");
    let (endpoint, _request_rx) = spawn_knowledge_read_server(
        500,
        r#"{"error":"upstream failed kc-degrade-secret"}"#,
    );
    let runtime = runtime_with_gbrain(
        Some(endpoint.as_str()),
        Some("CHUANG_TEST_KC_DEGRADE_TOKEN"),
        true,
    );

    let reader = LiveExternalKnowledgeReader::from_runtime(&runtime);
    let status = reader.read(ExternalKnowledgeReadRequest {
        source: ExternalKnowledgeSource::GBrain,
        query: "ops".to_string(),
        limit: 1,
    });
    // 结构化降级：不 panic、不静默吞掉，主对话侧把 knowledge 段标记 unavailable。
    assert!(!status.available);
    assert_eq!(status.reason_code, "knowledge_read_http_status");
    assert!(status.reason.contains("status_code=500"));
    assert_eq!(status.hit_count, 0);
    assert!(!format!("{status:?}").contains("kc-degrade-secret"));
}
