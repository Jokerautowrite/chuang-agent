use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::cli_runtime::run_with_options;
use crate::cli_types::{CliOptions, RunCliRequest};
use chuang_agent::runtime_config::{
    IdentityBootstrapConfig, IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig,
    RulesConfig, RuntimeConfig, SubagentQueueConfig,
};
use chuang_agent::runtime_config_file::{load_runtime_config_file, RuntimeConfigFileError};
use chuang_agent::tool_loop_meta::ToolLoopMeta;
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
            _ => {
                return Err(
                    "usage: cargo run -- app-server health [--workspace-root PATH] [--json]"
                        .to_string(),
                )
            }
        }
    }

    let normalized_workspace_root = normalize_workspace_root(&workspace_root);
    let runtime = build_runtime_for_workspace(&normalized_workspace_root)?;
    runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
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
        println!(
            "workspace_root: {}",
            result["workspace_root"].as_str().unwrap_or("")
        );
        println!("model: {}", result["model"].as_str().unwrap_or(""));
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
    let thread = create_thread(state, workspace_root, "workspace thread".to_string());
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
    let tool_run = run_turn_with_tools(&runtime, &thread_id, &workspace_root, &input_text)?;
    let result = tool_run.result.clone();
    let tool_trace = tool_run.tool_trace.clone();
    let tool_calls = tool_run.tool_calls.clone();
    let tool_report = tool_run.tool_report.clone();
    let tool_protocol_errors = tool_run.tool_protocol_errors.clone();
    let tool_events = tool_run.tool_events.clone();
    let tool_call_count = tool_calls.len();
    let tool_protocol_error_count = tool_protocol_errors.len();
    let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let turn_id = next_turn_id(state);
    let assistant_text = result.response.body.clone();
    let model_name = result.response.model_name.clone();
    let status = result
        .response
        .meta
        .finish_reason
        .clone()
        .unwrap_or_else(|| "completed".to_string());
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
                    "status": "completed",
                    "toolCallCount": tool_call_count,
                    "toolProtocolErrorCount": tool_protocol_error_count,
                    "toolTrace": tool_trace.clone(),
                    "toolReport": tool_report.clone(),
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
            }
        }),
    );

    Ok(json!({
        "thread": thread_to_resume_json(
            state.threads.get(&thread_id).ok_or_else(|| format!("unknown_thread: {thread_id}"))?
        ),
        "turn": {
            "id": thread_turn_id(state, &thread_id).unwrap_or_default(),
            "status": "completed",
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
            "trace": result.response.trace,
            "apiCallCount": 1,
            "toolCallCount": tool_call_count,
            "toolProtocolErrorCount": tool_protocol_error_count,
            "toolTrace": tool_trace,
            "toolReport": tool_report,
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
}

fn run_turn_with_tools(
    runtime: &RuntimeConfig,
    thread_id: &str,
    workspace_root: &str,
    original_input: &str,
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
        dispatch_subagent: false,
    };

    let (result, _) = run_with_options(&request)?;
    let tool_meta =
        ToolLoopMeta::<ToolExecutionRecord, ToolProtocolError, Value>::typed_from_extra(
            &result.response.meta.extra,
        )?;

    Ok(ToolLoopResult {
        result,
        tool_calls: tool_meta.tool_calls,
        tool_protocol_errors: tool_meta.tool_protocol_errors,
        tool_events: tool_meta.tool_events,
        tool_trace: tool_meta.tool_trace,
        tool_report: tool_meta.tool_report,
    })
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

pub(crate) fn build_runtime_for_workspace(workspace_root: &str) -> Result<RuntimeConfig, String> {
    let base_dir = workspace_base_dir(workspace_root);
    let config_path = base_dir.join("config.toml");
    let mut runtime = if config_path.exists() {
        load_runtime_config_file(&config_path).map_err(|error| runtime_config_file_error(&error))?
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

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
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
