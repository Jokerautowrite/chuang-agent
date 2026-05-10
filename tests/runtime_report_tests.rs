use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::{FakeResponder, ResponderMeta};
use chuang_agent::runtime_report::{
    build_runtime_report, report_metadata, runtime_observability_meta,
};
use chuang_agent::subagent_report::{ArtifactKind, ExecutionStatus};

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
                max_tokens: 348,
                reserve_system_tokens: 240,
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
    assert!(debug.drop_reasons.iter().any(|reason| {
        reason.segment_id == "working-user-input" && reason.reason == "budget_limit"
    }));
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
            max_tokens: 364,
            reserve_system_tokens: 240,
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

#[test]
fn runtime_report_metadata_exposes_governance_decision_when_present() {
    let mut report = build_runtime_report(
        &runtime(SqliteMemoryStore::open(temp_db_path("report-governance")).expect("sqlite store"))
            .run(&RuntimeRequest {
                user_input: "治理决策元数据".to_string(),
                recall_limit: 1,
                metadata: BTreeMap::new(),
                context_budget: None,
                extra_context_segments: Vec::new(),
            })
            .expect("runtime should succeed"),
        "report-governance",
        "task-governance",
        "agent-governance",
        None,
    );
    report.governance_decision = Some(chuang_agent::subagent_report::GovernanceDecisionSummary {
        action_id: "run-turn-1".to_string(),
        decision: "allowed".to_string(),
        reason: "read-only or draft action".to_string(),
    });

    let metadata = report_metadata(&report);
    assert_eq!(
        metadata.get("governance_action_id"),
        Some(&"run-turn-1".to_string())
    );
    assert_eq!(
        metadata.get("governance_decision"),
        Some(&"allowed:read-only or draft action".to_string())
    );
    assert_eq!(
        metadata.get("governance_reason"),
        Some(&"read-only or draft action".to_string())
    );
}

#[test]
fn runtime_report_observability_meta_promotes_goal_session_tool_provider_fields() {
    let mut extra = BTreeMap::new();
    extra.insert("transport".to_string(), "openai-compatible".to_string());
    extra.insert("transport_mode".to_string(), "native".to_string());
    extra.insert("status_code".to_string(), "200".to_string());
    extra.insert("runtime_report_id".to_string(), "report-turn-1".to_string());
    extra.insert("runtime_report_task_id".to_string(), "turn-1".to_string());
    extra.insert(
        "runtime_report_agent_id".to_string(),
        "chuang-cli".to_string(),
    );
    extra.insert("runtime_report_status".to_string(), "Success".to_string());
    extra.insert("governance_action_id".to_string(), "run-turn".to_string());
    extra.insert(
        "governance_decision".to_string(),
        "allowed:read-only or draft action".to_string(),
    );
    extra.insert(
        "governance_reason".to_string(),
        "read-only or draft action".to_string(),
    );
    extra.insert("goal_id".to_string(), "mainline-mvp".to_string());
    extra.insert("goal_objective".to_string(), "收尾可观测性".to_string());
    extra.insert("goal_context_injected".to_string(), "true".to_string());
    extra.insert(
        "knowledge_context_preview_count".to_string(),
        "1".to_string(),
    );
    extra.insert(
        "knowledge_context_injected_count".to_string(),
        "0".to_string(),
    );
    extra.insert(
        "knowledge_context_dropped_count".to_string(),
        "1".to_string(),
    );
    extra.insert(
        "knowledge_context_dropped_segment_ids".to_string(),
        r#"["external-knowledge-knowledge-segment-1"]"#.to_string(),
    );
    extra.insert("session_id".to_string(), "thread-a".to_string());
    extra.insert("session_memory_scope".to_string(), "session".to_string());
    extra.insert(
        "session_memory_recall_isolated".to_string(),
        "true".to_string(),
    );
    extra.insert(
        "session_memory_recall_hit_count".to_string(),
        "2".to_string(),
    );
    extra.insert(
        "session_memory_write_status".to_string(),
        "compacted".to_string(),
    );
    extra.insert(
        "session_memory_compacted_from_chars".to_string(),
        "2696".to_string(),
    );
    extra.insert(
        "session_memory_compacted_to_chars".to_string(),
        "948".to_string(),
    );
    extra.insert("tool_call_count".to_string(), "1".to_string());
    extra.insert("tool_protocol_error_count".to_string(), "0".to_string());

    let result = chuang_agent::agent_runtime::RuntimeResult {
        prompt: "prompt".to_string(),
        response: chuang_agent::agent_runtime::RuntimeResponse {
            model_name: "gpt-observable".to_string(),
            body: "body".to_string(),
            trace: "trace".to_string(),
            meta: ResponderMeta {
                provider: Some("local-openai-compatible".to_string()),
                recall_hit_count: Some(2),
                finish_reason: Some("stop".to_string()),
                extra,
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 2,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: 42,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    };

    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability.get("provider"),
        Some(&"local-openai-compatible".to_string())
    );
    assert_eq!(
        observability.get("model_name"),
        Some(&"gpt-observable".to_string())
    );
    assert_eq!(
        observability.get("goal_id"),
        Some(&"mainline-mvp".to_string())
    );
    assert_eq!(
        observability.get("governance_action_id"),
        Some(&"run-turn".to_string())
    );
    assert_eq!(
        observability.get("runtime_report_id"),
        Some(&"report-turn-1".to_string())
    );
    assert_eq!(
        observability.get("runtime_report_task_id"),
        Some(&"turn-1".to_string())
    );
    assert_eq!(
        observability.get("runtime_report_agent_id"),
        Some(&"chuang-cli".to_string())
    );
    assert_eq!(
        observability.get("runtime_report_status"),
        Some(&"Success".to_string())
    );
    assert_eq!(
        observability.get("governance_decision"),
        Some(&"allowed:read-only or draft action".to_string())
    );
    assert_eq!(
        observability.get("governance_reason"),
        Some(&"read-only or draft action".to_string())
    );
    assert_eq!(
        observability.get("knowledge_context_preview_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("knowledge_context_injected_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("knowledge_context_dropped_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("knowledge_context_dropped_segment_ids"),
        Some(&r#"["external-knowledge-knowledge-segment-1"]"#.to_string())
    );
    assert_eq!(
        observability.get("session_id"),
        Some(&"thread-a".to_string())
    );
    assert_eq!(
        observability.get("session_memory_write_status"),
        Some(&"compacted".to_string())
    );
    assert_eq!(
        observability.get("session_memory_compacted_from_chars"),
        Some(&"2696".to_string())
    );
    assert_eq!(
        observability.get("session_memory_compacted_to_chars"),
        Some(&"948".to_string())
    );
    assert_eq!(observability.get("tool_call_count"), Some(&"1".to_string()));
    assert_eq!(
        observability.get("tool_protocol_error_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("tool_typed_failure_count"),
        Some(&"0".to_string())
    );

    let report = build_runtime_report(&result, "report-obs", "task-obs", "agent-obs", None);
    let artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.observability")
        .expect("observability artifact should exist");
    let description = artifact.description.as_deref().expect("description");
    assert!(description.contains("provider=local-openai-compatible"));
    assert!(description.contains("goal=mainline-mvp"));
    assert!(description.contains("session=thread-a"));
    assert!(description.contains("tool_typed_failures=0"));
}

#[test]
fn runtime_report_observability_meta_includes_typed_execution_failures() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_events_json".to_string(),
        r#"[
            {"kind":"tool_call","ok":false,"failure_class":"adapter_unavailable"},
            {"kind":"tool_call","ok":false,"failure_class":"timeout"},
            {"kind":"protocol_error","protocol_error_code":"plain_text_response"}
        ]"#
        .to_string(),
    );
    extra.insert(
        "tool_calls_json".to_string(),
        r#"[{"ok":false,"failure_class":"invalid_output"}]"#.to_string(),
    );

    let result = chuang_agent::agent_runtime::RuntimeResult {
        prompt: "prompt".to_string(),
        response: chuang_agent::agent_runtime::RuntimeResponse {
            model_name: "gpt-observable".to_string(),
            body: "body".to_string(),
            trace: "trace".to_string(),
            meta: ResponderMeta {
                provider: Some("local-openai-compatible".to_string()),
                recall_hit_count: Some(0),
                finish_reason: Some("stop".to_string()),
                extra,
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: 20,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    };

    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability.get("tool_typed_failure_count"),
        Some(&"4".to_string())
    );
    assert_eq!(
        observability.get("tool_typed_failure_classes"),
        Some(&"adapter_unavailable,invalid_output,protocol_error,timeout".to_string())
    );

    let report = build_runtime_report(
        &result,
        "report-typed-failure",
        "task-typed-failure",
        "agent-typed-failure",
        None,
    );
    let observability_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.observability")
        .expect("observability artifact should exist");
    let description = observability_artifact
        .description
        .as_deref()
        .expect("description");
    assert!(description.contains("tool_typed_failures=4"));

    let tool_events_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.tool_events_json")
        .expect("tool events artifact should exist");
    let events_description = tool_events_artifact
        .description
        .as_deref()
        .expect("description");
    assert!(events_description.contains("typed_failures=2"));
}

#[test]
fn runtime_report_observability_meta_promotes_provider_failure_diagnostics() {
    let mut extra = BTreeMap::new();
    extra.insert("transport".to_string(), "openai-compatible".to_string());
    extra.insert("transport_mode".to_string(), "http".to_string());
    extra.insert(
        "request_url".to_string(),
        "http://127.0.0.1:8080/v1/chat/completions".to_string(),
    );
    extra.insert("request_method".to_string(), "POST".to_string());
    extra.insert("request_message_count".to_string(), "2".to_string());
    extra.insert("config_error_field".to_string(), "http_timeout".to_string());
    extra.insert("status_code".to_string(), "408".to_string());
    extra.insert("provider_response_ok".to_string(), "false".to_string());
    extra.insert("provider_retryable".to_string(), "true".to_string());
    extra.insert(
        "provider_error_class".to_string(),
        "http_timeout".to_string(),
    );
    extra.insert(
        "provider_error_message".to_string(),
        "request timed out after 20ms".to_string(),
    );
    extra.insert(
        "provider_failure_reason_code".to_string(),
        "request_timeout".to_string(),
    );
    extra.insert(
        "provider_failure_category".to_string(),
        "timeout".to_string(),
    );
    extra.insert(
        "provider_timeout_reason_code".to_string(),
        "request_timeout".to_string(),
    );
    extra.insert(
        "provider_timeout_category".to_string(),
        "timeout".to_string(),
    );
    extra.insert("provider_timeout_ms".to_string(), "20".to_string());

    let result = chuang_agent::agent_runtime::RuntimeResult {
        prompt: "prompt".to_string(),
        response: chuang_agent::agent_runtime::RuntimeResponse {
            model_name: "primary-model".to_string(),
            body: "body".to_string(),
            trace: "trace".to_string(),
            meta: ResponderMeta {
                provider: Some("primary-openai".to_string()),
                recall_hit_count: Some(0),
                finish_reason: Some("invalid-config".to_string()),
                extra,
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: 12,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    };

    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability.get("request_url"),
        Some(&"http://127.0.0.1:8080/v1/chat/completions".to_string())
    );
    assert_eq!(
        observability.get("request_method"),
        Some(&"POST".to_string())
    );
    assert_eq!(
        observability.get("request_message_count"),
        Some(&"2".to_string())
    );
    assert_eq!(
        observability.get("config_error_field"),
        Some(&"http_timeout".to_string())
    );
    assert_eq!(observability.get("status_code"), Some(&"408".to_string()));
    assert_eq!(
        observability.get("provider_response_ok"),
        Some(&"false".to_string())
    );
    assert_eq!(
        observability.get("provider_retryable"),
        Some(&"true".to_string())
    );
    assert_eq!(
        observability.get("provider_error_class"),
        Some(&"http_timeout".to_string())
    );
    assert_eq!(
        observability.get("provider_error_message"),
        Some(&"request timed out after 20ms".to_string())
    );
    assert_eq!(
        observability.get("provider_failure_reason_code"),
        Some(&"request_timeout".to_string())
    );
    assert_eq!(
        observability.get("provider_failure_category"),
        Some(&"timeout".to_string())
    );
    assert_eq!(
        observability.get("provider_timeout_reason_code"),
        Some(&"request_timeout".to_string())
    );
    assert_eq!(
        observability.get("provider_timeout_category"),
        Some(&"timeout".to_string())
    );
    assert_eq!(
        observability.get("provider_timeout_ms"),
        Some(&"20".to_string())
    );
}

#[test]
fn runtime_report_observability_meta_promotes_provider_fallback_diagnostics() {
    let mut extra = BTreeMap::new();
    extra.insert("provider_response_ok".to_string(), "true".to_string());
    extra.insert("provider_fallback_used".to_string(), "true".to_string());
    extra.insert(
        "provider_fallback_from".to_string(),
        "primary-openai".to_string(),
    );
    extra.insert(
        "provider_fallback_reason".to_string(),
        "status_code=429".to_string(),
    );
    extra.insert(
        "provider_fallback_configured".to_string(),
        "true".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_retryable".to_string(),
        "true".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_status_code".to_string(),
        "429".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_error_class".to_string(),
        "http_status".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_error_message".to_string(),
        "at capacity".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_request_url".to_string(),
        "http://127.0.0.1:8080/v1/chat/completions".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_request_method".to_string(),
        "POST".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_request_message_count".to_string(),
        "2".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_transport".to_string(),
        "openai-compatible".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_transport_mode".to_string(),
        "http".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_response_ok".to_string(),
        "false".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_failure_reason_code".to_string(),
        "model_capacity".to_string(),
    );
    extra.insert(
        "provider_fallback_primary_failure_category".to_string(),
        "capacity".to_string(),
    );

    let result = chuang_agent::agent_runtime::RuntimeResult {
        prompt: "prompt".to_string(),
        response: chuang_agent::agent_runtime::RuntimeResponse {
            model_name: "fallback-model".to_string(),
            body: "body".to_string(),
            trace: "trace".to_string(),
            meta: ResponderMeta {
                provider: Some("primary-openai->fallback-fake".to_string()),
                recall_hit_count: Some(0),
                finish_reason: Some("stop".to_string()),
                extra,
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: 12,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    };

    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability.get("provider_response_ok"),
        Some(&"true".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_configured"),
        Some(&"true".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_used"),
        Some(&"true".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_from"),
        Some(&"primary-openai".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_reason"),
        Some(&"status_code=429".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_retryable"),
        Some(&"true".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_status_code"),
        Some(&"429".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_error_class"),
        Some(&"http_status".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_error_message"),
        Some(&"at capacity".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_request_url"),
        Some(&"http://127.0.0.1:8080/v1/chat/completions".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_request_method"),
        Some(&"POST".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_request_message_count"),
        Some(&"2".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_transport"),
        Some(&"openai-compatible".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_transport_mode"),
        Some(&"http".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_response_ok"),
        Some(&"false".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_failure_reason_code"),
        Some(&"model_capacity".to_string())
    );
    assert_eq!(
        observability.get("provider_fallback_primary_failure_category"),
        Some(&"capacity".to_string())
    );
}

#[test]
fn runtime_report_promotes_tool_report_metadata_to_artifact() {
    let mut extra = BTreeMap::new();
    extra.insert("tool_call_count".to_string(), "1".to_string());
    extra.insert("tool_protocol_error_count".to_string(), "2".to_string());
    extra.insert(
        "tool_report_json".to_string(),
        r#"{"schema_version":6,"status":"completed","workspace_root":"/tmp/work","rounds":2,"call_count":1,"calls":[]}"#
            .to_string(),
    );
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
                extra,
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: 12,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    };

    let report = build_runtime_report(&result, "report-tool", "task-tool", "agent-tool", None);

    assert!(report.summary.contains("tool_calls=1"));
    assert!(report.summary.contains("tool_protocol_errors=2"));
    let tool_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.tool_report_json")
        .expect("tool report artifact should exist");
    assert_eq!(tool_artifact.kind, ArtifactKind::Log);
    assert!(tool_artifact
        .description
        .as_deref()
        .expect("description")
        .contains("calls=1"));
    assert!(report
        .artifacts
        .iter()
        .any(|artifact| artifact.locator == "runtime_meta.observability"));
}

#[test]
fn runtime_report_promotes_tool_events_metadata_to_artifact() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_events_json".to_string(),
        r#"[{"round":1,"kind":"tool_call","tool_name":"write_file","atomic_tool_name":"file_write","decision":"allowed:read-only or draft action","ok":true,"failure_class":null,"duration_ms":12,"retryable":false,"summary":"atomic_tool=file_write write_file path=notes/out.txt bytes=5","protocol_error_code":null,"protocol_error_message":null},{"round":2,"kind":"protocol_error","tool_name":null,"atomic_tool_name":null,"decision":null,"ok":null,"failure_class":null,"duration_ms":null,"retryable":null,"summary":null,"protocol_error_code":"plain_text_response","protocol_error_message":"tool loop requires ACTION or FINAL; plain text is not accepted"}]"#
            .to_string(),
    );
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
                extra,
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "preview".to_string(),
        packed_token_count: 12,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    };

    let report = build_runtime_report(
        &result,
        "report-events",
        "task-events",
        "agent-events",
        None,
    );

    let events_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.tool_events_json")
        .expect("tool events artifact should exist");
    assert_eq!(events_artifact.kind, ArtifactKind::Log);
    assert!(events_artifact
        .description
        .as_deref()
        .expect("description")
        .contains("tool_calls=1"));
    assert!(events_artifact
        .description
        .as_deref()
        .expect("description")
        .contains("protocol_errors=1"));
    assert!(report
        .artifacts
        .iter()
        .any(|artifact| artifact.locator == "runtime_meta.observability"));
}
