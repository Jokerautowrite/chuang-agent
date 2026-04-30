use std::collections::BTreeMap;

use chuang_agent::agent_runtime::{
    debug_pack_for_test, AgentRuntime, AgentRuntimeError, RuntimeRequest,
};
use chuang_agent::context_engine::ContextBudget;
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
fn agent_runtime_runs_minimal_loop_with_packed_context() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "创项目先自己跑起来，先闭环再优化。",
            &[("kind", "goal")],
            "2026-04-30T18:00:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::new(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "现在先把创项目跑起来".to_string(),
            recall_limit: 3,
            metadata: BTreeMap::new(),
            context_budget: None,
        })
        .expect("runtime should succeed");

    assert!(result.prompt.contains("[packed-context]"));
    assert!(result.prompt.contains("user_input=现在先把创项目跑起来"));
    assert!(result.packed_context_preview.contains("system-core"));
    assert!(result.packed_context_preview.contains("working-user-input"));
    assert_eq!(result.response.model_name, "stub-responder");
    assert!(result.response.body.contains("现在先把创项目跑起来"));
}

#[test]
fn agent_runtime_handles_empty_recall_hits() {
    let store = InMemoryMemoryStore::new();
    let runtime = AgentRuntime::new(store);

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "没有命中的问题".to_string(),
            recall_limit: 2,
            metadata: BTreeMap::new(),
            context_budget: None,
        })
        .expect("runtime should succeed");

    assert_eq!(result.recall_hit_count, 0);
    assert_eq!(result.response.model_name, "stub-responder");
    assert_eq!(result.response.meta.recall_hit_count, Some(0));
}

#[test]
fn agent_runtime_rejects_zero_recall_limit() {
    let store = InMemoryMemoryStore::new();
    let runtime = AgentRuntime::new(store);

    let error = runtime
        .run(&RuntimeRequest {
            user_input: "测试无效请求".to_string(),
            recall_limit: 0,
            metadata: BTreeMap::new(),
            context_budget: None,
        })
        .expect_err("zero recall limit should fail");

    assert_eq!(
        format!("{:?}", error),
        "Recall(InvalidRequest(\"limit_must_be_positive\"))"
    );
}

#[test]
fn debug_pack_for_test_drops_recall_segment_under_tight_budget() {
    let packed = debug_pack_for_test(
        "把创项目主线继续推进",
        &[
            chuang_agent::context_engine::ContextSegment {
                id: "mem-1".to_string(),
                source: chuang_agent::context_engine::SegmentSource::Memory,
                content: "很长的长期记忆片段，用来制造 budget 压力，而且这里故意把内容写得更长更长，让 token 估算明显超过剩余空间。".to_string(),
                tokens: Some(40),
                priority: 100,
                created_at: chrono::DateTime::parse_from_rfc3339("2026-04-30T18:00:00Z").unwrap().with_timezone(&chrono::Utc),
                last_accessed: chrono::DateTime::parse_from_rfc3339("2026-04-30T18:00:00Z").unwrap().with_timezone(&chrono::Utc),
                metadata: std::collections::HashMap::new(),
            },
        ],
        ContextBudget {
            max_tokens: 20,
            reserve_system_tokens: 16,
            min_working_tokens: 1,
            max_tool_results: 5,
            max_memory_segments: 20,
        },
    )
    .expect("debug pack should succeed");

    assert!(packed.dropped_ids.iter().any(|id| id == "mem-1"));
}

#[test]
fn agent_runtime_exposes_context_pack_debug_artifacts() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "很长的长期记忆片段，用来制造 budget 压力，而且这里故意把内容写得更长更长，让 token 估算明显超过剩余空间。",
            &[("kind", "goal")],
            "2026-04-30T18:00:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::new(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "把创项目主线继续推进".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(ContextBudget {
                max_tokens: 24,
                reserve_system_tokens: 16,
                min_working_tokens: 1,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
        })
        .expect("runtime should succeed");

    assert!(result
        .packed_context_preview
        .starts_with("[packed-context]"));
    assert!(result.packed_context_preview.contains("system-core"));
    assert!(result.packed_context_preview.contains("working-user-input"));
    assert!(result.packed_context_preview.contains("dropped="));
    assert!(result.packed_context_preview.contains("drop_reasons=none"));
    assert!(result
        .packed_context_preview
        .contains("budget_exceeded=false"));
    assert!(result.packed_token_count > 0);
}

#[test]
fn agent_runtime_exposes_budget_exceeded_reason_in_preview() {
    let runtime = AgentRuntime::new(InMemoryMemoryStore::new());

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "这是一段很长很长的用户输入，用来制造 working segment 无法装入预算的情况"
                .to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(ContextBudget {
                max_tokens: 10,
                reserve_system_tokens: 10,
                min_working_tokens: 5,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
        })
        .expect("runtime should succeed");

    assert!(result
        .packed_context_preview
        .contains("budget_exceeded=true"));
}

#[test]
fn agent_runtime_exposes_working_reservation_reason_when_memory_is_dropped() {
    let packed = debug_pack_for_test(
        "12345678901234567890",
        &[chuang_agent::context_engine::ContextSegment {
            id: "mem-1".to_string(),
            source: chuang_agent::context_engine::SegmentSource::Memory,
            content: "需要被挤掉的记忆段，给 working segment 腾预算。".to_string(),
            tokens: Some(8),
            priority: 100,
            created_at: chrono::DateTime::parse_from_rfc3339("2026-04-30T18:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            last_accessed: chrono::DateTime::parse_from_rfc3339("2026-04-30T18:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            metadata: std::collections::HashMap::new(),
        }],
        ContextBudget {
            max_tokens: 30,
            reserve_system_tokens: 16,
            min_working_tokens: 20,
            max_tool_results: 5,
            max_memory_segments: 20,
        },
    )
    .expect("debug pack should succeed");

    assert!(packed.dropped_ids.iter().any(|id| id == "mem-1"));
    let reservation = packed
        .working_reservation
        .expect("working reservation should exist");
    assert_eq!(reservation.reserved_segment_id, "working-user-input");
    assert_eq!(reservation.reserved_tokens, 20);
    assert_eq!(reservation.dropped_segment_ids, vec!["mem-1".to_string()]);
    assert_eq!(reservation.reason.as_str(), "minimum_working_tokens");
    assert!(packed
        .drop_reasons
        .iter()
        .any(|reason| reason.segment_id == "mem-1" && reason.reason.as_str() == "budget_limit"));
    assert!(!packed.budget_exceeded);
}

#[test]
fn agent_runtime_surfaces_context_pack_errors() {
    let store = InMemoryMemoryStore::new();
    let runtime = AgentRuntime::new(store);

    let error = runtime
        .run(&RuntimeRequest {
            user_input: "测试 context budget 失败".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(ContextBudget {
                max_tokens: 4,
                reserve_system_tokens: 4,
                min_working_tokens: 1,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
        })
        .expect_err("context pack should fail");

    assert!(matches!(error, AgentRuntimeError::ContextPack(_)));
}
