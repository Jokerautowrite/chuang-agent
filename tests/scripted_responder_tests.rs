use std::collections::BTreeMap;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::{InMemoryMemoryStore, MemoryRecord, MemoryStore};
use chuang_agent::responder::{Responder, ResponderMeta, ResponderRequest, ScriptedResponder};

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
fn scripted_responder_returns_structured_output() {
    let responder = ScriptedResponder::new("scripted-model", "这是脚本回复");
    let output = responder.generate(&ResponderRequest {
        prompt: "prompt-body".to_string(),
        user_input: "用户输入".to_string(),
        recall_hit_count: 2,
    });

    assert_eq!(output.model_name, "scripted-model");
    assert_eq!(output.body, "这是脚本回复");
    assert!(output.trace.contains("recall_hits=2"));
    assert_eq!(output.meta.provider.as_deref(), Some("scripted-responder"));
    assert_eq!(output.meta.recall_hit_count, Some(2));
    assert!(output.meta.extra.is_empty());
}

#[test]
fn agent_runtime_uses_scripted_responder_output() {
    let mut store = InMemoryMemoryStore::new();
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
        ScriptedResponder::new("scripted-model", "进入 scripted runtime 回复"),
    );
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "创项目继续推进".to_string(),
            recall_limit: 3,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert_eq!(result.response.model_name, "scripted-model");
    assert_eq!(result.response.body, "进入 scripted runtime 回复");
    assert!(result.response.trace.contains("recall_hits="));
    assert_eq!(
        result.response.meta.provider.as_deref(),
        Some("scripted-responder")
    );
}

#[test]
fn scripted_responder_keeps_runtime_trace_intact() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "创项目先跑通，最小闭环先跑通。",
            &[("kind", "goal")],
            "2026-04-30T21:10:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::with_responder(
        store,
        ScriptedResponder::new("scripted-model-v2", "结构化 trace 仍保留"),
    );
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "创项目先跑通".to_string(),
            recall_limit: 2,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert!(result.prompt.contains("[chuang-agent-runtime]"));
    assert!(result.recall_summary.contains("最小闭环先跑通"));
    assert_eq!(result.recall_hit_count, 1);
    assert_eq!(result.response.body, "结构化 trace 仍保留");
    assert!(result
        .response
        .trace
        .contains("user_input=《创项目先跑通》"));
    assert_eq!(
        result.response.meta,
        ResponderMeta {
            provider: Some("scripted-responder".to_string()),
            recall_hit_count: Some(1),
            finish_reason: Some("scripted".to_string()),
            extra: BTreeMap::new(),
        }
    );
}
