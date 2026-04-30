use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::FakeResponder;

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
fn agent_runtime_uses_fake_responder_output() {
    let mut store =
        SqliteMemoryStore::open(temp_db_path("fake-responder")).expect("sqlite store should open");
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
    assert!(result.response.body.contains("现在先把创项目跑起来"));
    assert_eq!(
        result.response.meta.provider.as_deref(),
        Some("fake-responder")
    );
}

#[test]
fn agent_runtime_preserves_prompt_and_trace_with_fake_responder() {
    let path = temp_db_path("fake-trace");
    let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
    store
        .put(record(
            "mem-1",
            "创项目先跑起来，这轮目标是最小闭环先跑通。",
            &[("kind", "goal")],
            "2026-04-30T20:10:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::with_responder(store, FakeResponder::new("fake-model-v2"));
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "创项目先跑起来".to_string(),
            recall_limit: 2,
            metadata: BTreeMap::new(),
            context_budget: None,
        })
        .expect("runtime should succeed");

    assert!(result.prompt.contains("[chuang-agent-runtime]"));
    assert_eq!(result.recall_hit_count, 1);
    assert!(result.recall_summary.contains("最小闭环先跑通"));
    assert_eq!(result.response.model_name, "fake-model-v2");
    assert!(result.response.trace.contains("recall_hits=1"));
    assert_eq!(
        result.response.meta.finish_reason.as_deref(),
        Some("stubbed")
    );
    let _ = fs::remove_file(path);
}
