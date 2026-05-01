use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::{FakeResponder, ResponderMeta};
use chuang_agent::runtime_report::{build_runtime_report, report_metadata};
use chuang_agent::subagent_report::ExecutionStatus;

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
fn runtime_report_builder_carries_runtime_debug_fields() {
    let mut store =
        SqliteMemoryStore::open(temp_db_path("report-builder")).expect("sqlite store should open");
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
                max_tokens: 10,
                reserve_system_tokens: 10,
                min_working_tokens: 5,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    let report = build_runtime_report(
        &result,
        "report-runtime-bridge",
        "task-runtime-bridge",
        "agent-runtime-bridge",
        Some("parent-runtime-bridge".to_string()),
    );

    assert_eq!(report.report_id.0, "report-runtime-bridge");
    assert_eq!(report.task_id.0, "task-runtime-bridge");
    assert_eq!(report.agent_id.0, "agent-runtime-bridge");
    assert_eq!(
        report.parent_agent_id.expect("parent should exist").0,
        "parent-runtime-bridge"
    );
    assert_eq!(report.status, ExecutionStatus::Success);
    assert!(report.summary.contains("model=stub-responder"));
    assert!(report.summary.contains("packed_tokens="));
    assert_eq!(report.stdout_preview, Some(result.response.body.clone()));
    let debug = report.context_debug.expect("context debug should exist");
    assert_eq!(debug.dropped_segment_ids, result.dropped_segment_ids);
    assert_eq!(debug.drop_reasons.len(), 1);
    assert_eq!(debug.drop_reasons[0].segment_id, "working-user-input");
    assert_eq!(debug.drop_reasons[0].reason, "budget_limit");
    assert!(debug.budget_exceeded);
    assert_eq!(
        debug.budget_exceeded_reasons,
        vec!["min_working_tokens_unmet".to_string()]
    );
    assert!(debug.working_reservation.is_none());
}

#[test]
fn runtime_report_builder_carries_working_reservation_debug() {
    let packed = chuang_agent::agent_runtime::debug_pack_for_test(
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
        chuang_agent::context_engine::ContextBudget {
            max_tokens: 30,
            reserve_system_tokens: 16,
            min_working_tokens: 20,
            max_tool_results: 5,
            max_memory_segments: 20,
        },
    )
    .expect("debug pack should succeed");

    let result = chuang_agent::agent_runtime::RuntimeResult {
        prompt: "prompt".to_string(),
        response: chuang_agent::agent_runtime::RuntimeResponse {
            model_name: "stub-responder".to_string(),
            body: "body".to_string(),
            trace: "trace".to_string(),
            meta: ResponderMeta {
                provider: None,
                recall_hit_count: None,
                finish_reason: None,
                extra: BTreeMap::new(),
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: packed.total_tokens,
        dropped_segment_ids: packed.dropped_ids.clone(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: packed.drop_reasons.clone(),
            budget_exceeded: packed.budget_exceeded,
            budget_exceeded_reasons: packed.budget_exceeded_reasons.clone(),
            working_reservation: packed.working_reservation.clone(),
        },
    };

    let report = build_runtime_report(&result, "report-wr-1", "task-wr-1", "agent-wr-1", None);
    let debug = report.context_debug.expect("context debug should exist");
    let reservation = debug
        .working_reservation
        .expect("working reservation should exist");
    assert_eq!(reservation.reserved_segment_id, "working-user-input");
    assert_eq!(reservation.reserved_tokens, 20);
    assert_eq!(reservation.dropped_segment_ids, vec!["mem-1".to_string()]);
    assert_eq!(reservation.reason, "minimum_working_tokens");
}

#[test]
fn runtime_report_metadata_exposes_core_fields() {
    let mut store =
        SqliteMemoryStore::open(temp_db_path("report-metadata")).expect("sqlite store should open");
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
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    let report = build_runtime_report(
        &result,
        "report-meta-1",
        "task-meta-1",
        "agent-meta-1",
        None,
    );
    let metadata = report_metadata(&report);

    assert_eq!(metadata.get("schema_version"), Some(&"1.0.0".to_string()));
    assert_eq!(metadata.get("status"), Some(&"Success".to_string()));
    assert_eq!(
        metadata.get("report_id"),
        Some(&"report-meta-1".to_string())
    );
    assert_eq!(metadata.get("task_id"), Some(&"task-meta-1".to_string()));
    assert_eq!(metadata.get("agent_id"), Some(&"agent-meta-1".to_string()));
    assert!(metadata
        .get("summary")
        .expect("summary should exist")
        .contains("recall_hits=1"));
    assert!(!metadata.contains_key("parent_agent_id"));
}
