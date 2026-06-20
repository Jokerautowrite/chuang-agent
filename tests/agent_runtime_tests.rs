use std::collections::BTreeMap;

use chuang_agent::agent_runtime::{
    debug_pack_for_test, AgentRuntime, AgentRuntimeError, RuntimeRequest,
};
use chuang_agent::context_engine::{
    ContextBudget, ContextEngineKind, ContextSegment, SegmentSource,
};
use chuang_agent::memory_store::{InMemoryMemoryStore, MemoryRecord, MemoryStore};
use chuang_agent::responder::{FakeResponder, ScriptedResponder};

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

fn scripted_runtime(
    extra: BTreeMap<String, String>,
) -> AgentRuntime<InMemoryMemoryStore, ScriptedResponder> {
    AgentRuntime::with_responder(
        InMemoryMemoryStore::new(),
        ScriptedResponder::new("scripted", "ACTION: malformed").with_extra_meta(extra),
    )
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

    let runtime = runtime(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "现在先把创项目跑起来".to_string(),
            recall_limit: 3,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result.prompt.contains("[packed-context]"));
    assert!(result.prompt.contains("user_input=现在先把创项目跑起来"));
    assert!(result.packed_context_preview.contains("system-core"));
    assert!(result.packed_context_preview.contains("working-user-input"));
    assert_eq!(result.context_engine_kind, "deterministic_budget");
    assert_eq!(result.response.model_name, "stub-responder");
    assert!(result.response.body.contains("现在先把创项目跑起来"));
}

#[test]
fn agent_runtime_can_use_summary_compression_context_engine() {
    let runtime = AgentRuntime::with_responder_and_context_engine(
        InMemoryMemoryStore::new(),
        FakeResponder::new("stub-responder"),
        ContextEngineKind::SummaryCompression,
    );

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "切换上下文引擎".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert_eq!(result.context_engine_kind, "summary_compression");
    assert!(result.prompt.contains("切换上下文引擎"));
}

#[test]
fn agent_runtime_handles_empty_recall_hits() {
    let store = InMemoryMemoryStore::new();
    let runtime = runtime(store);

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "没有命中的问题".to_string(),
            recall_limit: 2,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert_eq!(result.recall_hit_count, 0);
    assert_eq!(result.response.model_name, "stub-responder");
    assert_eq!(result.response.meta.recall_hit_count, Some(0));
}

#[test]
fn agent_runtime_packs_extra_identity_context_segments() {
    let store = InMemoryMemoryStore::new();
    let runtime = runtime(store);

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "读取身份记忆".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: vec![ContextSegment {
                id: "identity-user".to_string(),
                source: SegmentSource::Identity,
                content: "老爸偏好简洁中文状态汇报".to_string(),
                tokens: Some(14),
                priority: 245,
                created_at: chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
                    .expect("timestamp parses")
                    .with_timezone(&chrono::Utc),
                last_accessed: chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
                    .expect("timestamp parses")
                    .with_timezone(&chrono::Utc),
                metadata: Default::default(),
            }],
        })
        .expect("runtime should succeed");

    assert!(result.packed_context_preview.contains("identity-user"));
    assert!(result.prompt.contains("老爸偏好简洁中文状态汇报"));
}

#[test]
fn agent_runtime_keeps_capability_primer_under_budget_pressure() {
    let store = InMemoryMemoryStore::new();
    let runtime = runtime(store);

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "查看默认能力".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(ContextBudget {
                max_tokens: 390,
                reserve_system_tokens: 32,
                min_working_tokens: 1,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
            extra_context_segments: vec![ContextSegment {
                id: "memory-pressure".to_string(),
                source: SegmentSource::Memory,
                content: "这是一段很长很长的记忆片段，用来制造预算压力，让上下文打包时必须优先保留系统能力 primer，而把普通记忆挤掉。"
                    .repeat(2),
                tokens: Some(120),
                priority: 100,
                created_at: chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
                    .expect("timestamp parses")
                    .with_timezone(&chrono::Utc),
                last_accessed: chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
                    .expect("timestamp parses")
                    .with_timezone(&chrono::Utc),
                metadata: Default::default(),
            }],
        })
        .expect("runtime should succeed");

    assert!(result
        .packed_context_preview
        .contains("system-capabilities"));
    assert!(!result
        .dropped_segment_ids
        .iter()
        .any(|id| id == "system-capabilities"));
    assert!(!result
        .packed_context_preview
        .contains("drop_reasons=system-capabilities:budget_limit"));
    assert!(result.packed_context_preview.contains("memory-pressure"));
    assert!(result
        .dropped_segment_ids
        .iter()
        .any(|id| id == "memory-pressure"));
}

#[test]
fn agent_runtime_surfaces_readonly_desktop_browser_and_knowledge_guidance_in_prompt() {
    let store = InMemoryMemoryStore::new();
    let runtime = runtime(store);

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "查看能力和工具说明".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result.prompt.contains("[chuang-agent-runtime]"));
    assert!(result.prompt.contains("[packed-context]"));
    assert!(result.prompt.contains("system-capabilities"));
    assert!(result.prompt.contains("默认注入能力 primer"));
    assert!(result
        .prompt
        .contains("file_read/file_write/code_execute/list_dir=治理内读写/执行"));
    assert!(result.prompt.contains("goal/subagent 派活"));
    assert!(result
        .prompt
        .contains("subagent=dispatch/list/run-once/run-loop/report/collect"));
    assert!(result.prompt.contains("locate/screenshot=只读观察"));
    assert!(result.prompt.contains("open_app/mouse/keyboard=桌面交互"));
    assert!(result.prompt.contains("memory/session=回溯"));
    assert!(result.prompt.contains("不伪造完成"));
    assert!(result.prompt.contains("授权业务/授权网安任务"));
    assert!(result.prompt.contains("Feishu/终端/HTTP/桌面只是入口"));
    assert!(result
        .packed_context_preview
        .contains("system-capabilities"));
    assert!(result
        .packed_context_preview
        .contains("locate/screenshot=只读观察"));
    assert!(result
        .packed_context_preview
        .contains("subagent=dispatch/list/run-once/run-loop/report/collect"));
}

#[test]
fn agent_runtime_rejects_zero_recall_limit() {
    let store = InMemoryMemoryStore::new();
    let runtime = runtime(store);

    let error = runtime
        .run(&RuntimeRequest {
            user_input: "测试无效请求".to_string(),
            recall_limit: 0,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
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
                content: "很长的长期记忆片段，用来制造 budget 压力，而且这里故意把内容写得更长更长，让 token 估算明显超过剩余空间。"
                    .repeat(4),
                tokens: Some(240),
                priority: 100,
                created_at: chrono::DateTime::parse_from_rfc3339("2026-04-30T18:00:00Z").unwrap().with_timezone(&chrono::Utc),
                last_accessed: chrono::DateTime::parse_from_rfc3339("2026-04-30T18:00:00Z").unwrap().with_timezone(&chrono::Utc),
                metadata: std::collections::HashMap::new(),
            },
        ],
        ContextBudget {
            max_tokens: 390,
            reserve_system_tokens: 32,
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
            &"很长的长期记忆片段，用来制造 budget 压力，而且这里故意把内容写得更长更长，让 token 估算明显超过剩余空间。"
                .repeat(4),
            &[("kind", "goal")],
            "2026-04-30T18:00:00Z",
        ))
        .expect("put should succeed");

    let runtime = runtime(store);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "把创项目主线继续推进".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(ContextBudget {
                max_tokens: 390,
                reserve_system_tokens: 32,
                min_working_tokens: 1,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result
        .packed_context_preview
        .starts_with("[packed-context]"));
    assert!(result.packed_context_preview.contains("system-core"));
    assert!(result.packed_context_preview.contains("working-user-input"));
    assert!(result.packed_context_preview.contains("dropped="));
    assert!(!result
        .dropped_segment_ids
        .iter()
        .any(|id| id == "system-capabilities"));
    assert!(result
        .packed_context_preview
        .contains("budget_exceeded=false"));
    assert!(result.packed_token_count > 0);
}

#[test]
fn agent_runtime_exposes_budget_exceeded_reason_in_preview() {
    let runtime = runtime(InMemoryMemoryStore::new());

    let result = runtime
        .run(&RuntimeRequest {
            user_input: "这是一段很长很长的用户输入，用来制造 working segment 无法装入预算的情况"
                .to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: Some(ContextBudget {
                max_tokens: 350,
                reserve_system_tokens: 32,
                min_working_tokens: 5,
                max_tool_results: 5,
                max_memory_segments: 20,
            }),
            extra_context_segments: Vec::new(),
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
            content: "需要被挤掉的记忆段，给 working segment 腾预算。".repeat(4),
            tokens: Some(140),
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
            max_tokens: 380,
            reserve_system_tokens: 32,
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
    assert!(reservation
        .dropped_segment_ids
        .iter()
        .any(|id| id == "mem-1"));
    assert_eq!(reservation.reason.as_str(), "minimum_working_tokens");
    assert!(packed
        .drop_reasons
        .iter()
        .any(|reason| reason.segment_id == "mem-1" && reason.reason.as_str() == "budget_limit"));
    assert!(!packed.budget_exceeded);
}

#[test]
fn agent_runtime_derives_correction_context_for_invalid_action_json() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_protocol_errors_json".to_string(),
        r#"[{"code":"invalid_action_json","message":"ACTION payload is invalid or unsupported: expected value at line 1 column 10"}]"#.to_string(),
    );
    let runtime = scripted_runtime(extra);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "修正协议".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result
        .response
        .meta
        .extra
        .get("tool_protocol_correction_context")
        .expect("correction context should exist")
        .contains("ACTION JSON"));
}

#[test]
fn agent_runtime_derives_correction_context_for_action_final_trailing() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_protocol_errors_json".to_string(),
        r#"[{"code":"invalid_action_json","message":"ACTION payload has trailing text; output only one ACTION or FINAL per response"}]"#.to_string(),
    );
    let runtime = scripted_runtime(extra);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "修正 trailing".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result
        .response
        .meta
        .extra
        .get("tool_protocol_correction_context")
        .expect("correction context should exist")
        .contains("trailing"));
}

#[test]
fn agent_runtime_derives_correction_context_for_wrong_tool_name() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_protocol_errors_json".to_string(),
        r#"[{"code":"invalid_action_json","message":"ACTION payload is invalid or unsupported: unknown variant `write_file`, expected one of `file_read`, `file_write`"}]"#.to_string(),
    );
    let runtime = scripted_runtime(extra);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "修正工具名".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result
        .response
        .meta
        .extra
        .get("tool_protocol_correction_context")
        .expect("correction context should exist")
        .contains("ACTION JSON"));
}

#[test]
fn agent_runtime_marks_missing_final_as_typed_failure_when_loop_exhausts() {
    let mut extra = BTreeMap::new();
    extra.insert(
        "tool_loop_status".to_string(),
        "implicit_final_plain_text".to_string(),
    );
    let runtime = scripted_runtime(extra);
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "缺少 FINAL".to_string(),
            recall_limit: 1,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert_eq!(
        result
            .response
            .meta
            .extra
            .get("tool_protocol_typed_failure_code")
            .map(String::as_str),
        Some("missing_final")
    );
    assert!(result
        .response
        .meta
        .extra
        .get("tool_protocol_typed_failure_message")
        .expect("typed failure message should exist")
        .contains("without valid FINAL"));
}

#[test]
fn agent_runtime_surfaces_context_pack_errors() {
    let store = InMemoryMemoryStore::new();
    let runtime = runtime(store);

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
            extra_context_segments: Vec::new(),
        })
        .expect_err("context pack should fail");

    assert!(matches!(error, AgentRuntimeError::ContextPack(_)));
}
