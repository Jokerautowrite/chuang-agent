use std::collections::BTreeMap;

use chuang_agent::memory_recall::{MemoryRecallPipeline, RecallRequest};
use chuang_agent::memory_store::{InMemoryMemoryStore, MemoryRecord, MemoryStore};

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
fn recall_pipeline_returns_ranked_hits_for_query() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "长期记忆主线优先，先把 recall pipeline 跑通",
            &[("kind", "plan")],
            "2026-04-30T17:00:00Z",
        ))
        .expect("first put should succeed");
    store
        .put(record(
            "mem-2",
            "BrowserWorker 不要抢长期记忆主线",
            &[("kind", "warning")],
            "2026-04-30T17:01:00Z",
        ))
        .expect("second put should succeed");

    let pipeline = MemoryRecallPipeline::new(store);
    let result = pipeline
        .recall(&RecallRequest {
            query_text: "长期记忆主线".to_string(),
            metadata: BTreeMap::new(),
            limit: 5,
        })
        .expect("recall should succeed");

    assert_eq!(result.hits.len(), 2);
    assert_eq!(result.hits[0].record.id, "mem-1");
    assert_eq!(result.hits[0].rank, 1);
    assert!(result.summary.contains("长期记忆主线优先"));
}

#[test]
fn recall_pipeline_respects_metadata_filter_and_limit() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "长期记忆要优先收口",
            &[("kind", "plan")],
            "2026-04-30T17:00:00Z",
        ))
        .expect("first put should succeed");
    store
        .put(record(
            "mem-2",
            "长期记忆的风险先记下来",
            &[("kind", "warning")],
            "2026-04-30T17:01:00Z",
        ))
        .expect("second put should succeed");
    store
        .put(record(
            "mem-3",
            "长期记忆下一步是 recall pipeline",
            &[("kind", "plan")],
            "2026-04-30T17:02:00Z",
        ))
        .expect("third put should succeed");

    let mut metadata = BTreeMap::new();
    metadata.insert("kind".to_string(), "plan".to_string());

    let pipeline = MemoryRecallPipeline::new(store);
    let result = pipeline
        .recall(&RecallRequest {
            query_text: "长期记忆".to_string(),
            metadata,
            limit: 1,
        })
        .expect("recall should succeed");

    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].record.id, "mem-1");
    assert_eq!(result.hits[0].rank, 1);
}

#[test]
fn recall_pipeline_rejects_zero_limit_request() {
    let store = InMemoryMemoryStore::new();
    let pipeline = MemoryRecallPipeline::new(store);

    let error = pipeline
        .recall(&RecallRequest {
            query_text: "长期记忆".to_string(),
            metadata: BTreeMap::new(),
            limit: 0,
        })
        .expect_err("zero limit should fail");

    assert_eq!(
        format!("{:?}", error),
        "InvalidRequest(\"limit_must_be_positive\")"
    );
}

#[test]
fn recall_pipeline_builds_memory_segments_from_hits() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "创项目主线先把 context engine 收口。",
            &[("kind", "goal")],
            "2026-04-30T17:00:00Z",
        ))
        .expect("first put should succeed");
    store
        .put(record(
            "mem-2",
            "BrowserWorker 继续降级成并行能力线。",
            &[("kind", "constraint")],
            "2026-04-30T17:01:00Z",
        ))
        .expect("second put should succeed");

    let pipeline = MemoryRecallPipeline::new(store);
    let result = pipeline
        .recall(&RecallRequest {
            query_text: "线".to_string(),
            metadata: BTreeMap::new(),
            limit: 2,
        })
        .expect("recall should succeed");

    assert_eq!(result.segments.len(), 2);
    assert_eq!(result.segments[0].id, "mem-1");
    assert_eq!(format!("{:?}", result.segments[0].source), "Memory");
    assert_eq!(result.segments[0].priority, 100);
    assert!(result.segments[0].tokens.is_some());
    assert_eq!(
        result.segments[0].metadata.get("kind"),
        Some(&"goal".to_string())
    );
    assert_eq!(
        result.segments[0].metadata.get("memory_boundary"),
        Some(&"recall_only_no_writeback".to_string())
    );
    assert_eq!(
        result.segments[0].metadata.get("decay_review_only"),
        Some(&"true".to_string())
    );
}

#[test]
fn recall_pipeline_builds_agent_input_block_from_hits() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "创项目最小闭环已经先跑起来。",
            &[("kind", "goal")],
            "2026-04-30T17:00:00Z",
        ))
        .expect("first put should succeed");
    store
        .put(record(
            "mem-2",
            "创项目后面再继续优化并准备开源。",
            &[("kind", "goal")],
            "2026-04-30T17:01:00Z",
        ))
        .expect("second put should succeed");

    let pipeline = MemoryRecallPipeline::new(store);
    let result = pipeline
        .recall(&RecallRequest {
            query_text: "创项目".to_string(),
            metadata: BTreeMap::new(),
            limit: 2,
        })
        .expect("recall should succeed");

    assert!(result.agent_input.starts_with("[memory-recall]"));
    assert!(result.agent_input.contains("query=创项目"));
    assert!(result
        .agent_input
        .contains("boundary=recall_only archive_read_only=true maintenance_writeback=false"));
    assert!(result
        .agent_input
        .contains("[mem-1] layer=internal_identity writeback_target=manual_review_only"));
    assert!(result
        .agent_input
        .contains("创项目后面再继续优化并准备开源。"));
}
