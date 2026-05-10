use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::chuang_kernel::{
    ChuangKernel, ChuangKernelConfig, ChuangKernelTurn, IdentityBootstrapSnapshot,
    DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::context_engine::{ContextSegment, SegmentSource};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::governance::{risk_decision_label, Governance};
use chuang_agent::hermes_memory::{DualFileMemoryStore, FileDualFileMemoryStore, HotMemoryEntry};
use chuang_agent::memory_store::MemoryStore;
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::runtime_config::{RuntimeConfig, SubagentConfig};
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::subagent_report::governance_metadata;
use chuang_agent::subagent_spawner::{
    ContextIsolation, SpawnRequest, SubagentSpawner, SubagentToolPolicy,
};
use chuang_agent::tool_runtime::{
    parse_tool_model_output, ExecutionSlot, MemoryToolContext, ToolCall, ToolExecutionConfig,
    ToolExecutionRecord, ToolLoopEvent, ToolLoopReport, ToolModelOutput, ToolProtocolError,
    ToolSurfaceStatus,
};
use chuang_agent::{common::AgentId, common::TaskId};

use crate::cli_memory::{preview_local_knowledge_context, MemoryKnowledgePreviewContextOutput};
use crate::cli_types::{CliOptions, ConversationHistoryItem, RememberedRecords, RunCliRequest};

pub(crate) fn run_with_options(
    request: &RunCliRequest,
) -> Result<
    (
        chuang_agent::agent_runtime::RuntimeResult,
        RememberedRecords,
    ),
    String,
> {
    if request.remember_session && request.session_id.is_none() {
        return Err("remember_session_requires_session_id: pass --session-id".to_string());
    }

    request
        .options
        .runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    let mut runtime = request.options.runtime.clone();
    if let Some(session_id) = &request.session_id {
        runtime
            .metadata
            .insert("session_id".to_string(), session_id.clone());
        runtime
            .metadata
            .insert("memory_scope".to_string(), "session".to_string());
    }

    let tool_workspace_root = request.workspace_root.clone().map(Ok).unwrap_or_else(|| {
        std::env::current_dir().map_err(|e| format!("workspace_root_discovery_failed: {e}"))
    })?;
    runtime.metadata.insert(
        "workspace_root".to_string(),
        tool_workspace_root.display().to_string(),
    );
    let mut slots = build_runtime_slots(&runtime)
        .map_err(|err| format!("config_invalid: {}: {}", err.field, err.message))?;
    let mut store = SqliteMemoryStore::open(&runtime.db_path)
        .map_err(|e| format!("failed_to_open_db: {e:?}"))?;
    seed_default_memory_if_empty(&mut store)?;
    let mut kernel =
        ChuangKernel::with_responder(kernel_config_from_runtime(&runtime)?, store, slots.provider);
    let mut runtime_context = goal_context_segments(request.goal_spec.as_ref())?;
    runtime_context.extend(conversation_history_context_segments(
        &request.conversation_history,
    ));
    let knowledge_preview = knowledge_context_preview(request)?;
    if let Some(preview) = &knowledge_preview {
        runtime_context.extend(knowledge_preview_context_segments(preview));
    }

    run_governed_turn_with_tools(
        &mut kernel,
        &mut slots.governance,
        &tool_workspace_root,
        runtime.tool_loop.max_rounds,
        ToolExecutionConfig {
            shell_timeout_ms: runtime.tool_loop.shell_timeout_ms,
            shell_risk_rules: runtime.tool_loop.shell_risk_rules.clone(),
            memory: Some(MemoryToolContext {
                db_path: runtime.db_path.clone(),
                session_id: request.session_id.clone(),
                default_limit: runtime.recall_limit.max(1).min(5),
                max_limit: runtime.recall_limit.max(1).max(10),
            }),
            actuator: Some(runtime.actuator.clone()),
        },
        request.user_input.clone(),
        runtime_context,
    )
    .map_err(|e| format!("runtime_failed: {e:?}"))
    .and_then(|mut turn| {
        insert_knowledge_context_metadata(&mut turn, knowledge_preview.as_ref())?;
        insert_conversation_history_metadata(&mut turn, &request.conversation_history);
        remember_turn_if_requested(&request.options, &mut kernel, turn, request)
    })
}

pub(crate) fn kernel_config_from_runtime(
    runtime: &RuntimeConfig,
) -> Result<ChuangKernelConfig, String> {
    let dual_file_config = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let identity_snapshot = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;
    let identity_bootstrap_snapshot = load_identity_bootstrap_snapshot(runtime)?;

    Ok(ChuangKernelConfig {
        agent_id: "chuang-cli".to_string(),
        parent_agent_id: None,
        recall_limit: runtime.recall_limit,
        metadata: runtime.metadata.clone(),
        context_budget: Some(runtime.context_budget.clone()),
        context_engine_kind: Some(runtime.context_engine.to_context_engine_kind()),
        memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        identity_snapshot: Some(identity_snapshot),
        identity_bootstrap_snapshot: Some(identity_bootstrap_snapshot),
    })
}

pub(crate) fn default_db_path() -> PathBuf {
    PathBuf::from("./data/chuang-agent.db")
}

fn load_identity_bootstrap_snapshot(
    runtime: &RuntimeConfig,
) -> Result<IdentityBootstrapSnapshot, String> {
    Ok(IdentityBootstrapSnapshot {
        soul: read_optional_identity_file(&runtime.identity_bootstrap.soul_path)?,
        soul_exists: runtime.identity_bootstrap.soul_path.exists(),
        story: read_optional_identity_file(&runtime.identity_bootstrap.story_path)?,
        story_exists: runtime.identity_bootstrap.story_path.exists(),
        first_wake: read_optional_identity_file(&runtime.identity_bootstrap.first_wake_path)?,
        first_wake_exists: runtime.identity_bootstrap.first_wake_path.exists(),
        agents_registry: read_optional_identity_file(
            &runtime.identity_bootstrap.agents_registry_path,
        )?,
        agents_registry_exists: runtime.identity_bootstrap.agents_registry_path.exists(),
    })
}

fn conversation_history_context_segments(
    history: &[ConversationHistoryItem],
) -> Vec<ContextSegment> {
    let content = render_conversation_history(history);
    if content.is_empty() {
        return Vec::new();
    }
    vec![ContextSegment {
        id: "recent-conversation-history".to_string(),
        source: chuang_agent::context_engine::SegmentSource::Working,
        tokens: Some(content.chars().count().min(u32::MAX as usize) as u32),
        content,
        priority: 241,
        created_at: default_cli_context_timestamp(),
        last_accessed: default_cli_context_timestamp(),
        metadata: std::collections::HashMap::from([(
            "kind".to_string(),
            "recent_conversation_history".to_string(),
        )]),
    }]
}

fn render_conversation_history(history: &[ConversationHistoryItem]) -> String {
    let rendered = history
        .iter()
        .filter_map(|item| {
            let role = item.role.trim();
            let text = item.text.trim();
            if text.is_empty() {
                return None;
            }
            let role = match role {
                "user" | "assistant" | "system" | "tool" => role,
                _ => "note",
            };
            Some(format!("{role}: {}", truncate_history_text(text, 1200)))
        })
        .collect::<Vec<_>>();
    if rendered.is_empty() {
        return String::new();
    }
    format!(
        "[recent-conversation-history]\n说明：这是同一 thread/session 的最近原文对话，优先用于理解“继续/刚才/他说的”等承接短句。\n{}",
        rendered.join("\n")
    )
}

fn truncate_history_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn insert_conversation_history_metadata(
    turn: &mut ChuangKernelTurn,
    history: &[ConversationHistoryItem],
) {
    let non_empty_item_count = history
        .iter()
        .filter(|item| !item.text.trim().is_empty())
        .count();
    let user_item_count = history
        .iter()
        .filter(|item| item.role.trim() == "user" && !item.text.trim().is_empty())
        .count();
    let dropped = turn
        .result
        .dropped_segment_ids
        .iter()
        .any(|id| id == "recent-conversation-history");
    let injected = non_empty_item_count > 0 && !dropped;

    let extra = &mut turn.result.response.meta.extra;
    extra.insert(
        "recent_conversation_history_item_count".to_string(),
        non_empty_item_count.to_string(),
    );
    extra.insert(
        "recent_conversation_history_turn_count".to_string(),
        user_item_count.to_string(),
    );
    extra.insert(
        "recent_conversation_history_injected".to_string(),
        injected.to_string(),
    );
    extra.insert(
        "recent_conversation_history_dropped".to_string(),
        dropped.to_string(),
    );
    extra.insert(
        "recent_conversation_history_model_facing".to_string(),
        injected.to_string(),
    );
}

fn default_cli_context_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-04-30T00:00:00Z")
        .expect("static cli context timestamp should parse")
        .with_timezone(&chrono::Utc)
}

fn run_governed_turn_with_tools<S, R, G>(
    kernel: &mut ChuangKernel<S, R>,
    governance: &mut G,
    workspace_root: &Path,
    max_tool_rounds: usize,
    tool_config: ToolExecutionConfig,
    original_input: String,
    extra_context_segments: Vec<ContextSegment>,
) -> Result<ChuangKernelTurn, String>
where
    S: MemoryStore,
    R: chuang_agent::responder::Responder,
    G: Governance,
{
    let mut tool_calls: Vec<ToolExecutionRecord> = Vec::new();
    let mut protocol_errors: Vec<ToolProtocolError> = Vec::new();
    let mut tool_events: Vec<ToolLoopEvent> = Vec::new();
    let mut transcript: Vec<String> = Vec::new();
    let mut turn_context = extra_context_segments;
    turn_context.push(tool_instruction_segment(workspace_root));
    let execution_slot = ExecutionSlot::generic_agent_mvp(tool_config);
    let mut current_input = original_input.clone();
    let mut last_turn: Option<ChuangKernelTurn> = None;
    let mut last_plain_text_answer: Option<String> = None;
    if should_auto_observe_desktop(&original_input) {
        let call = ToolCall::Locate {
            target: Some("screen".to_string()),
        };
        let outcome = execution_slot.execute_or_reject_with_governance(
            workspace_root,
            governance,
            &call,
            "cli",
            "pre-model:tool:1",
        )?;
        let record = outcome.record;
        transcript.push(format!(
            "call={} output={}",
            tool_call_name(&record.call),
            record.output.as_deref().unwrap_or("")
        ));
        tool_events.push(ToolLoopEvent {
            round: 1,
            kind: "tool_call".to_string(),
            tool_name: Some(tool_call_name(&record.call).to_string()),
            atomic_tool_name: record.atomic_tool_name.clone(),
            decision: Some(risk_decision_label(&outcome.decision)),
            ok: Some(record.ok),
            failure_class: record.failure_class.clone(),
            duration_ms: Some(record.duration_ms),
            retryable: Some(record.retryable),
            summary: Some(record.summary.clone()),
            protocol_error_code: None,
            protocol_error_message: None,
        });
        tool_calls.push(record);
        current_input = format!(
            "{}\nreq:{}\ntool:{}\nFINAL:<最终答复>",
            read_only_capability_banner(),
            original_input,
            transcript.join("\n")
        );
    }

    for round_index in 0..max_tool_rounds {
        let mut turn = kernel
            .run_governed_turn_with_extra_context(
                current_input.clone(),
                governance,
                turn_context.clone(),
            )
            .map_err(|e| format!("{e:?}"))?;
        let body = turn.result.response.body.trim().to_string();

        match parse_tool_model_output(&body) {
            ToolModelOutput::FinalAnswer(final_answer) => {
                turn.result.response.body = final_answer;
                turn.user_input = original_input.clone();
                insert_tool_surface_metadata(&mut turn, workspace_root)?;
                if !tool_calls.is_empty() || !protocol_errors.is_empty() {
                    insert_tool_metadata(
                        &mut turn,
                        workspace_root,
                        round_index + 1,
                        &tool_calls,
                        &protocol_errors,
                        &tool_events,
                        &transcript,
                    )?;
                }
                return Ok(turn);
            }
            ToolModelOutput::ToolCall(call) => {
                let task_id = format!("{}:tool:{}", turn.turn_id, tool_calls.len() + 1);
                let outcome = execution_slot.execute_or_reject_with_governance(
                    workspace_root,
                    governance,
                    &call,
                    "cli",
                    task_id,
                )?;
                let record = outcome.record;
                transcript.push(format!(
                    "call={} decision={} ok={} summary={}",
                    tool_call_name(&record.call),
                    risk_decision_label(&outcome.decision),
                    record.ok,
                    record.summary
                ));
                tool_events.push(ToolLoopEvent {
                    round: round_index + 1,
                    kind: "tool_call".to_string(),
                    tool_name: Some(tool_call_name(&record.call).to_string()),
                    atomic_tool_name: record.atomic_tool_name.clone(),
                    decision: Some(risk_decision_label(&outcome.decision)),
                    ok: Some(record.ok),
                    failure_class: record.failure_class.clone(),
                    duration_ms: Some(record.duration_ms),
                    retryable: Some(record.retryable),
                    summary: Some(record.summary.clone()),
                    protocol_error_code: None,
                    protocol_error_message: None,
                });
                tool_calls.push(record);
                if tool_calls
                    .last()
                    .and_then(|record| record.failure_class.as_deref())
                    == Some("human_input_required")
                {
                    turn.user_input = original_input.clone();
                    turn.result.response.body = tool_calls
                        .last()
                        .and_then(|record| record.output.clone())
                        .unwrap_or_else(|| "human_input_required".to_string());
                    insert_tool_surface_metadata(&mut turn, workspace_root)?;
                    insert_tool_metadata_with_status(
                        &mut turn,
                        workspace_root,
                        round_index + 1,
                        &tool_calls,
                        &protocol_errors,
                        &tool_events,
                        &transcript,
                        "human_input_required",
                    )?;
                    turn.result.response.meta.extra.insert(
                        "tool_loop_status".to_string(),
                        "human_input_required".to_string(),
                    );
                    turn.result
                        .response
                        .meta
                        .extra
                        .insert("human_input_required".to_string(), "true".to_string());
                    return Ok(turn);
                }
                current_input = format!(
                    "原始用户请求:\n{}\n\n工具执行记录:\n{}\n\n请继续。若已完成，请输出 FINAL: <最终答复>。",
                    original_input,
                    transcript.join("\n")
                );
                last_turn = Some(turn);
                continue;
            }
            ToolModelOutput::ProtocolError(error) => {
                transcript.push(format!(
                    "protocol_error code={} message={} raw={}",
                    error.code, error.message, error.raw
                ));
                tool_events.push(ToolLoopEvent {
                    round: round_index + 1,
                    kind: "protocol_error".to_string(),
                    tool_name: None,
                    atomic_tool_name: None,
                    decision: None,
                    ok: None,
                    failure_class: None,
                    duration_ms: None,
                    retryable: None,
                    summary: None,
                    protocol_error_code: Some(error.code.clone()),
                    protocol_error_message: Some(error.message.clone()),
                });
                protocol_errors.push(error);
                current_input = format!(
                    "原始用户请求:\n{}\n\n工具协议错误:\n{}\n\n请修正为正式 ACTION JSON，或输出 FINAL: <最终答复>。",
                    original_input,
                    transcript.join("\n")
                );
                last_turn = Some(turn);
                continue;
            }
            ToolModelOutput::PlainText(_) => {
                if tool_calls.is_empty() && protocol_errors.is_empty() && round_index == 0 {
                    insert_tool_surface_metadata(&mut turn, workspace_root)?;
                    return Ok(turn);
                }
                last_plain_text_answer = Some(body.clone());
                let raw = body.clone();
                let error = ToolProtocolError {
                    code: "plain_text_response".to_string(),
                    message: "tool loop requires ACTION or FINAL; plain text is not accepted"
                        .to_string(),
                    raw,
                };
                transcript.push(format!(
                    "protocol_error code={} message={} raw={}",
                    error.code, error.message, error.raw
                ));
                tool_events.push(ToolLoopEvent {
                    round: round_index + 1,
                    kind: "protocol_error".to_string(),
                    tool_name: None,
                    atomic_tool_name: None,
                    decision: None,
                    ok: None,
                    failure_class: None,
                    duration_ms: None,
                    retryable: None,
                    summary: None,
                    protocol_error_code: Some(error.code.clone()),
                    protocol_error_message: Some(error.message.clone()),
                });
                protocol_errors.push(error);
                current_input = format!(
                    "原始用户请求:\n{}\n\n工具协议错误:\n{}\n\n请修正为正式 ACTION JSON，或输出 FINAL: <最终答复>。",
                    original_input,
                    transcript.join("\n")
                );
                last_turn = Some(turn);
                continue;
            }
        }
    }

    if let Some(answer) = last_plain_text_answer {
        if let Some(mut turn) = last_turn.take() {
            turn.result.response.body = answer;
            turn.user_input = original_input;
            insert_tool_surface_metadata(&mut turn, workspace_root)?;
            insert_tool_metadata_with_status(
                &mut turn,
                workspace_root,
                max_tool_rounds,
                &tool_calls,
                &protocol_errors,
                &tool_events,
                &transcript,
                "implicit_final_plain_text",
            )?;
            return Ok(turn);
        }
    }

    if let Some(mut turn) = last_turn {
        if let Some(record) = tool_calls
            .last()
            .filter(|record| terminal_tool_failure(record))
        {
            turn.result.response.body = terminal_tool_failure_answer(record);
            turn.user_input = original_input;
            insert_tool_surface_metadata(&mut turn, workspace_root)?;
            insert_tool_metadata_with_status(
                &mut turn,
                workspace_root,
                max_tool_rounds,
                &tool_calls,
                &protocol_errors,
                &tool_events,
                &transcript,
                "terminal_tool_failure",
            )?;
            return Ok(turn);
        }
    }

    Err("tool_loop_exhausted: model did not produce FINAL response".to_string())
}

fn terminal_tool_failure(record: &ToolExecutionRecord) -> bool {
    !record.ok
        && record
            .failure_class
            .as_deref()
            .is_some_and(|failure_class| {
                matches!(
                    failure_class,
                    "actuator_failed"
                        | "actuator_unconfigured"
                        | "governance_rejected"
                        | "tool_failed"
                )
            })
}

fn terminal_tool_failure_answer(record: &ToolExecutionRecord) -> String {
    let tool_name = tool_call_name(&record.call);
    let decision = record.decision.as_deref().unwrap_or("unknown");
    let failure_class = record.failure_class.as_deref().unwrap_or("tool_failed");
    format!(
        "本轮没有完成真实动作。\n动作：{tool_name}\n结果：未执行点击、输入或修改。\n拦截原因：{failure_class}; {summary}\n治理决策：{decision}\n下一步：如果确实要执行真实桌面动作，需要先补 action allowlist、治理审批和 operator receipt；否则只能继续做只读 observe/screenshot 取证。",
        summary = record.summary
    )
}

fn should_auto_observe_desktop(user_input: &str) -> bool {
    let text = user_input.to_lowercase();
    let asks_observation = [
        "桌面",
        "屏幕",
        "窗口",
        "当前窗口",
        "窗口标题",
        "页面",
        "网页",
        "浏览器",
        "截图",
        "看一下",
        "看下",
        "只读",
        "observe",
        "screenshot",
        "locate",
        "current window",
        "current page",
        "screen",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let asks_read_only = [
        "只读",
        "不要点击",
        "不点击",
        "不要输入",
        "不输入",
        "不要提交",
        "不提交",
        "不要发送",
        "不发送",
        "不要删除",
        "不删除",
        "不要修改",
        "不修改",
        "read-only",
        "readonly",
        "do not click",
        "don't click",
        "without clicking",
        "do not type",
        "don't type",
        "without typing",
        "do not submit",
        "don't submit",
        "without submitting",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let asks_mutation = [
        "点击", "输入", "提交", "发送", "删除", "修改", "click", "type", "submit", "send", "delete",
    ]
    .iter()
    .any(|needle| text.contains(needle));

    asks_observation && (!asks_mutation || asks_read_only)
}

fn goal_context_segments(goal_spec: Option<&GoalSpec>) -> Result<Vec<ContextSegment>, String> {
    goal_spec
        .map(|goal| {
            goal.render_context_segment()
                .map(|segment| vec![segment])
                .map_err(|e| format!("goal_spec_invalid: {}: {}", e.field, e.message))
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn knowledge_context_preview(
    request: &RunCliRequest,
) -> Result<Option<MemoryKnowledgePreviewContextOutput>, String> {
    let Some(knowledge) = &request.knowledge_context else {
        return Ok(None);
    };
    if !knowledge.enabled {
        return Ok(None);
    }
    preview_local_knowledge_context(&knowledge.root, &knowledge.query, knowledge.limit).map(Some)
}

fn knowledge_preview_context_segments(
    preview: &MemoryKnowledgePreviewContextOutput,
) -> Vec<ContextSegment> {
    let now = chrono::Utc::now();
    preview
        .segments
        .iter()
        .map(|segment| {
            let content = format!(
                "[external_knowledge_preview]\nboundary=local_readonly_context_preview_only\nmodel_facing=true\nlive_wiki_gbrain_connected=false\npath={}:{}\nscore={}\npreview={}",
                segment.path, segment.line, segment.score, segment.preview
            );
            ContextSegment {
                id: format!("external-knowledge-{}", segment.segment_id),
                source: SegmentSource::Memory,
                tokens: Some(content.chars().count().min(u32::MAX as usize) as u32),
                content,
                priority: 170,
                created_at: now,
                last_accessed: now,
                metadata: std::collections::HashMap::from([
                    ("kind".to_string(), "external_knowledge_preview".to_string()),
                    ("adapter".to_string(), preview.adapter.clone()),
                    (
                        "source_boundary".to_string(),
                        "local_markdown_text_preview_only".to_string(),
                    ),
                    ("model_facing".to_string(), "true".to_string()),
                    (
                        "live_wiki_gbrain_connected".to_string(),
                        "false".to_string(),
                    ),
                    ("path".to_string(), segment.path.clone()),
                    ("line".to_string(), segment.line.to_string()),
                    ("read_only".to_string(), segment.read_only.to_string()),
                    (
                        "connects_real_service".to_string(),
                        segment.connects_real_service.to_string(),
                    ),
                    (
                        "writes_automatically".to_string(),
                        segment.writes_automatically.to_string(),
                    ),
                    ("runtime_injection_applied".to_string(), "true".to_string()),
                    (
                        "runtime_retrieval_wired".to_string(),
                        segment.runtime_retrieval_wired.to_string(),
                    ),
                ]),
            }
        })
        .collect()
}

fn insert_knowledge_context_metadata(
    turn: &mut ChuangKernelTurn,
    preview: Option<&MemoryKnowledgePreviewContextOutput>,
) -> Result<(), String> {
    let extra = &mut turn.result.response.meta.extra;
    let Some(preview) = preview else {
        extra.insert(
            "knowledge_context_preview_enabled".to_string(),
            "false".to_string(),
        );
        extra.insert(
            "knowledge_context_injected".to_string(),
            "false".to_string(),
        );
        extra.insert(
            "knowledge_context_segment_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_preview_segment_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_preview_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_injected_segment_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_injected_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_dropped_segment_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_dropped_count".to_string(),
            "0".to_string(),
        );
        extra.insert(
            "knowledge_context_dropped_segment_ids".to_string(),
            "[]".to_string(),
        );
        extra.insert(
            "knowledge_context_model_facing".to_string(),
            "false".to_string(),
        );
        extra.insert(
            "knowledge_context_source_boundary".to_string(),
            "disabled_or_not_requested".to_string(),
        );
        extra.insert(
            "knowledge_context_live_wiki_gbrain_connected".to_string(),
            "false".to_string(),
        );
        return Ok(());
    };

    let preview_segment_count = preview.segment_count;
    let preview_runtime_segment_ids = knowledge_preview_runtime_segment_ids(preview);
    let dropped_preview_segment_ids: Vec<String> = preview_runtime_segment_ids
        .into_iter()
        .filter(|segment_id| {
            turn.result
                .dropped_segment_ids
                .iter()
                .any(|dropped_id| dropped_id == segment_id)
        })
        .collect();
    let dropped_preview_segment_count = dropped_preview_segment_ids.len();
    let injected_segment_count =
        preview_segment_count.saturating_sub(dropped_preview_segment_count);

    extra.insert(
        "knowledge_context_preview_enabled".to_string(),
        "true".to_string(),
    );
    extra.insert(
        "knowledge_context_injected".to_string(),
        (injected_segment_count > 0).to_string(),
    );
    extra.insert(
        "knowledge_context_segment_count".to_string(),
        preview_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_preview_segment_count".to_string(),
        preview_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_preview_count".to_string(),
        preview_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_injected_segment_count".to_string(),
        injected_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_injected_count".to_string(),
        injected_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_dropped_segment_count".to_string(),
        dropped_preview_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_dropped_count".to_string(),
        dropped_preview_segment_count.to_string(),
    );
    extra.insert(
        "knowledge_context_dropped_segment_ids".to_string(),
        serde_json::to_string(&dropped_preview_segment_ids)
            .map_err(|e| format!("knowledge_context_dropped_segment_ids_json_failed: {e}"))?,
    );
    extra.insert(
        "knowledge_context_model_facing".to_string(),
        (injected_segment_count > 0).to_string(),
    );
    extra.insert(
        "knowledge_context_source_boundary".to_string(),
        "local_markdown_text_preview_only".to_string(),
    );
    extra.insert(
        "knowledge_context_live_wiki_gbrain_connected".to_string(),
        "false".to_string(),
    );
    extra.insert("knowledge_context_root".to_string(), preview.root.clone());
    extra.insert("knowledge_context_query".to_string(), preview.query.clone());
    extra.insert(
        "knowledge_context_read_only".to_string(),
        preview.read_only.to_string(),
    );
    extra.insert(
        "knowledge_context_connects_real_service".to_string(),
        preview.connects_real_service.to_string(),
    );
    extra.insert(
        "knowledge_context_writes_automatically".to_string(),
        preview.writes_automatically.to_string(),
    );
    extra.insert(
        "knowledge_context_runtime_retrieval_wired".to_string(),
        preview.runtime_retrieval_wired.to_string(),
    );
    extra.insert(
        "knowledge_context_preview_json".to_string(),
        serde_json::to_string(preview)
            .map_err(|e| format!("knowledge_context_preview_json_failed: {e}"))?,
    );
    Ok(())
}

fn knowledge_preview_runtime_segment_ids(
    preview: &MemoryKnowledgePreviewContextOutput,
) -> Vec<String> {
    preview
        .segments
        .iter()
        .map(|segment| format!("external-knowledge-{}", segment.segment_id))
        .collect()
}

fn insert_tool_metadata(
    turn: &mut ChuangKernelTurn,
    workspace_root: &Path,
    rounds: usize,
    tool_calls: &[ToolExecutionRecord],
    protocol_errors: &[ToolProtocolError],
    tool_events: &[ToolLoopEvent],
    transcript: &[String],
) -> Result<(), String> {
    insert_tool_metadata_with_status(
        turn,
        workspace_root,
        rounds,
        tool_calls,
        protocol_errors,
        tool_events,
        transcript,
        "completed",
    )
}

fn insert_tool_metadata_with_status(
    turn: &mut ChuangKernelTurn,
    workspace_root: &Path,
    rounds: usize,
    tool_calls: &[ToolExecutionRecord],
    protocol_errors: &[ToolProtocolError],
    tool_events: &[ToolLoopEvent],
    transcript: &[String],
    status: &str,
) -> Result<(), String> {
    let report = ToolLoopReport::with_status(workspace_root, rounds, tool_calls.to_vec(), status);
    turn.result
        .response
        .meta
        .extra
        .insert("tool_call_count".to_string(), tool_calls.len().to_string());
    turn.result.response.meta.extra.insert(
        "tool_protocol_error_count".to_string(),
        protocol_errors.len().to_string(),
    );
    turn.result
        .response
        .meta
        .extra
        .insert("tool_trace".to_string(), transcript.join("\n"));
    turn.result.response.meta.extra.insert(
        "tool_calls_json".to_string(),
        serde_json::to_string(tool_calls).map_err(|e| format!("tool_calls_json_failed: {e}"))?,
    );
    turn.result.response.meta.extra.insert(
        "tool_protocol_errors_json".to_string(),
        serde_json::to_string(protocol_errors)
            .map_err(|e| format!("tool_protocol_errors_json_failed: {e}"))?,
    );
    turn.result.response.meta.extra.insert(
        "tool_events_json".to_string(),
        serde_json::to_string(tool_events).map_err(|e| format!("tool_events_json_failed: {e}"))?,
    );
    turn.result.response.meta.extra.insert(
        "tool_report_json".to_string(),
        serde_json::to_string(&report).map_err(|e| format!("tool_report_json_failed: {e}"))?,
    );
    turn.result
        .response
        .meta
        .extra
        .insert("tool_loop_status".to_string(), status.to_string());
    Ok(())
}

fn insert_tool_surface_metadata(
    turn: &mut ChuangKernelTurn,
    workspace_root: &Path,
) -> Result<(), String> {
    let surface = ToolSurfaceStatus::generic_agent_mvp(workspace_root);
    let extra = &mut turn.result.response.meta.extra;
    extra.insert("tool_surface_available".to_string(), "true".to_string());
    extra.insert("tool_surface_governed".to_string(), "true".to_string());
    extra.insert("tool_surface_source".to_string(), surface.source.clone());
    extra.insert(
        "tool_surface_callable_tools".to_string(),
        surface.callable_tools.join(","),
    );
    extra.insert(
        "tool_surface_mapped_atomic_tools".to_string(),
        surface.mapped_atomic_tools.join(","),
    );
    extra.insert(
        "tool_surface_interface_only_atomic_tools".to_string(),
        surface.interface_only_atomic_tools.join(","),
    );
    extra.insert(
        "tool_action_schema_version".to_string(),
        surface.action_schema_version.to_string(),
    );
    extra.insert(
        "tool_report_schema_version".to_string(),
        surface.report_schema_version.to_string(),
    );
    extra.insert(
        "tool_instruction_context_injected".to_string(),
        surface.instruction_context_injected.to_string(),
    );
    extra.insert(
        "tool_surface_json".to_string(),
        serde_json::to_string(&surface).map_err(|e| format!("tool_surface_json_failed: {e}"))?,
    );
    extra
        .entry("tool_call_count".to_string())
        .or_insert_with(|| "0".to_string());
    extra
        .entry("tool_protocol_error_count".to_string())
        .or_insert_with(|| "0".to_string());
    extra
        .entry("tool_trace".to_string())
        .or_insert_with(String::new);
    Ok(())
}

fn tool_instruction_segment(workspace_root: &Path) -> ContextSegment {
    let content = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default())
        .tool_instruction_block(workspace_root);
    let now = chrono::Utc::now();
    ContextSegment {
        id: "tool-instructions".to_string(),
        source: SegmentSource::Identity,
        tokens: Some(content.chars().count().min(u32::MAX as usize) as u32),
        priority: 252,
        created_at: now,
        last_accessed: now,
        metadata: std::collections::HashMap::from([(
            "kind".to_string(),
            "tool_protocol".to_string(),
        )]),
        content,
    }
}

fn read_optional_identity_file(path: &PathBuf) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| {
        format!(
            "identity_bootstrap_read_failed path={}: {e}",
            path.display()
        )
    })
}

fn remember_turn_if_requested<S, R>(
    options: &CliOptions,
    kernel: &mut ChuangKernel<S, R>,
    mut turn: chuang_agent::chuang_kernel::ChuangKernelTurn,
    request: &RunCliRequest,
) -> Result<
    (
        chuang_agent::agent_runtime::RuntimeResult,
        RememberedRecords,
    ),
    String,
>
where
    S: MemoryStore,
    R: chuang_agent::responder::Responder,
{
    let mut records = RememberedRecords::default();
    records.runtime_report_id = Some(turn.report.report_id.0.clone());
    insert_runtime_report_metadata(&mut turn);

    if let Some(decision) = &turn.report.governance_decision {
        turn.result
            .response
            .meta
            .extra
            .extend(governance_metadata(decision));
        records.governance_decision = Some(format!("{}:{}", decision.decision, decision.reason));
    } else {
        records.governance_decision = turn.governance_decision.as_ref().map(risk_decision_label);
    }

    if let Some(goal) = &request.goal_spec {
        turn.result
            .response
            .meta
            .extra
            .insert("goal_id".to_string(), goal.goal_id.clone());
        turn.result
            .response
            .meta
            .extra
            .insert("goal_objective".to_string(), goal.objective.clone());
        turn.result
            .response
            .meta
            .extra
            .insert("goal_context_injected".to_string(), "true".to_string());
    }

    if request.remember {
        records.sqlite_record_id = Some(
            kernel
                .remember_turn(&turn)
                .map_err(format_kernel_memory_error)?,
        );
    }

    if request.remember_session {
        let session_id = request
            .session_id
            .as_ref()
            .ok_or_else(|| "remember_session_requires_session_id: pass --session-id".to_string())?;
        match kernel.remember_session_turn(&turn, session_id) {
            Ok(receipt) => {
                records.session_record_id = Some(receipt.record_id);
                turn.result.response.meta.extra.insert(
                    "session_memory_write_status".to_string(),
                    if receipt.compacted {
                        "compacted"
                    } else {
                        "written"
                    }
                    .to_string(),
                );
                turn.result.response.meta.extra.insert(
                    "session_memory_summary_kind".to_string(),
                    if receipt.compacted {
                        "compacted_turn_summary"
                    } else {
                        "turn_summary"
                    }
                    .to_string(),
                );
                if receipt.compacted {
                    turn.result.response.meta.extra.insert(
                        "session_memory_compacted_from_chars".to_string(),
                        receipt.attempted_chars.to_string(),
                    );
                    turn.result.response.meta.extra.insert(
                        "session_memory_compacted_to_chars".to_string(),
                        receipt.stored_chars.to_string(),
                    );
                }
            }
            Err(chuang_agent::chuang_kernel::ChuangKernelMemoryError::HardLimitExceeded {
                limit_chars,
                attempted_chars,
                existing_entries,
            }) => {
                turn.result.response.meta.extra.insert(
                    "session_memory_write_status".to_string(),
                    "hard_limit_exceeded".to_string(),
                );
                turn.result.response.meta.extra.insert(
                    "session_memory_write_error".to_string(),
                    format!(
                        "memory_write_hard_limit_exceeded limit_chars={} attempted_chars={} existing_entries={}",
                        limit_chars,
                        attempted_chars,
                        if existing_entries.is_empty() {
                            "none".to_string()
                        } else {
                            existing_entries
                                .into_iter()
                                .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                                .collect::<Vec<_>>()
                                .join(",")
                        }
                    ),
                );
                turn.result.response.meta.extra.insert(
                    "session_memory_write_requested".to_string(),
                    "true".to_string(),
                );
                turn.result.response.meta.extra.insert(
                    "session_memory_summary_kind".to_string(),
                    "none".to_string(),
                );
            }
            Err(err) => return Err(format_kernel_memory_error(err)),
        }
    }

    insert_session_memory_metadata(&mut turn, request, &records);

    if request.remember_identity {
        records.identity_record_id = Some(remember_identity_turn(options, &turn)?);
    }

    if request.remember_experience {
        records.experience_record_id = Some(remember_experience_turn(options, &turn)?);
    }

    if request.dispatch_subagent {
        let receipt = dispatch_subagent_turn(options, &turn)?;
        records.subagent_dispatch_run_id = Some(receipt.run_id.0);
        records.subagent_dispatch_agent_id = Some(receipt.agent_id.0);
        records.subagent_dispatch_task_id = Some(turn.report.task_id.0.clone());
    }

    Ok((turn.result, records))
}

fn insert_runtime_report_metadata(turn: &mut chuang_agent::chuang_kernel::ChuangKernelTurn) {
    let extra = &mut turn.result.response.meta.extra;
    extra.insert(
        "runtime_report_id".to_string(),
        turn.report.report_id.0.clone(),
    );
    extra.insert(
        "runtime_report_task_id".to_string(),
        turn.report.task_id.0.clone(),
    );
    extra.insert(
        "runtime_report_agent_id".to_string(),
        turn.report.agent_id.0.clone(),
    );
    extra.insert(
        "runtime_report_status".to_string(),
        match turn.report.status {
            chuang_agent::subagent_report::ExecutionStatus::Success => "Success",
            chuang_agent::subagent_report::ExecutionStatus::Failed => "Failed",
            chuang_agent::subagent_report::ExecutionStatus::TimedOut => "TimedOut",
            chuang_agent::subagent_report::ExecutionStatus::Cancelled => "Cancelled",
        }
        .to_string(),
    );
}

fn insert_session_memory_metadata(
    turn: &mut chuang_agent::chuang_kernel::ChuangKernelTurn,
    request: &RunCliRequest,
    records: &RememberedRecords,
) {
    let extra = &mut turn.result.response.meta.extra;
    let Some(session_id) = &request.session_id else {
        extra.insert("session_memory_scope".to_string(), "global".to_string());
        extra.insert(
            "session_memory_recall_isolated".to_string(),
            "false".to_string(),
        );
        extra.insert(
            "session_memory_write_requested".to_string(),
            "false".to_string(),
        );
        extra.insert(
            "session_memory_summary_kind".to_string(),
            "none".to_string(),
        );
        return;
    };

    extra.insert("session_id".to_string(), session_id.clone());
    extra.insert("session_memory_scope".to_string(), "session".to_string());
    extra.insert(
        "session_memory_recall_isolated".to_string(),
        "true".to_string(),
    );
    extra.insert(
        "session_memory_recall_filter".to_string(),
        format!("memory_scope=session,session_id={session_id}"),
    );
    extra.insert(
        "session_memory_recall_hit_count".to_string(),
        turn.result.recall_hit_count.to_string(),
    );
    extra.insert(
        "session_memory_write_requested".to_string(),
        request.remember_session.to_string(),
    );
    extra
        .entry("session_memory_summary_kind".to_string())
        .or_insert_with(|| {
            if records.session_record_id.is_some() {
                "turn_summary"
            } else {
                "none"
            }
            .to_string()
        });
    if let Some(record_id) = &records.session_record_id {
        extra.insert("session_memory_record_id".to_string(), record_id.clone());
    }
}

fn dispatch_subagent_turn(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<chuang_agent::subagent_spawner::SpawnReceipt, String> {
    if options.runtime.subagent != SubagentConfig::QueuedExternal {
        return Err(
            "subagent_dispatch_requires_queued_external: pass --subagent queued_external"
                .to_string(),
        );
    }

    let mut slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId(turn.report.task_id.0.clone()),
            parent_agent_id: AgentId("chuang-cli".to_string()),
            agent_name: "worker".to_string(),
            task: format!(
                "处理 runtime report {}: user={} summary={}",
                turn.report.report_id.0, turn.user_input, turn.report.summary
            ),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 1024,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: BTreeMap::from([
                ("source".to_string(), "cli-run".to_string()),
                ("turn_id".to_string(), turn.turn_id.clone()),
                ("report_id".to_string(), turn.report.report_id.0.clone()),
            ]),
        })
        .map_err(|e| format!("subagent_dispatch_failed: {e:?}"))
}

fn remember_identity_turn(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<String, String> {
    let dual_file_config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let mut store = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
    let entry_id = unique_identity_turn_id(&turn.turn_id)?;
    let content = format!(
        "user={}\nresponse={}\nsummary={}",
        turn.user_input, turn.result.response.body, turn.report.summary
    );
    store
        .append_memory(HotMemoryEntry {
            id: entry_id.clone(),
            content,
        })
        .map_err(format_identity_memory_error)?;
    Ok(entry_id)
}

fn remember_experience_turn(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<String, String> {
    let dual_file_config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let mut store = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
    let entry_id = unique_experience_turn_id(&turn.turn_id)?;
    let governance = turn
        .report
        .governance_decision
        .as_ref()
        .map(|decision| format!("{}:{}", decision.decision, decision.reason))
        .or_else(|| turn.governance_decision.as_ref().map(risk_decision_label))
        .unwrap_or_else(|| "unknown".to_string());
    let content = format!(
        "source=runtime_turn\nturn_id={}\nreport_id={}\nagent_id={}\ngovernance={}\nuser={}\nsummary={}\nlesson={}",
        turn.turn_id,
        turn.report.report_id.0,
        turn.report.agent_id.0,
        governance,
        turn.user_input,
        turn.report.summary,
        extract_experience_lesson(turn),
    );
    store
        .append_experience(HotMemoryEntry {
            id: entry_id.clone(),
            content,
        })
        .map_err(format_identity_memory_error)?;
    Ok(entry_id)
}

fn unique_identity_turn_id(turn_id: &str) -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    Ok(format!(
        "identity-{}-{}-{}",
        turn_id,
        std::process::id(),
        nanos
    ))
}

fn unique_experience_turn_id(turn_id: &str) -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    Ok(format!(
        "experience-{}-{}-{}",
        turn_id,
        std::process::id(),
        nanos
    ))
}

fn extract_experience_lesson(turn: &chuang_agent::chuang_kernel::ChuangKernelTurn) -> String {
    let body = turn.result.response.body.trim();
    if body.is_empty() {
        return "本轮没有可沉淀正文，保留 report summary 作为来源。".to_string();
    }

    body.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().chars().take(180).collect())
        .unwrap_or_else(|| "本轮没有可沉淀正文，保留 report summary 作为来源。".to_string())
}

fn format_identity_memory_error(err: chuang_agent::hermes_memory::DualFileMemoryError) -> String {
    match err {
        chuang_agent::hermes_memory::DualFileMemoryError::StorageUnavailable { path } => {
            format!("identity_memory_write_failed path={}", path.display())
        }
        chuang_agent::hermes_memory::DualFileMemoryError::DuplicateEntry { id } => {
            format!("identity_memory_duplicate_entry id={id}")
        }
        chuang_agent::hermes_memory::DualFileMemoryError::HardLimitExceeded {
            scope,
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "identity_memory_hard_limit_exceeded scope={scope:?} limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

fn format_kernel_memory_error(err: chuang_agent::chuang_kernel::ChuangKernelMemoryError) -> String {
    match err {
        chuang_agent::chuang_kernel::ChuangKernelMemoryError::Store(store_err) => {
            format!("memory_write_failed: {store_err:?}")
        }
        chuang_agent::chuang_kernel::ChuangKernelMemoryError::HardLimitExceeded {
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "memory_write_hard_limit_exceeded limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

fn read_only_capability_banner() -> &'static str {
    "[cap]locate/screenshot;wiki/gbrain(read-only)"
}

fn tool_call_name(call: &ToolCall) -> &'static str {
    match call {
        ToolCall::ListDir { .. } => "list_dir",
        ToolCall::ReadFile { .. } => "read_file",
        ToolCall::WriteFile { .. } => "write_file",
        ToolCall::Mouse { .. } => "mouse",
        ToolCall::Keyboard { .. } => "keyboard",
        ToolCall::Screenshot { .. } => "screenshot",
        ToolCall::Locate { .. } => "locate",
        ToolCall::Wait { .. } => "wait",
        ToolCall::HumanSuspend { .. } => "human_suspend",
        ToolCall::ApplyPatch { .. } => "apply_patch",
        ToolCall::ShellExec { .. } => "shell_exec",
        ToolCall::MemoryRecall { .. } => "memory_recall",
    }
}

fn seed_default_memory_if_empty(store: &mut SqliteMemoryStore) -> Result<(), String> {
    let existing = store
        .search(&chuang_agent::memory_store::MemoryQuery {
            text: None,
            metadata: BTreeMap::new(),
            limit: 1,
        })
        .map_err(|e| format!("seed_search_failed: {e:?}"))?;

    if !existing.is_empty() {
        return Ok(());
    }

    store
        .put(chuang_agent::memory_store::MemoryRecord {
            id: "boot-seed-1".to_string(),
            content: "创项目先跑起来，先闭环再优化。".to_string(),
            metadata: BTreeMap::from([("kind".to_string(), "goal".to_string())]),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            expires_at: None,
        })
        .map_err(|e| format!("seed_put_failed: {e:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};

    #[test]
    fn run_with_options_surfaces_governance_metadata_in_runtime_result() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-runtime-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let db_path = temp_dir.join("memory.db");
        let identity_root = temp_dir.join("identity");
        fs::create_dir_all(&identity_root).expect("identity root should be created");

        let mut runtime = RuntimeConfig::new(db_path);
        runtime.identity_memory =
            chuang_agent::runtime_config::IdentityMemoryConfig::HermesDualFile {
                root: identity_root,
                user_max_chars: chuang_agent::hermes_memory::DEFAULT_USER_MEMORY_MAX_CHARS,
                memory_max_chars: chuang_agent::hermes_memory::DEFAULT_HOT_MEMORY_MAX_CHARS,
            };

        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "确认治理元数据进入 runtime result".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };

        let (result, records) = run_with_options(&request).expect("run should succeed");

        let decision = records
            .governance_decision
            .as_deref()
            .expect("governance decision should be present");
        assert!(decision.starts_with("allowed:read-only or draft action"));
        assert!(decision.contains("rules="));
        let meta_decision = result
            .response
            .meta
            .extra
            .get("governance_decision")
            .expect("governance decision metadata should be present");
        assert!(meta_decision.starts_with("allowed:read-only or draft action"));
        assert!(meta_decision.contains("rules="));
        let meta_reason = result
            .response
            .meta
            .extra
            .get("governance_reason")
            .expect("governance reason metadata should be present");
        assert!(meta_reason.starts_with("read-only or draft action"));
        assert!(meta_reason.contains("rules="));
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("governance_action_id")
                .map(String::as_str),
            Some("run-turn-1")
        );
        assert_eq!(records.runtime_report_id.as_deref(), Some("report-turn-1"));
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("runtime_report_id")
                .map(String::as_str),
            Some("report-turn-1")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("runtime_report_task_id")
                .map(String::as_str),
            Some("turn-1")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("runtime_report_status")
                .map(String::as_str),
            Some("Success")
        );
    }

    #[test]
    fn run_with_options_surfaces_goal_metadata_without_polluting_user_input() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-runtime-goal-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let mut runtime = test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity"));
        runtime.context_budget = goal_context_test_budget();
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "保持主链输入稳定".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: Some(GoalSpec::mainline_mvp("通过 CLI 注入 goal context")),
            knowledge_context: None,
        };

        let (result, _) = run_with_options(&request).expect("run should succeed");

        assert!(result.response.body.contains("保持主链输入稳定"));
        assert!(!result.response.body.contains("GOAL_SPEC"));
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("goal_id")
                .map(String::as_str),
            Some("mainline-mvp")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("goal_objective")
                .map(String::as_str),
            Some("通过 CLI 注入 goal context")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("goal_context_injected")
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn run_with_options_can_inject_readonly_knowledge_context_when_enabled() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-runtime-knowledge-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let knowledge_root = temp_dir.join("knowledge");
        fs::create_dir_all(&knowledge_root).expect("knowledge root should be created");
        fs::write(
            knowledge_root.join("wiki.md"),
            "runtime knowledge context marker should enter preview only\n",
        )
        .expect("knowledge doc should write");

        let mut runtime = test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity"));
        runtime.context_budget = goal_context_test_budget();
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "检查外脑上下文".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: Some(crate::cli_types::KnowledgeContextCliRequest {
                root: knowledge_root,
                query: "marker".to_string(),
                limit: 2,
                enabled: true,
            }),
        };

        let (result, _) = run_with_options(&request).expect("run should succeed");

        assert!(result
            .packed_context_preview
            .contains("external-knowledge-knowledge-segment-1"));
        assert!(result
            .packed_context_preview
            .contains("boundary=local_readonly_context_preview_only"));
        assert!(result
            .packed_context_preview
            .contains("live_wiki_gbrain_connected=false"));
        assert!(result
            .packed_context_preview
            .contains("runtime knowledge context marker"));
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_preview_enabled")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_injected")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_preview_count")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_injected_count")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_dropped_count")
                .map(String::as_str),
            Some("0")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_model_facing")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_source_boundary")
                .map(String::as_str),
            Some("local_markdown_text_preview_only")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_live_wiki_gbrain_connected")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_connects_real_service")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_runtime_retrieval_wired")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn run_with_options_reports_knowledge_context_drops_under_tight_budget() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-runtime-knowledge-drop-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let knowledge_root = temp_dir.join("knowledge");
        fs::create_dir_all(&knowledge_root).expect("knowledge root should be created");
        fs::write(
            knowledge_root.join("wiki.md"),
            "runtime knowledge context marker should enter preview only and be dropped by budget\n",
        )
        .expect("knowledge doc should write");

        let mut runtime = test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity"));
        runtime.context_budget = chuang_agent::context_engine::ContextBudget {
            max_tokens: 2700,
            reserve_system_tokens: 32,
            min_working_tokens: 1,
            max_tool_results: 5,
            max_memory_segments: 20,
        };
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "查".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: Some(crate::cli_types::KnowledgeContextCliRequest {
                root: knowledge_root,
                query: "marker".to_string(),
                limit: 1,
                enabled: true,
            }),
        };

        let (result, _) = run_with_options(&request).expect("run should succeed");

        assert!(result
            .dropped_segment_ids
            .iter()
            .any(|id| id == "external-knowledge-knowledge-segment-1"));
        assert!(result
            .packed_context_preview
            .contains("external-knowledge-knowledge-segment-1"));
        assert!(!result.packed_context_preview.contains(
            "runtime knowledge context marker should enter preview only and be dropped by budget"
        ));
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_preview_enabled")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_injected")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_preview_count")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_injected_count")
                .map(String::as_str),
            Some("0")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_dropped_count")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_dropped_segment_ids")
                .map(String::as_str),
            Some(r#"["external-knowledge-knowledge-segment-1"]"#)
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("knowledge_context_model_facing")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn run_with_options_can_remember_experience_with_provenance() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-experience-memory-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let identity_root = temp_dir.join("identity");
        let runtime = test_runtime(temp_dir.join("memory.db"), identity_root.clone());

        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "沉淀一次经验".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: true,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };

        let (_, records) = run_with_options(&request).expect("run should succeed");
        let record_id = records
            .experience_record_id
            .as_deref()
            .expect("experience record id should exist");
        assert!(record_id.starts_with("experience-turn-1-"));

        let experiences = fs::read_to_string(identity_root.join("experiences.md"))
            .expect("experiences should be readable");
        assert!(experiences.contains("source=runtime_turn"));
        assert!(experiences.contains("turn_id=turn-1"));
        assert!(experiences.contains("report_id=report-turn-1"));
        assert!(experiences.contains("user=沉淀一次经验"));
        assert!(experiences.contains("lesson="));
    }

    #[test]
    fn run_with_options_remembers_and_recalls_session_turns() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-session-memory-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let runtime = test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity"));

        let first = RunCliRequest {
            options: CliOptions {
                runtime: runtime.clone(),
            },
            user_input: "会话记忆锚点A".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };
        let (_, first_records) = run_with_options(&first).expect("first run should succeed");
        assert!(first_records
            .session_record_id
            .as_deref()
            .unwrap_or_default()
            .starts_with("turn-memory-session-alpha-turn-1-"));

        let second = RunCliRequest {
            options: CliOptions {
                runtime: runtime.clone(),
            },
            user_input: "会话记忆锚点A".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };
        let (same_session, _) = run_with_options(&second).expect("second run should succeed");
        assert_eq!(same_session.recall_hit_count, 1);
        assert!(same_session.recall_summary.contains("会话记忆锚点A"));
        assert_eq!(
            same_session
                .response
                .meta
                .extra
                .get("session_memory_scope")
                .map(String::as_str),
            Some("session")
        );
        assert_eq!(
            same_session
                .response
                .meta
                .extra
                .get("session_memory_recall_isolated")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            same_session
                .response
                .meta
                .extra
                .get("session_memory_recall_filter")
                .map(String::as_str),
            Some("memory_scope=session,session_id=alpha")
        );
        assert_eq!(
            same_session
                .response
                .meta
                .extra
                .get("session_memory_recall_hit_count")
                .map(String::as_str),
            Some("1")
        );

        let beta_write = RunCliRequest {
            options: CliOptions {
                runtime: runtime.clone(),
            },
            user_input: "会话记忆锚点B".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("beta".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };
        let (beta_written, beta_records) =
            run_with_options(&beta_write).expect("beta run should succeed");
        assert!(beta_records.session_record_id.is_some());
        assert_eq!(
            beta_written
                .response
                .meta
                .extra
                .get("session_memory_summary_kind")
                .map(String::as_str),
            Some("turn_summary")
        );
        assert_eq!(
            beta_written
                .response
                .meta
                .extra
                .get("session_memory_write_requested")
                .map(String::as_str),
            Some("true")
        );

        let third = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "会话记忆锚点B".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };
        let (other_session, _) = run_with_options(&third).expect("third run should succeed");
        assert_eq!(other_session.recall_hit_count, 0);
        assert_eq!(
            other_session
                .response
                .meta
                .extra
                .get("session_memory_recall_filter")
                .map(String::as_str),
            Some("memory_scope=session,session_id=alpha")
        );
    }

    #[test]
    fn run_with_options_compacts_session_memory_hard_limit_without_failing_turn() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-session-memory-hard-limit-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let runtime = test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity"));

        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "超限".repeat(1200),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };

        let (result, records) = run_with_options(&request).expect("run should not fail");
        assert!(records.session_record_id.is_some());
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("session_memory_write_requested")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("session_memory_write_status")
                .map(String::as_str),
            Some("compacted")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("session_memory_summary_kind")
                .map(String::as_str),
            Some("compacted_turn_summary")
        );
        assert!(result
            .response
            .meta
            .extra
            .get("session_memory_record_id")
            .is_some());
        assert!(result
            .response
            .meta
            .extra
            .get("session_memory_compacted_from_chars")
            .is_some());
        assert!(result
            .response
            .meta
            .extra
            .get("session_memory_compacted_to_chars")
            .is_some());
        assert!(!result
            .response
            .meta
            .extra
            .contains_key("session_memory_write_error"));
    }

    #[test]
    fn remember_session_requires_session_id() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-session-required-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let request = RunCliRequest {
            options: CliOptions {
                runtime: test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity")),
            },
            user_input: "缺 session id".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: None,
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };

        let error = run_with_options(&request).expect_err("run should reject missing session id");
        assert_eq!(
            error,
            "remember_session_requires_session_id: pass --session-id"
        );
    }

    #[test]
    fn run_with_options_executes_tool_calls_before_final_answer() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-loop-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"write_file","path":"notes/out.txt","content":"hello"}}"#,
                r#"ACTION: {"type":"final","answer":"已经写好了"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig {
                shell_timeout_ms: 30_000,
                ..ToolExecutionConfig::default()
            },
            "请先写一个文件".to_string(),
            Vec::new(),
        )
        .expect("tool loop should succeed");

        assert_eq!(turn.result.response.body, "已经写好了");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("1")
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_trace")
            .expect("tool trace should exist")
            .contains("write_file"));
        let tool_calls_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist");
        assert!(tool_calls_json.contains("\"tool_name\":\"write_file\""));
        assert!(tool_calls_json.contains("\"changed_files\""));
        let tool_events_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_events_json")
            .expect("tool events json should exist");
        assert!(tool_events_json.contains("\"kind\":\"tool_call\""));
        assert!(tool_events_json.contains("\"tool_name\":\"write_file\""));
        assert!(tool_events_json.contains("\"atomic_tool_name\":\"file_write\""));
        assert!(tool_events_json.contains("\"duration_ms\":"));
        assert!(tool_events_json.contains("\"retryable\":false"));
        let tool_report_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_report_json")
            .expect("tool report json should exist");
        assert!(tool_report_json.contains("\"schema_version\":6"));
        assert!(tool_report_json.contains("\"status\":\"completed\""));
        assert!(tool_report_json.contains("\"call_count\":1"));

        let written = fs::read_to_string(workspace_root.join("notes/out.txt"))
            .expect("tool should have written output file");
        assert_eq!(written, "hello");
    }

    #[test]
    fn run_with_options_stops_tool_loop_on_human_suspend() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-human-suspend-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = SequenceResponder::new(vec![
            r#"ACTION: {"type":"tool_call","call":{"tool":"human_suspend","reason":"uncertain live state","prompt":"approve next action?"}}"#,
            r#"ACTION: {"type":"final","answer":"this should not be reached"}"#,
        ]);
        let remaining_outputs = responder.outputs.clone();
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "如果状态不确定就暂停".to_string(),
            Vec::new(),
        )
        .expect("human suspend should return the current turn");

        assert!(turn.result.response.body.contains("human_input_required"));
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("human_input_required")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("human_input_required")
        );
        let tool_report_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_report_json")
            .expect("tool report json should exist");
        assert!(tool_report_json.contains("\"status\":\"human_input_required\""));
        assert!(tool_report_json.contains("\"failure_class\":\"human_input_required\""));
        assert_eq!(
            remaining_outputs
                .lock()
                .expect("sequence lock should succeed")
                .len(),
            1,
            "tool loop should not ask the model for the next round after human_suspend"
        );
    }

    #[test]
    fn run_with_options_adds_desktop_observation_hint_for_screen_requests() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-desktop-observation-hint-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = CaptureResponder::new(r#"ACTION: {"type":"final","answer":"ok"}"#);
        let captured = responder.captured.clone();
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "请看一下当前窗口标题".to_string(),
            Vec::new(),
        )
        .expect("desktop observation turn should succeed");

        assert_eq!(turn.result.response.body, "ok");
        assert!(turn
            .result
            .packed_context_preview
            .contains("locate/screenshot=只读观察"));
        let captured = captured.lock().expect("capture lock should succeed");
        assert!(captured[0].prompt.contains("locate/screenshot=只读观察"));
        assert!(captured[0].prompt.contains("mouse/keyboard=GA 桌面交互"));
    }

    #[test]
    fn run_with_options_exposes_capability_surface_for_read_only_checks() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-capability-surface-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = CaptureResponder::new(r#"ACTION: {"type":"final","answer":"ok"}"#);
        let captured = responder.captured.clone();
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "请做一次只读检查".to_string(),
            Vec::new(),
        )
        .expect("capability surface turn should succeed");

        assert_eq!(turn.result.response.body, "ok");
        let captured = captured.lock().expect("capture lock should succeed");
        assert!(captured[0]
            .prompt
            .contains("[cap]locate/screenshot;wiki/gbrain(read-only)"));
        assert!(captured[0].prompt.contains("req:请做一次只读检查"));
        assert!(captured[0].prompt.contains("tool:call=locate output="));
        assert!(captured[0].prompt.contains("FINAL:<最终答复>"));
    }

    #[test]
    fn run_with_options_auto_observes_desktop_before_model_answer() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-auto-observe-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = SequenceResponder::new(vec![
            "我看不了当前窗口",
            r#"ACTION: {"type":"final","answer":"当前窗口已通过工具观察。"}"#,
        ]);
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig {
                actuator: Some(chuang_agent::runtime_config::ActuatorConfig::Fake),
                ..ToolExecutionConfig::default()
            },
            "请做一次只读桌面检查，回复当前窗口标题".to_string(),
            Vec::new(),
        )
        .expect("auto observe turn should succeed");

        assert_eq!(turn.result.response.body, "当前窗口已通过工具观察。");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_protocol_error_count")
                .map(String::as_str),
            Some("1")
        );
        let tool_calls_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist");
        assert!(tool_calls_json.contains("\"tool_name\":\"locate\""));
        assert!(tool_calls_json.contains("\"atomic_tool_name\":\"locate\""));
        assert!(tool_calls_json.contains("fake observation"));
        let tool_trace = turn
            .result
            .response
            .meta
            .extra
            .get("tool_trace")
            .expect("tool trace should exist");
        assert!(tool_trace.contains("fake observation"));
    }

    #[test]
    fn run_with_options_auto_observes_read_only_requests_with_negated_actions() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-auto-observe-negated-actions-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = SequenceResponder::new(vec![
            "当前会话没有桌面工具",
            r#"ACTION: {"type":"final","answer":"已完成只读检查。"}"#,
        ]);
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig {
                actuator: Some(chuang_agent::runtime_config::ActuatorConfig::Fake),
                ..ToolExecutionConfig::default()
            },
            "请做一次只读桌面和浏览器检查，不要点击、不输入、不提交任何内容。".to_string(),
            Vec::new(),
        )
        .expect("auto observe turn should succeed");

        assert_eq!(turn.result.response.body, "已完成只读检查。");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("1")
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_trace")
            .expect("tool trace should exist")
            .contains("call=locate"));
    }

    #[test]
    fn run_with_options_injects_session_context_for_bound_turns() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-session-context-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = CaptureResponder::new(r#"ACTION: {"type":"final","answer":"ok"}"#);
        let captured = responder.captured.clone();
        let mut config = test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity"));
        config
            .metadata
            .insert("session_id".to_string(), "thread-1".to_string());
        config
            .metadata
            .insert("memory_scope".to_string(), "session".to_string());
        config.metadata.insert(
            "workspace_root".to_string(),
            workspace_root.display().to_string(),
        );
        let mut kernel = ChuangKernel::with_responder(
            config,
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "保持会话上下文".to_string(),
            Vec::new(),
        )
        .expect("session context turn should succeed");

        assert_eq!(turn.result.response.body, "ok");
        let captured = captured.lock().expect("capture lock should succeed");
        assert!(captured[0].prompt.contains("[session-context]"));
        assert!(captured[0].prompt.contains("session_id=thread-1"));
        assert!(captured[0].prompt.contains("memory_scope=session"));
        assert!(captured[0]
            .prompt
            .contains(&format!("workspace_root={}", workspace_root.display())));
    }

    #[test]
    fn run_with_options_injects_recent_conversation_history() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-recent-history-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = CaptureResponder::new(r#"ACTION: {"type":"final","answer":"ok"}"#);
        let captured = responder.captured.clone();
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();
        let history = vec![
            ConversationHistoryItem {
                role: "user".to_string(),
                text: "第一句：我叫这个变量 anchor_alpha".to_string(),
            },
            ConversationHistoryItem {
                role: "assistant".to_string(),
                text: "记住了 anchor_alpha。".to_string(),
            },
        ];

        let mut turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "继续刚才那个".to_string(),
            conversation_history_context_segments(&history),
        )
        .expect("history context turn should succeed");

        insert_conversation_history_metadata(&mut turn, &history);

        assert_eq!(turn.result.response.body, "ok");
        let captured = captured.lock().expect("capture lock should succeed");
        assert!(captured[0].prompt.contains("[recent-conversation-history]"));
        assert!(captured[0].prompt.contains("anchor_alpha"));
        assert_eq!(captured[0].user_input, "继续刚才那个");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("recent_conversation_history_item_count")
                .map(String::as_str),
            Some("2")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("recent_conversation_history_injected")
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn run_with_options_keeps_session_tool_and_recent_history_under_budget_pressure() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-context-pressure-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut runtime = test_runtime(temp_dir.join("memory.db"), temp_dir.join("identity"));
        runtime.context_budget = chuang_agent::context_engine::ContextBudget {
            max_tokens: 3600,
            reserve_system_tokens: 64,
            min_working_tokens: 1,
            max_tool_results: 5,
            max_memory_segments: 20,
        };

        let mut store = SqliteMemoryStore::open(&runtime.db_path)
            .expect("sqlite store should open for pressure seed");
        store
            .put(chuang_agent::memory_store::MemoryRecord {
                id: "pressure-memory".to_string(),
                content: format!(
                    "alpha_session_guard PRESSURE-MEM-KEEP-OUT {}",
                    "x".repeat(6000)
                ),
                metadata: BTreeMap::from([("kind".to_string(), "goal".to_string())]),
                created_at: "2026-04-30T18:00:00Z".to_string(),
                expires_at: None,
            })
            .expect("pressure memory should seed");
        drop(store);

        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "请回忆 alpha_session_guard".to_string(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("thread-pressure".to_string()),
            remember_session: false,
            conversation_history: vec![
                ConversationHistoryItem {
                    role: "user".to_string(),
                    text: "刚才我们确认 workspace_root 不能丢".to_string(),
                },
                ConversationHistoryItem {
                    role: "assistant".to_string(),
                    text: "保留 workspace_root 和 session 上下文".to_string(),
                },
            ],
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
        };

        let (result, _) = run_with_options(&request).expect("run should succeed");

        assert!(result.prompt.contains("[session-context]"));
        assert!(result.prompt.contains("[tool-instructions]"));
        assert!(result.prompt.contains("[recent-conversation-history]"));
        assert!(result.prompt.contains("workspace_root="));
        assert!(result.prompt.contains("thread-pressure"));
        assert!(result.prompt.contains("workspace_root"));
        assert!(!result.prompt.contains("PRESSURE-MEM-KEEP-OUT"));
    }

    #[test]
    fn goal_spec_enters_extra_context_without_polluting_user_input() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-goal-context-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder =
            CaptureResponder::new("ACTION: {\"type\":\"final\",\"answer\":\"目标已进入上下文\"}");
        let captured = responder.captured.clone();
        let mut config = test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity"));
        config.context_budget = Some(goal_context_test_budget());
        let mut kernel = ChuangKernel::with_responder(
            config,
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();
        let goal = chuang_agent::goal_mode::GoalSpec::mainline_mvp("只接入 goal context");

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "保持原始输入".to_string(),
            goal_context_segments(Some(&goal)).expect("goal context should render"),
        )
        .expect("goal context turn should succeed");

        assert_eq!(turn.user_input, "保持原始输入");
        assert_eq!(turn.result.response.body, "目标已进入上下文");
        assert!(
            turn.result.prompt.contains("GOAL_SPEC"),
            "{}",
            turn.result.prompt
        );
        assert!(turn
            .result
            .prompt
            .contains("objective: 只接入 goal context"));
        assert!(!turn.user_input.contains("GOAL_SPEC"));
        let captured = captured.lock().expect("capture lock should succeed");
        assert_eq!(captured[0].user_input, "保持原始输入");
        assert!(captured[0].prompt.contains("GOAL_SPEC"));
    }

    #[test]
    fn missing_goal_spec_keeps_runtime_context_unchanged() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-no-goal-context-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder =
            CaptureResponder::new("ACTION: {\"type\":\"final\",\"answer\":\"无目标上下文\"}");
        let captured = responder.captured.clone();
        let mut config = test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity"));
        config.context_budget = Some(goal_context_test_budget());
        let mut kernel = ChuangKernel::with_responder(
            config,
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "普通输入".to_string(),
            Vec::new(),
        )
        .expect("turn without goal context should succeed");

        assert_eq!(turn.user_input, "普通输入");
        assert!(!turn.result.prompt.contains("GOAL_SPEC"));
        assert!(!turn.result.packed_context_preview.contains("goal-spec"));
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_surface_available")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_surface_governed")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("0")
        );
        let tool_surface_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_surface_json")
            .expect("tool surface json should exist without tool calls");
        assert!(tool_surface_json.contains("\"available\":true"));
        assert!(tool_surface_json.contains("\"governed\":true"));
        assert!(tool_surface_json.contains("\"file_read\""));
        assert!(tool_surface_json.contains("\"list_dir\""));
        let captured = captured.lock().expect("capture lock should succeed");
        assert_eq!(captured[0].user_input, "普通输入");
        assert!(!captured[0].prompt.contains("GOAL_SPEC"));
    }

    #[test]
    fn run_with_options_feeds_governance_rejection_back_to_model() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-reject-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"cat .env","cwd":"."}}"#,
                r#"ACTION: {"type":"final","answer":"工具被治理层拒绝，未执行。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "读取环境文件".to_string(),
            Vec::new(),
        )
        .expect("tool loop should continue after governance rejection");

        assert_eq!(turn.result.response.body, "工具被治理层拒绝，未执行。");
        let tool_calls_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist");
        assert!(tool_calls_json.contains("\"failure_class\":\"governance_rejected\""));
        assert!(tool_calls_json.contains("\"ok\":false"));
        let tool_events_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_events_json")
            .expect("tool events json should exist");
        assert!(tool_events_json.contains("\"failure_class\":\"governance_rejected\""));
        assert!(tool_events_json.contains("\"atomic_tool_name\":\"code_execute\""));
        assert!(governance
            .audit_records()
            .iter()
            .any(|record| record.operation == "tool.code_execute.rejected"));
    }

    #[test]
    fn run_with_options_feeds_protocol_error_back_to_model() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-protocol-error-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_read"}}"#,
                r#"ACTION: {"type":"final","answer":"已修正协议错误。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "读取文件".to_string(),
            Vec::new(),
        )
        .expect("tool loop should continue after protocol error");

        assert_eq!(turn.result.response.body, "已修正协议错误。");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("0")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_protocol_error_count")
                .map(String::as_str),
            Some("1")
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("invalid_action_json"));
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_events_json")
            .expect("tool events json should exist")
            .contains("\"kind\":\"protocol_error\""));
        assert!(!governance
            .audit_records()
            .iter()
            .any(|record| record.operation.starts_with("tool.")));
    }

    #[test]
    fn run_with_options_feeds_plain_text_back_to_model() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-plain-text-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"write_file","path":"notes/plain.txt","content":"hello"}}"#,
                "我先直接说一句普通话",
                r#"ACTION: {"type":"final","answer":"已改成正式 FINAL。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "请先写完再按协议回答".to_string(),
            Vec::new(),
        )
        .expect("tool loop should continue after plain text protocol error");

        assert_eq!(turn.result.response.body, "已改成正式 FINAL。");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_protocol_error_count")
                .map(String::as_str),
            Some("1")
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("plain_text_response"));
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist")
            .contains("\"tool_name\":\"write_file\""));
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_events_json")
            .expect("tool events json should exist")
            .contains("\"kind\":\"protocol_error\""));
    }

    #[test]
    fn run_with_options_falls_back_to_last_plain_text_when_tool_loop_exhausts() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-plain-text-fallback-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"write_file","path":"notes/plain.txt","content":"hello"}}"#,
                "最后我就直接说这句普通话",
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            2,
            ToolExecutionConfig::default(),
            "请先写完再收口".to_string(),
            Vec::new(),
        )
        .expect("plain text fallback should succeed");

        assert_eq!(turn.result.response.body, "最后我就直接说这句普通话");
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("implicit_final_plain_text")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_protocol_error_count")
                .map(String::as_str),
            Some("1")
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("plain_text_response"));
    }

    #[test]
    fn run_with_options_returns_terminal_tool_failure_for_unapproved_desktop_action() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-terminal-tool-failure-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"mouse","x":1910,"y":10}}"#,
                r#"ACTION: {"type":"tool_call","call":{"tool":"mouse","x":1910,"y":10}}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            2,
            ToolExecutionConfig::default(),
            "试试点一下飞书右上角的关闭".to_string(),
            Vec::new(),
        )
        .expect("terminal tool failure should return a user-facing response");

        assert!(turn.result.response.body.contains("本轮没有完成真实动作"));
        assert!(turn.result.response.body.contains("未执行点击、输入或修改"));
        assert!(turn.result.response.body.contains("actuator_unconfigured"));
        assert!(turn
            .result
            .response
            .body
            .contains("action allowlist、治理审批和 operator receipt"));
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("terminal_tool_failure")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_call_count")
                .map(String::as_str),
            Some("2")
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_report_json")
            .expect("tool report json should exist")
            .contains("\"status\":\"terminal_tool_failure\""));
    }

    fn test_runtime(db_path: PathBuf, identity_root: PathBuf) -> RuntimeConfig {
        let mut runtime = RuntimeConfig::new(db_path);
        runtime.identity_memory =
            chuang_agent::runtime_config::IdentityMemoryConfig::HermesDualFile {
                root: identity_root,
                user_max_chars: chuang_agent::hermes_memory::DEFAULT_USER_MEMORY_MAX_CHARS,
                memory_max_chars: chuang_agent::hermes_memory::DEFAULT_HOT_MEMORY_MAX_CHARS,
            };
        runtime.provider = chuang_agent::runtime_config::ProviderConfig::Fake {
            provider_id: "session-test".to_string(),
            model_name: "session-stub".to_string(),
        };
        runtime
    }

    fn unique_record_suffix_for_test() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough")
            .as_nanos();
        format!("{}-{nanos}", std::process::id())
    }

    fn goal_context_test_budget() -> chuang_agent::context_engine::ContextBudget {
        chuang_agent::context_engine::ContextBudget {
            max_tokens: 4096,
            reserve_system_tokens: 64,
            min_working_tokens: 1,
            max_tool_results: 5,
            max_memory_segments: 20,
        }
    }

    #[derive(Debug, Clone)]
    struct SequenceResponder {
        outputs: Arc<Mutex<Vec<String>>>,
    }

    impl SequenceResponder {
        fn new(outputs: Vec<&str>) -> Self {
            Self {
                outputs: Arc::new(Mutex::new(
                    outputs.into_iter().map(|value| value.to_string()).collect(),
                )),
            }
        }
    }

    impl chuang_agent::responder::ProviderAdapterResponder for SequenceResponder {
        fn identity(&self) -> chuang_agent::responder::ProviderIdentity {
            chuang_agent::responder::ProviderIdentity {
                provider_id: "sequence-responder".to_string(),
                model_name: "sequence-model".to_string(),
            }
        }

        fn respond(
            &self,
            request: &chuang_agent::responder::ResponderRequest,
        ) -> chuang_agent::responder::ProviderAdapterResponse {
            let mut outputs = self.outputs.lock().expect("sequence lock should succeed");
            let body = if outputs.is_empty() {
                "FINAL: 默认结束".to_string()
            } else {
                outputs.remove(0)
            };

            chuang_agent::responder::ProviderAdapterResponse {
                body,
                trace: format!(
                    "provider=sequence-responder user_input=《{}》 recall_hits={}",
                    request.user_input, request.recall_hit_count
                ),
                finish_reason: Some("sequence".to_string()),
                extra_meta: std::collections::BTreeMap::new(),
            }
        }
    }

    #[derive(Debug, Clone)]
    struct CapturedResponderRequest {
        prompt: String,
        user_input: String,
    }

    #[derive(Debug, Clone)]
    struct CaptureResponder {
        body: String,
        captured: Arc<Mutex<Vec<CapturedResponderRequest>>>,
    }

    impl CaptureResponder {
        fn new(body: &str) -> Self {
            Self {
                body: body.to_string(),
                captured: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl chuang_agent::responder::ProviderAdapterResponder for CaptureResponder {
        fn identity(&self) -> chuang_agent::responder::ProviderIdentity {
            chuang_agent::responder::ProviderIdentity {
                provider_id: "capture-responder".to_string(),
                model_name: "capture-model".to_string(),
            }
        }

        fn respond(
            &self,
            request: &chuang_agent::responder::ResponderRequest,
        ) -> chuang_agent::responder::ProviderAdapterResponse {
            self.captured
                .lock()
                .expect("capture lock should succeed")
                .push(CapturedResponderRequest {
                    prompt: request.prompt.clone(),
                    user_input: request.user_input.clone(),
                });

            chuang_agent::responder::ProviderAdapterResponse {
                body: self.body.clone(),
                trace: "provider=capture-responder".to_string(),
                finish_reason: Some("capture".to_string()),
                extra_meta: std::collections::BTreeMap::new(),
            }
        }
    }

    fn test_kernel_config(db_path: PathBuf, identity_root: PathBuf) -> ChuangKernelConfig {
        let mut runtime = RuntimeConfig::new(db_path);
        runtime.identity_memory =
            chuang_agent::runtime_config::IdentityMemoryConfig::HermesDualFile {
                root: identity_root,
                user_max_chars: chuang_agent::hermes_memory::DEFAULT_USER_MEMORY_MAX_CHARS,
                memory_max_chars: chuang_agent::hermes_memory::DEFAULT_HOT_MEMORY_MAX_CHARS,
            };

        kernel_config_from_runtime(&runtime).expect("kernel config should build")
    }
}
