use chuang_agent::knowledge_read::{
    preflight_knowledge_read_status, FakeKnowledgeReadAdapter, KnowledgeReadAdapter,
    KnowledgeReadConfig, KnowledgeReadHit, KnowledgeReadQuery, KnowledgeReadSourceConfig,
    ReadonlyHttpKnowledgeReadAdapter, UnavailableKnowledgeReadAdapter,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

#[test]
fn fake_knowledge_read_adapter_returns_injected_hits() {
    let adapter = FakeKnowledgeReadAdapter::new(vec![
        KnowledgeReadHit {
            source: "wiki".to_string(),
            title: "Wiki note".to_string(),
            uri: "wiki://note/live-gap".to_string(),
            preview: "wiki live is pending".to_string(),
            provenance: "fake_fixture".to_string(),
        },
        KnowledgeReadHit {
            source: "gbrain".to_string(),
            title: "GBrain note".to_string(),
            uri: "gbrain://note/live-gap".to_string(),
            preview: "browser_read and gbrain live are pending".to_string(),
            provenance: "fake_fixture".to_string(),
        },
    ]);

    let status = adapter.status();
    assert!(!status.available);
    assert_eq!(status.adapter_kind, "fake");
    assert_eq!(status.state, "fake_contract_only");
    assert_eq!(status.reason_code, "fake_contract_only");
    assert_eq!(status.sources, vec!["wiki", "gbrain"]);
    assert!(status.local_preview_is_separate);
    assert!(!status.connects_real_service);
    assert!(!status.writes_automatically);

    let result = adapter
        .query(KnowledgeReadQuery {
            source: "gbrain".to_string(),
            query: "live gaps".to_string(),
            limit: 1,
        })
        .expect("fake adapter should return injected hit");
    assert_eq!(result.source, "gbrain");
    assert_eq!(result.query, "live gaps");
    assert!(result.read_only);
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].uri, "gbrain://note/live-gap");
    assert_eq!(result.receipt, "fake_knowledge_read_contract_receipt");
}

#[test]
fn fake_knowledge_read_adapter_rejects_non_wiki_gbrain_sources() {
    let adapter = FakeKnowledgeReadAdapter::new(vec![]);

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "local_preview".to_string(),
            query: "live gaps".to_string(),
            limit: 1,
        })
        .expect_err("fake adapter must still enforce the wiki/GBrain contract");

    assert_eq!(error.code, "knowledge_read_unknown_source");
    assert_eq!(error.adapter_kind, "fake");
    assert!(!error.retryable);
    assert!(error.message.contains("expected wiki or gbrain"));
}

#[test]
fn unavailable_knowledge_read_adapter_never_claims_live_wiki_or_gbrain() {
    let adapter = UnavailableKnowledgeReadAdapter;

    let status = adapter.status();
    assert!(!status.available);
    assert_eq!(status.adapter_kind, "unavailable");
    assert_eq!(status.state, "unavailable");
    assert_eq!(status.reason_code, "real_adapter_missing");
    assert!(status.reason.contains("must not be reported as real"));
    assert!(status.local_preview_is_separate);
    assert!(!status.connects_real_service);
    assert!(!status.writes_automatically);

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "wiki".to_string(),
            query: "live gaps".to_string(),
            limit: 3,
        })
        .expect_err("missing real adapter must be structured unavailable");
    assert_eq!(error.code, "knowledge_read_unavailable");
    assert_eq!(error.adapter_kind, "unavailable");
    assert!(!error.retryable);
    assert!(error.message.contains("cannot query real wiki service"));
}

#[test]
fn unavailable_knowledge_read_adapter_rejects_unknown_source_before_unavailable() {
    let adapter = UnavailableKnowledgeReadAdapter;

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "local_preview".to_string(),
            query: "live gaps".to_string(),
            limit: 3,
        })
        .expect_err("unknown sources must not be treated as wiki/GBrain unavailable");

    assert_eq!(error.code, "knowledge_read_unknown_source");
    assert_eq!(error.adapter_kind, "unavailable");
    assert!(!error.retryable);
}

#[test]
fn knowledge_read_preflight_is_unavailable_without_endpoint_or_token_env() {
    let config = KnowledgeReadConfig {
        wiki: KnowledgeReadSourceConfig::disabled(),
        gbrain: KnowledgeReadSourceConfig::disabled(),
    };

    let wiki = preflight_knowledge_read_status(&config, "wiki", "<missing>");
    assert!(!wiki.available);
    assert_eq!(wiki.adapter_kind, "preflight_only");
    assert_eq!(wiki.state, "unavailable");
    assert_eq!(wiki.reason_code, "endpoint_missing");
    assert!(wiki.reason.contains("endpoint is missing"));
    assert!(!wiki.connects_real_service);
    assert!(!wiki.writes_automatically);

    let gbrain = preflight_knowledge_read_status(&config, "gbrain", "<missing>");
    assert!(!gbrain.available);
    assert_eq!(gbrain.adapter_kind, "preflight_only");
    assert_eq!(gbrain.state, "unavailable");
    assert_eq!(gbrain.reason_code, "endpoint_missing");
    assert!(gbrain.local_preview_is_separate);
}

#[test]
fn knowledge_read_preflight_distinguishes_missing_token_env_and_token() {
    let config = KnowledgeReadConfig {
        wiki: KnowledgeReadSourceConfig {
            endpoint: Some("https://wiki.example.invalid/query".to_string()),
            token_env: None,
            timeout_ms: Some(5000),
        },
        gbrain: KnowledgeReadSourceConfig {
            endpoint: Some("https://gbrain.example.invalid/query".to_string()),
            token_env: Some("GBRAIN_TOKEN".to_string()),
            timeout_ms: Some(7000),
        },
    };

    let wiki = preflight_knowledge_read_status(&config, "wiki", "<missing>");
    assert!(!wiki.available);
    assert_eq!(wiki.state, "unavailable");
    assert_eq!(wiki.reason_code, "token_env_missing");
    assert!(wiki.reason.contains("token env is missing"));

    let gbrain = preflight_knowledge_read_status(&config, "gbrain", "<missing>");
    assert!(!gbrain.available);
    assert_eq!(gbrain.state, "unavailable");
    assert_eq!(gbrain.reason_code, "token_missing");
    assert!(gbrain.reason.contains("token is unavailable"));
}

#[test]
fn knowledge_read_preflight_never_claims_live_adapter_without_wiring() {
    let config = KnowledgeReadConfig {
        wiki: KnowledgeReadSourceConfig {
            endpoint: Some("https://wiki.example.invalid/query".to_string()),
            token_env: Some("WIKI_TOKEN".to_string()),
            timeout_ms: Some(5000),
        },
        gbrain: KnowledgeReadSourceConfig::disabled(),
    };

    let wiki = preflight_knowledge_read_status(&config, "wiki", "<set>");
    assert!(!wiki.available);
    assert_eq!(wiki.adapter_kind, "preflight_only");
    assert_eq!(wiki.state, "preflight_ready_adapter_missing");
    assert_eq!(wiki.reason_code, "real_adapter_missing");
    assert!(wiki.reason.contains("no audited live adapter is wired"));
    assert!(wiki.local_preview_is_separate);
    assert!(!wiki.connects_real_service);
    assert!(!wiki.writes_automatically);

    let gbrain = preflight_knowledge_read_status(&config, "gbrain", "<set>");
    assert!(!gbrain.available);
    assert_eq!(gbrain.adapter_kind, "preflight_only");
    assert_eq!(gbrain.state, "unavailable");
    assert_eq!(gbrain.reason_code, "endpoint_missing");
}

#[test]
fn knowledge_read_preflight_rejects_unknown_source_without_claiming_live() {
    let config = KnowledgeReadConfig::disabled();

    let status = preflight_knowledge_read_status(&config, "local_preview", "<set>");
    assert!(!status.available);
    assert_eq!(status.adapter_kind, "preflight_only");
    assert_eq!(status.state, "unavailable");
    assert_eq!(status.reason_code, "unknown_source");
    assert_eq!(status.sources, vec!["wiki", "gbrain"]);
    assert!(status.reason.contains("only wiki and gbrain"));
    assert!(status.local_preview_is_separate);
    assert!(!status.connects_real_service);
    assert!(!status.writes_automatically);
}

#[test]
fn readonly_http_wiki_adapter_queries_local_service_and_returns_receipt() {
    let (endpoint, request_rx) = spawn_knowledge_read_server(
        200,
        r#"{"hits":[{"source":"wiki","title":"Runbook","uri":"wiki://runbook/1","preview":"Readonly hit","provenance":"wiki_fixture"},{"title":"Second","url":"wiki://runbook/2","snippet":"Second hit"}]}"#,
    );
    let token = "test-token-knowledge-read";
    let adapter = ReadonlyHttpKnowledgeReadAdapter::new_wiki(endpoint, token, 5_000);

    let status = adapter.status();
    assert!(status.available);
    assert_eq!(status.adapter_kind, "readonly_http");
    assert_eq!(status.state, "ready");
    assert_eq!(status.sources, vec!["wiki"]);
    assert!(status.local_preview_is_separate);
    assert!(status.connects_real_service);
    assert!(!status.writes_automatically);

    let result = adapter
        .query(KnowledgeReadQuery {
            source: "wiki".to_string(),
            query: "runbook".to_string(),
            limit: 1,
        })
        .expect("wiki readonly HTTP adapter should return hits");
    let request = request_rx
        .recv()
        .expect("test server should capture request");
    let request_lower = request.to_ascii_lowercase();
    assert!(request.contains("POST /query HTTP/1.1"));
    assert!(request_lower.contains("authorization: bearer test-token-knowledge-read"));
    assert!(request.contains("\"source\":\"wiki\""));
    assert!(request.contains("\"read_only\":true"));
    assert_eq!(result.source, "wiki");
    assert_eq!(result.query, "runbook");
    assert!(result.read_only);
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].title, "Runbook");
    assert_eq!(result.hits[0].uri, "wiki://runbook/1");
    assert_eq!(result.hits[0].provenance, "wiki_fixture");
    assert!(result.receipt.contains("\"adapter\":\"readonly_http\""));
    assert!(result.receipt.contains("\"source\":\"wiki\""));
    assert!(result.receipt.contains("\"read_only\":true"));
    assert!(result.receipt.contains("\"writes_automatically\":false"));
    assert!(result.receipt.contains("\"token\":\"<redacted>\""));
    assert!(!result.receipt.contains(token));
    assert!(!format!("{result:?}").contains(token));
}

#[test]
fn readonly_http_gbrain_adapter_queries_local_service_and_returns_receipt() {
    let (endpoint, request_rx) = spawn_knowledge_read_server(
        200,
        r#"{"hits":[{"source":"gbrain","title":"Ops Playbook","uri":"gbrain://ops/1","preview":"Readonly GBrain hit","provenance":"gbrain_fixture"},{"title":"Fallback Source","url":"gbrain://ops/2","snippet":"Fallback snippet"}]}"#,
    );
    let token = "test-gbrain-token-knowledge-read";
    let adapter = ReadonlyHttpKnowledgeReadAdapter::new_gbrain(endpoint, token, 5_000);

    let status = adapter.status();
    assert!(status.available);
    assert_eq!(status.adapter_kind, "readonly_http");
    assert_eq!(status.state, "ready");
    assert_eq!(status.sources, vec!["gbrain"]);
    assert_eq!(status.reason_code, "gbrain_readonly_http_configured");
    assert!(status
        .reason
        .contains("gbrain read-only HTTP adapter is configured"));
    assert!(status.local_preview_is_separate);
    assert!(status.connects_real_service);
    assert!(!status.writes_automatically);

    let result = adapter
        .query(KnowledgeReadQuery {
            source: "gbrain".to_string(),
            query: "ops".to_string(),
            limit: 2,
        })
        .expect("gbrain readonly HTTP adapter should return hits");
    let request = request_rx
        .recv()
        .expect("test server should capture request");
    let request_lower = request.to_ascii_lowercase();
    assert!(request.contains("POST /query HTTP/1.1"));
    assert!(request_lower.contains("authorization: bearer test-gbrain-token-knowledge-read"));
    assert!(request.contains("\"source\":\"gbrain\""));
    assert!(request.contains("\"read_only\":true"));
    assert_eq!(result.source, "gbrain");
    assert_eq!(result.query, "ops");
    assert!(result.read_only);
    assert_eq!(result.hits.len(), 2);
    assert_eq!(result.hits[0].title, "Ops Playbook");
    assert_eq!(result.hits[0].uri, "gbrain://ops/1");
    assert_eq!(result.hits[0].provenance, "gbrain_fixture");
    assert_eq!(result.hits[1].provenance, "gbrain_readonly_http");
    assert!(result.receipt.contains("\"adapter\":\"readonly_http\""));
    assert!(result.receipt.contains("\"source\":\"gbrain\""));
    assert!(result.receipt.contains("\"read_only\":true"));
    assert!(result.receipt.contains("\"writes_automatically\":false"));
    assert!(result.receipt.contains("\"token\":\"<redacted>\""));
    assert!(!result.receipt.contains(token));
    assert!(!format!("{result:?}").contains(token));
}

#[test]
fn readonly_http_wiki_adapter_returns_structured_non_2xx_without_leaking_body_or_token() {
    let token = "test-token-must-not-leak";
    let (endpoint, _request_rx) = spawn_knowledge_read_server(
        500,
        r#"{"error":"upstream failed test-token-must-not-leak"}"#,
    );
    let adapter = ReadonlyHttpKnowledgeReadAdapter::new_wiki(endpoint, token, 5_000);

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "wiki".to_string(),
            query: "runbook".to_string(),
            limit: 3,
        })
        .expect_err("non-2xx must be a structured knowledge_read error");

    assert_eq!(error.code, "knowledge_read_http_status");
    assert_eq!(error.adapter_kind, "readonly_http");
    assert!(error.retryable);
    assert!(error.message.contains("status_code=500"));
    assert!(!error.message.contains(token));
    assert!(!format!("{error:?}").contains(token));
}

#[test]
fn readonly_http_gbrain_adapter_keeps_wiki_unavailable_in_gbrain_only_slice() {
    let adapter =
        ReadonlyHttpKnowledgeReadAdapter::new_gbrain("http://127.0.0.1:9/query", "test-token", 10);

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "wiki".to_string(),
            query: "runbook".to_string(),
            limit: 3,
        })
        .expect_err("wiki must remain unavailable in gbrain-only slice");

    assert_eq!(error.code, "knowledge_read_source_unavailable");
    assert_eq!(error.adapter_kind, "readonly_http");
    assert!(!error.retryable);
    assert!(error.message.contains("only gbrain is available"));
}

#[test]
fn readonly_http_adapter_keeps_gbrain_unavailable_in_wiki_only_slice() {
    let adapter =
        ReadonlyHttpKnowledgeReadAdapter::new_wiki("http://127.0.0.1:9/query", "test-token", 10);

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "gbrain".to_string(),
            query: "runbook".to_string(),
            limit: 3,
        })
        .expect_err("gbrain must remain unavailable until a separate adapter is wired");

    assert_eq!(error.code, "knowledge_read_source_unavailable");
    assert_eq!(error.adapter_kind, "readonly_http");
    assert!(!error.retryable);
    assert!(error.message.contains("only wiki is available"));
}

#[test]
fn readonly_http_adapter_rejects_unknown_source_before_network() {
    let adapter =
        ReadonlyHttpKnowledgeReadAdapter::new_wiki("http://127.0.0.1:9/query", "test-token", 10);

    let error = adapter
        .query(KnowledgeReadQuery {
            source: "local_preview".to_string(),
            query: "runbook".to_string(),
            limit: 3,
        })
        .expect_err("unknown source must be rejected before any network call");

    assert_eq!(error.code, "knowledge_read_unknown_source");
    assert_eq!(error.adapter_kind, "readonly_http");
    assert!(!error.retryable);
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
    let Some((headers, body)) = request_text.split_once("\r\n\r\n") else {
        return false;
    };
    let Some(length_line) = headers
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("content-length:"))
    else {
        return true;
    };
    let Some(length_raw) = length_line.split_once(':').map(|(_, value)| value.trim()) else {
        return false;
    };
    let Ok(content_length) = length_raw.parse::<usize>() else {
        return false;
    };
    body.len() >= content_length
}

struct ReadOnlyEnvGuard {
    name: String,
}

impl ReadOnlyEnvGuard {
    fn set(name: &str, value: &str) -> Self {
        std::env::set_var(name, value);
        Self {
            name: name.to_string(),
        }
    }

    fn clear(name: &str) -> Self {
        std::env::remove_var(name);
        Self {
            name: name.to_string(),
        }
    }
}

impl Drop for ReadOnlyEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var(&self.name);
    }
}

#[test]
fn readonly_http_gbrain_from_config_resolves_token_from_env_and_reads() {
    let _guard = ReadOnlyEnvGuard::set("CHUANG_TEST_FROM_CONFIG_TOKEN", "from-config-secret");
    let (endpoint, request_rx) = spawn_knowledge_read_server(
        200,
        r#"{"hits":[{"source":"gbrain","title":"Note","uri":"gbrain://note/1","preview":"From config","provenance":"gbrain_fixture"}]}"#,
    );
    let config = KnowledgeReadSourceConfig {
        endpoint: Some(endpoint),
        token_env: Some("CHUANG_TEST_FROM_CONFIG_TOKEN".to_string()),
        timeout_ms: Some(5_000),
    };

    let adapter =
        ReadonlyHttpKnowledgeReadAdapter::new_gbrain_from_config(&config).expect("build from config");
    let result = adapter
        .query(KnowledgeReadQuery {
            source: "gbrain".to_string(),
            query: "note".to_string(),
            limit: 1,
        })
        .expect("gbrain from-config adapter should return hits");
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].uri, "gbrain://note/1");
    let request = request_rx
        .recv()
        .expect("test server should capture request");
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer from-config-secret"));
    assert!(!result.receipt.contains("from-config-secret"));
}

#[test]
fn readonly_http_from_config_returns_structured_error_when_token_env_unset() {
    let _guard = ReadOnlyEnvGuard::clear("CHUANG_TEST_FROM_CONFIG_UNSET_TOKEN");
    let config = KnowledgeReadSourceConfig {
        endpoint: Some("https://gbrain.example.invalid/query".to_string()),
        token_env: Some("CHUANG_TEST_FROM_CONFIG_UNSET_TOKEN".to_string()),
        timeout_ms: Some(5_000),
    };

    let error = match ReadonlyHttpKnowledgeReadAdapter::new_gbrain_from_config(&config) {
        Ok(_) => panic!("unset token must be a structured error"),
        Err(error) => error,
    };
    assert_eq!(error.code, "knowledge_read_token_missing");
    assert_eq!(error.adapter_kind, "readonly_http");
    assert!(!error.retryable);
}
