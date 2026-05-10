use std::collections::BTreeMap;

use crate::capability_primer::capability_primer_segment;
use crate::context_engine::{
    BudgetExceededReason, ContextBudget, ContextEngineKind, ContextPackError, ContextSegment,
    DropReason, PackedContext, SegmentSource,
};
use crate::memory_recall::{MemoryRecallError, MemoryRecallPipeline, RecallRequest};
use crate::memory_store::MemoryStore;
use crate::responder::{Responder, ResponderMeta, ResponderOutput, ResponderRequest};
use crate::runtime_config::default_context_budget as runtime_default_context_budget;
use crate::tool_loop_meta::{
    derive_tool_protocol_correction_context, derive_tool_protocol_typed_failure,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequest {
    pub user_input: String,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: Option<ContextBudget>,
    pub extra_context_segments: Vec<ContextSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResponse {
    pub model_name: String,
    pub body: String,
    pub trace: String,
    pub meta: ResponderMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResult {
    pub prompt: String,
    pub response: RuntimeResponse,
    pub recall_summary: String,
    pub recall_hit_count: usize,
    pub context_engine_kind: String,
    pub packed_context_preview: String,
    pub packed_token_count: u32,
    pub dropped_segment_ids: Vec<String>,
    pub context_debug: ContextDebugInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDebugInfo {
    pub drop_reasons: Vec<DropReason>,
    pub budget_exceeded: bool,
    pub budget_exceeded_reasons: Vec<BudgetExceededReason>,
    pub working_reservation: Option<crate::context_engine::WorkingReservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRuntimeError {
    Recall(MemoryRecallError),
    ContextPack(ContextPackError),
}

pub struct AgentRuntime<S, R> {
    recall: MemoryRecallPipeline<S>,
    responder: R,
    context_engine_kind: ContextEngineKind,
}

impl<S, R> AgentRuntime<S, R> {
    pub fn with_responder(store: S, responder: R) -> Self {
        Self::with_responder_and_context_engine(store, responder, ContextEngineKind::default())
    }

    pub fn with_responder_and_context_engine(
        store: S,
        responder: R,
        context_engine_kind: ContextEngineKind,
    ) -> Self {
        Self {
            recall: MemoryRecallPipeline::new(store),
            responder,
            context_engine_kind,
        }
    }

    pub fn memory_store_mut(&mut self) -> &mut S {
        self.recall.store_mut()
    }
}

impl<S: MemoryStore, R: Responder> AgentRuntime<S, R> {
    pub fn run(&self, request: &RuntimeRequest) -> Result<RuntimeResult, AgentRuntimeError> {
        let recall_result = self
            .recall
            .recall(&RecallRequest {
                query_text: request.user_input.clone(),
                metadata: request.metadata.clone(),
                limit: request.recall_limit,
            })
            .map_err(AgentRuntimeError::Recall)?;

        let packed_context = self
            .pack_context(request, &recall_result.segments)
            .map_err(AgentRuntimeError::ContextPack)?;

        let packed_context_preview = packed_context.render_prompt();
        let prompt = format!(
            "[chuang-agent-runtime]\nuser_input={}\n{}",
            request.user_input, packed_context_preview
        );

        let responder_output = self.responder.generate(&ResponderRequest {
            prompt: prompt.clone(),
            user_input: request.user_input.clone(),
            recall_hit_count: recall_result.hits.len(),
        });

        Ok(RuntimeResult {
            prompt,
            response: map_runtime_response(responder_output),
            recall_summary: recall_result.summary,
            recall_hit_count: recall_result.hits.len(),
            context_engine_kind: self.context_engine_kind.as_str().to_string(),
            packed_context_preview,
            packed_token_count: packed_context.total_tokens,
            dropped_segment_ids: packed_context.dropped_ids.clone(),
            context_debug: ContextDebugInfo {
                drop_reasons: packed_context.drop_reasons.clone(),
                budget_exceeded: packed_context.budget_exceeded,
                budget_exceeded_reasons: packed_context.budget_exceeded_reasons.clone(),
                working_reservation: packed_context.working_reservation.clone(),
            },
        })
    }

    fn pack_context(
        &self,
        request: &RuntimeRequest,
        recall_segments: &[ContextSegment],
    ) -> Result<PackedContext, ContextPackError> {
        let mut segments = vec![
            build_system_segment(),
            capability_primer_segment(),
            build_working_segment(&request.user_input),
        ];
        segments.extend(request.extra_context_segments.iter().cloned());
        segments.extend(recall_segments.iter().cloned());

        self.context_engine_kind.pack(
            request
                .context_budget
                .clone()
                .unwrap_or_else(default_context_budget),
            segments,
        )
    }
}

fn map_runtime_response(output: ResponderOutput) -> RuntimeResponse {
    let mut meta = output.meta;
    if !meta.extra.contains_key("tool_protocol_correction_context") {
        if let Some(correction) = derive_tool_protocol_correction_context(&meta.extra) {
            meta.extra
                .insert("tool_protocol_correction_context".to_string(), correction);
        }
    }
    if !meta.extra.contains_key("tool_protocol_typed_failure_code")
        || !meta
            .extra
            .contains_key("tool_protocol_typed_failure_message")
    {
        if let Some((code, message)) = derive_tool_protocol_typed_failure(&meta.extra) {
            meta.extra
                .insert("tool_protocol_typed_failure_code".to_string(), code);
            meta.extra
                .insert("tool_protocol_typed_failure_message".to_string(), message);
        }
    }

    RuntimeResponse {
        model_name: output.model_name,
        body: output.body,
        trace: output.trace,
        meta,
    }
}

pub fn debug_pack_for_test(
    user_input: &str,
    recall_segments: &[ContextSegment],
    context_budget: ContextBudget,
) -> Result<PackedContext, ContextPackError> {
    let mut segments = vec![
        build_system_segment(),
        capability_primer_segment(),
        build_working_segment(user_input),
    ];
    segments.extend(recall_segments.iter().cloned());
    ContextEngineKind::DeterministicBudget.pack(context_budget, segments)
}

fn build_system_segment() -> ContextSegment {
    let content = "你是创项目本地 Agent 内核，先闭环，再优化。".to_string();
    ContextSegment {
        id: "system-core".to_string(),
        source: SegmentSource::System,
        tokens: Some(content.chars().count().min(u32::MAX as usize) as u32),
        content,
        priority: 255,
        created_at: default_timestamp(),
        last_accessed: default_timestamp(),
        metadata: std::collections::HashMap::new(),
    }
}

fn build_working_segment(user_input: &str) -> ContextSegment {
    ContextSegment {
        id: "working-user-input".to_string(),
        source: SegmentSource::Working,
        content: format!("当前用户输入：{}", user_input),
        tokens: Some(user_input.chars().count().min(u32::MAX as usize) as u32),
        priority: 220,
        created_at: default_timestamp(),
        last_accessed: default_timestamp(),
        metadata: std::collections::HashMap::new(),
    }
}

fn default_context_budget() -> ContextBudget {
    runtime_default_context_budget()
}

fn default_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-04-30T00:00:00Z")
        .expect("static runtime timestamp should parse")
        .with_timezone(&chrono::Utc)
}
