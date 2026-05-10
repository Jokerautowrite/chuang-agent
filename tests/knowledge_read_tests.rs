use chuang_agent::knowledge_read::{
    preflight_knowledge_read_status, FakeKnowledgeReadAdapter, KnowledgeReadAdapter,
    KnowledgeReadConfig, KnowledgeReadHit, KnowledgeReadQuery, KnowledgeReadSourceConfig,
    UnavailableKnowledgeReadAdapter,
};

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
