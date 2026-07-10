use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
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
use chuang_agent::identity_registry::{
    compatibility_default_identity, IdentityRegistry, IdentityRegistryError,
};
use chuang_agent::memory_admission::{
    preview_chars, MemoryEntryView, TextMemoryAdmission, TextMemoryAdmissionDecision,
};
use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::runtime_config::{RuntimeConfig, SubagentConfig};
use chuang_agent::runtime_event_ledger::{
    InMemoryRuntimeEventLedger, RuntimeEvent, RuntimeEventLedger,
};
use chuang_agent::session_archive::SqliteSessionArchive;
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::subagent_queue::FileSubagentQueueConfig;
use chuang_agent::subagent_report::governance_metadata;
use chuang_agent::subagent_spawner::{
    ContextIsolation, SpawnRequest, SubagentSpawner, SubagentToolPolicy,
};
use chuang_agent::terminal_event::{StepStatus, TerminalEvent};
use chuang_agent::tool_runtime::{
    parse_tool_model_output, ExecutionSlot, MemoryToolContext, PendingApproval, ToolCall,
    ToolExecutionConfig, ToolExecutionRecord, ToolLoopEvent, ToolLoopReport, ToolModelOutput,
    ToolProtocolError, ToolSurfaceStatus,
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
    ensure_cli_channel_metadata(&mut runtime.metadata);
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

    run_governed_turn_with_tools_live(
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
        request.live_guidance_path.as_deref(),
        request.progress_path.as_deref(),
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
    let mut metadata = runtime.metadata.clone();
    ensure_cli_channel_metadata(&mut metadata);
    let dual_file_config = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let identity_snapshot = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;
    let identity_bootstrap_snapshot = load_identity_bootstrap_snapshot(runtime)?;
    let agent_id = identity_bootstrap_snapshot
        .active_identity
        .as_ref()
        .map(|identity| identity.agent_id.clone())
        .unwrap_or_else(|| "chuang-cli".to_string());

    Ok(ChuangKernelConfig {
        agent_id,
        parent_agent_id: None,
        recall_limit: runtime.recall_limit,
        metadata,
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

pub(crate) fn load_identity_bootstrap_snapshot(
    runtime: &RuntimeConfig,
) -> Result<IdentityBootstrapSnapshot, String> {
    let registry_path = &runtime.identity_bootstrap.agents_registry_path;
    let agents_registry_exists = registry_path.exists();
    let agents_registry = read_optional_identity_file(registry_path)?;
    let active_identity = if agents_registry_exists {
        let registry =
            IdentityRegistry::parse(&agents_registry).map_err(format_identity_registry_error)?;
        Some(
            registry
                .select_active(None, configured_runtime_channel(runtime).or(Some("cli")))
                .map_err(format_identity_registry_error)?,
        )
    } else {
        Some(compatibility_default_identity("chuang-cli"))
    };

    Ok(IdentityBootstrapSnapshot {
        soul: read_optional_identity_file(&runtime.identity_bootstrap.soul_path)?,
        soul_exists: runtime.identity_bootstrap.soul_path.exists(),
        story: read_optional_identity_file(&runtime.identity_bootstrap.story_path)?,
        story_exists: runtime.identity_bootstrap.story_path.exists(),
        first_wake: read_optional_identity_file(&runtime.identity_bootstrap.first_wake_path)?,
        first_wake_exists: runtime.identity_bootstrap.first_wake_path.exists(),
        agents_registry,
        agents_registry_exists,
        active_identity,
    })
}

fn format_identity_registry_error(error: IdentityRegistryError) -> String {
    format!("identity_registry_invalid: {error:?}")
}

fn configured_runtime_channel(runtime: &RuntimeConfig) -> Option<&str> {
    runtime
        .metadata
        .get("channel")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn ensure_cli_channel_metadata(metadata: &mut BTreeMap<String, String>) {
    match metadata.get_mut("channel") {
        Some(value) if value.trim().is_empty() => *value = "cli".to_string(),
        Some(_) => {}
        None => {
            metadata.insert("channel".to_string(), "cli".to_string());
        }
    }
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

#[cfg(test)]
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
    run_governed_turn_with_tools_live(
        kernel,
        governance,
        workspace_root,
        max_tool_rounds,
        tool_config,
        original_input,
        extra_context_segments,
        None,
        None,
    )
}

fn run_governed_turn_with_tools_live<S, R, G>(
    kernel: &mut ChuangKernel<S, R>,
    governance: &mut G,
    workspace_root: &Path,
    max_tool_rounds: usize,
    tool_config: ToolExecutionConfig,
    original_input: String,
    extra_context_segments: Vec<ContextSegment>,
    live_guidance_path: Option<&Path>,
    progress_path: Option<&Path>,
) -> Result<ChuangKernelTurn, String>
where
    S: MemoryStore,
    R: chuang_agent::responder::Responder,
    G: Governance,
{
    let mut tool_calls: Vec<ToolExecutionRecord> = Vec::new();
    let mut protocol_errors: Vec<ToolProtocolError> = Vec::new();
    let mut tool_events: Vec<ToolLoopEvent> = Vec::new();
    let mut runtime_event_ledger = InMemoryRuntimeEventLedger::new();
    let mut transcript: Vec<String> = Vec::new();
    let mut turn_context = extra_context_segments;
    turn_context.push(tool_instruction_segment(workspace_root));
    let execution_slot = ExecutionSlot::generic_agent_mvp(tool_config);
    let mut current_input = original_input.clone();
    let mut live_guidance_cursor = 0usize;
    let mut last_turn: Option<ChuangKernelTurn> = None;
    let mut last_plain_text_answer: Option<String> = None;
    write_terminal_event(
        progress_path,
        &TerminalEvent::TurnStarted {
            input_preview: truncate_history_text(&original_input, 120),
            max_tool_rounds,
        },
    )?;
    write_terminal_event(
        progress_path,
        &TerminalEvent::StepStarted {
            title: "prepare context".to_string(),
            detail: Some(
                "identity, recent history, working context, and tool instructions".to_string(),
            ),
        },
    )?;
    write_terminal_event(
        progress_path,
        &TerminalEvent::StepFinished {
            title: "prepare context".to_string(),
            status: StepStatus::Ok,
            detail: Some(format!("segments={}", turn_context.len())),
        },
    )?;
    if should_auto_observe_desktop(&original_input) {
        write_terminal_event(
            progress_path,
            &TerminalEvent::ToolStarted {
                round: 1,
                tool: "locate".to_string(),
                summary: Some("auto desktop observation before model call".to_string()),
            },
        )?;
        let call = ToolCall::Locate {
            target: Some("screen".to_string()),
        };
        let outcome = execution_slot.execute_or_reject_with_governance_and_ledger(
            &mut runtime_event_ledger,
            "cli",
            "pre-model",
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
        write_terminal_event(
            progress_path,
            &TerminalEvent::ToolFinished {
                round: 1,
                tool: "locate".to_string(),
                ok: tool_calls.last().map(|record| record.ok).unwrap_or(false),
                decision: tool_events.last().and_then(|event| event.decision.clone()),
                summary: tool_calls
                    .last()
                    .map(|record| record.summary.clone())
                    .unwrap_or_default(),
            },
        )?;
        current_input = format!(
            "{}\nreq:{}\ntool:{}\nFINAL:<最终答复>",
            read_only_capability_banner(),
            original_input,
            transcript.join("\n")
        );
    }

    for round_index in 0..max_tool_rounds {
        if let Some(guidance) =
            read_new_live_guidance(live_guidance_path, &mut live_guidance_cursor)?
        {
            transcript.push(format!(
                "operator_guidance {}",
                guidance.replace('\n', " | ")
            ));
            current_input = inject_live_guidance_into_prompt(&current_input, &guidance);
            write_terminal_event(
                progress_path,
                &TerminalEvent::GuidanceInjected {
                    round: round_index + 1,
                    chars: guidance.chars().count(),
                },
            )?;
        }
        write_terminal_event(
            progress_path,
            &TerminalEvent::ModelStarted {
                round: round_index + 1,
            },
        )?;
        let mut turn = kernel
            .run_governed_turn_with_extra_context(
                current_input.clone(),
                governance,
                turn_context.clone(),
            )
            .map_err(|e| format!("{e:?}"))?;
        let body = turn.result.response.body.trim().to_string();
        write_terminal_event(
            progress_path,
            &TerminalEvent::ModelFinished {
                round: round_index + 1,
                finish: turn
                    .result
                    .response
                    .meta
                    .finish_reason
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                chars: body.chars().count(),
            },
        )?;

        match parse_tool_model_output(&body) {
            ToolModelOutput::FinalAnswer(final_answer) => {
                if tool_calls.is_empty() && should_require_action_for_local_task(&original_input) {
                    let error = ToolProtocolError {
                        code: "missing_required_action".to_string(),
                        message: "local task requires at least one ACTION tool call before FINAL"
                            .to_string(),
                        raw: final_answer,
                    };
                    transcript.push(format!(
                        "protocol_error code={} message={} rejected_unverified_answer=true",
                        error.code, error.message
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
                    write_terminal_event(
                        progress_path,
                        &TerminalEvent::ProtocolError {
                            round: round_index + 1,
                            code: "missing_required_action".to_string(),
                        },
                    )?;
                    current_input = tool_protocol_repair_prompt(
                        &original_input,
                        &transcript,
                        should_require_action_for_local_task(&original_input)
                            && tool_calls.is_empty(),
                    );
                    last_turn = Some(turn);
                    continue;
                }
                turn.result.response.body = final_answer;
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::AnswerReady {
                        chars: turn.result.response.body.chars().count(),
                        truncated: false,
                        snapshot_path: None,
                    },
                )?;
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
                    insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
                }
                return Ok(turn);
            }
            ToolModelOutput::ToolCall(call) => {
                let tool_name = tool_call_name(&call).to_string();
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::ToolStarted {
                        round: round_index + 1,
                        tool: tool_name,
                        summary: None,
                    },
                )?;
                let task_id = format!("{}:tool:{}", turn.turn_id, tool_calls.len() + 1);
                let outcome = execution_slot.execute_or_reject_with_governance_and_ledger(
                    &mut runtime_event_ledger,
                    "cli",
                    turn.turn_id.clone(),
                    workspace_root,
                    governance,
                    &call,
                    "cli",
                    task_id,
                )?;
                let pending_approval = outcome.pending_approval.clone();
                let record = outcome.record;
                transcript.push(tool_evidence_for_model(
                    &record,
                    &risk_decision_label(&outcome.decision),
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
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::ToolFinished {
                        round: round_index + 1,
                        tool: tool_calls
                            .last()
                            .map(|record| tool_call_name(&record.call).to_string())
                            .unwrap_or_default(),
                        ok: tool_calls.last().map(|record| record.ok).unwrap_or(false),
                        decision: tool_events.last().and_then(|event| event.decision.clone()),
                        summary: tool_calls
                            .last()
                            .map(|record| record.summary.clone())
                            .unwrap_or_default(),
                    },
                )?;
                if let Some(pending) = pending_approval {
                    return finish_pending_approval_turn(
                        turn,
                        &original_input,
                        workspace_root,
                        round_index + 1,
                        &tool_calls,
                        &protocol_errors,
                        &tool_events,
                        &transcript,
                        &runtime_event_ledger,
                        &pending,
                    );
                }
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
                    insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
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
                    "原始用户请求:\n{}\n\n工具执行记录:\n{}\n\n请继续。本次回复只能输出一个结构：一条 ACTION 或一条 FINAL；不要把 ACTION 和 FINAL 粘在一起。若已完成，请输出 FINAL: <最终答复>。",
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
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::ProtocolError {
                        round: round_index + 1,
                        code: protocol_errors
                            .last()
                            .map(|error| error.code.clone())
                            .unwrap_or_default(),
                    },
                )?;
                current_input = tool_protocol_repair_prompt(
                    &original_input,
                    &transcript,
                    should_require_action_for_local_task(&original_input) && tool_calls.is_empty(),
                );
                last_turn = Some(turn);
                continue;
            }
            ToolModelOutput::PlainText(_) => {
                let requires_local_action =
                    should_require_action_for_local_task(&original_input) && tool_calls.is_empty();
                if tool_calls.is_empty() && protocol_errors.is_empty() && round_index == 0 {
                    if !requires_local_action {
                        insert_tool_surface_metadata(&mut turn, workspace_root)?;
                        return Ok(turn);
                    }
                }
                if !requires_local_action {
                    last_plain_text_answer = Some(body.clone());
                }
                let raw = body.clone();
                let error = ToolProtocolError {
                    code: if requires_local_action {
                        "missing_required_action".to_string()
                    } else {
                        "plain_text_response".to_string()
                    },
                    message: if requires_local_action {
                        "local task requires at least one ACTION tool call before FINAL".to_string()
                    } else {
                        "tool loop requires ACTION or FINAL; plain text is not accepted".to_string()
                    },
                    raw,
                };
                if requires_local_action {
                    transcript.push(format!(
                        "protocol_error code={} message={} rejected_unverified_answer=true",
                        error.code, error.message
                    ));
                } else {
                    transcript.push(format!(
                        "protocol_error code={} message={} raw={}",
                        error.code, error.message, error.raw
                    ));
                }
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
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::ProtocolError {
                        round: round_index + 1,
                        code: protocol_errors
                            .last()
                            .map(|error| error.code.clone())
                            .unwrap_or_default(),
                    },
                )?;
                current_input = tool_protocol_repair_prompt(
                    &original_input,
                    &transcript,
                    should_require_action_for_local_task(&original_input) && tool_calls.is_empty(),
                );
                last_turn = Some(turn);
                continue;
            }
        }
    }

    if let Some(answer) = last_plain_text_answer {
        if let Some(mut turn) = last_turn.take() {
            if should_require_action_for_local_task(&original_input) && tool_calls.is_empty() {
                turn.result.response.body =
                    missing_required_action_exhausted_answer(&original_input, max_tool_rounds);
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
                    "missing_required_action",
                )?;
                insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
                return Ok(turn);
            }
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
            insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
            return Ok(turn);
        }
    }

    if let Some(mut turn) = last_turn {
        if should_require_action_for_local_task(&original_input) && tool_calls.is_empty() {
            turn.result.response.body =
                missing_required_action_exhausted_answer(&original_input, max_tool_rounds);
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
                "missing_required_action",
            )?;
            insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
            return Ok(turn);
        }
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
            insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
            return Ok(turn);
        }

        if let Some(guidance) =
            read_new_live_guidance(live_guidance_path, &mut live_guidance_cursor)?
        {
            transcript.push(format!(
                "operator_guidance {}",
                guidance.replace('\n', " | ")
            ));
            write_terminal_event(
                progress_path,
                &TerminalEvent::GuidanceInjected {
                    round: max_tool_rounds + 1,
                    chars: guidance.chars().count(),
                },
            )?;
        }

        write_terminal_event(
            progress_path,
            &TerminalEvent::StepStarted {
                title: "finalize answer".to_string(),
                detail: Some("tool execution disabled after round limit".to_string()),
            },
        )?;
        match attempt_tool_loop_finalization(
            kernel,
            governance,
            &original_input,
            &turn_context,
            &transcript,
            max_tool_rounds + 1,
            progress_path,
        ) {
            Ok(ToolLoopFinalization::Completed {
                mut turn,
                answer,
                response_kind,
            }) => {
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
                    "completed_after_tool_limit",
                )?;
                insert_tool_finalization_metadata(
                    &mut turn,
                    "succeeded",
                    response_kind,
                    false,
                    max_tool_rounds + 1,
                );
                insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::StepFinished {
                        title: "finalize answer".to_string(),
                        status: StepStatus::Ok,
                        detail: Some(format!("accepted {response_kind} response")),
                    },
                )?;
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::AnswerReady {
                        chars: turn.result.response.body.chars().count(),
                        truncated: false,
                        snapshot_path: None,
                    },
                )?;
                return Ok(turn);
            }
            Ok(ToolLoopFinalization::Rejected {
                final_turn,
                status,
                detail,
                blocked_tool_call,
            }) => {
                turn = final_turn;
                insert_tool_finalization_metadata(
                    &mut turn,
                    status,
                    "rejected",
                    blocked_tool_call,
                    max_tool_rounds + 1,
                );
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::StepFinished {
                        title: "finalize answer".to_string(),
                        status: StepStatus::Failed,
                        detail: Some(detail),
                    },
                )?;
            }
            Err(error) => {
                insert_tool_finalization_metadata(
                    &mut turn,
                    "provider_error",
                    "unavailable",
                    false,
                    max_tool_rounds + 1,
                );
                write_terminal_event(
                    progress_path,
                    &TerminalEvent::StepFinished {
                        title: "finalize answer".to_string(),
                        status: StepStatus::Failed,
                        detail: Some(
                            "provider call failed; preserved prior tool evidence".to_string(),
                        ),
                    },
                )?;
                turn.result.response.meta.extra.insert(
                    "tool_finalization_error".to_string(),
                    truncate_history_text(&error, 240),
                );
            }
        }

        turn.result.response.body = tool_loop_exhausted_answer(
            &original_input,
            max_tool_rounds,
            tool_calls.len(),
            protocol_errors.len(),
        );
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
            "tool_loop_exhausted",
        )?;
        insert_runtime_event_ledger_metadata(&mut turn, &runtime_event_ledger)?;
        write_terminal_event(
            progress_path,
            &TerminalEvent::AnswerReady {
                chars: turn.result.response.body.chars().count(),
                truncated: false,
                snapshot_path: None,
            },
        )?;
        return Ok(turn);
    }

    Err("tool_loop_exhausted: model did not produce FINAL response".to_string())
}

enum ToolLoopFinalization {
    Completed {
        turn: ChuangKernelTurn,
        answer: String,
        response_kind: &'static str,
    },
    Rejected {
        final_turn: ChuangKernelTurn,
        status: &'static str,
        detail: String,
        blocked_tool_call: bool,
    },
}

#[allow(clippy::too_many_arguments)]
fn attempt_tool_loop_finalization<S, R, G>(
    kernel: &mut ChuangKernel<S, R>,
    governance: &mut G,
    original_input: &str,
    turn_context: &[ContextSegment],
    transcript: &[String],
    model_round: usize,
    progress_path: Option<&Path>,
) -> Result<ToolLoopFinalization, String>
where
    S: MemoryStore,
    R: chuang_agent::responder::Responder,
    G: Governance,
{
    let mut finalization_context = turn_context
        .iter()
        .filter(|segment| segment.id != "tool-instructions")
        .cloned()
        .collect::<Vec<_>>();
    finalization_context.push(tool_finalization_context_segment());
    let finalization_input = tool_finalization_prompt(original_input, transcript);

    write_terminal_event(
        progress_path,
        &TerminalEvent::ModelStarted { round: model_round },
    )?;
    let turn = kernel
        .run_governed_turn_with_extra_context(finalization_input, governance, finalization_context)
        .map_err(|error| format!("tool_finalization_provider_failed: {error:?}"))?;
    let body = turn.result.response.body.trim().to_string();
    write_terminal_event(
        progress_path,
        &TerminalEvent::ModelFinished {
            round: model_round,
            finish: turn
                .result
                .response
                .meta
                .finish_reason
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            chars: body.chars().count(),
        },
    )?;

    match parse_tool_model_output(&body) {
        ToolModelOutput::FinalAnswer(answer) if !answer.trim().is_empty() => {
            Ok(ToolLoopFinalization::Completed {
                turn,
                answer: answer.trim().to_string(),
                response_kind: "final",
            })
        }
        ToolModelOutput::PlainText(answer) if !answer.trim().is_empty() => {
            Ok(ToolLoopFinalization::Completed {
                turn,
                answer: answer.trim().to_string(),
                response_kind: "plain_text",
            })
        }
        ToolModelOutput::ToolCall(call) => Ok(ToolLoopFinalization::Rejected {
            final_turn: turn,
            status: "rejected_tool_call",
            detail: format!(
                "blocked unexpected {} request; no additional tool was executed",
                tool_call_name(&call)
            ),
            blocked_tool_call: true,
        }),
        ToolModelOutput::ProtocolError(error) => Ok(ToolLoopFinalization::Rejected {
            final_turn: turn,
            status: "protocol_error",
            detail: format!("invalid final response: {}", error.code),
            blocked_tool_call: false,
        }),
        ToolModelOutput::FinalAnswer(_) | ToolModelOutput::PlainText(_) => {
            Ok(ToolLoopFinalization::Rejected {
                final_turn: turn,
                status: "empty_response",
                detail: "model returned an empty final response".to_string(),
                blocked_tool_call: false,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_pending_approval_turn(
    mut turn: ChuangKernelTurn,
    original_input: &str,
    workspace_root: &Path,
    rounds: usize,
    tool_calls: &[ToolExecutionRecord],
    protocol_errors: &[ToolProtocolError],
    tool_events: &[ToolLoopEvent],
    transcript: &[String],
    runtime_event_ledger: &InMemoryRuntimeEventLedger,
    pending: &PendingApproval,
) -> Result<ChuangKernelTurn, String> {
    let pending_path = persist_pending_approval(workspace_root, pending)?;
    turn.user_input = original_input.to_string();
    turn.result.response.body = format!(
        "本轮已暂停，等待明确审批。\napproval_id={}\npending_file={}\n未执行目标动作。",
        pending.approval_id,
        pending_path.display()
    );
    insert_tool_surface_metadata(&mut turn, workspace_root)?;
    insert_tool_metadata_with_status(
        &mut turn,
        workspace_root,
        rounds,
        tool_calls,
        protocol_errors,
        tool_events,
        transcript,
        "human_input_required",
    )?;
    insert_runtime_event_ledger_metadata(&mut turn, runtime_event_ledger)?;
    let extra = &mut turn.result.response.meta.extra;
    extra.insert(
        "tool_loop_status".to_string(),
        "human_input_required".to_string(),
    );
    extra.insert("human_input_required".to_string(), "true".to_string());
    extra.insert(
        "pending_approval_id".to_string(),
        pending.approval_id.clone(),
    );
    extra.insert(
        "pending_approval_path".to_string(),
        pending_path.display().to_string(),
    );
    extra.insert(
        "pending_approval_policy_marker".to_string(),
        pending.policy_marker.clone(),
    );
    Ok(turn)
}

fn persist_pending_approval(
    workspace_root: &Path,
    pending: &PendingApproval,
) -> Result<PathBuf, String> {
    let normalized_root = fs::canonicalize(workspace_root).map_err(|error| {
        format!(
            "pending_approval_workspace_invalid path={} error={error}",
            workspace_root.display()
        )
    })?;
    let relative = PathBuf::from(".chuang")
        .join("runtime")
        .join("pending-approvals")
        .join(format!("{}.json", pending.approval_id));
    let candidate = chuang_agent::path_utils::resolve_candidate_preserving_existing_symlinks(
        &workspace_root.join(relative),
    )?;
    if !candidate.starts_with(&normalized_root) {
        return Err("pending_approval_path_outside_workspace".to_string());
    }
    let parent = candidate
        .parent()
        .ok_or_else(|| "pending_approval_parent_invalid".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("pending_approval_dir_create_failed: {error}"))?;
    let bytes = serde_json::to_vec_pretty(pending)
        .map_err(|_| "pending_approval_serialize_failed".to_string())?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    match options.open(&candidate) {
        Ok(mut file) => {
            file.write_all(&bytes)
                .map_err(|error| format!("pending_approval_write_failed: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("pending_approval_sync_failed: {error}"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(&candidate)
                .map_err(|read_error| format!("pending_approval_read_failed: {read_error}"))?;
            if existing != bytes {
                return Err("pending_approval_file_conflict".to_string());
            }
        }
        Err(error) => return Err(format!("pending_approval_create_failed: {error}")),
    }
    Ok(candidate)
}

fn write_terminal_event(progress_path: Option<&Path>, event: &TerminalEvent) -> Result<(), String> {
    let Some(path) = progress_path else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("progress_dir_create_failed: {e}"))?;
    }
    let event = serde_json::json!({
        "schema_version": 2,
        "ts_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0),
        "event": event,
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("progress_open_failed: {e}"))?;
    writeln!(file, "{event}").map_err(|e| format!("progress_write_failed: {e}"))
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

fn read_new_live_guidance(
    path: Option<&Path>,
    cursor: &mut usize,
) -> Result<Option<String>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| {
        format!(
            "live_guidance_read_failed path={} error={e}",
            path.display()
        )
    })?;
    if content.len() <= *cursor {
        return Ok(None);
    }
    let new_content = content[*cursor..].trim().to_string();
    *cursor = content.len();
    if new_content.is_empty() {
        Ok(None)
    } else {
        Ok(Some(new_content))
    }
}

fn inject_live_guidance_into_prompt(current_input: &str, guidance: &str) -> String {
    format!(
        "{}\n\n[live-operator-guidance]\n{}\nApply this correction immediately in the current task. If it changes direction, stop the old direction at the next safe point and continue with this guidance.\n",
        current_input,
        guidance.trim()
    )
}

fn terminal_tool_failure_answer(record: &ToolExecutionRecord) -> String {
    let tool_name = tool_call_name(&record.call);
    let decision = record.decision.as_deref().unwrap_or("unknown");
    let failure_class = record.failure_class.as_deref().unwrap_or("tool_failed");
    format!(
        "本轮没有完成真实动作。\n动作：{tool_name}\n结果：未执行点击、输入或修改。\n拦截原因：{failure_class}; {summary}\n治理决策：{decision}\n下一步：如果这是普通桌面动作，补齐 actuator adapter、live gate 和 action allowlist 后应直接执行，不需要人工审批；只有删除/清理/重置/卸载/支付/验证码/服务或网络变更/密钥访问等高危操作才询问老爸或拒绝。",
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

fn should_require_action_for_local_task(user_input: &str) -> bool {
    let text = user_input.to_lowercase();
    let asks_runtime_health_check = [
        "体检",
        "自检",
        "健康检查",
        "健康度",
        "诊断自己",
        "检查自己",
        "self-check",
        "self check",
        "health check",
        "diagnose yourself",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let asks_local_surface = [
        "桌面",
        "文件夹",
        "目录",
        "文件",
        "项目",
        "命令",
        "终端",
        "窗口",
        "应用",
        "软件",
        "浏览器",
        "git",
        "日志",
        "状态",
        "进程",
        "服务",
        "容器",
        "仓库",
        "desktop",
        "folder",
        "directory",
        "file",
        "project",
        "command",
        "terminal",
        "window",
        "app",
        "browser",
        "repo",
        "repository",
        "status",
        "log",
        "process",
        "service",
        "container",
        "docker",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    let asks_local_action = [
        "看",
        "查看",
        "检查",
        "查",
        "读",
        "列",
        "列出",
        "找",
        "搜索",
        "确认",
        "新建",
        "创建",
        "建一个",
        "写入",
        "保存",
        "修改",
        "运行",
        "执行",
        "打开",
        "点击",
        "输入",
        "git ",
        "git status",
        "git diff",
        "git log",
        "ls",
        "cat",
        "grep",
        "rg",
        "find",
        "check",
        "inspect",
        "show",
        "read",
        "list",
        "search",
        "mkdir",
        "create",
        "write",
        "save",
        "modify",
        "run",
        "execute",
        "open",
        "click",
        "type",
    ]
    .iter()
    .any(|needle| text.contains(needle));

    asks_runtime_health_check || (asks_local_surface && asks_local_action)
}

fn tool_protocol_repair_prompt(
    original_input: &str,
    transcript: &[String],
    must_call_tool_before_final: bool,
) -> String {
    if must_call_tool_before_final {
        format!(
            "原始用户请求:\n{}\n\n工具协议错误:\n{}\n\n这是本地/仓库/终端任务，你还没有调用任何工具，所以不能输出 FINAL，也不能说已完成或没能力。下一条回复必须只输出一条正式 ACTION JSON 工具调用，按任务选择 code_execute、list_dir、file_read 或 file_write。示例：ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"code_execute\",\"command\":\"git status --short\",\"cwd\":\".\"}}}}。本次回复只能输出一个 ACTION，不要解释，不要把 ACTION 和 FINAL 粘在一起。",
            original_input,
            transcript.join("\n")
        )
    } else {
        format!(
            "原始用户请求:\n{}\n\n工具协议错误:\n{}\n\n请修正为正式 ACTION JSON，或输出 FINAL: <最终答复>。本次回复只能输出一个结构，不要把 ACTION 和 FINAL 粘在一起。",
            original_input,
            transcript.join("\n")
        )
    }
}

fn missing_required_action_exhausted_answer(
    original_input: &str,
    max_tool_rounds: usize,
) -> String {
    format!(
        "本轮没有完成真实本地动作：模型连续 {max_tool_rounds} 轮没有调用工具。原始请求是：{original_input}。请重试，或把动作拆成更直接的命令。"
    )
}

fn tool_loop_exhausted_answer(
    original_input: &str,
    max_tool_rounds: usize,
    tool_call_count: usize,
    protocol_error_count: usize,
) -> String {
    format!(
        "本轮工具执行已经停止，但最终总结仍未生成。\n\n模型跑满了 {max_tool_rounds} 轮工具预算，系统随后额外进行了一次禁用工具的强制收口，仍没有得到可用答复。\n已执行工具：{tool_call_count} 次。协议修复：{protocol_error_count} 次。\n原始请求：{original_input}\n\n建议：直接重试本轮。系统不会自动重复执行已经完成的工具动作。"
    )
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

fn insert_runtime_event_ledger_metadata(
    turn: &mut ChuangKernelTurn,
    ledger: &InMemoryRuntimeEventLedger,
) -> Result<(), String> {
    let mut events = turn
        .result
        .response
        .meta
        .extra
        .get("runtime_event_ledger_json")
        .and_then(|raw| serde_json::from_str::<Vec<RuntimeEvent>>(raw).ok())
        .unwrap_or_default();
    events.extend(
        ledger
            .list()
            .map_err(|e| format!("runtime_event_ledger_read_failed: {e}"))?,
    );
    let extra = &mut turn.result.response.meta.extra;
    extra.insert(
        "runtime_event_ledger_available".to_string(),
        "true".to_string(),
    );
    extra.insert("runtime_event_count".to_string(), events.len().to_string());
    extra.insert(
        "runtime_event_ledger_json".to_string(),
        serde_json::to_string(&events)
            .map_err(|e| format!("runtime_event_ledger_json_failed: {e}"))?,
    );
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

fn tool_finalization_context_segment() -> ContextSegment {
    let content = "TOOL FINALIZATION MODE\n\
This is a one-shot answer finalization pass after the tool round budget was exhausted.\n\
All tools are disabled. Do not output ACTION, TOOL_CALL, commands, or requests for another tool.\n\
Use the supplied original request and completed tool evidence to answer the user now.\n\
Return either FINAL: <answer>, ACTION with type=final, or a direct plain-text final answer.\n\
Do not claim an action succeeded unless the supplied evidence says it succeeded."
        .to_string();
    let now = chrono::Utc::now();
    ContextSegment {
        id: "tool-finalization-instructions".to_string(),
        source: SegmentSource::Identity,
        tokens: Some(content.chars().count().min(u32::MAX as usize) as u32),
        priority: 255,
        created_at: now,
        last_accessed: now,
        metadata: std::collections::HashMap::from([(
            "kind".to_string(),
            "tool_finalization".to_string(),
        )]),
        content,
    }
}

fn tool_finalization_prompt(original_input: &str, transcript: &[String]) -> String {
    let evidence = if transcript.is_empty() {
        "none".to_string()
    } else {
        truncate_history_text(&transcript.join("\n"), 8_000)
    };
    format!(
        "[tool-finalization]\n\
工具预算已经耗尽，工具现已禁用。你不能再调用、重放或建议系统自动执行任何工具。\n\
请仅根据下面的原始请求和已完成证据，立即给用户最终答复。\n\
如果任务已经完成，简洁报告结果；如果证据显示未完成，明确说明未完成项。不要要求再增加工具轮次。\n\n\
原始请求:\n{original_input}\n\n\
已完成工具证据:\n{evidence}\n\n\
现在只输出最终答复。"
    )
}

fn insert_tool_finalization_metadata(
    turn: &mut ChuangKernelTurn,
    status: &str,
    response_kind: &str,
    blocked_tool_call: bool,
    model_call_count: usize,
) {
    let extra = &mut turn.result.response.meta.extra;
    extra.insert(
        "tool_finalization_attempted".to_string(),
        "true".to_string(),
    );
    extra.insert("tool_finalization_status".to_string(), status.to_string());
    extra.insert(
        "tool_finalization_response_kind".to_string(),
        response_kind.to_string(),
    );
    extra.insert(
        "tool_finalization_tool_call_blocked".to_string(),
        blocked_tool_call.to_string(),
    );
    extra.insert(
        "tool_model_call_count".to_string(),
        model_call_count.to_string(),
    );
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

fn remember_turn_if_requested<R>(
    options: &CliOptions,
    kernel: &mut ChuangKernel<SqliteMemoryStore, R>,
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
    R: chuang_agent::responder::Responder,
{
    let mut records = RememberedRecords::default();
    let mut pending_session_archive: Option<PendingSessionArchive> = None;
    let mut remember_commit = RememberCommitTracker::new(request);
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
        match kernel.prepare_session_turn_memory(&turn, session_id) {
            Ok(prepared) => {
                pending_session_archive = Some(PendingSessionArchive::Prepared {
                    session_id: session_id.clone(),
                    summary: prepared.record,
                    record_id: prepared.receipt.record_id,
                    metadata: SessionMemoryWriteMetadata::Prepared {
                        compacted: prepared.receipt.compacted,
                        attempted_chars: prepared.receipt.attempted_chars,
                        stored_chars: prepared.receipt.stored_chars,
                    },
                });
            }
            Err(chuang_agent::chuang_kernel::ChuangKernelMemoryError::HardLimitExceeded {
                limit_chars,
                attempted_chars,
                existing_entries,
            }) => {
                pending_session_archive = Some(PendingSessionArchive::HardLimitExceeded {
                    session_id: session_id.clone(),
                    metadata: SessionMemoryWriteMetadata::HardLimitExceeded {
                        error: format!(
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
                    },
                });
            }
            Err(err) => return Err(format_kernel_memory_error(err)),
        }
    }

    let pending_identity_write = if request.remember_identity {
        Some(prepare_identity_turn_write(options, &turn)?)
    } else {
        None
    };

    let pending_experience_write = if request.remember_experience {
        Some(prepare_experience_turn_write(options, &turn)?)
    } else {
        None
    };

    let pending_subagent_dispatch = if request.dispatch_subagent {
        Some(prepare_subagent_dispatch(options, &turn)?)
    } else {
        None
    };

    if let Some(pending) = pending_session_archive {
        records.session_record_id = pending.commit(options, &mut turn)?;
        remember_commit.mark_applied(RememberWorkflowStep::Archive);
    }

    if request.remember_identity {
        match pending_identity_write
            .expect("identity write should be prepared when requested")
            .commit()
        {
            Ok(record_id) => {
                records.identity_record_id = Some(record_id);
                remember_commit.mark_applied(RememberWorkflowStep::Identity);
            }
            Err(error) => {
                if !remember_commit.has_applied_steps() {
                    return Err(error);
                }
                remember_commit.mark_failed(RememberWorkflowStep::Identity, error);
                insert_session_memory_metadata(&mut turn, request, &records);
                remember_commit.apply_metadata(&mut turn);
                return Ok((turn.result, records));
            }
        }
    }

    if request.remember_experience {
        match pending_experience_write
            .expect("experience write should be prepared when requested")
            .commit()
        {
            Ok(record_id) => {
                records.experience_record_id = Some(record_id);
                remember_commit.mark_applied(RememberWorkflowStep::Experience);
            }
            Err(error) => {
                if !remember_commit.has_applied_steps() {
                    return Err(error);
                }
                remember_commit.mark_failed(RememberWorkflowStep::Experience, error);
                insert_session_memory_metadata(&mut turn, request, &records);
                remember_commit.apply_metadata(&mut turn);
                return Ok((turn.result, records));
            }
        }
    }

    if request.dispatch_subagent {
        match pending_subagent_dispatch
            .expect("subagent dispatch should be prepared when requested")
            .commit()
        {
            Ok(receipt) => {
                records.subagent_dispatch_run_id = Some(receipt.run_id.0);
                records.subagent_dispatch_agent_id = Some(receipt.agent_id.0);
                records.subagent_dispatch_task_id = Some(turn.report.task_id.0.clone());
                remember_commit.mark_applied(RememberWorkflowStep::QueuedDispatch);
            }
            Err(error) => {
                if !remember_commit.has_applied_steps() {
                    return Err(error);
                }
                remember_commit.mark_failed(RememberWorkflowStep::QueuedDispatch, error);
                insert_session_memory_metadata(&mut turn, request, &records);
                remember_commit.apply_metadata(&mut turn);
                return Ok((turn.result, records));
            }
        }
    }

    insert_session_memory_metadata(&mut turn, request, &records);
    remember_commit.apply_metadata(&mut turn);
    Ok((turn.result, records))
}

enum PendingSessionArchive {
    Prepared {
        session_id: String,
        summary: MemoryRecord,
        record_id: String,
        metadata: SessionMemoryWriteMetadata,
    },
    HardLimitExceeded {
        session_id: String,
        metadata: SessionMemoryWriteMetadata,
    },
}

enum SessionMemoryWriteMetadata {
    Prepared {
        compacted: bool,
        attempted_chars: usize,
        stored_chars: usize,
    },
    HardLimitExceeded {
        error: String,
    },
}

impl PendingSessionArchive {
    fn commit(
        self,
        options: &CliOptions,
        turn: &mut ChuangKernelTurn,
    ) -> Result<Option<String>, String> {
        match self {
            Self::Prepared {
                session_id,
                summary,
                record_id,
                metadata,
            } => {
                archive_session_turn(options, turn, &session_id, Some(summary))?;
                metadata.apply(turn);
                Ok(Some(record_id))
            }
            Self::HardLimitExceeded {
                session_id,
                metadata,
            } => {
                archive_session_turn(options, turn, &session_id, None)?;
                metadata.apply(turn);
                Ok(None)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RememberWorkflowStep {
    Archive,
    Identity,
    Experience,
    QueuedDispatch,
}

impl RememberWorkflowStep {
    fn as_str(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::Identity => "identity",
            Self::Experience => "experience",
            Self::QueuedDispatch => "queued_dispatch",
        }
    }

    fn status_key(self) -> &'static str {
        match self {
            Self::Archive => "remember_archive_status",
            Self::Identity => "remember_identity_status",
            Self::Experience => "remember_experience_status",
            Self::QueuedDispatch => "remember_queued_dispatch_status",
        }
    }
}

struct RememberCommitTracker {
    requested_steps: Vec<RememberWorkflowStep>,
    applied_steps: Vec<RememberWorkflowStep>,
    failed_step: Option<RememberWorkflowStep>,
    failure_message: Option<String>,
    step_statuses: BTreeMap<&'static str, &'static str>,
}

impl RememberCommitTracker {
    fn new(request: &RunCliRequest) -> Self {
        let mut requested_steps = Vec::new();
        let mut step_statuses = BTreeMap::new();
        for (requested, step) in [
            (request.remember_session, RememberWorkflowStep::Archive),
            (request.remember_identity, RememberWorkflowStep::Identity),
            (
                request.remember_experience,
                RememberWorkflowStep::Experience,
            ),
            (
                request.dispatch_subagent,
                RememberWorkflowStep::QueuedDispatch,
            ),
        ] {
            if requested {
                requested_steps.push(step);
                step_statuses.insert(step.status_key(), "pending");
            } else {
                step_statuses.insert(step.status_key(), "not_requested");
            }
        }
        Self {
            requested_steps,
            applied_steps: Vec::new(),
            failed_step: None,
            failure_message: None,
            step_statuses,
        }
    }

    fn mark_applied(&mut self, step: RememberWorkflowStep) {
        self.step_statuses.insert(step.status_key(), "applied");
        if !self.applied_steps.contains(&step) {
            self.applied_steps.push(step);
        }
    }

    fn mark_failed(&mut self, step: RememberWorkflowStep, error: String) {
        self.step_statuses.insert(step.status_key(), "failed");
        self.failed_step = Some(step);
        self.failure_message = Some(error);
    }

    fn pending_steps(&self) -> Vec<RememberWorkflowStep> {
        self.requested_steps
            .iter()
            .copied()
            .filter(|step| !self.applied_steps.contains(step) && self.failed_step != Some(*step))
            .collect()
    }

    fn has_applied_steps(&self) -> bool {
        !self.applied_steps.is_empty()
    }

    fn overall_status(&self) -> &'static str {
        if self.requested_steps.is_empty() {
            "not_requested"
        } else if self.failed_step.is_some() {
            "partial_success"
        } else {
            "complete"
        }
    }

    fn blind_retry_safe(&self) -> bool {
        self.requested_steps.is_empty()
    }

    fn apply_metadata(&self, turn: &mut ChuangKernelTurn) {
        let applied_steps = self
            .applied_steps
            .iter()
            .map(|step| step.as_str().to_string())
            .collect::<Vec<_>>();
        let pending_steps = self
            .pending_steps()
            .into_iter()
            .map(|step| step.as_str().to_string())
            .collect::<Vec<_>>();
        let failed_step = self.failed_step.map(RememberWorkflowStep::as_str);
        let failure_message = self.failure_message.clone().unwrap_or_default();

        let extra = &mut turn.result.response.meta.extra;
        extra.insert(
            "remember_commit_status".to_string(),
            self.overall_status().to_string(),
        );
        extra.insert(
            "remember_blind_retry_safe".to_string(),
            self.blind_retry_safe().to_string(),
        );
        extra.insert(
            "remember_failed_step".to_string(),
            failed_step.unwrap_or("none").to_string(),
        );
        extra.insert(
            "remember_failure_message".to_string(),
            failure_message.clone(),
        );
        extra.insert(
            "remember_applied_steps_json".to_string(),
            serde_json::json!(applied_steps).to_string(),
        );
        extra.insert(
            "remember_pending_steps_json".to_string(),
            serde_json::json!(pending_steps).to_string(),
        );
        extra.insert(
            "remember_repair_json".to_string(),
            serde_json::json!({
                "status": self.overall_status(),
                "blind_retry_safe": self.blind_retry_safe(),
                "failed_step": failed_step,
                "failure_message": if failure_message.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(failure_message)
                },
                "applied_steps": applied_steps,
                "pending_steps": pending_steps,
                "recommended_action": match self.overall_status() {
                    "partial_success" => "resume_from_failed_step_without_blind_replaying_archive",
                    "complete" => "no_repair_needed",
                    _ => "no_repair_needed",
                },
            })
            .to_string(),
        );
        for (key, status) in &self.step_statuses {
            extra.insert((*key).to_string(), (*status).to_string());
        }
    }
}

impl SessionMemoryWriteMetadata {
    fn apply(self, turn: &mut ChuangKernelTurn) {
        let extra = &mut turn.result.response.meta.extra;
        match self {
            Self::Prepared {
                compacted,
                attempted_chars,
                stored_chars,
            } => {
                extra.insert(
                    "session_memory_write_status".to_string(),
                    if compacted { "compacted" } else { "written" }.to_string(),
                );
                extra.insert(
                    "session_memory_summary_kind".to_string(),
                    if compacted {
                        "compacted_turn_summary"
                    } else {
                        "turn_summary"
                    }
                    .to_string(),
                );
                if compacted {
                    extra.insert(
                        "session_memory_compacted_from_chars".to_string(),
                        attempted_chars.to_string(),
                    );
                    extra.insert(
                        "session_memory_compacted_to_chars".to_string(),
                        stored_chars.to_string(),
                    );
                }
            }
            Self::HardLimitExceeded { error } => {
                extra.insert(
                    "session_memory_write_status".to_string(),
                    "hard_limit_exceeded".to_string(),
                );
                extra.insert("session_memory_write_error".to_string(), error);
                extra.insert(
                    "session_memory_write_requested".to_string(),
                    "true".to_string(),
                );
                extra.insert(
                    "session_memory_summary_kind".to_string(),
                    "none".to_string(),
                );
            }
        }
    }
}

fn archive_session_turn(
    options: &CliOptions,
    turn: &mut ChuangKernelTurn,
    session_id: &str,
    searchable_summary: Option<MemoryRecord>,
) -> Result<(), String> {
    let runtime_event_refs = turn
        .result
        .response
        .meta
        .extra
        .get("runtime_event_ledger_json")
        .and_then(|raw| serde_json::from_str::<Vec<RuntimeEvent>>(raw).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|event| event.evidence_ref)
        .collect::<Vec<_>>();
    let runtime_report_ref = format!("runtime-report://{}", turn.report.report_id.0);
    let archive = SqliteSessionArchive::open(&options.runtime.db_path)
        .map_err(|error| format!("session_archive_open_failed: {error}"))?;
    let archived = match searchable_summary {
        Some(summary) => archive.append_with_summary(
            session_id,
            turn.user_input.clone(),
            turn.result.response.body.clone(),
            runtime_event_refs,
            vec![runtime_report_ref],
            summary,
        ),
        None => archive.append(
            session_id,
            turn.user_input.clone(),
            turn.result.response.body.clone(),
            runtime_event_refs,
            vec![runtime_report_ref],
            None,
        ),
    }
    .map_err(|error| format!("session_archive_append_failed: {error}"))?;

    let extra = &mut turn.result.response.meta.extra;
    extra.insert("session_archive_status".to_string(), "written".to_string());
    extra.insert(
        "session_archive_sequence".to_string(),
        archived.sequence.to_string(),
    );
    extra.insert(
        "session_archive_created_at".to_string(),
        archived.created_at,
    );
    extra.insert("session_archive_replayable".to_string(), "true".to_string());
    Ok(())
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

fn build_subagent_dispatch_request(
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> SpawnRequest {
    SpawnRequest {
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
    }
}

fn prepare_identity_turn_write(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<PendingIdentityTurnWrite, String> {
    let dual_file_config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let entry_id = unique_identity_turn_id(&turn.turn_id)?;
    let content = format!(
        "user={}\nresponse={}\nsummary={}",
        turn.user_input, turn.result.response.body, turn.report.summary
    );
    preview_identity_memory_append(&dual_file_config, &entry_id, &content)?;
    Ok(PendingIdentityTurnWrite {
        dual_file_config,
        entry_id,
        content,
    })
}

fn prepare_experience_turn_write(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<PendingExperienceTurnWrite, String> {
    let dual_file_config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
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
    preview_identity_experience_append(&dual_file_config, &entry_id, &content)?;
    Ok(PendingExperienceTurnWrite {
        dual_file_config,
        entry_id,
        content,
    })
}

fn prepare_subagent_dispatch(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<PendingSubagentDispatch, String> {
    if options.runtime.subagent != SubagentConfig::QueuedExternal {
        return Err(
            "subagent_dispatch_requires_queued_external: pass --subagent queued_external"
                .to_string(),
        );
    }
    let queue_config = options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    preview_subagent_queue_paths(&queue_config)?;
    let request = build_subagent_dispatch_request(turn);
    preview_subagent_dispatch_request(&request)?;
    Ok(PendingSubagentDispatch {
        runtime: options.runtime.clone(),
        request,
    })
}

struct PendingIdentityTurnWrite {
    dual_file_config: chuang_agent::hermes_memory::DualFileMemoryConfig,
    entry_id: String,
    content: String,
}

impl PendingIdentityTurnWrite {
    fn commit(self) -> Result<String, String> {
        let mut store = FileDualFileMemoryStore::open(self.dual_file_config)
            .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
        store
            .append_memory(HotMemoryEntry {
                id: self.entry_id.clone(),
                content: self.content,
            })
            .map_err(format_identity_memory_error)?;
        Ok(self.entry_id)
    }
}

struct PendingExperienceTurnWrite {
    dual_file_config: chuang_agent::hermes_memory::DualFileMemoryConfig,
    entry_id: String,
    content: String,
}

impl PendingExperienceTurnWrite {
    fn commit(self) -> Result<String, String> {
        let mut store = FileDualFileMemoryStore::open(self.dual_file_config)
            .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
        store
            .append_experience(HotMemoryEntry {
                id: self.entry_id.clone(),
                content: self.content,
            })
            .map_err(format_identity_memory_error)?;
        Ok(self.entry_id)
    }
}

struct PendingSubagentDispatch {
    runtime: RuntimeConfig,
    request: SpawnRequest,
}

impl PendingSubagentDispatch {
    fn commit(self) -> Result<chuang_agent::subagent_spawner::SpawnReceipt, String> {
        let mut slots = build_runtime_slots(&self.runtime)
            .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
        slots
            .subagent
            .spawn(self.request)
            .map_err(|e| format!("subagent_dispatch_failed: {e:?}"))
    }
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

fn preview_identity_memory_append(
    config: &chuang_agent::hermes_memory::DualFileMemoryConfig,
    entry_id: &str,
    content: &str,
) -> Result<(), String> {
    preview_dual_file_append(
        &config.root,
        &config.memory_path(),
        config.memory_max_chars,
        entry_id,
        content,
        false,
    )
}

fn preview_identity_experience_append(
    config: &chuang_agent::hermes_memory::DualFileMemoryConfig,
    entry_id: &str,
    content: &str,
) -> Result<(), String> {
    preview_dual_file_append(
        &config.root,
        &config.experiences_path(),
        config.memory_max_chars,
        entry_id,
        content,
        true,
    )
}

fn preview_dual_file_append(
    root: &Path,
    path: &Path,
    max_chars: usize,
    entry_id: &str,
    content: &str,
    experiences: bool,
) -> Result<(), String> {
    if root.exists() && !root.is_dir() {
        return Err(format!(
            "identity_memory_open_failed: StorageUnavailable {{ path: {} }}",
            root.display()
        ));
    }

    let current = if path.exists() {
        fs::read_to_string(path).map_err(|_| {
            format!(
                "identity_memory_open_failed: StorageUnavailable {{ path: {} }}",
                path.display()
            )
        })?
    } else {
        String::new()
    };
    let existing_entries = parse_memory_entry_views_for_preview(&current);
    if existing_entries.iter().any(|view| view.id == entry_id) {
        return Err(format!("identity_memory_duplicate_entry id={entry_id}"));
    }
    let next = append_hot_memory_entry_text(&current, entry_id, content);
    match TextMemoryAdmission::new(max_chars).evaluate(&next, existing_entries) {
        TextMemoryAdmissionDecision::Accepted => Ok(()),
        TextMemoryAdmissionDecision::Rejected {
            limit_chars,
            attempted_chars,
            existing_entries,
        } => Err(format_identity_memory_error(
            chuang_agent::hermes_memory::DualFileMemoryError::HardLimitExceeded {
                scope: if experiences {
                    chuang_agent::hermes_memory::DualFileMemoryScope::Experiences
                } else {
                    chuang_agent::hermes_memory::DualFileMemoryScope::Memory
                },
                limit_chars,
                attempted_chars,
                existing_entries,
            },
        )),
    }
}

fn preview_subagent_queue_paths(config: &FileSubagentQueueConfig) -> Result<(), String> {
    preview_directory_target(&config.root)?;
    preview_directory_target(&config.root.join(&config.dispatch_dir))?;
    preview_directory_target(&config.root.join(&config.report_dir))?;
    preview_directory_target(&config.root.join(&config.claim_dir))?;
    preview_directory_target(&config.root.join(&config.claim_release_dir))?;
    Ok(())
}

fn preview_subagent_dispatch_request(request: &SpawnRequest) -> Result<(), String> {
    if request.task_id.0.trim().is_empty() {
        return Err(
            "subagent_dispatch_failed: InvalidRequest(\"task_id must not be empty\")".to_string(),
        );
    }
    if request.parent_agent_id.0.trim().is_empty() {
        return Err(
            "subagent_dispatch_failed: InvalidRequest(\"parent_agent_id must not be empty\")"
                .to_string(),
        );
    }
    if request.agent_name.trim().is_empty() {
        return Err(
            "subagent_dispatch_failed: InvalidRequest(\"agent_name must not be empty\")"
                .to_string(),
        );
    }
    if request.task.trim().is_empty() {
        return Err(
            "subagent_dispatch_failed: InvalidRequest(\"task must not be empty\")".to_string(),
        );
    }
    if request.token_budget == 0 {
        return Err(
            "subagent_dispatch_failed: InvalidRequest(\"token_budget must be greater than zero\")"
                .to_string(),
        );
    }
    if matches!(request.tool_policy, SubagentToolPolicy::Analyze) && request.recursive_spawn {
        return Err("subagent_dispatch_failed: InvalidRequest(\"analyze policy cannot enable recursive spawn\")".to_string());
    }
    Ok(())
}

fn preview_directory_target(path: &Path) -> Result<(), String> {
    if path.exists() && !path.is_dir() {
        return Err(format!(
            "subagent_dispatch_failed: StorageUnavailable {{ path: {} }}",
            path.display()
        ));
    }
    Ok(())
}

fn append_hot_memory_entry_text(current: &str, entry_id: &str, content: &str) -> String {
    let mut next = current.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str("## ");
    next.push_str(entry_id);
    next.push('\n');
    next.push_str(content.trim());
    next.push('\n');
    next
}

fn parse_memory_entry_views_for_preview(content: &str) -> Vec<MemoryEntryView> {
    let mut entries = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(id) = line.strip_prefix("## ") {
            push_memory_entry_view(
                &mut entries,
                current_id.take().or_else(|| {
                    if current_body.trim().is_empty() {
                        None
                    } else {
                        Some("MEMORY.md:preamble".to_string())
                    }
                }),
                &current_body,
            );
            current_id = Some(id.trim().to_string());
            current_body.clear();
        } else {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }

    push_memory_entry_view(&mut entries, current_id, &current_body);
    entries
}

fn push_memory_entry_view(entries: &mut Vec<MemoryEntryView>, id: Option<String>, body: &str) {
    let Some(id) = id else {
        return;
    };
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    entries.push(MemoryEntryView {
        id,
        content_preview: preview_chars(trimmed, 80),
        chars: trimmed.chars().count(),
    });
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
        ToolCall::OpenApp { .. } => "open_app",
        ToolCall::Wait { .. } => "wait",
        ToolCall::HumanSuspend { .. } => "human_suspend",
        ToolCall::ApplyPatch { .. } => "apply_patch",
        ToolCall::ShellExec { .. } => "shell_exec",
        ToolCall::MemoryRecall { .. } => "memory_recall",
    }
}

fn tool_evidence_for_model(record: &ToolExecutionRecord, decision: &str) -> String {
    let mut fields = vec![
        format!("call={}", tool_call_name(&record.call)),
        format!(
            "atomic={}",
            record.atomic_tool_name.as_deref().unwrap_or("auxiliary")
        ),
        format!("decision={decision}"),
        format!("execution_succeeded={}", record.ok),
        format!("duration_ms={}", record.duration_ms),
        format!(
            "failure_class={}",
            record.failure_class.as_deref().unwrap_or("none")
        ),
    ];
    if let Some(exit_code) = record.exit_code {
        fields.push(format!("exit_code={exit_code}"));
    }
    if let Some(bytes) = record.output_bytes {
        fields.push(format!("output_bytes={bytes}"));
    }
    if let Some(lines) = record.output_lines {
        fields.push(format!("output_lines={lines}"));
    }
    if let Some(bytes) = record.stderr_bytes {
        fields.push(format!("stderr_bytes={bytes}"));
    }
    if let Some(lines) = record.stderr_lines {
        fields.push(format!("stderr_lines={lines}"));
    }
    fields.push(format!(
        "content_redacted={}",
        record.output_redacted || record.stdout_redacted || record.stderr_redacted
    ));
    fields.push(format!(
        "content_truncated={}",
        record.output_truncated || record.stdout_truncated || record.stderr_truncated
    ));

    let mut evidence = format!("tool_result {}", fields.join(" "));
    if record.output_redacted || record.stdout_redacted || record.stderr_redacted {
        evidence.push_str(
            " note=content_was_protected_for_secret_safety; this does_not mean the tool was unavailable, empty, or failed; trust execution_succeeded, exit_code, and output statistics",
        );
    } else {
        evidence.push_str(&format!(
            " summary={}",
            truncate_history_text(&record.summary.replace('\n', " | "), 2_000)
        ));
    }
    evidence
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
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
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
            live_guidance_path: None,
            progress_path: None,
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
    fn run_with_options_defaults_missing_channel_metadata_to_cli_for_identity_selection() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-default-channel-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let identity_root = temp_dir.join("identity");
        write_identity_registry(
            &identity_root,
            r#"memory_body_id = "chuang-body"
active_agent_id = "chuang"

[[agents]]
agent_id = "chuang"
display_name = "Chuang"
shell_kind = "codex-rust"
role = "kernel"
memory_body_id = "chuang-body"
allowed_channels = ["app-server"]
"#,
        );

        let mut runtime = test_runtime(temp_dir.join("memory.db"), identity_root.clone());
        runtime.identity_bootstrap =
            chuang_agent::runtime_config::IdentityBootstrapConfig::new(&identity_root);

        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: "验证默认 cli channel".to_string(),
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
            live_guidance_path: None,
            progress_path: None,
        };

        let error = run_with_options(&request).expect_err("cli default channel should be enforced");
        assert!(error.contains("ChannelNotAllowed"), "{error}");
    }

    #[test]
    fn load_identity_bootstrap_snapshot_preserves_explicit_non_cli_channel() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-explicit-channel-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let identity_root = temp_dir.join("identity");
        write_identity_registry(
            &identity_root,
            r#"memory_body_id = "chuang-body"
active_agent_id = "chuang"

[[agents]]
agent_id = "chuang"
display_name = "Chuang"
shell_kind = "codex-rust"
role = "kernel"
memory_body_id = "chuang-body"
allowed_channels = ["app-server"]
"#,
        );

        let mut runtime = test_runtime(temp_dir.join("memory.db"), identity_root.clone());
        runtime.identity_bootstrap =
            chuang_agent::runtime_config::IdentityBootstrapConfig::new(&identity_root);
        runtime
            .metadata
            .insert("channel".to_string(), "app-server".to_string());

        let snapshot =
            load_identity_bootstrap_snapshot(&runtime).expect("app-server channel should select");
        assert_eq!(
            snapshot
                .active_identity
                .as_ref()
                .map(|identity| identity.agent_id.as_str()),
            Some("chuang")
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
            live_guidance_path: None,
            progress_path: None,
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
            live_guidance_path: None,
            progress_path: None,
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
            max_tokens: 3600,
            reserve_system_tokens: 3200,
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
            live_guidance_path: None,
            progress_path: None,
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
            live_guidance_path: None,
            progress_path: None,
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
            live_guidance_path: None,
            progress_path: None,
        };
        let (_, first_records) = run_with_options(&first).expect("first run should succeed");
        assert!(first_records
            .session_record_id
            .as_deref()
            .unwrap_or_default()
            .starts_with("turn-memory-session-alpha-turn-1-"));
        let archived = SqliteSessionArchive::open(&runtime.db_path)
            .expect("session archive should reopen")
            .replay("alpha")
            .expect("session archive should replay");
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].sequence, 1);
        assert_eq!(archived[0].raw_user_input, "会话记忆锚点A");
        assert!(archived[0].raw_response.contains("会话记忆锚点A"));
        assert_eq!(
            archived[0].runtime_report_refs,
            vec!["runtime-report://report-turn-1"]
        );
        assert!(!archived[0].runtime_event_refs.is_empty());
        assert!(archived[0]
            .searchable_summary_pointer
            .as_deref()
            .unwrap_or_default()
            .starts_with("memory://turn-memory-session-alpha-turn-1-"));

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
            live_guidance_path: None,
            progress_path: None,
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
            live_guidance_path: None,
            progress_path: None,
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
            live_guidance_path: None,
            progress_path: None,
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
    fn remember_turn_if_requested_returns_partial_success_when_identity_commit_fails_after_archive_commit(
    ) {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-remember-partial-identity-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let kernel_db_path = temp_dir.join("kernel-memory.db");
        let session_archive_path = temp_dir.join("session-archive.db");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");
        let kernel_identity_root = temp_dir.join("kernel-identity");
        fs::create_dir_all(&kernel_identity_root).expect("identity root should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(kernel_db_path, kernel_identity_root.clone()),
            SqliteMemoryStore::open(temp_dir.join("kernel-store.db"))
                .expect("sqlite store should open"),
            CaptureResponder::new("ACTION: {\"type\":\"final\",\"answer\":\"ok\"}"),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();
        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            1,
            ToolExecutionConfig::default(),
            "先跑一轮，再尝试写 session+identity".to_string(),
            Vec::new(),
        )
        .expect("turn should succeed");

        let runtime = test_runtime(session_archive_path.clone(), session_archive_path.clone());
        let options = CliOptions {
            runtime: runtime.clone(),
        };
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: turn.user_input.clone(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: true,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: None,
            progress_path: None,
        };

        let (result, records) = remember_turn_if_requested(&options, &mut kernel, turn, &request)
            .expect("archive should commit and identity failure should be partial success");
        assert!(records.session_record_id.is_some());
        assert!(records.identity_record_id.is_none());
        let archived = SqliteSessionArchive::open(&session_archive_path)
            .expect("session archive should open")
            .replay("alpha")
            .expect("replay should succeed");
        assert_eq!(archived.len(), 1);
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_commit_status")
                .map(String::as_str),
            Some("partial_success")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_blind_retry_safe")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_failed_step")
                .map(String::as_str),
            Some("identity")
        );
        assert!(result
            .response
            .meta
            .extra
            .get("remember_failure_message")
            .expect("failure message should exist")
            .contains("identity_memory_open_failed"));
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                result
                    .response
                    .meta
                    .extra
                    .get("remember_applied_steps_json")
                    .expect("applied steps should exist")
            )
            .expect("applied steps json should parse"),
            vec!["archive".to_string()]
        );
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                result
                    .response
                    .meta
                    .extra
                    .get("remember_pending_steps_json")
                    .expect("pending steps should exist")
            )
            .expect("pending steps json should parse"),
            Vec::<String>::new()
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_archive_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_identity_status")
                .map(String::as_str),
            Some("failed")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_experience_status")
                .map(String::as_str),
            Some("not_requested")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_queued_dispatch_status")
                .map(String::as_str),
            Some("not_requested")
        );
    }

    #[test]
    fn remember_turn_if_requested_returns_partial_success_when_dispatch_commit_fails_after_prior_commits(
    ) {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-remember-partial-dispatch-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let session_archive_path = temp_dir.join("session-archive.db");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");
        let identity_root = temp_dir.join("identity");
        fs::create_dir_all(&identity_root).expect("identity root should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("kernel-memory.db"), identity_root.clone()),
            SqliteMemoryStore::open(temp_dir.join("kernel-store.db"))
                .expect("sqlite store should open"),
            CaptureResponder::new("ACTION: {\"type\":\"final\",\"answer\":\"ok\"}"),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();
        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            1,
            ToolExecutionConfig::default(),
            "先跑一轮，再尝试写 session+identity+experience+subagent".to_string(),
            Vec::new(),
        )
        .expect("turn should succeed");

        let mut runtime = test_runtime(session_archive_path.clone(), identity_root.clone());
        runtime.subagent = SubagentConfig::QueuedExternal;
        runtime.subagent_queue.root = session_archive_path.clone();
        let options = CliOptions {
            runtime: runtime.clone(),
        };
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: turn.user_input.clone(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: true,
            remember_experience: true,
            dispatch_subagent: true,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: None,
            progress_path: None,
        };

        let (result, records) = remember_turn_if_requested(&options, &mut kernel, turn, &request)
            .expect("dispatch failure should be partial success after prior commits");
        assert!(records.session_record_id.is_some());
        assert!(records.identity_record_id.is_some());
        assert!(records.experience_record_id.is_some());
        assert!(records.subagent_dispatch_run_id.is_none());
        let archived = SqliteSessionArchive::open(&session_archive_path)
            .expect("session archive should open")
            .replay("alpha")
            .expect("replay should succeed");
        assert_eq!(archived.len(), 1);
        assert!(fs::read_to_string(identity_root.join("MEMORY.md"))
            .expect("memory file should exist")
            .contains("user=先跑一轮，再尝试写 session+identity+experience+subagent"));
        assert!(fs::read_to_string(identity_root.join("experiences.md"))
            .expect("experiences file should exist")
            .contains("source=runtime_turn"));
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_commit_status")
                .map(String::as_str),
            Some("partial_success")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_failed_step")
                .map(String::as_str),
            Some("queued_dispatch")
        );
        assert!(!result
            .response
            .meta
            .extra
            .get("remember_failure_message")
            .expect("failure message should exist")
            .is_empty());
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                result
                    .response
                    .meta
                    .extra
                    .get("remember_applied_steps_json")
                    .expect("applied steps should exist")
            )
            .expect("applied steps json should parse"),
            vec![
                "archive".to_string(),
                "identity".to_string(),
                "experience".to_string()
            ]
        );
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                result
                    .response
                    .meta
                    .extra
                    .get("remember_pending_steps_json")
                    .expect("pending steps should exist")
            )
            .expect("pending steps json should parse"),
            Vec::<String>::new()
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_archive_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_identity_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_experience_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_queued_dispatch_status")
                .map(String::as_str),
            Some("failed")
        );
    }

    #[test]
    fn remember_turn_if_requested_marks_complete_when_all_commits_succeed() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-remember-complete-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let session_archive_path = temp_dir.join("session-archive.db");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");
        let identity_root = temp_dir.join("identity");
        fs::create_dir_all(&identity_root).expect("identity root should be created");
        let queue_root = temp_dir.join("subagent-queue");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("kernel-memory.db"), identity_root.clone()),
            SqliteMemoryStore::open(temp_dir.join("kernel-store.db"))
                .expect("sqlite store should open"),
            CaptureResponder::new("ACTION: {\"type\":\"final\",\"answer\":\"ok\"}"),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();
        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            1,
            ToolExecutionConfig::default(),
            "先跑一轮，再完整写入 remember 组合".to_string(),
            Vec::new(),
        )
        .expect("turn should succeed");

        let mut runtime = test_runtime(session_archive_path.clone(), identity_root);
        runtime.subagent = SubagentConfig::QueuedExternal;
        runtime.subagent_queue.root = queue_root.clone();
        let options = CliOptions {
            runtime: runtime.clone(),
        };
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: turn.user_input.clone(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: true,
            remember_experience: true,
            dispatch_subagent: true,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: None,
            progress_path: None,
        };

        let (result, records) = remember_turn_if_requested(&options, &mut kernel, turn, &request)
            .expect("all remember commits should succeed");
        assert!(records.session_record_id.is_some());
        assert!(records.identity_record_id.is_some());
        assert!(records.experience_record_id.is_some());
        assert!(records.subagent_dispatch_run_id.is_some());
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_commit_status")
                .map(String::as_str),
            Some("complete")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_blind_retry_safe")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_failed_step")
                .map(String::as_str),
            Some("none")
        );
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                result
                    .response
                    .meta
                    .extra
                    .get("remember_applied_steps_json")
                    .expect("applied steps should exist")
            )
            .expect("applied steps json should parse"),
            vec![
                "archive".to_string(),
                "identity".to_string(),
                "experience".to_string(),
                "queued_dispatch".to_string()
            ]
        );
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                result
                    .response
                    .meta
                    .extra
                    .get("remember_pending_steps_json")
                    .expect("pending steps should exist")
            )
            .expect("pending steps json should parse"),
            Vec::<String>::new()
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_archive_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_identity_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_experience_status")
                .map(String::as_str),
            Some("applied")
        );
        assert_eq!(
            result
                .response
                .meta
                .extra
                .get("remember_queued_dispatch_status")
                .map(String::as_str),
            Some("applied")
        );
        assert!(queue_root.join("dispatch").exists());
    }

    #[test]
    fn remember_turn_if_requested_does_not_write_identity_experience_or_dispatch_before_archive_failure(
    ) {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-session-archive-side-effect-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let valid_db_path = temp_dir.join("memory.db");
        let invalid_archive_path = temp_dir.join("archive-target-is-dir");
        fs::create_dir_all(&invalid_archive_path).expect("archive failure dir should exist");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");
        let identity_root = temp_dir.join("identity");
        let queue_root = temp_dir.join("subagent-queue");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(valid_db_path, identity_root.clone()),
            SqliteMemoryStore::open(temp_dir.join("kernel-memory.db"))
                .expect("sqlite store should open"),
            CaptureResponder::new("ACTION: {\"type\":\"final\",\"answer\":\"ok\"}"),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();
        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            1,
            ToolExecutionConfig::default(),
            "先跑一轮，再尝试写 session+identity+experience+subagent".to_string(),
            Vec::new(),
        )
        .expect("turn should succeed");

        let mut runtime = test_runtime(invalid_archive_path, identity_root.clone());
        runtime.subagent = SubagentConfig::QueuedExternal;
        runtime.subagent_queue.root = queue_root.clone();
        let options = CliOptions {
            runtime: runtime.clone(),
        };
        let request = RunCliRequest {
            options: CliOptions { runtime },
            user_input: turn.user_input.clone(),
            workspace_root: Some(temp_dir.clone()),
            remember: false,
            session_id: Some("alpha".to_string()),
            remember_session: true,
            conversation_history: Vec::new(),
            remember_identity: true,
            remember_experience: true,
            dispatch_subagent: true,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: None,
            progress_path: None,
        };

        let error = remember_turn_if_requested(&options, &mut kernel, turn, &request)
            .expect_err("archive write should fail");
        assert!(error.contains("session_archive_open_failed"), "{error}");
        let memory_path = identity_root.join("MEMORY.md");
        let experiences_path = identity_root.join("experiences.md");
        assert!(
            !memory_path.exists()
                || fs::read_to_string(&memory_path)
                    .expect("memory file should be readable")
                    .trim()
                    .is_empty()
        );
        assert!(
            !experiences_path.exists()
                || fs::read_to_string(&experiences_path)
                    .expect("experiences file should be readable")
                    .trim()
                    .is_empty()
        );
        assert!(!queue_root.join("dispatch").exists());
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
            live_guidance_path: None,
            progress_path: None,
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
            live_guidance_path: None,
            progress_path: None,
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
        let tool_call_count = turn
            .result
            .response
            .meta
            .extra
            .get("tool_call_count")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("tool_call_count should be numeric");
        assert!(
            tool_call_count >= 1,
            "git inspection should execute at least one tool"
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
        let runtime_events_json = turn
            .result
            .response
            .meta
            .extra
            .get("runtime_event_ledger_json")
            .expect("runtime event ledger json should exist");
        assert!(runtime_events_json.contains("\"event_type\":\"tool_started\""));
        assert!(runtime_events_json.contains("\"event_type\":\"tool_finished\""));
        assert!(runtime_events_json.contains("\"event_type\":\"context_packed\""));
        assert!(runtime_events_json.contains("\"event_type\":\"provider_requested\""));
        assert!(runtime_events_json.contains("\"event_type\":\"provider_responded\""));
        assert!(runtime_events_json.contains("\"risk_decision\""));
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("runtime_event_count")
                .map(String::as_str),
            Some("7")
        );
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
    fn run_with_options_covers_mainchain_terminal_task_matrix() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-mainchain-matrix-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(workspace_root.join("src")).expect("workspace src should be created");
        fs::create_dir_all(workspace_root.join("logs")).expect("workspace logs should be created");
        fs::write(
            workspace_root.join("src/lib.rs"),
            "pub fn target_fn() -> &'static str { \"ok\" }\n",
        )
        .expect("source fixture should be written");
        fs::write(
            workspace_root.join("Cargo.toml"),
            "[package]\nname = \"mainchain-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("cargo fixture should be written");
        fs::write(
            workspace_root.join("logs/app.log"),
            "INFO boot\nERROR fixture\n",
        )
        .expect("log fixture should be written");

        let cases = vec![
            MainchainCase {
                name: "git_status",
                user_input: "看一下 git 状态",
                outputs: vec![
                    "我没有 git 能力。",
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"git status --short","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已查看 git 状态。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["git status --short"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: Some("plain_text_response"),
            },
            MainchainCase {
                name: "git_diff",
                user_input: "看一下 git diff",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"git diff -- src/lib.rs","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已查看 diff。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["git diff -- src/lib.rs"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "git_log",
                user_input: "看一下 git log",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"git log --oneline -3","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已查看 git log。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["git log --oneline -3"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "list_dir",
                user_input: "列一下项目目录",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"list_dir","path":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已列出目录。"}"#,
                ],
                required_tools: vec!["list_dir"],
                required_output_fragments: vec!["Cargo.toml"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "read_file",
                user_input: "读一下 src/lib.rs",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_read","path":"src/lib.rs"}}"#,
                    r#"ACTION: {"type":"final","answer":"已读取 src/lib.rs。"}"#,
                ],
                required_tools: vec!["read_file"],
                required_output_fragments: vec!["target_fn"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "search_function",
                user_input: "搜索 target_fn 在哪里",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"rg -n target_fn src","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已找到 target_fn。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["src/lib.rs", "target_fn"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "create_file",
                user_input: "新建 notes/mainchain-create.txt",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/mainchain-create.txt","content":"created-ok"}}"#,
                    r#"ACTION: {"type":"final","answer":"已新建文件。"}"#,
                ],
                required_tools: vec!["write_file"],
                required_output_fragments: vec!["created-ok"],
                expected_files: vec![("notes/mainchain-create.txt", "created-ok")],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "modify_file",
                user_input: "修改 notes/mainchain-create.txt",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/mainchain-create.txt","content":"modified-ok"}}"#,
                    r#"ACTION: {"type":"final","answer":"已修改文件。"}"#,
                ],
                required_tools: vec!["write_file"],
                required_output_fragments: vec!["modified-ok"],
                expected_files: vec![("notes/mainchain-create.txt", "modified-ok")],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "cat_verify",
                user_input: "cat notes/mainchain-create.txt 验证",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"cat notes/mainchain-create.txt","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"验证通过。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["modified-ok"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "test_command",
                user_input: "跑一下测试命令",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"printf test-ok","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"测试命令通过。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["test-ok"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "test_failure_then_fix",
                user_input: "根据测试失败继续修",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"sh -c 'echo failing-test >&2; exit 1'","cwd":"."}}"#,
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/fix.txt","content":"fixed-after-failure"}}"#,
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"cat notes/fix.txt","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已根据失败继续修并验证。"}"#,
                ],
                required_tools: vec!["code_execute", "write_file", "code_execute"],
                required_output_fragments: vec!["failing-test", "fixed-after-failure"],
                expected_files: vec![("notes/fix.txt", "fixed-after-failure")],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "log_read",
                user_input: "看一下日志错误",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"tail -n 20 logs/app.log","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"日志里有 ERROR fixture。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["ERROR fixture"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "process_status",
                user_input: "看一下进程状态",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"ps -p $$ -o comm=","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已查看进程。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["sh"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "port_status",
                user_input: "看一下端口状态",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"python3 - <<'PY'\nprint('port-check-ok')\nPY","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"端口检查命令已执行。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["port-check-ok"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "docker_status",
                user_input: "看一下 docker 状态",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"printf docker-status-ok","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"已查看 docker 状态。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["docker-status-ok"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "config_change",
                user_input: "修改配置文件",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"config/app.conf","content":"enabled=true\n"}}"#,
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"grep -n enabled config/app.conf","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"配置已修改并验证。"}"#,
                ],
                required_tools: vec!["write_file", "code_execute"],
                required_output_fragments: vec!["enabled=true"],
                expected_files: vec![("config/app.conf", "enabled=true\n")],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "start_local_service",
                user_input: "启动一个本地服务并检查",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"printf service-start-ok","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"本地服务启动检查通过。"}"#,
                ],
                required_tools: vec!["code_execute"],
                required_output_fragments: vec!["service-start-ok"],
                expected_files: vec![],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "bug_fix",
                user_input: "做一次 bug 修复",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_read","path":"src/lib.rs"}}"#,
                    r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"src/lib.rs","content":"pub fn target_fn() -> &'static str { \"fixed\" }\n"}}"#,
                    r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"grep -n fixed src/lib.rs","cwd":"."}}"#,
                    r#"ACTION: {"type":"final","answer":"bug 已修复并验证。"}"#,
                ],
                required_tools: vec!["read_file", "write_file", "code_execute"],
                required_output_fragments: vec!["fixed"],
                expected_files: vec![(
                    "src/lib.rs",
                    "pub fn target_fn() -> &'static str { \"fixed\" }\n",
                )],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "report_generation",
                user_input: "生成任务报告",
                outputs: vec![
                    r##"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/report.md","content":"# Report\n\n- evidence: ok\n"}}"##,
                    r#"ACTION: {"type":"final","answer":"报告已生成。"}"#,
                ],
                required_tools: vec!["write_file"],
                required_output_fragments: vec!["notes/report.md"],
                expected_files: vec![("notes/report.md", "# Report\n\n- evidence: ok\n")],
                expected_status: Some("completed"),
                expected_protocol_error: None,
            },
            MainchainCase {
                name: "danger_requires_human",
                user_input: "删除重要数据",
                outputs: vec![
                    r#"ACTION: {"type":"tool_call","call":{"tool":"human_suspend","reason":"delete requires explicit approval","prompt":"请确认具体删除目标"}}"#,
                    r#"ACTION: {"type":"final","answer":"不应到达。"}"#,
                ],
                required_tools: vec!["human_suspend"],
                required_output_fragments: vec!["human_input_required"],
                expected_files: vec![],
                expected_status: Some("human_input_required"),
                expected_protocol_error: None,
            },
        ];

        assert_eq!(cases.len(), 20);
        for case in cases {
            run_mainchain_case(&temp_dir, &workspace_root, case);
        }
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
    fn governed_tool_loop_persists_dangerous_call_without_executing_it() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-pending-approval-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = SequenceResponder::new(vec![
            r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"printf executed > must-not-exist.txt; # rm -rf notes","cwd":"."}}"#,
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
            "删除 notes 前先等待审批".to_string(),
            Vec::new(),
        )
        .expect("dangerous call should suspend with a persisted approval");

        assert!(!workspace_root.join("must-not-exist.txt").exists());
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("human_input_required")
        );
        let pending_path = PathBuf::from(
            turn.result
                .response
                .meta
                .extra
                .get("pending_approval_path")
                .expect("pending approval path should be recorded"),
        );
        assert!(pending_path.starts_with(
            fs::canonicalize(&workspace_root).expect("workspace should canonicalize")
        ));
        let pending: PendingApproval = serde_json::from_slice(
            &fs::read(&pending_path).expect("pending approval file should exist"),
        )
        .expect("pending approval file should deserialize");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&pending_path)
                    .expect("pending metadata should exist")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert_eq!(
            turn.result.response.meta.extra.get("pending_approval_id"),
            Some(&pending.approval_id)
        );
        assert_eq!(
            remaining_outputs
                .lock()
                .expect("sequence lock should succeed")
                .len(),
            1,
            "tool loop must stop before asking the model for another round"
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
        assert!(captured[0]
            .prompt
            .contains("open_app/mouse/keyboard=桌面交互"));
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
        let tool_call_count = turn
            .result
            .response
            .meta
            .extra
            .get("tool_call_count")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("tool_call_count should be numeric");
        assert!(
            tool_call_count >= 1,
            "git inspection should execute at least one tool"
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
        let tool_call_count = turn
            .result
            .response
            .meta
            .extra
            .get("tool_call_count")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("tool_call_count should be numeric");
        assert!(
            tool_call_count >= 1,
            "git inspection should execute at least one tool"
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
            live_guidance_path: None,
            progress_path: None,
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
        let runtime_events_json = turn
            .result
            .response
            .meta
            .extra
            .get("runtime_event_ledger_json")
            .expect("runtime event ledger json should exist");
        assert!(runtime_events_json.contains("\"event_type\":\"tool_started\""));
        assert!(runtime_events_json.contains("\"event_type\":\"tool_finished\""));
        assert!(runtime_events_json.contains("\"decision\":\"draft_only:"));
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
            "测试工具协议纠错".to_string(),
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
    fn run_with_options_forces_action_after_local_capability_denial() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-local-denial-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                "无法完成：当前环境没有文件创建能力。",
                r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"mkdir -p notes/local","cwd":"."}}"#,
                r#"ACTION: {"type":"final","answer":"已创建。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "在项目目录新建文件夹 notes/local".to_string(),
            Vec::new(),
        )
        .expect("local capability denial should be corrected into an ACTION");

        assert_eq!(turn.result.response.body, "已创建。");
        assert!(workspace_root.join("notes/local").is_dir());
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
            .contains("missing_required_action"));
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist")
            .contains("\"atomic_tool_name\":\"code_execute\""));
    }

    #[test]
    fn run_with_options_forces_action_for_git_inspection_request() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-git-inspection-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                "无法查看 git：当前环境没有 git 能力。",
                r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"git status --short","cwd":"."}}"#,
                r#"ACTION: {"type":"final","answer":"已查看 git 状态。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "看一下 git 状态".to_string(),
            Vec::new(),
        )
        .expect("git inspection denial should be corrected into a code_execute ACTION");

        assert_eq!(turn.result.response.body, "已查看 git 状态。");
        let tool_call_count = turn
            .result
            .response
            .meta
            .extra
            .get("tool_call_count")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("tool_call_count should be numeric");
        assert!(
            tool_call_count >= 1,
            "git inspection should execute at least one tool"
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("plain_text_response"));
        let tool_calls_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist");
        assert!(tool_calls_json.contains("\"atomic_tool_name\":\"code_execute\""));
        assert!(tool_calls_json.contains("git status --short"));
    }

    #[test]
    fn run_with_options_forces_evidence_for_self_health_check() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-self-health-check-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = SequenceResponder::new(vec![
            r#"ACTION: {"schema_version":1,"type":"final","answer":"健康度约 70%，工具不可用。"}"#,
            r#"ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"code_execute","command":"printf health-ok","cwd":"."}}"#,
            r#"ACTION: {"schema_version":1,"type":"final","answer":"体检完成，工具链正常。"}"#,
        ]);
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
            "给自己做个体检".to_string(),
            Vec::new(),
        )
        .expect("self health check should require real tool evidence");

        assert_eq!(turn.result.response.body, "体检完成，工具链正常。");
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
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("missing_required_action"));
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist")
            .contains("health-ok"));
        let captured = captured.lock().expect("capture lock should succeed");
        assert_eq!(captured.len(), 3);
        assert!(!captured[1].prompt.contains("健康度约 70%"));
        assert!(captured[1]
            .prompt
            .contains("rejected_unverified_answer=true"));
        assert!(captured[2].prompt.contains("execution_succeeded=true"));
        assert!(captured[2].prompt.contains("exit_code=0"));
    }

    #[test]
    fn run_with_options_rejects_local_final_before_any_tool_call() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-local-final-before-tool-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"final","answer":"已完成。"}"#,
                r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"mkdir -p notes/local-final","cwd":"."}}"#,
                r#"ACTION: {"type":"final","answer":"已创建。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig::default(),
            "在项目目录新建文件夹 notes/local-final".to_string(),
            Vec::new(),
        )
        .expect("local final before tool call should be corrected into an ACTION");

        assert_eq!(turn.result.response.body, "已创建。");
        assert!(workspace_root.join("notes/local-final").is_dir());
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
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("missing_required_action"));
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .expect("tool calls json should exist")
            .contains("\"atomic_tool_name\":\"code_execute\""));
    }

    #[test]
    fn run_with_options_reports_failure_when_local_action_never_calls_tool() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-local-action-no-tool-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                "无法完成：当前环境没有文件创建能力。",
                r#"ACTION: {"type":"final","answer":"已完成。"}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            2,
            ToolExecutionConfig::default(),
            "在项目目录新建文件夹 notes/no-tool".to_string(),
            Vec::new(),
        )
        .expect("local action without tool call should return a structured failure");

        assert!(turn.result.response.body.contains("没有完成真实本地动作"));
        assert!(!workspace_root.join("notes/no-tool").is_dir());
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("missing_required_action")
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
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_protocol_errors_json")
            .expect("tool protocol errors json should exist")
            .contains("missing_required_action"));
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
    fn tool_loop_uses_one_no_tool_finalization_call_after_round_limit() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-finalization-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let responder = SequenceResponder::new(vec![
            r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/health.txt","content":"healthy"}}"#,
            r#"ACTION: {"type":"tool_call","call":{"tool":"file_read","path":"notes/health.txt"}}"#,
            r#"ACTION: {"schema_version":1,"type":"final","answer":"体检完成，检查结果正常。"}"#,
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
            2,
            ToolExecutionConfig::default(),
            "给自己做个体检".to_string(),
            Vec::new(),
        )
        .expect("finalization should return a final answer");

        assert_eq!(turn.result.response.body, "体检完成，检查结果正常。");
        assert_eq!(
            fs::read_to_string(workspace_root.join("notes/health.txt"))
                .expect("health evidence should exist"),
            "healthy"
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("completed_after_tool_limit")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_finalization_status")
                .map(String::as_str),
            Some("succeeded")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_finalization_response_kind")
                .map(String::as_str),
            Some("final")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_model_call_count")
                .map(String::as_str),
            Some("3")
        );
        assert!(remaining_outputs
            .lock()
            .expect("sequence lock should succeed")
            .is_empty());
    }

    #[test]
    fn tool_loop_finalization_blocks_another_tool_call_without_executing_it() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-finalization-block-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/allowed.txt","content":"done"}}"#,
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_read","path":"notes/allowed.txt"}}"#,
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/must-not-run.txt","content":"unexpected"}}"#,
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            2,
            ToolExecutionConfig::default(),
            "检查文件后总结".to_string(),
            Vec::new(),
        )
        .expect("blocked finalization tool call should return a readable fallback");

        assert!(workspace_root.join("notes/allowed.txt").is_file());
        assert!(!workspace_root.join("notes/must-not-run.txt").exists());
        assert!(turn.result.response.body.contains("额外进行了一次禁用工具"));
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("tool_loop_exhausted")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_finalization_status")
                .map(String::as_str),
            Some("rejected_tool_call")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_finalization_tool_call_blocked")
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
            Some("2")
        );
    }

    #[test]
    fn tool_loop_finalization_accepts_plain_text_after_real_tool_work() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-finalization-plain-text-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/plain-final.txt","content":"done"}}"#,
                "文件已经写入，任务完成。",
            ]),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            1,
            ToolExecutionConfig::default(),
            "写入 notes/plain-final.txt 后总结".to_string(),
            Vec::new(),
        )
        .expect("plain text finalization should be accepted after real tool work");

        assert_eq!(turn.result.response.body, "文件已经写入，任务完成。");
        assert!(workspace_root.join("notes/plain-final.txt").is_file());
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("completed_after_tool_limit")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_finalization_response_kind")
                .map(String::as_str),
            Some("plain_text")
        );
    }

    #[test]
    fn tool_loop_finalization_provider_error_preserves_tool_evidence_and_falls_back() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-cli-tool-finalization-provider-error-test-{}",
            unique_record_suffix_for_test()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");

        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(vec![
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/provider-error.txt","content":"done"}}"#,
                r#"ACTION: {"type":"tool_call","call":{"tool":"file_read","path":"notes/provider-error.txt"}}"#,
            ]),
        );
        let mut governance = FailOnClassifyGovernance::new(5);

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            &workspace_root,
            2,
            ToolExecutionConfig::default(),
            "写入并读取 notes/provider-error.txt 后总结".to_string(),
            Vec::new(),
        )
        .expect("provider error during finalization should preserve the prior turn");

        assert!(workspace_root.join("notes/provider-error.txt").is_file());
        assert!(turn.result.response.body.contains("最终总结仍未生成"));
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str),
            Some("tool_loop_exhausted")
        );
        assert_eq!(
            turn.result
                .response
                .meta
                .extra
                .get("tool_finalization_status")
                .map(String::as_str),
            Some("provider_error")
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
            .get("tool_finalization_error")
            .is_some_and(|error| error.contains("governance unavailable")));
    }

    #[test]
    fn tool_loop_exhausted_answer_is_operator_readable() {
        let answer = tool_loop_exhausted_answer("给自己体检下", 4, 3, 1);

        assert!(answer.contains("最终总结仍未生成"));
        assert!(answer.contains("额外进行了一次禁用工具的强制收口"));
        assert!(answer.contains("已执行工具：3 次"));
        assert!(answer.contains("协议修复：1 次"));
        assert!(answer.contains("不会自动重复执行"));
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
        assert!(turn.result.response.body.contains("普通桌面动作"));
        assert!(turn.result.response.body.contains("不需要人工审批"));
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
            max_tokens: 8192,
            reserve_system_tokens: 64,
            min_working_tokens: 1,
            max_tool_results: 5,
            max_memory_segments: 20,
        }
    }

    struct MainchainCase {
        name: &'static str,
        user_input: &'static str,
        outputs: Vec<&'static str>,
        required_tools: Vec<&'static str>,
        required_output_fragments: Vec<&'static str>,
        expected_files: Vec<(&'static str, &'static str)>,
        expected_status: Option<&'static str>,
        expected_protocol_error: Option<&'static str>,
    }

    fn run_mainchain_case(temp_dir: &Path, workspace_root: &Path, case: MainchainCase) {
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(
                temp_dir.join(format!("{}.db", case.name)),
                temp_dir.join(format!("identity-{}", case.name)),
            ),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            SequenceResponder::new(case.outputs),
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools(
            &mut kernel,
            &mut governance,
            workspace_root,
            6,
            ToolExecutionConfig::default(),
            case.user_input.to_string(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("case {} should complete: {error}", case.name));

        let tool_call_count = turn
            .result
            .response
            .meta
            .extra
            .get("tool_call_count")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        assert!(
            tool_call_count >= case.required_tools.len(),
            "case {} should execute at least {} tools, got {}",
            case.name,
            case.required_tools.len(),
            tool_call_count
        );

        let tool_calls_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_calls_json")
            .map(String::as_str)
            .unwrap_or("");
        let tool_events_json = turn
            .result
            .response
            .meta
            .extra
            .get("tool_events_json")
            .map(String::as_str)
            .unwrap_or("");
        let tool_trace = turn
            .result
            .response
            .meta
            .extra
            .get("tool_trace")
            .map(String::as_str)
            .unwrap_or("");

        for tool in &case.required_tools {
            assert!(
                tool_calls_json.contains(&format!("\"tool_name\":\"{tool}\""))
                    || tool_calls_json.contains(&format!("\"atomic_tool_name\":\"{tool}\"")),
                "case {} should include tool {}, calls={}",
                case.name,
                tool,
                tool_calls_json
            );
        }

        let combined_evidence = format!(
            "{}\n{}\n{}\n{}",
            turn.result.response.body, tool_calls_json, tool_events_json, tool_trace
        );
        for fragment in &case.required_output_fragments {
            assert!(
                combined_evidence.contains(fragment),
                "case {} missing evidence fragment {:?}\nevidence={}",
                case.name,
                fragment,
                combined_evidence
            );
        }

        for (path, expected) in &case.expected_files {
            let actual = fs::read_to_string(workspace_root.join(path)).unwrap_or_else(|error| {
                panic!("case {} missing file {}: {error}", case.name, path)
            });
            assert_eq!(
                actual, *expected,
                "case {} file {} content mismatch",
                case.name, path
            );
        }

        if let Some(status) = case.expected_status {
            assert_eq!(
                turn.result
                    .response
                    .meta
                    .extra
                    .get("tool_loop_status")
                    .map(String::as_str),
                Some(status),
                "case {} status mismatch",
                case.name
            );
        }

        if let Some(error_code) = case.expected_protocol_error {
            assert!(
                turn.result
                    .response
                    .meta
                    .extra
                    .get("tool_protocol_errors_json")
                    .map(|value| value.contains(error_code))
                    .unwrap_or(false),
                "case {} should include protocol error {}",
                case.name,
                error_code
            );
        }

        assert!(
            turn.result
                .response
                .meta
                .extra
                .get("runtime_event_ledger_json")
                .map(|value| value.contains("\"event_type\":\"tool_started\""))
                .unwrap_or(false),
            "case {} should record runtime tool events",
            case.name
        );
    }

    #[test]
    fn run_governed_turn_injects_live_guidance_before_next_model_round() {
        let temp_dir = std::env::temp_dir().join(format!(
            "chuang-agent-live-guidance-test-{}",
            unique_record_suffix_for_test()
        ));
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace should be created");
        let guidance_path = temp_dir.join("guidance.txt");
        let guidance_command = format!(
            "printf '%s\\n' '改成写 beta，不要继续 alpha' >> {}",
            guidance_path.display()
        );
        let action = format!(
            r#"ACTION: {{"type":"tool_call","call":{{"tool":"shell_exec","command":"{}","cwd":"."}}}}"#,
            guidance_command.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let responder = SequenceResponder::new(vec![
            &action,
            r#"ACTION: {"type":"tool_call","call":{"tool":"write_file","path":"notes/beta.txt","content":"beta"}}"#,
            "FINAL: 已按实时纠正改为 beta",
        ]);
        let mut kernel = ChuangKernel::with_responder(
            test_kernel_config(temp_dir.join("memory.db"), temp_dir.join("identity")),
            chuang_agent::memory_store::InMemoryMemoryStore::new(),
            responder,
        );
        let mut governance = chuang_agent::governance::StaticRuleGovernance::new();

        let turn = run_governed_turn_with_tools_live(
            &mut kernel,
            &mut governance,
            &workspace_root,
            4,
            ToolExecutionConfig {
                shell_timeout_ms: 30_000,
                ..ToolExecutionConfig::default()
            },
            "先写 alpha".to_string(),
            Vec::new(),
            Some(&guidance_path),
            None,
        )
        .expect("live guidance turn should succeed");

        assert_eq!(
            fs::read_to_string(workspace_root.join("notes/beta.txt"))
                .expect("beta file should exist"),
            "beta"
        );
        assert!(turn
            .result
            .response
            .meta
            .extra
            .get("tool_trace")
            .map(|value| value.contains("operator_guidance"))
            .unwrap_or(false));
    }

    #[derive(Debug, Clone)]
    struct SequenceResponder {
        outputs: Arc<Mutex<Vec<String>>>,
        captured: Arc<Mutex<Vec<CapturedResponderRequest>>>,
    }

    impl SequenceResponder {
        fn new(outputs: Vec<&str>) -> Self {
            Self {
                outputs: Arc::new(Mutex::new(
                    outputs.into_iter().map(|value| value.to_string()).collect(),
                )),
                captured: Arc::new(Mutex::new(Vec::new())),
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
            self.captured
                .lock()
                .expect("capture lock should succeed")
                .push(CapturedResponderRequest {
                    prompt: request.prompt.clone(),
                    user_input: request.user_input.clone(),
                });
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

    #[derive(Debug)]
    struct FailOnClassifyGovernance {
        classify_count: AtomicUsize,
        fail_on: usize,
    }

    impl FailOnClassifyGovernance {
        fn new(fail_on: usize) -> Self {
            Self {
                classify_count: AtomicUsize::new(0),
                fail_on,
            }
        }
    }

    impl chuang_agent::governance::Governance for FailOnClassifyGovernance {
        fn classify(
            &self,
            _action: &chuang_agent::governance::ProposedAction,
        ) -> Result<chuang_agent::governance::RiskDecision, chuang_agent::governance::GovernanceError>
        {
            let current = self.classify_count.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            if current == self.fail_on {
                return Err(chuang_agent::governance::GovernanceError {
                    message: "governance unavailable during finalization".to_string(),
                });
            }
            Ok(chuang_agent::governance::RiskDecision::Allowed {
                reason: "test allows pre-finalization work".to_string(),
            })
        }

        fn audit(
            &mut self,
            _record: chuang_agent::common::AuditRecord,
        ) -> Result<(), chuang_agent::governance::GovernanceError> {
            Ok(())
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

    fn write_identity_registry(identity_root: &Path, content: &str) {
        fs::create_dir_all(identity_root).expect("identity root should be created");
        fs::write(identity_root.join("agents.toml"), content)
            .expect("agents registry should write");
    }
}
