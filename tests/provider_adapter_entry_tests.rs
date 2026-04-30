use std::collections::BTreeMap;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::{InMemoryMemoryStore, MemoryRecord, MemoryStore};
use chuang_agent::responder::{
    OpenAICompatibleProviderAdapter, ProviderAdapterResponder, ProviderAdapterResponse,
    ProviderIdentity, Responder, ResponderRequest,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeProviderAdapter {
    identity: ProviderIdentity,
    body: String,
}

impl FakeProviderAdapter {
    fn new(provider_id: &str, model_name: &str, body: &str) -> Self {
        Self {
            identity: ProviderIdentity {
                provider_id: provider_id.to_string(),
                model_name: model_name.to_string(),
            },
            body: body.to_string(),
        }
    }
}

impl ProviderAdapterResponder for FakeProviderAdapter {
    fn identity(&self) -> ProviderIdentity {
        self.identity.clone()
    }

    fn respond(&self, request: &ResponderRequest) -> ProviderAdapterResponse {
        ProviderAdapterResponse {
            body: self.body.clone(),
            trace: format!(
                "provider={} model={} user_input=《{}》 recall_hits={}",
                self.identity.provider_id,
                self.identity.model_name,
                request.user_input,
                request.recall_hit_count
            ),
            finish_reason: Some("adapter-backed".to_string()),
            extra_meta: BTreeMap::from([(
                "transport".to_string(),
                "openai-compatible".to_string(),
            )]),
        }
    }
}

#[test]
fn provider_adapter_responder_behaves_like_real_provider_entry() {
    let responder =
        FakeProviderAdapter::new("openai-compatible", "gpt-realish", "真实 provider 占位回复");

    let output = responder.generate(&ResponderRequest {
        prompt: "prompt-body".to_string(),
        user_input: "创项目继续".to_string(),
        recall_hit_count: 2,
    });

    assert_eq!(output.model_name, "gpt-realish");
    assert_eq!(output.body, "真实 provider 占位回复");
    assert_eq!(output.meta.provider.as_deref(), Some("openai-compatible"));
    assert_eq!(output.meta.finish_reason.as_deref(), Some("adapter-backed"));
}

#[test]
fn runtime_accepts_provider_adapter_responder_without_shape_change() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "创项目先跑起来，先闭环再优化。",
            &[("kind", "goal")],
            "2026-04-30T22:00:00Z",
        ))
        .expect("put should succeed");

    let runtime = AgentRuntime::with_responder(
        store,
        FakeProviderAdapter::new(
            "openai-compatible",
            "gpt-realish",
            "真实 provider 已挂上入口",
        ),
    );
    let result = runtime
        .run(&RuntimeRequest {
            user_input: "创项目继续".to_string(),
            recall_limit: 3,
            metadata: BTreeMap::new(),
            context_budget: None,
            extra_context_segments: Vec::new(),
        })
        .expect("runtime should succeed");

    assert_eq!(result.response.model_name, "gpt-realish");
    assert_eq!(result.response.body, "真实 provider 已挂上入口");
    assert_eq!(
        result.response.meta.provider.as_deref(),
        Some("openai-compatible")
    );
    assert_eq!(
        result.response.meta.extra.get("transport"),
        Some(&"openai-compatible".to_string())
    );
    assert!(result.response.trace.contains("provider=openai-compatible"));
}

#[test]
fn openai_compatible_adapter_exposes_minimal_config_shape() {
    let responder = OpenAICompatibleProviderAdapter::new(
        "custom-openai",
        "https://api.example.com/v1",
        "test-key",
        "gpt-4.1-mini",
    );

    let identity = responder.identity();
    assert_eq!(identity.provider_id, "custom-openai");
    assert_eq!(identity.model_name, "gpt-4.1-mini");

    let output = responder.generate(&ResponderRequest {
        prompt: "system+context prompt".to_string(),
        user_input: "继续推进创项目".to_string(),
        recall_hit_count: 1,
    });

    assert_eq!(output.model_name, "gpt-4.1-mini");
    assert_eq!(output.meta.provider.as_deref(), Some("custom-openai"));
    assert_eq!(
        output.meta.extra.get("request_url"),
        Some(&"https://api.example.com/v1/chat/completions".to_string())
    );
    assert_eq!(
        output.meta.extra.get("transport"),
        Some(&"openai-compatible".to_string())
    );
    assert_eq!(output.meta.finish_reason.as_deref(), Some("stop"));
}
