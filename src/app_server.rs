use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::cli_runtime::kernel_config_from_runtime;
use crate::cli_runtime::run_with_options;
use crate::cli_types::{CliOptions, RunCliRequest};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::path_utils::normalize_path_lexically;
use chuang_agent::runtime_config::{
    ConfigSummary, IdentityBootstrapConfig, IdentityMemoryConfig, OpenAICompatibleConfig,
    ProviderConfig, RulesConfig, RuntimeConfig, SubagentQueueConfig,
};
use chuang_agent::runtime_config_file::{
    load_runtime_config_file, load_runtime_config_file_with_options, RuntimeConfigFileError,
    RuntimeConfigFileOptions,
};
use chuang_agent::runtime_report::runtime_observability_meta;
use chuang_agent::tool_loop_meta::{parse_json_value, ToolLoopMeta};
use chuang_agent::tool_runtime::{ToolExecutionRecord, ToolProtocolError};

#[derive(Debug, Default)]
struct AppServerState {
    next_thread_seq: u64,
    next_turn_seq: u64,
    threads: BTreeMap<String, ThreadState>,
}

#[derive(Debug, Clone)]
struct ThreadState {
    id: String,
    workspace_root: String,
    display_name: String,
    created_at: u64,
    updated_at: u64,
    turns: Vec<TurnState>,
}

#[derive(Debug, Clone)]
struct TurnState {
    id: String,
    user_text: String,
    assistant_text: String,
    model_name: String,
    status: String,
    tool_trace: String,
    tool_surface: Option<Value>,
    updated_at: u64,
}

pub(crate) fn app_server_command(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("health") {
        return app_server_health_command(&args[1..]);
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = AppServerState::default();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("app_server_read_failed: {e}"))?;
        let raw = line.trim();
        if raw.is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) => {
                let _ = write_json_line(
                    &mut stdout,
                    &json!({
                        "error": {
                            "message": format!("invalid_json: {error}"),
                        }
                    }),
                );
                continue;
            }
        };

        let Some(method) = parsed.get("method").and_then(|value| value.as_str()) else {
            continue;
        };
        let id = parsed.get("id").cloned();
        let params = parsed.get("params").cloned().unwrap_or(Value::Null);

        if method == "initialized" {
            continue;
        }

        let result = match method {
            "initialize" => Ok(handle_initialize()),
            "model/list" => handle_model_list(&params),
            "thread/start" => handle_thread_start(&mut state, &params),
            "thread/resume" => handle_thread_resume(&state, &params),
            "thread/list" => Ok(handle_thread_list(&state)),
            "turn/start" => handle_turn_start(&mut state, &params),
            "turn/interrupt" => Ok(json!({"ok": true})),
            _ => Err(format!("unsupported_method: {method}")),
        };

        if let Some(id) = id {
            match result {
                Ok(result) => {
                    write_json_line(&mut stdout, &json!({ "id": id, "result": result }))?;
                }
                Err(message) => {
                    write_json_line(
                        &mut stdout,
                        &json!({ "id": id, "error": { "message": message } }),
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn app_server_health_command(args: &[String]) -> Result<(), String> {
    let mut workspace_root = String::new();
    let mut output_json = false;
    let mut diagnostic = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace-root" => {
                workspace_root = args
                    .get(index + 1)
                    .ok_or_else(|| {
                        "app-server health requires value after --workspace-root".to_string()
                    })?
                    .clone();
                index += 2;
            }
            "--json" => {
                output_json = true;
                index += 1;
            }
            "--diagnostic" => {
                diagnostic = true;
                index += 1;
            }
            _ => {
                return Err(
                    "usage: cargo run -- app-server health [--workspace-root PATH] [--diagnostic] [--json]"
                        .to_string(),
                )
            }
        }
    }

    let normalized_workspace_root = normalize_workspace_root(&workspace_root);
    let runtime = if diagnostic {
        build_runtime_for_workspace_with_options(
            &normalized_workspace_root,
            RuntimeConfigFileOptions::allow_missing_env(),
        )?
    } else {
        build_runtime_for_workspace(&normalized_workspace_root)?
    };
    runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let kernel = kernel_config_from_runtime(&runtime)?;
    let status = build_chuang_mvp_status(&runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let config_summary = runtime.summary();
    let diagnostic_status = app_server_health_diagnostic_status(&config_summary);
    let diagnostic_summary = app_server_health_diagnostic_summary(&config_summary, diagnostic);
    let next_actions = app_server_health_next_actions(&config_summary);
    let identity_memory_root = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?
        .root
        .display()
        .to_string();
    let result = json!({
        "ok": true,
        "server": "chuang-agent-app-server",
        "version": env!("CARGO_PKG_VERSION"),
        "workspace_root": normalized_workspace_root,
        "model": provider_summary_model_name(&runtime),
        "diagnostic_mode": diagnostic,
        "diagnostic_status": diagnostic_status,
        "diagnostic_summary": diagnostic_summary.clone(),
        "next_actions": next_actions.clone(),
        "api_key_state": config_summary.api_key_state,
        "placeholder_warnings": config_summary.placeholder_warnings,
        "goal_mode": status.goal_mode,
        "goal_run": status.goal_run,
        "provider_readiness": status.provider_readiness,
        "project_readiness": status.project_readiness,
        "local_contract_readiness": status.local_contract_readiness,
        "release_readiness": status.release_readiness,
        "third_test_candidate": status.third_test_candidate,
        "channel_readiness": status.channel_readiness,
        "subagent_readiness": status.subagent_readiness,
        "live_adapter_gates": status.live_adapter_gates,
        "external_ai_readiness": status.external_ai_readiness,
        "db_path": runtime.db_path.display().to_string(),
        "identity_memory_root": identity_memory_root,
        "identity_soul_path": runtime.identity_bootstrap.soul_path.display().to_string(),
        "rules_core_path": runtime.rules.core_path.display().to_string(),
        "subagent_queue_root": runtime.subagent_queue.root.display().to_string(),
    });

    if output_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("json_render_failed: {e}"))?
        );
    } else {
        println!("app_server_ok: true");
        println!("diagnostic_mode: {diagnostic}");
        println!("diagnostic_status: {}", diagnostic_status);
        println!("diagnostic_summary: {}", diagnostic_summary);
        println!(
            "workspace_root: {}",
            result["workspace_root"].as_str().unwrap_or("")
        );
        println!("model: {}", result["model"].as_str().unwrap_or(""));
        println!(
            "api_key_state: {}",
            result["api_key_state"].as_str().unwrap_or("none")
        );
        if config_summary.placeholder_warnings.is_empty() {
            println!("placeholder_warnings: none");
        } else {
            println!(
                "placeholder_warnings: {}",
                config_summary.placeholder_warnings.join(";")
            );
        }
        if next_actions.is_empty() {
            println!("next_actions: none");
        } else {
            println!("next_actions: {}", next_actions.join(";"));
        }
        println!(
            "provider_readiness: ok={} state={} kind={} transport={} fallback_configured={} timeout_ms={} api_key_state={} placeholder_warnings={}",
            status.provider_readiness.ok,
            status.provider_readiness.overall_state,
            status.provider_readiness.provider_kind,
            status.provider_readiness.transport,
            status.provider_readiness.fallback_configured,
            status
                .provider_readiness
                .request_timeout_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            status
                .provider_readiness
                .api_key_state
                .as_deref()
                .unwrap_or("none"),
            status.provider_readiness.placeholder_warning_count
        );
        println!(
            "provider_readiness_current: {}",
            status.provider_readiness.current
        );
        println!(
            "provider_readiness_next_action: {}",
            status.provider_readiness.next_action
        );
        println!(
            "goal_mode: ok={} kind={} cli_entrypoint={} context_source={} default_goal_id={} allowed_slots={} checkpoint_policy=progress_log:{} handoff:{} commit:{} final_report_policy=validation:{} next_steps:{} bypasses_governance={} adds_core_slot={}",
            status.goal_mode.ok,
            status.goal_mode.kind,
            status.goal_mode.cli_entrypoint,
            status.goal_mode.context_source,
            status.goal_mode.default_goal_id,
            status.goal_mode.default_allowed_slots.join(","),
            status.goal_mode.checkpoint_policy.update_progress_log,
            status.goal_mode.checkpoint_policy.update_handoff,
            status.goal_mode.checkpoint_policy.commit_checkpoint,
            status.goal_mode.final_report_policy.include_validation,
            status.goal_mode.final_report_policy.include_next_steps,
            status.goal_mode.bypasses_governance,
            status.goal_mode.adds_core_slot
        );
        println!(
            "goal_run: ok={} plan_exists={} goal_id={} checkpoints={} workers={} validation_commands={} path={}",
            status.goal_run.ok,
            status.goal_run.plan_exists,
            status.goal_run.goal_id,
            status.goal_run.checkpoint_count,
            status.goal_run.worker_count,
            status.goal_run.validation_command_count,
            status.goal_run.path
        );
        println!(
            "goal_run_readiness: ok={} plan_exists={} goal_id={} checkpoints={} workers={} validation_commands={} checkpoint_log_complete={} last_checkpoint={} last_summary={} last_created_at={} last_completed_worker_ids={} last_validation_notes={} incomplete_reasons={}",
            status.goal_run.ok,
            status.goal_run.plan_exists,
            status.goal_run.goal_id,
            status.goal_run.checkpoint_count,
            status.goal_run.worker_count,
            status.goal_run.validation_command_count,
            status.goal_run.checkpoint_log_complete,
            status
                .goal_run
                .last_checkpoint_id
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_summary
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_created_at
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_completed_worker_ids
                .as_ref()
                .map(|values| values.join(","))
                .unwrap_or_else(|| "none".to_string()),
            status
                .goal_run
                .last_checkpoint_validation_notes
                .as_ref()
                .map(|values| values.join(" | "))
                .unwrap_or_else(|| "none".to_string()),
            if status.goal_run.incomplete_reasons.is_empty() {
                "none".to_string()
            } else {
                status.goal_run.incomplete_reasons.join(";")
            }
        );
        println!(
            "goal_run_checkpoint_log_complete: {}",
            status.goal_run.checkpoint_log_complete
        );
        println!(
            "goal_run_last_checkpoint: {}",
            status
                .goal_run
                .last_checkpoint_id
                .as_deref()
                .unwrap_or("none")
        );
        println!(
            "goal_run_last_checkpoint_summary: {}",
            status
                .goal_run
                .last_checkpoint_summary
                .as_deref()
                .unwrap_or("none")
        );
        println!(
            "goal_run_last_checkpoint_created_at: {}",
            status
                .goal_run
                .last_checkpoint_created_at
                .as_deref()
                .unwrap_or("none")
        );
        println!(
            "goal_run_last_checkpoint_completed_worker_ids: {}",
            status
                .goal_run
                .last_checkpoint_completed_worker_ids
                .as_ref()
                .map(|values| values.join(","))
                .unwrap_or_else(|| "none".to_string())
        );
        println!(
            "goal_run_last_checkpoint_validation_notes: {}",
            status
                .goal_run
                .last_checkpoint_validation_notes
                .as_ref()
                .map(|values| values.join(" | "))
                .unwrap_or_else(|| "none".to_string())
        );
        if let Some(read_error) = &status.goal_run.read_error {
            println!("goal_run_read_error: {read_error}");
        }
        println!(
            "goal_run_incomplete_reasons: {}",
            if status.goal_run.incomplete_reasons.is_empty() {
                "none".to_string()
            } else {
                status.goal_run.incomplete_reasons.join(";")
            }
        );
        println!(
            "local_contract_readiness: ok={} state={} contracts={} connects_real_external_services={} writes_core_memory={} executes_plugins={}",
            status.local_contract_readiness.ok,
            status.local_contract_readiness.overall_state,
            status.local_contract_readiness.contract_count,
            status.local_contract_readiness.connects_real_external_services,
            status.local_contract_readiness.writes_core_memory,
            status.local_contract_readiness.executes_plugins
        );
        println!(
            "subagent_readiness: ok={} state={} mode={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} layers={} ready={} partial={} deferred={} blocked={} live_worker_available={} worker_runtime_state={} worker_runtime_blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={}",
            status.subagent_readiness.ok,
            status.subagent_readiness.overall_state,
            status.subagent_readiness.mode,
            status.subagent_readiness.local_contract_ready,
            status.subagent_readiness.local_contract_state,
            status.subagent_readiness.live_adapter_ready,
            status.subagent_readiness.live_adapter_state,
            status.subagent_readiness.layer_count,
            status.subagent_readiness.ready_count,
            status.subagent_readiness.partial_count,
            status.subagent_readiness.deferred_count,
            status.subagent_readiness.blocked_count,
            status.subagent_readiness.live_worker_available,
            status.subagent_readiness.worker_runtime_state,
            status.subagent_readiness.worker_runtime_blocked_reason,
            status.subagent_readiness.capability_route_state,
            status.subagent_readiness.capability_mismatch_blocks_live,
            status.subagent_readiness.capability_mismatch_reason
        );
        println!(
            "subagent_worker_runtime_reason: {}",
            status.subagent_readiness.worker_runtime_reason
        );
        println!(
            "subagent_readiness_local_contract_reason: {}",
            status.subagent_readiness.local_contract_reason
        );
        println!(
            "subagent_readiness_live_adapter_reason: {}",
            status.subagent_readiness.live_adapter_reason
        );
        for layer in &status.subagent_readiness.layers {
            println!(
                "subagent_layer name={} state={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} live_worker_available={} worker_runtime_state={} blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={} boundary={} local_contract_reason={} live_adapter_reason={} next={}",
                layer.name,
                layer.state,
                layer.local_contract_ready,
                layer.local_contract_state,
                layer.live_adapter_ready,
                layer.live_adapter_state,
                layer.live_worker_available,
                layer.worker_runtime_state,
                layer.blocked_reason,
                layer.capability_route_state,
                layer.capability_mismatch_blocks_live,
                layer.capability_mismatch_reason,
                layer.boundary,
                layer.local_contract_reason,
                layer.live_adapter_reason,
                layer.next_action
            );
        }
        println!(
            "live_adapter_gates: ok={} state={} gates={} enabled={} disabled={}",
            status.live_adapter_gates.ok,
            status.live_adapter_gates.overall_state,
            status.live_adapter_gates.gate_count,
            status.live_adapter_gates.enabled_count,
            status.live_adapter_gates.disabled_count
        );
        for gate in &status.live_adapter_gates.gates {
            println!(
                "live_adapter_gate name={} state={} enabled={} default_enabled={} env_value_state={} required_env={} audit_label={} preflight={} must_reject={} reason={} next={}",
                gate.name,
                gate.state,
                gate.enabled,
                gate.default_enabled,
                gate.env_value_state,
                gate.required_env,
                gate.audit_label,
                format_text_list(&gate.preflight_checks),
                format_text_list(&gate.must_reject_capabilities),
                gate.reason,
                gate.next_action
            );
        }
        println!(
            "release_readiness: ok={} name={} state={}",
            status.release_readiness.ok,
            status.release_readiness.release_name,
            status.release_readiness.overall_state
        );
        println!(
            "release_acceptance: count={} connects_real_external_services={} verifies_real_external_services={} uses_stub_or_local_fixtures={}",
            status.release_readiness.acceptance_count,
            status.release_readiness.connects_real_external_services,
            status.release_readiness.verifies_real_external_services,
            status.release_readiness.uses_stub_or_local_fixtures
        );
        println!(
            "third_test_candidate: ok={} state={} local_gate_ready={} smoke_script={} marker={} requires_manual_live_check={} connects_real_external_services={} operator_env_blocks_100_percent={} real_live_ready={}",
            status.third_test_candidate.ok,
            status.third_test_candidate.overall_state,
            status.third_test_candidate.local_gate_ready,
            status.third_test_candidate.smoke_script,
            status.third_test_candidate.marker,
            status.third_test_candidate.requires_manual_live_check,
            status.third_test_candidate.connects_real_external_services,
            status.third_test_candidate.operator_env_blocks_100_percent,
            status.third_test_candidate.real_live_ready
        );
    }

    Ok(())
}

fn handle_initialize() -> Value {
    json!({
        "serverInfo": {
            "name": "chuang-agent-app-server",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {
            "threads": true,
            "models": true,
            "turns": true,
        }
    })
}

fn handle_model_list(params: &Value) -> Result<Value, String> {
    let workspace_root = params
        .get("workspaceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let runtime = build_runtime_for_workspace(&workspace_root)?;
    let model_name = provider_summary_model_name(&runtime);
    Ok(json!({
        "data": [{
            "id": model_name,
            "model": model_name,
            "displayName": model_name,
            "isDefault": true,
            "supportedReasoningEfforts": ["low", "medium", "high", "xhigh"],
            "defaultReasoningEffort": "medium",
        }]
    }))
}

fn handle_thread_start(state: &mut AppServerState, params: &Value) -> Result<Value, String> {
    let workspace_root = normalize_workspace_root(
        params
            .get("cwd")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    let display_name = params
        .get("displayName")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace thread".to_string());
    let thread = create_thread(state, workspace_root, display_name);
    Ok(json!({
        "thread": thread_to_json(&thread),
    }))
}

fn handle_thread_resume(state: &AppServerState, params: &Value) -> Result<Value, String> {
    let thread_id = params
        .get("threadId")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let Some(thread) = state.threads.get(thread_id) else {
        return Err(format!("unknown_thread: {thread_id}"));
    };

    Ok(json!({
        "thread": thread_to_resume_json(thread),
    }))
}

fn handle_thread_list(state: &AppServerState) -> Value {
    let mut threads = state
        .threads
        .values()
        .map(thread_to_list_json)
        .collect::<Vec<_>>();
    threads.sort_by(|left, right| right["updatedAt"].as_u64().cmp(&left["updatedAt"].as_u64()));
    json!({
        "data": threads,
        "nextCursor": "",
    })
}

fn handle_turn_start(state: &mut AppServerState, params: &Value) -> Result<Value, String> {
    let thread_id = normalize_text(params.get("threadId").and_then(|value| value.as_str()));
    let workspace_root = params
        .get("workspaceRoot")
        .and_then(|value| value.as_str())
        .map(normalize_workspace_root)
        .unwrap_or_default();
    let input_text = extract_turn_input_text(params);
    if input_text.is_empty() {
        return Err("turn/start requires non-empty input".to_string());
    }
    let goal_spec = extract_turn_goal(params)?;

    let thread_id = if thread_id.is_empty() {
        let thread = create_thread(
            state,
            workspace_root.clone(),
            thread_display_name(&workspace_root),
        );
        thread.id
    } else {
        if !state.threads.contains_key(&thread_id) {
            let thread = create_thread(
                state,
                workspace_root.clone(),
                thread_display_name(&workspace_root),
            );
            thread.id
        } else {
            thread_id
        }
    };

    let runtime = build_runtime_for_workspace(&workspace_root)?;
    let runtime = override_runtime_model(runtime, params);
    let context_max_tokens = runtime.context_budget.max_tokens;
    let started_at = Instant::now();
    let tool_run = run_turn_with_tools(
        &runtime,
        &thread_id,
        &workspace_root,
        &input_text,
        goal_spec,
    )?;
    let result = tool_run.result.clone();
    let tool_trace = tool_run.tool_trace.clone();
    let tool_calls = tool_run.tool_calls.clone();
    let tool_report = tool_run.tool_report.clone();
    let tool_surface = tool_run.tool_surface.clone();
    let tool_protocol_errors = tool_run.tool_protocol_errors.clone();
    let tool_events = tool_run.tool_events.clone();
    let tool_call_count = tool_calls.len();
    let tool_protocol_error_count = tool_protocol_errors.len();
    let runtime_observability = runtime_observability_meta(&result);
    let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let turn_id = next_turn_id(state);
    let assistant_text = result.response.body.clone();
    let model_name = result.response.model_name.clone();
    let status = app_server_turn_status(&result.response.meta.extra).to_string();
    let now = now_millis();
    let mut out = io::stdout();

    let thread = state
        .threads
        .get_mut(&thread_id)
        .ok_or_else(|| format!("unknown_thread: {thread_id}"))?;
    thread.updated_at = now;
    thread.turns.push(TurnState {
        id: turn_id.clone(),
        user_text: input_text.clone(),
        assistant_text: assistant_text.clone(),
        model_name: model_name.clone(),
        status: status.clone(),
        tool_trace: tool_trace.clone(),
        tool_surface: tool_surface.clone(),
        updated_at: now,
    });

    let _ = write_json_line(
        &mut out,
        &json!({
            "method": "turn/started",
            "params": {
                "threadId": thread_id,
                "turn": { "id": turn_id },
            }
        }),
    );
    let _ = write_json_line(
        &mut out,
        &json!({
            "method": "item/agentMessage/delta",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "delta": assistant_text,
            }
        }),
    );
    let _ = write_json_line(
        &mut out,
        &json!({
            "method": "item/completed",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "item": {
                    "type": "agentMessage",
                    "text": assistant_text,
                }
            }
        }),
    );
    let _ = write_json_line(
        &mut out,
        &json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": status,
                    "runtimeReportId": tool_run.runtime_report_id.clone(),
                    "toolCallCount": tool_call_count,
                    "toolProtocolErrorCount": tool_protocol_error_count,
                    "toolTrace": tool_trace.clone(),
                    "toolReport": tool_report.clone(),
                    "toolSurface": tool_surface.clone(),
                    "toolCalls": tool_calls
                        .iter()
                        .map(tool_execution_record_to_json)
                        .collect::<Vec<_>>(),
                    "toolProtocolErrors": tool_protocol_errors
                        .iter()
                        .map(tool_protocol_error_to_json)
                        .collect::<Vec<_>>(),
                    "toolEvents": tool_events,
                    "providerMeta": result.response.meta.extra.clone(),
                    "runtimeObservability": runtime_observability.clone(),
                }
            }
        }),
    );

    Ok(json!({
        "thread": thread_to_resume_json(
            state.threads.get(&thread_id).ok_or_else(|| format!("unknown_thread: {thread_id}"))?
        ),
        "turn": {
            "id": thread_turn_id(state, &thread_id).unwrap_or_default(),
            "status": status,
            "runtimeReportId": tool_run.runtime_report_id,
            "modelName": model_name,
            "finishReason": result
                .response
                .meta
                .finish_reason
                .clone()
                .unwrap_or_else(|| "completed".to_string()),
            "elapsedMs": elapsed_ms,
            "recallHitCount": result.recall_hit_count,
            "packedTokenCount": result.packed_token_count,
            "contextEngineKind": result.context_engine_kind,
            "contextMaxTokens": context_max_tokens,
            "providerMeta": result.response.meta.extra,
            "runtimeObservability": runtime_observability,
            "trace": result.response.trace,
            "apiCallCount": 1,
            "toolCallCount": tool_call_count,
            "toolProtocolErrorCount": tool_protocol_error_count,
            "toolTrace": tool_trace,
            "toolReport": tool_report,
            "toolSurface": tool_surface,
            "toolCalls": tool_calls
                .iter()
                .map(tool_execution_record_to_json)
                .collect::<Vec<_>>(),
            "toolProtocolErrors": tool_protocol_errors
                .iter()
                .map(tool_protocol_error_to_json)
                .collect::<Vec<_>>(),
            "toolEvents": tool_events,
        }
    }))
}

#[derive(Debug)]
struct ToolLoopResult {
    result: chuang_agent::agent_runtime::RuntimeResult,
    tool_calls: Vec<ToolExecutionRecord>,
    tool_protocol_errors: Vec<ToolProtocolError>,
    tool_events: Vec<Value>,
    tool_trace: String,
    tool_report: Option<Value>,
    tool_surface: Option<Value>,
    runtime_report_id: Option<String>,
}

fn run_turn_with_tools(
    runtime: &RuntimeConfig,
    thread_id: &str,
    workspace_root: &str,
    original_input: &str,
    goal_spec: Option<GoalSpec>,
) -> Result<ToolLoopResult, String> {
    let request = RunCliRequest {
        options: CliOptions {
            runtime: runtime.clone(),
        },
        user_input: original_input.to_string(),
        workspace_root: Some(PathBuf::from(workspace_root)),
        remember: false,
        session_id: Some(thread_id.to_string()),
        remember_session: true,
        remember_identity: false,
        remember_experience: false,
        dispatch_subagent: false,
        goal_spec,
        knowledge_context: None,
    };

    let (result, records) = run_with_options(&request)?;
    let tool_meta =
        ToolLoopMeta::<ToolExecutionRecord, ToolProtocolError, Value>::typed_from_extra(
            &result.response.meta.extra,
        )?;
    let tool_surface = parse_json_value(&result.response.meta.extra, "tool_surface_json")?;

    Ok(ToolLoopResult {
        result,
        tool_calls: tool_meta.tool_calls,
        tool_protocol_errors: tool_meta.tool_protocol_errors,
        tool_events: tool_meta.tool_events,
        tool_trace: tool_meta.tool_trace,
        tool_report: tool_meta.tool_report,
        tool_surface,
        runtime_report_id: records.runtime_report_id,
    })
}

fn app_server_turn_status(provider_meta: &BTreeMap<String, String>) -> &'static str {
    if provider_meta.contains_key("provider_failure_reason_code")
        || provider_meta.contains_key("provider_error_class")
        || provider_meta
            .get("provider_response_ok")
            .map(|value| value == "false")
            .unwrap_or(false)
    {
        "provider_error"
    } else {
        "completed"
    }
}

fn tool_execution_record_to_json(record: &ToolExecutionRecord) -> Value {
    json!({
        "tool": record.tool_name,
        "atomicTool": record.atomic_tool_name,
        "ok": record.ok,
        "summary": record.summary,
        "decision": record.decision,
        "durationMs": record.duration_ms,
        "retryable": record.retryable,
        "targetPath": record.target_path,
        "resolvedPath": record.resolved_path,
        "cwd": record.cwd,
        "command": record.command,
        "entries": record.entries,
        "outputBytes": record.output_bytes,
        "outputLines": record.output_lines,
        "stderrBytes": record.stderr_bytes,
        "stderrLines": record.stderr_lines,
        "output": record.output,
        "stdout": record.stdout,
        "stderr": record.stderr,
        "exitCode": record.exit_code,
        "changedFiles": record.changed_files,
        "writeBeforeBytes": record.write_before_bytes,
        "writeAfterBytes": record.write_after_bytes,
        "writeChanged": record.write_changed,
        "writeOperation": record.write_operation,
        "writeDiffPreview": record.write_diff_preview,
        "writeDiffTruncated": record.write_diff_truncated,
        "failureClass": record.failure_class,
        "outputRedacted": record.output_redacted,
        "stdoutRedacted": record.stdout_redacted,
        "stderrRedacted": record.stderr_redacted,
        "outputTruncated": record.output_truncated,
        "stdoutTruncated": record.stdout_truncated,
        "stderrTruncated": record.stderr_truncated,
        "call": &record.call,
    })
}

fn tool_protocol_error_to_json(error: &ToolProtocolError) -> Value {
    json!({
        "code": error.code,
        "message": error.message,
        "raw": error.raw,
    })
}

fn create_thread(
    state: &mut AppServerState,
    workspace_root: String,
    display_name: String,
) -> ThreadState {
    state.next_thread_seq += 1;
    let thread_id = format!("chuang-thread-{}", state.next_thread_seq);
    let now = now_millis();
    let thread = ThreadState {
        id: thread_id.clone(),
        workspace_root,
        display_name,
        created_at: now,
        updated_at: now,
        turns: Vec::new(),
    };
    state.threads.insert(thread_id, thread.clone());
    thread
}

fn next_turn_id(state: &mut AppServerState) -> String {
    state.next_turn_seq += 1;
    format!("chuang-turn-{}", state.next_turn_seq)
}

fn thread_turn_id(state: &AppServerState, thread_id: &str) -> Option<String> {
    state
        .threads
        .get(thread_id)?
        .turns
        .last()
        .map(|turn| turn.id.clone())
}

fn thread_to_json(thread: &ThreadState) -> Value {
    json!({
        "id": thread.id,
        "cwd": thread.workspace_root,
        "name": thread.display_name,
        "preview": thread.turns.last().map(|turn| turn.assistant_text.clone()).unwrap_or_default(),
        "createdAt": thread.created_at,
        "updatedAt": thread.updated_at,
        "sourceKind": "appServer",
        "turns": thread.turns.iter().map(turn_to_json).collect::<Vec<_>>(),
    })
}

fn thread_to_resume_json(thread: &ThreadState) -> Value {
    thread_to_json(thread)
}

fn thread_to_list_json(thread: &ThreadState) -> Value {
    json!({
        "id": thread.id,
        "cwd": thread.workspace_root,
        "name": thread.display_name,
        "preview": thread.turns.last().map(|turn| turn.assistant_text.clone()).unwrap_or_default(),
        "updatedAt": thread.updated_at,
        "sourceKind": "appServer",
    })
}

fn turn_to_json(turn: &TurnState) -> Value {
    json!({
        "id": turn.id,
        "updatedAt": turn.updated_at,
        "status": turn.status,
        "toolTrace": turn.tool_trace,
        "toolSurface": turn.tool_surface,
        "items": [
            {
                "type": "userMessage",
                "content": [
                    {
                        "type": "text",
                        "text": turn.user_text,
                    }
                ],
            },
            {
                "type": "agentMessage",
                "text": turn.assistant_text,
                "model": turn.model_name,
            }
        ]
    })
}

fn extract_turn_input_text(params: &Value) -> String {
    if let Some(text) = params.get("text").and_then(|value| value.as_str()) {
        return normalize_text(Some(text));
    }

    if let Some(input) = params.get("input").and_then(|value| value.as_array()) {
        let mut parts = Vec::new();
        for item in input {
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                let normalized = normalize_text(Some(text));
                if !normalized.is_empty() {
                    parts.push(normalized);
                }
            }
        }
        return parts.join("\n");
    }

    String::new()
}

fn extract_turn_goal(params: &Value) -> Result<Option<GoalSpec>, String> {
    let Some(goal) = params.get("goal").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    let goal = normalize_text(Some(goal));
    if goal.is_empty() {
        return Err("turn/start goal must not be empty".to_string());
    }
    Ok(Some(GoalSpec::mainline_mvp(goal)))
}

pub(crate) fn build_runtime_for_workspace(workspace_root: &str) -> Result<RuntimeConfig, String> {
    build_runtime_for_workspace_with_options(workspace_root, RuntimeConfigFileOptions::strict())
}

fn build_runtime_for_workspace_with_options(
    workspace_root: &str,
    options: RuntimeConfigFileOptions,
) -> Result<RuntimeConfig, String> {
    let base_dir = workspace_base_dir(workspace_root);
    let config_path = base_dir.join("config.toml");
    let mut runtime = if config_path.exists() {
        if options == RuntimeConfigFileOptions::strict() {
            load_runtime_config_file(&config_path)
                .map_err(|error| runtime_config_file_error(&error))?
        } else {
            load_runtime_config_file_with_options(&config_path, options)
                .map_err(|error| runtime_config_file_error(&error))?
        }
    } else {
        RuntimeConfig::new(base_dir.join("data/chuang-agent.db"))
    };

    normalize_runtime_paths(&mut runtime, &base_dir);
    Ok(runtime)
}

fn override_runtime_model(mut runtime: RuntimeConfig, params: &Value) -> RuntimeConfig {
    let requested_model = params
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(requested_model) = requested_model else {
        return runtime;
    };

    runtime.provider = match runtime.provider {
        ProviderConfig::Fake { provider_id, .. } => ProviderConfig::Fake {
            provider_id,
            model_name: requested_model,
        },
        ProviderConfig::OpenAICompatible(config) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                model_name: requested_model,
                ..config
            })
        }
        ProviderConfig::Fallback {
            primary,
            fallback,
            policy,
        } => ProviderConfig::Fallback {
            primary: Box::new(override_provider_model(*primary, requested_model)),
            fallback,
            policy,
        },
    };

    runtime
}

fn normalize_runtime_paths(runtime: &mut RuntimeConfig, base_dir: &Path) {
    runtime.db_path = resolve_path_if_relative(base_dir, runtime.db_path.clone());
    runtime.identity_memory = match runtime.identity_memory.clone() {
        IdentityMemoryConfig::HermesDualFile {
            root,
            user_max_chars,
            memory_max_chars,
        } => IdentityMemoryConfig::HermesDualFile {
            root: resolve_path_if_relative(base_dir, root),
            user_max_chars,
            memory_max_chars,
        },
    };
    runtime.subagent_queue = SubagentQueueConfig {
        root: resolve_path_if_relative(base_dir, runtime.subagent_queue.root.clone()),
    };
    runtime.identity_bootstrap = IdentityBootstrapConfig {
        root: resolve_path_if_relative(base_dir, runtime.identity_bootstrap.root.clone()),
        soul_path: resolve_path_if_relative(base_dir, runtime.identity_bootstrap.soul_path.clone()),
        story_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.story_path.clone(),
        ),
        first_wake_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.first_wake_path.clone(),
        ),
        agents_registry_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.agents_registry_path.clone(),
        ),
    };
    runtime.rules = RulesConfig {
        root: resolve_path_if_relative(base_dir, runtime.rules.root.clone()),
        core_path: resolve_path_if_relative(base_dir, runtime.rules.core_path.clone()),
    };
    normalize_provider_paths(&mut runtime.provider, base_dir);
}

fn override_provider_model(provider: ProviderConfig, model_name: String) -> ProviderConfig {
    match provider {
        ProviderConfig::Fake { provider_id, .. } => ProviderConfig::Fake {
            provider_id,
            model_name,
        },
        ProviderConfig::OpenAICompatible(config) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                model_name,
                ..config
            })
        }
        ProviderConfig::Fallback {
            primary,
            fallback,
            policy,
        } => ProviderConfig::Fallback {
            primary: Box::new(override_provider_model(*primary, model_name)),
            fallback,
            policy,
        },
    }
}

fn normalize_provider_paths(provider: &mut ProviderConfig, base_dir: &Path) {
    match provider {
        ProviderConfig::Fake { .. } => {}
        ProviderConfig::OpenAICompatible(config) => {
            if let Some(path) = &config.tls_ca_cert_path {
                config.tls_ca_cert_path = Some(resolve_path_if_relative(base_dir, path.clone()));
            }
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => {
            normalize_provider_paths(primary, base_dir);
            normalize_provider_paths(fallback, base_dir);
        }
    }
}

fn resolve_path_if_relative(base_dir: &Path, path: PathBuf) -> PathBuf {
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    if let Ok(canonical) = resolved.canonicalize() {
        canonical
    } else {
        normalize_path_lexically(&resolved)
    }
}

fn workspace_base_dir(workspace_root: &str) -> PathBuf {
    let trimmed = workspace_root.trim();
    if trimmed.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(trimmed)
    }
}

fn normalize_workspace_root(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| ".".to_string())
    } else {
        trimmed.to_string()
    }
}

fn thread_display_name(workspace_root: &str) -> String {
    let path = PathBuf::from(workspace_root);
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace thread".to_string())
}

fn runtime_config_file_error(error: &RuntimeConfigFileError) -> String {
    match error {
        RuntimeConfigFileError::ReadFailed { path } => {
            format!("runtime_config_read_failed: {}", path.display())
        }
        RuntimeConfigFileError::InvalidLine { line, content } => {
            format!("runtime_config_invalid_line:{line}:{content}")
        }
        RuntimeConfigFileError::InvalidValue { key, value } => {
            format!("runtime_config_invalid_value:{key}:{value}")
        }
        RuntimeConfigFileError::MissingEnv { name } => {
            format!("runtime_config_missing_env:{name}")
        }
    }
}

fn write_json_line(writer: &mut dyn Write, value: &Value) -> Result<(), String> {
    let rendered = serde_json::to_string(value).map_err(|e| format!("json_render_failed: {e}"))?;
    writer
        .write_all(rendered.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|e| format!("app_server_write_failed: {e}"))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn normalize_text(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_string()
}

fn provider_summary_model_name(runtime: &RuntimeConfig) -> String {
    match &runtime.provider {
        ProviderConfig::Fake { model_name, .. } => model_name.clone(),
        ProviderConfig::OpenAICompatible(OpenAICompatibleConfig { model_name, .. }) => {
            model_name.clone()
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => format!(
            "{}->{}",
            provider_config_model_name(primary),
            provider_config_model_name(fallback)
        ),
    }
}

pub(crate) fn app_server_health_diagnostic_status(summary: &ConfigSummary) -> &'static str {
    if summary.placeholder_warnings.is_empty() {
        "ready"
    } else {
        "warning"
    }
}

pub(crate) fn app_server_health_diagnostic_summary(
    summary: &ConfigSummary,
    diagnostic_mode: bool,
) -> String {
    if summary.placeholder_warnings.is_empty() {
        if diagnostic_mode {
            "app-server workspace config is ready in diagnostic mode; no live provider request was made."
                .to_string()
        } else {
            "app-server workspace config is ready; no live provider request was made.".to_string()
        }
    } else {
        let mode = if diagnostic_mode {
            "diagnostic mode"
        } else {
            "workspace config"
        };
        format!(
            "app-server {mode} loaded with {} local warning(s).",
            summary.placeholder_warnings.len()
        )
    }
}

pub(crate) fn app_server_health_next_actions(summary: &ConfigSummary) -> Vec<String> {
    let mut actions = Vec::new();

    if let Some(api_key_state) = &summary.api_key_state {
        if let Some(env_name) = api_key_state
            .strip_prefix("<missing:")
            .and_then(|value| value.strip_suffix('>'))
        {
            push_unique_action(
                &mut actions,
                format!(
                    "set {env_name} in the workspace environment before switching app-server out of diagnostic mode"
                ),
            );
        }
    }

    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("provider=fake"))
    {
        push_unique_action(
            &mut actions,
            "configure an openai_compatible provider for real conversation".to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("transport=stub"))
    {
        push_unique_action(
            &mut actions,
            "switch provider transport to native or curl for real calls".to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("actuator=fake"))
    {
        push_unique_action(
            &mut actions,
            "configure command-backed actuator before expecting desktop/browser operation"
                .to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("subagent=fake"))
    {
        push_unique_action(
            &mut actions,
            "configure queued_external subagents before expecting live worker dispatch".to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("control_plane=fake_local"))
    {
        push_unique_action(
            &mut actions,
            "configure command control before expecting real service control".to_string(),
        );
    }

    actions
}

pub(crate) fn push_unique_action(actions: &mut Vec<String>, action: String) {
    if !actions.iter().any(|existing| existing == &action) {
        actions.push(action);
    }
}

fn format_text_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}

fn provider_config_model_name(provider: &ProviderConfig) -> String {
    match provider {
        ProviderConfig::Fake { model_name, .. } => model_name.clone(),
        ProviderConfig::OpenAICompatible(OpenAICompatibleConfig { model_name, .. }) => {
            model_name.clone()
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => format!(
            "{}->{}",
            provider_config_model_name(primary),
            provider_config_model_name(fallback)
        ),
    }
}
