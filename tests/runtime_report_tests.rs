use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::agent_runtime::{debug_pack_for_test, AgentRuntime, RuntimeRequest};
use chuang_agent::context_engine::{ContextBudget, SegmentSource};
use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::{FakeResponder, ResponderMeta};
use chuang_agent::runtime_report::{
    build_runtime_report, report_metadata, runtime_observability_meta,
};
use chuang_agent::subagent_report::{ArtifactKind, ExecutionStatus};
use serde_json::json;

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

fn pressure_budget(user_input: &str, keep_working: bool, min_working_tokens: u32) -> ContextBudget {
    let baseline = debug_pack_for_test(
        user_input,
        &[],
        ContextBudget {
            max_tokens: 100_000,
            reserve_system_tokens: 100_000,
            min_working_tokens: 0,
            max_tool_results: 5,
            max_memory_segments: 20,
        },
    )
    .expect("baseline context should pack");
    let working_tokens = baseline
        .segments
        .iter()
        .find(|segment| segment.id == "working-user-input")
        .and_then(|segment| segment.tokens)
        .expect("working segment should exist");
    let protected_tokens = baseline
        .segments
        .iter()
        .filter(|segment| segment.id != "working-user-input")
        .map(|segment| segment.tokens.unwrap_or(0))
        .sum::<u32>();
    let system_tokens = baseline
        .segments
        .iter()
        .filter(|segment| matches!(segment.source, SegmentSource::System))
        .map(|segment| segment.tokens.unwrap_or(0))
        .sum::<u32>();

    ContextBudget {
        max_tokens: protected_tokens.saturating_add(if keep_working { working_tokens } else { 0 }),
        reserve_system_tokens: system_tokens,
        min_working_tokens,
        max_tool_results: 5,
        max_memory_segments: 20,
    }
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
            context_budget: Some(pressure_budget(
                "这是一段很长很长的用户输入，用来制造 working segment 无法装入预算的情况",
                false,
                5,
            )),
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
    let trace_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_response.trace")
        .expect("runtime response trace artifact should exist");
    assert_eq!(trace_artifact.kind, ArtifactKind::Log);
    assert!(trace_artifact
        .description
        .as_deref()
        .expect("trace description should exist")
        .contains("runtime_response_trace chars="));
    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability.get("runtime_response_trace_chars"),
        Some(&result.response.trace.chars().count().to_string())
    );
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
        pressure_budget("12345678901234567890", true, 20),
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
                extra: {
                    let mut extra = BTreeMap::new();
                    extra.insert(
                        "context_compaction_summary_json".to_string(),
                        serde_json::json!({
                            "event_count": 3,
                            "started_count": 1,
                            "completed_count": 1,
                            "dropped_count": 1,
                            "dropped_segment_ids": ["mem-1"],
                            "drop_reason_counts": {"duplicate_content": 1},
                            "trace_steps": ["normalize_tokens", "dedupe"]
                        })
                        .to_string(),
                    );
                    extra
                },
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
    extra.insert("runtime_event_count".to_string(), "2".to_string());
    extra.insert(
        "runtime_event_ledger_json".to_string(),
        r#"[{"event_type":"tool_started"},{"event_type":"tool_finished"}]"#.to_string(),
    );

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
        observability.get("runtime_event_count"),
        Some(&"2".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_tool_started_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_tool_finished_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_approval_requested_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_approval_resolved_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_elicitation_requested_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("tool_typed_failure_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("tool_unified_execution_status"),
        Some(&"ok".to_string())
    );
    assert_eq!(
        observability.get("tool_unified_execution_failure_count"),
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
    assert!(description.contains("tool_unified_execution_status=ok"));
    assert!(description.contains("tool_unified_execution_failures=0"));
    assert!(description.contains("runtime_events=2"));
    assert!(description.contains("tool_started=1"));
    assert!(description.contains("tool_finished=1"));
    let runtime_event_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.runtime_event_ledger_json")
        .expect("runtime event ledger artifact should exist");
    assert!(runtime_event_artifact
        .description
        .as_deref()
        .expect("description")
        .contains("tool_started=1"));
}

#[test]
fn runtime_report_observability_meta_defaults_runtime_event_and_handoff_counts_without_ledgers() {
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
                extra: BTreeMap::new(),
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
    for key in [
        "runtime_event_count",
        "runtime_event_tool_started_count",
        "runtime_event_tool_finished_count",
        "runtime_event_approval_requested_count",
        "runtime_event_approval_resolved_count",
        "runtime_event_elicitation_requested_count",
        "goal_handoff_parent_context_handoff_count",
        "goal_handoff_report_admission_ref_count",
        "subagent_children_child_count",
        "subagent_children_accepted_report_count",
        "subagent_children_report_admission_ref_count",
        "subagent_children_missing_report_count",
    ] {
        assert_eq!(observability.get(key), Some(&"0".to_string()));
    }
    for key in [
        "goal_handoff_report_admission_refs",
        "goal_handoff_report_admission_reason_codes",
        "subagent_children_report_admission_refs",
        "subagent_children_report_reason_codes",
        "goal_handoff_query_summary_json",
        "subagent_children_summary_json",
    ] {
        assert_eq!(observability.get(key), Some(&"none".to_string()));
    }
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
    assert_eq!(
        observability.get("tool_unified_execution_status"),
        Some(&"failed".to_string())
    );
    assert_eq!(
        observability.get("tool_unified_execution_failure_count"),
        Some(&"3".to_string())
    );
    assert_eq!(
        observability.get("tool_unified_execution_failure_classes"),
        Some(&"adapter_unavailable,invalid_output,timeout".to_string())
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
    assert!(description.contains("tool_unified_execution_status=failed"));
    assert!(description.contains("tool_unified_execution_failures=3"));

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
fn runtime_report_observability_meta_includes_tool_protocol_typed_failure_code() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_protocol_typed_failure_code".to_string(),
        "protocol_error".to_string(),
    );
    extra.insert(
        "tool_protocol_typed_failure_message".to_string(),
        "sensitive details should not appear".to_string(),
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
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("tool_typed_failure_classes"),
        Some(&"protocol_error".to_string())
    );

    let report = build_runtime_report(
        &result,
        "report-typed-failure-code",
        "task-typed-failure-code",
        "agent-typed-failure-code",
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
    assert!(description.contains("tool_typed_failures=1"));
    assert!(!description.contains("sensitive details should not appear"));
}

#[test]
fn runtime_report_observability_meta_promotes_provider_failure_diagnostics() {
    let mut extra = BTreeMap::new();
    extra.insert("transport".to_string(), "openai-compatible".to_string());
    extra.insert("transport_mode".to_string(), "http".to_string());
    extra.insert(
        "request_url".to_string(),
        "http://127.0.0.1:8080/v1/responses".to_string(),
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
        Some(&"http://127.0.0.1:8080/v1/responses".to_string())
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
        "http://127.0.0.1:8080/v1/responses".to_string(),
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
        Some(&"http://127.0.0.1:8080/v1/responses".to_string())
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
    extra.insert(
        "tool_protocol_errors_json".to_string(),
        r#"[{"code":"invalid_action_json","message":"ACTION payload is invalid","raw":"ACTION: {"},{"code":"plain_text_response","message":"plain text is not accepted","raw":"hello"}]"#
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
    let protocol_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.tool_protocol_errors_json")
        .expect("tool protocol errors artifact should exist");
    assert_eq!(protocol_artifact.kind, ArtifactKind::Log);
    let protocol_description = protocol_artifact
        .description
        .as_deref()
        .expect("protocol description");
    assert!(protocol_description.contains("count=2"));
    assert!(protocol_description.contains("invalid_action_json"));
    assert!(protocol_description.contains("plain_text_response"));
    assert!(!protocol_description.contains("ACTION payload is invalid"));
    assert!(!protocol_description.contains("ACTION: {"));
    assert!(!protocol_description.contains("plain text is not accepted"));
    assert!(!protocol_description.contains("hello"));
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

#[test]
fn runtime_report_promotes_runtime_event_ledger_metadata_to_artifact() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "runtime_event_ledger_json".to_string(),
        r#"[
            {"schema_version":1,"event_type":"approval_requested","thread_id":"thread-report","turn_id":"turn-report","call_id":"call-approval-secret-value","created_at":"2026-05-12T00:00:00Z","risk_decision":{"decision":"prompt","reason":"external send requires approval","policy_ref":"policy://local/high-risk"},"evidence_ref":"approval://secret-preview-value"},
            {"schema_version":1,"event_type":"elicitation_requested","thread_id":"thread-report","turn_id":"turn-report","call_id":"call-elicit-secret-value","created_at":"2026-05-12T00:00:01Z","risk_decision":{"decision":"deny_secret_elicitation","reason":"operator input requested","policy_ref":"policy://local/secret-denied"},"evidence_ref":"elicitation://secret-preview-value"},
            {"schema_version":1,"event_type":"tool_started","thread_id":"thread-report","turn_id":"turn-report","call_id":"call-tool","created_at":"2026-05-12T00:00:02Z","risk_decision":null,"evidence_ref":null},
            {"schema_version":1,"event_type":"tool_finished","thread_id":"thread-report","turn_id":"turn-report","call_id":"call-tool","created_at":"2026-05-12T00:00:03Z","risk_decision":null,"evidence_ref":null}
        ]"#
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
        "report-runtime-events",
        "task-runtime-events",
        "agent-runtime-events",
        None,
    );

    let runtime_events_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.runtime_event_ledger_json")
        .expect("runtime event ledger artifact should exist");
    assert_eq!(runtime_events_artifact.kind, ArtifactKind::Log);
    let description = runtime_events_artifact
        .description
        .as_deref()
        .expect("description");
    assert!(description.contains("count=4"));
    assert!(description.contains("tool_started=1"));
    assert!(description.contains("tool_finished=1"));
    assert!(description.contains("approval_requested=1"));
    assert!(description.contains("approval_resolved=0"));
    assert!(description.contains("elicitation_requested=1"));
    assert!(!description.contains("secret-preview-value"));
    assert!(!description.contains("call-approval-secret-value"));

    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability.get("runtime_event_count"),
        Some(&"4".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_tool_started_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_tool_finished_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_approval_requested_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_approval_resolved_count"),
        Some(&"0".to_string())
    );
    assert_eq!(
        observability.get("runtime_event_elicitation_requested_count"),
        Some(&"1".to_string())
    );

    let observability_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.observability")
        .expect("observability artifact should exist");
    let observability_description = observability_artifact
        .description
        .as_deref()
        .expect("description");
    assert!(observability_description.contains("tool_started=1"));
    assert!(observability_description.contains("tool_finished=1"));
    assert!(observability_description.contains("approval_requested=1"));
    assert!(observability_description.contains("elicitation_requested=1"));
}

#[test]
fn runtime_report_promotes_context_pack_trace_and_compaction_events() {
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
                extra: {
                    let mut extra = BTreeMap::new();
                    extra.insert(
                        "context_compaction_summary_json".to_string(),
                        serde_json::json!({
                            "event_count": 3,
                            "started_count": 1,
                            "completed_count": 1,
                            "dropped_count": 1,
                            "dropped_segment_ids": ["mem-1"],
                            "drop_reason_counts": {"duplicate_content": 1},
                            "trace_steps": ["normalize_tokens", "dedupe"]
                        })
                        .to_string(),
                    );
                    extra
                },
            },
        },
        recall_summary: "summary".to_string(),
        recall_hit_count: 0,
        context_engine_kind: "deterministic_budget".to_string(),
        packed_context_preview: "[packed-context]\npack_trace=normalize_tokens:5->5(-0),dedupe:5->4(-1)\ncompaction_events=context_compaction_started,context_segment_dropped:mem-1:duplicate_content:@dedupe,context_compaction_completed:packed:@merge_under_budget".to_string(),
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
        observability.get("context_pack_trace"),
        Some(&"normalize_tokens:5->5(-0),dedupe:5->4(-1)".to_string())
    );
    assert_eq!(
        observability.get("context_compaction_events"),
        Some(&"context_compaction_started,context_segment_dropped:mem-1:duplicate_content:@dedupe,context_compaction_completed:packed:@merge_under_budget".to_string())
    );

    let report = build_runtime_report(
        &result,
        "report-context-compaction",
        "task-context-compaction",
        "agent-context-compaction",
        None,
    );
    assert!(report
        .artifacts
        .iter()
        .any(|artifact| artifact.locator == "runtime_meta.context_pack_trace"));
    assert!(report
        .artifacts
        .iter()
        .any(|artifact| artifact.locator == "runtime_meta.context_compaction_events"));
    assert!(report
        .artifacts
        .iter()
        .any(|artifact| artifact.locator == "runtime_meta.context_compaction_summary_json"));
}

#[test]
fn runtime_report_promotes_context_compaction_summary_without_segment_payloads() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "context_compaction_summary_json".to_string(),
        json!({
            "event_count": 3,
            "started_count": 1,
            "completed_count": 1,
            "dropped_count": 1,
            "dropped_segment_ids": ["mem-1"],
            "drop_reason_counts": {"duplicate_content": 1},
            "trace_steps": ["normalize_tokens", "dedupe"]
        })
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

    let observability = runtime_observability_meta(&result);
    assert_eq!(
        observability
            .get("context_compaction_summary_json")
            .is_some(),
        true
    );

    let report = build_runtime_report(
        &result,
        "report-context-compaction-summary",
        "task-context-compaction-summary",
        "agent-context-compaction-summary",
        None,
    );
    let artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.context_compaction_summary_json")
        .expect("context compaction summary artifact should exist");
    assert_eq!(artifact.kind, ArtifactKind::Log);
    assert!(artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("events=3"));
    assert!(artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("trace_steps=normalize_tokens,dedupe"));
}

#[test]
fn runtime_report_promotes_goal_and_subagent_handoff_query_summaries() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "goal_handoff_query_summary_json".to_string(),
        json!({
            "parent_context_handoff_count": 1,
            "parent_context_handoff_refs": ["report://agent-1/report-1"],
            "report_admission_ref_count": 1,
            "report_admission_reason_codes": {"report_validated": 1},
            "report_admission_refs": [{
                "admission_id": "goal-report-admission://agent-1/report-1",
                "report_id": "report-1",
                "task_id": "task-1",
                "agent_id": "agent-1",
                "admission_status": "Accepted",
                "reason_code": "report_validated",
                "evidence_ref": "report://agent-1/report-1"
            }]
        })
        .to_string(),
    );
    extra.insert(
        "subagent_children_summary_json".to_string(),
        json!({
            "parent_thread_id": "root-thread",
            "child_count": 2,
            "open_child_count": 1,
            "reported_child_count": 1,
            "closed_child_count": 1,
            "accepted_report_count": 1,
            "rejected_report_count": 0,
            "missing_report_count": 1,
            "child_thread_ids": ["child-thread-1", "child-thread-2"],
            "report_admission_refs": [{
                "admission_id": "admission-1",
                "report_id": "report-1",
                "status": "Accepted",
                "reason_code": "report_validated",
                "evidence_ref": "queue://reports/report-1"
            }],
            "report_reason_codes": {"report_validated": 1}
        })
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

    let observability = runtime_observability_meta(&result);
    assert!(observability.contains_key("goal_handoff_query_summary_json"));
    assert!(observability.contains_key("subagent_children_summary_json"));
    assert_eq!(
        observability.get("goal_handoff_parent_context_handoff_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("goal_handoff_report_admission_ref_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("subagent_children_child_count"),
        Some(&"2".to_string())
    );
    assert_eq!(
        observability.get("subagent_children_accepted_report_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("subagent_children_report_admission_ref_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("subagent_children_missing_report_count"),
        Some(&"1".to_string())
    );
    assert_eq!(
        observability.get("goal_handoff_report_admission_reason_codes"),
        Some(&"report_validated=1".to_string())
    );
    assert_eq!(
        observability.get("goal_handoff_report_admission_refs"),
        Some(&"goal-report-admission://agent-1/report-1".to_string())
    );
    assert_eq!(
        observability.get("subagent_children_report_reason_codes"),
        Some(&"report_validated=1".to_string())
    );
    assert_eq!(
        observability.get("subagent_children_report_admission_refs"),
        Some(&"admission-1".to_string())
    );

    let report = build_runtime_report(
        &result,
        "report-handoff-query-summary",
        "task-handoff-query-summary",
        "agent-handoff-query-summary",
        None,
    );
    let goal_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.goal_handoff_query_summary_json")
        .expect("goal handoff query summary artifact should exist");
    assert_eq!(goal_artifact.kind, ArtifactKind::Log);
    assert!(goal_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("report_admission_refs=1"));
    assert!(goal_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("reason_codes=report_validated=1"));
    let observability_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.observability")
        .expect("observability artifact should exist");
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("goal_handoffs=1"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("goal_admissions=1"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("goal_admission_ref_locators=goal-report-admission://agent-1/report-1"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("subagent_children=2"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("subagent_accepted_reports=1"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("subagent_admission_refs=1"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("subagent_admission_ref_locators=admission-1"));
    assert!(observability_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("subagent_missing_reports=1"));

    let subagent_artifact = report
        .artifacts
        .iter()
        .find(|artifact| artifact.locator == "runtime_meta.subagent_children_summary_json")
        .expect("subagent children summary artifact should exist");
    assert_eq!(subagent_artifact.kind, ArtifactKind::Log);
    assert!(subagent_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("children=2"));
    assert!(subagent_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("report_admission_refs=1"));
    assert!(subagent_artifact
        .description
        .as_deref()
        .expect("description should exist")
        .contains("missing_reports=1"));

    let encoded = serde_json::to_string(&report.artifacts).expect("artifacts should serialize");
    assert!(!encoded.contains("secret report payload"));
    assert!(!encoded.contains("stdout secret value"));
}
