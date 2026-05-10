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

fn runtime<S>(store: S) -> AgentRuntime<S, FakeResponder> {
    AgentRuntime::with_responder(store, FakeResponder::new("stub-responder"))
}

#[test]
fn agent_runtime_runs_with_sqlite_memory_store() {
    let path = temp_db_path("sqlite-loop");
    let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
    store
        .put(record(
            "mem-1",
            "现在先把创项目跑起来，再慢慢优化。",
            &[("kind", "goal")],
            "2026-04-30T19:00:00Z",
        ))
        .expect("put should succeed");
    store
        .put(record(
            "mem-2",
            "开源放到闭环跑稳之后。",
            &[("kind", "plan")],
            "2026-04-30T19:01:00Z",
        ))
        .expect("put should succeed");

    let runtime = runtime(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "现在先把创项目跑起来".to_string(),
            recall_limit: 5,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result.prompt.contains("[chuang-agent-runtime]"));
    assert!(result.prompt.contains("[packed-context]"));
    assert!(result.prompt.contains("system-core"));
    assert!(result.prompt.contains("mem-1"));
    assert_eq!(result.response.model_name, "stub-responder");
    assert!(result.response.body.contains("现在先把创项目跑起来"));
    let _ = fs::remove_file(path);
}

#[test]
fn agent_runtime_returns_structured_trace_fields() {
    let mut metadata = BTreeMap::new();
    metadata.insert("kind".to_string(), "goal".to_string());

    let mut store =
        SqliteMemoryStore::open(temp_db_path("trace")).expect("sqlite store should open");
    store
        .put(record(
            "mem-1",
            "创项目先跑起来，这轮目标是最小闭环先跑通。",
            &[("kind", "goal")],
            "2026-04-30T19:10:00Z",
        ))
        .expect("put should succeed");

    let runtime = runtime(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "创项目先跑起来".to_string(),
            recall_limit: 3,
            metadata,
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert_eq!(result.recall_hit_count, 1);
    assert!(result.recall_summary.contains("最小闭环先跑通"));
    assert!(result.response.trace.contains("recall_hits=1"));
    assert_eq!(result.response.meta.recall_hit_count, Some(1));
}

#[test]
fn agent_runtime_exposes_structured_context_debug_fields() {
    let mut store =
        SqliteMemoryStore::open(temp_db_path("context-debug")).expect("sqlite store should open");
    store
        .put(record(
            "mem-1",
            "这是一段非常长的长期记忆，用来制造预算压力并触发 recall segment 被裁掉。",
            &[("kind", "goal")],
            "2026-04-30T19:20:00Z",
        ))
        .expect("put should succeed");

    let runtime = runtime(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "这是一段很长很长的用户输入，用来制造 working segment 无法装入预算的情况"
                .to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(chuang_agent::context_engine::ContextBudget {
                max_tokens: 280,
                reserve_system_tokens: 240,
                min_working_tokens: 5,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result
        .dropped_segment_ids
        .iter()
        .any(|id| id == "working-user-input"));
    assert!(result
        .context_debug
        .drop_reasons
        .iter()
        .any(|reason| reason.segment_id == "working-user-input"
            && reason.reason.as_str() == "budget_limit"));
    assert!(result.context_debug.budget_exceeded);
    assert_eq!(
        result.context_debug.budget_exceeded_reasons[0].as_str(),
        "min_working_tokens_unmet"
    );
}
