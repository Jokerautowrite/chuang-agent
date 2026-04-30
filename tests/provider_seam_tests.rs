use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::{FakeResponder, Responder, ScriptedResponder};

fn temp_db_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "chuang-agent-runtime-{name}-{}.db",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    path
}

fn record(id: &str, content: &str, metadata: &[(&str, &str)], created_at: &str) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        content: content.to_string(),
        metadata: metadata
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        created_at: created_at.to_string(),
        expires_at: None,
    }
}

#[test]
fn fake_responder_exposes_provider_identity_seam() {
    let responder = FakeResponder::new("fake-model-v0");
    let provider = responder.provider();

    assert_eq!(provider.provider_id, "fake-responder");
    assert_eq!(provider.model_name, "fake-model-v0");
}

#[test]
fn scripted_responder_exposes_provider_identity_seam() {
    let responder = ScriptedResponder::new("scripted-model-v0", "ok");
    let provider = responder.provider();

    assert_eq!(provider.provider_id, "scripted-responder");
    assert_eq!(provider.model_name, "scripted-model-v0");
}

#[test]
fn agent_runtime_preserves_provider_identity_from_fake_responder() {
    let mut store = SqliteMemoryStore::open(temp_db_path("provider-seam-fake"))
        .expect("sqlite store should open");
    store
        .put(record(
            "mem-1",
            "现在先把创项目跑起来，再慢慢优化。",
            &[("kind", "goal")],
            "2026-04-30T20:00:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::with_responder(store, FakeResponder::new("fake-model-v1"));
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "现在先把创项目跑起来".to_string(),
            recall_limit: 3,
            metadata: BTreeMap::new(),
            context_budget: None,
        })
        .expect("runtime should succeed");

    assert_eq!(result.response.model_name, "fake-model-v1");
    assert_eq!(
        result.response.meta.provider.as_deref(),
        Some("fake-responder")
    );
}

#[test]
fn agent_runtime_preserves_provider_identity_from_scripted_responder() {
    let mut store = SqliteMemoryStore::open(temp_db_path("provider-seam-scripted"))
        .expect("sqlite store should open");
    store
        .put(record(
            "mem-1",
            "创项目先跑起来，别停。",
            &[("kind", "goal")],
            "2026-04-30T21:00:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::with_responder(
        store,
        ScriptedResponder::new("scripted-model-v1", "进入 scripted runtime 回复"),
    );
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "创项目继续推进".to_string(),
            recall_limit: 3,
            metadata: BTreeMap::new(),
            context_budget: None,
        })
        .expect("runtime should succeed");

    assert_eq!(result.response.model_name, "scripted-model-v1");
    assert_eq!(
        result.response.meta.provider.as_deref(),
        Some("scripted-responder")
    );
}
