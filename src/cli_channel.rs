//! `cli_channel` 模块。内部实现模块（无公开顶层项）。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::channel_adapter::{
    app_server_turn_start_request, ChannelInboundMessage, ChannelOutboundMessage,
};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::kernel_status::{build_chuang_mvp_status, LiveReadinessStatus};
use serde::Serialize;
use serde_json::Value;

use crate::app_server::build_runtime_for_workspace;
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_runtime::{kernel_config_from_runtime, run_with_options};
use crate::cli_types::{CliOptions, RunCliRequest};
use chuang_agent::runtime_report::runtime_observability_meta;
use chuang_agent::tool_loop_meta::{parse_json_value, ToolLoopMeta};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ChannelSimulateOutput {
    inbound: ChannelInboundMessage,
    app_server_request: Value,
    outbound: ChannelOutboundMessage,
    runtime_report_id: Option<String>,
    model_name: String,
    finish_reason: Option<String>,
    tool_call_count: usize,
    tool_protocol_error_count: usize,
    tool_trace: String,
    tool_report: Option<Value>,
    tool_surface: Option<Value>,
    tool_calls: Vec<Value>,
    tool_protocol_errors: Vec<Value>,
    tool_events: Vec<Value>,
    provider_meta: BTreeMap<String, String>,
    runtime_observability: BTreeMap<String, String>,
    live_readiness: LiveReadinessStatus,
}

pub(crate) fn channel_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("simulate") => channel_simulate_command(&args[1..]),
        Some("feishu-check") => channel_feishu_check_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn channel_feishu_check_command(args: &[String]) -> Result<(), String> {
    let request = parse_channel_feishu_check(args)?;
    let values = parse_env_file(&request.env_file)?;
    let required = [
        "CHUANG_FEISHU_APP_ID",
        "CHUANG_FEISHU_APP_SECRET",
        "CHUANG_AGENT_WORKSPACE_ROOT",
    ];
    let missing = required
        .iter()
        .filter(|name| {
            values
                .get(**name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    let legacy_var_names = values
        .keys()
        .filter(|key| forbidden_feishu_credential_env_names().contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let has_legacy_names = !legacy_var_names.is_empty();
    let workspace_root = values
        .get("CHUANG_AGENT_WORKSPACE_ROOT")
        .cloned()
        .unwrap_or_default();
    let workspace_path = PathBuf::from(&workspace_root);
    let workspace_root_exists = !workspace_root.trim().is_empty() && workspace_path.is_dir();
    let workspace_config_exists =
        workspace_root_exists && workspace_path.join("config.toml").is_file();
    let connection_mode = values
        .get("CHUANG_FEISHU_CONNECTION_MODE")
        .cloned()
        .unwrap_or_else(|| "websocket".to_string());
    let connection_mode_ok = matches!(connection_mode.as_str(), "websocket" | "webhook");
    let env_scope = classify_feishu_env_file_scope(&request.env_file);
    let next_actions = feishu_check_next_actions(
        &missing,
        has_legacy_names,
        workspace_root_exists,
        workspace_config_exists,
        connection_mode_ok,
        env_scope.is_chuang_scoped,
        &env_scope.warnings,
    );
    let diagnostic_status = if next_actions.is_empty() {
        "ready".to_string()
    } else {
        "blocked".to_string()
    };
    let diagnostic_summary = if next_actions.is_empty() {
        "Chuang Feishu env is ready for local bridge startup; no live Feishu call was made."
            .to_string()
    } else {
        format!(
            "Chuang Feishu env is not ready for bridge startup; {} local issue(s) need attention.",
            next_actions.len()
        )
    };
    let output = FeishuCheckOutput {
        ok: missing.is_empty()
            && !has_legacy_names
            && workspace_root_exists
            && workspace_config_exists
            && connection_mode_ok
            && env_scope.is_chuang_scoped,
        diagnostic_status,
        diagnostic_summary,
        next_actions,
        env_file: request.env_file.display().to_string(),
        env_file_is_chuang_scoped: env_scope.is_chuang_scoped,
        env_file_scope_warnings: env_scope.warnings,
        workspace_root,
        workspace_root_exists,
        workspace_config_exists,
        connection_mode,
        connection_mode_ok,
        required_vars: required
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    if values.contains_key(*name) {
                        "<set>".to_string()
                    } else {
                        "<missing>".to_string()
                    },
                )
            })
            .collect(),
        optional_vars: [
            "CHUANG_FEISHU_BOT_ID",
            "CHUANG_FEISHU_VERIFICATION_TOKEN",
            "CHUANG_FEISHU_ENCRYPT_KEY",
            "CHUANG_FEISHU_CONNECTION_MODE",
        ]
        .iter()
        .map(|name| {
            (
                name.to_string(),
                if values.contains_key(*name) {
                    "<set>".to_string()
                } else {
                    "<unset>".to_string()
                },
            )
        })
        .collect(),
        missing,
        legacy_var_names,
        has_legacy_names,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("feishu_check_ok: {}", output.ok);
            println!("diagnostic_status: {}", output.diagnostic_status);
            println!("diagnostic_summary: {}", output.diagnostic_summary);
            println!("env_file: {}", output.env_file);
            println!(
                "env_file_is_chuang_scoped: {}",
                output.env_file_is_chuang_scoped
            );
            if !output.env_file_scope_warnings.is_empty() {
                println!(
                    "env_file_scope_warnings: {}",
                    output.env_file_scope_warnings.join(",")
                );
            }
            println!("workspace_root: {}", output.workspace_root);
            println!("workspace_root_exists: {}", output.workspace_root_exists);
            println!(
                "workspace_config_exists: {}",
                output.workspace_config_exists
            );
            println!("connection_mode: {}", output.connection_mode);
            println!("connection_mode_ok: {}", output.connection_mode_ok);
            if output.missing.is_empty() {
                println!("missing: none");
            } else {
                println!("missing: {}", output.missing.join(","));
            }
            if !output.optional_vars.is_empty() {
                println!("optional_vars: {}", output.optional_vars.len());
            }
            println!("has_legacy_names: {}", output.has_legacy_names);
            if !output.legacy_var_names.is_empty() {
                println!("legacy_var_names: {}", output.legacy_var_names.join(","));
            }
            if output.next_actions.is_empty() {
                println!("next_actions: none");
            } else {
                println!("next_actions: {}", output.next_actions.join(";"));
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn forbidden_feishu_credential_env_names() -> &'static [&'static str] {
    &[
        "FEISHU_APP_ID",
        "FEISHU_APP_SECRET",
        "FEISHU_BOT_ID",
        "FEISHU_VERIFICATION_TOKEN",
        "FEISHU_ENCRYPT_KEY",
        "HERMES_FEISHU_APP_ID",
        "HERMES_FEISHU_APP_SECRET",
        "HERMES_FEISHU_BOT_ID",
        "HERMES_FEISHU_VERIFICATION_TOKEN",
        "HERMES_FEISHU_ENCRYPT_KEY",
        "CODEX_FEISHU_APP_ID",
        "CODEX_FEISHU_APP_SECRET",
        "CODEX_FEISHU_BOT_ID",
        "CODEX_FEISHU_VERIFICATION_TOKEN",
        "CODEX_FEISHU_ENCRYPT_KEY",
    ]
}

fn channel_simulate_command(args: &[String]) -> Result<(), String> {
    let request = parse_channel_simulate(args)?;
    let app_server_request = app_server_turn_start_request(1, &request.inbound)?;
    let mut runtime = build_runtime_for_workspace(&request.inbound.workspace_root)?;
    runtime
        .metadata
        .insert("channel".to_string(), request.inbound.channel.clone());
    let kernel = kernel_config_from_runtime(&runtime)?;
    let live_readiness = build_chuang_mvp_status(&runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?
        .live_readiness;
    let thread_id = request
        .inbound
        .thread_id
        .clone()
        .unwrap_or_else(|| format!("channel-{}", request.inbound.message_id));
    let (result, records) = run_with_options(&RunCliRequest {
        options: CliOptions { runtime },
        user_input: request.inbound.text.clone(),
        workspace_root: Some(PathBuf::from(&request.inbound.workspace_root)),
        remember: false,
        session_id: Some(thread_id.clone()),
        remember_session: true,
        conversation_history: Vec::new(),
        remember_identity: false,
        remember_experience: false,
        dispatch_subagent: false,
        goal_spec: request
            .inbound
            .goal
            .as_ref()
            .map(|goal| GoalSpec::mainline_mvp(goal.clone())),
        knowledge_context: None,
        live_guidance_path: None,
        progress_path: None,
    })?;
    let tool_meta = ToolLoopMeta::from_extra(&result.response.meta.extra)?;
    let runtime_observability = runtime_observability_meta(&result);
    let outbound = ChannelOutboundMessage {
        channel: request.inbound.channel.clone(),
        message_id: request.inbound.message_id.clone(),
        thread_id: Some(thread_id),
        text: result.response.body.clone(),
    };
    let output = ChannelSimulateOutput {
        inbound: request.inbound,
        app_server_request,
        outbound,
        model_name: result.response.model_name,
        finish_reason: result.response.meta.finish_reason,
        tool_call_count: tool_meta.tool_call_count,
        tool_protocol_error_count: tool_meta.tool_protocol_error_count,
        tool_trace: tool_meta.tool_trace,
        tool_report: tool_meta.tool_report,
        tool_surface: parse_json_value(&result.response.meta.extra, "tool_surface_json")?,
        runtime_report_id: records.runtime_report_id,
        tool_calls: tool_meta.tool_calls,
        tool_protocol_errors: tool_meta.tool_protocol_errors,
        tool_events: tool_meta.tool_events,
        provider_meta: result.response.meta.extra,
        runtime_observability,
        live_readiness,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("channel: {}", output.outbound.channel);
            println!("message_id: {}", output.outbound.message_id);
            println!(
                "thread_id: {}",
                output.outbound.thread_id.as_deref().unwrap_or("none")
            );
            println!(
                "runtime_report_id: {}",
                output.runtime_report_id.as_deref().unwrap_or("none")
            );
            println!("model: {}", output.model_name);
            if let Some(reason) = &output.finish_reason {
                println!("finish_reason: {reason}");
            }
            println!("tool_call_count: {}", output.tool_call_count);
            println!(
                "live_readiness_state: {}",
                output.live_readiness.overall_state
            );
            println!(
                "live_readiness_real_external_acceptance_pending: {}",
                output.live_readiness.real_external_acceptance_pending
            );
            println!(
                "live_readiness_ready_does_not_mean_live: {}",
                output.live_readiness.ready_does_not_mean_live
            );
            println!(
                "tool_surface_available: {}",
                format_tool_surface_bool(&output.tool_surface, "available")
            );
            println!(
                "tool_surface_governed: {}",
                format_tool_surface_bool(&output.tool_surface, "governed")
            );
            println!(
                "tool_surface_callable_tools: {}",
                format_tool_surface_callable_tools(&output.tool_surface)
            );
            println!(
                "tool_unified_execution_status: {}",
                output
                    .runtime_observability
                    .get("tool_unified_execution_status")
                    .map(String::as_str)
                    .unwrap_or("unknown")
            );
            println!(
                "tool_unified_execution_failure_count: {}",
                output
                    .runtime_observability
                    .get("tool_unified_execution_failure_count")
                    .map(String::as_str)
                    .unwrap_or("0")
            );
            println!(
                "tool_protocol_error_count: {}",
                output.tool_protocol_error_count
            );
            println!(
                "tool_protocol_error_codes: {}",
                format_tool_protocol_error_codes(&output.tool_protocol_errors)
            );
            println!("reply: {}", output.outbound.text);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn format_tool_surface_bool(surface: &Option<Value>, key: &str) -> String {
    surface
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_tool_surface_callable_tools(surface: &Option<Value>) -> String {
    let tools = surface
        .as_ref()
        .and_then(|value| value.get("callable_tools"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(",")
    }
}

fn format_tool_protocol_error_codes(errors: &[Value]) -> String {
    let codes = errors
        .iter()
        .filter_map(|error| error.get("code").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if codes.is_empty() {
        "none".to_string()
    } else {
        codes.join(",")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelSimulateRequest {
    inbound: ChannelInboundMessage,
    output: ControlOutputFormat,
}

fn parse_channel_simulate(args: &[String]) -> Result<ChannelSimulateRequest, String> {
    let mut channel = "feishu-dedicated-chuang".to_string();
    let mut message_id: Option<String> = None;
    let mut sender_id: Option<String> = None;
    let mut workspace_root: Option<PathBuf> = None;
    let mut text: Option<String> = None;
    let mut thread_id: Option<String> = None;
    let mut goal: Option<String> = None;
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--channel" => {
                channel = take_value(args, &mut index, "--channel")?;
            }
            "--message-id" => {
                message_id = Some(take_value(args, &mut index, "--message-id")?);
            }
            "--sender-id" => {
                sender_id = Some(take_value(args, &mut index, "--sender-id")?);
            }
            "--workspace-root" => {
                workspace_root = Some(PathBuf::from(take_value(
                    args,
                    &mut index,
                    "--workspace-root",
                )?));
            }
            "--text" => {
                text = Some(take_value(args, &mut index, "--text")?);
            }
            "--thread-id" => {
                thread_id = Some(take_value(args, &mut index, "--thread-id")?);
            }
            "--goal" => {
                let value = take_value(args, &mut index, "--goal")?;
                if value.trim().is_empty() {
                    return Err("channel simulate requires non-empty --goal".to_string());
                }
                goal = Some(value);
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let workspace_root = workspace_root
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let inbound = ChannelInboundMessage {
        channel,
        message_id: message_id
            .ok_or_else(|| "channel simulate requires --message-id".to_string())?,
        sender_id: sender_id.ok_or_else(|| "channel simulate requires --sender-id".to_string())?,
        workspace_root: workspace_root.display().to_string(),
        text: text.ok_or_else(|| "channel simulate requires --text".to_string())?,
        thread_id,
        goal,
    };
    inbound.validate()?;

    Ok(ChannelSimulateRequest { inbound, output })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("channel simulate requires value after {flag}"))?
        .clone();
    *index += 2;
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelFeishuCheckRequest {
    env_file: PathBuf,
    output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FeishuCheckOutput {
    ok: bool,
    diagnostic_status: String,
    diagnostic_summary: String,
    next_actions: Vec<String>,
    env_file: String,
    env_file_is_chuang_scoped: bool,
    env_file_scope_warnings: Vec<String>,
    workspace_root: String,
    workspace_root_exists: bool,
    workspace_config_exists: bool,
    connection_mode: String,
    connection_mode_ok: bool,
    required_vars: BTreeMap<String, String>,
    optional_vars: BTreeMap<String, String>,
    missing: Vec<String>,
    legacy_var_names: Vec<String>,
    has_legacy_names: bool,
}

fn feishu_check_next_actions(
    missing: &[String],
    has_legacy_names: bool,
    workspace_root_exists: bool,
    workspace_config_exists: bool,
    connection_mode_ok: bool,
    env_file_is_chuang_scoped: bool,
    env_file_scope_warnings: &[String],
) -> Vec<String> {
    let mut actions = Vec::new();
    if !missing.is_empty() {
        actions.push(format!("set_missing_chuang_env_vars:{}", missing.join(",")));
    }
    if !env_file_is_chuang_scoped {
        let warning = if env_file_scope_warnings.is_empty() {
            "env_file_not_chuang_scoped".to_string()
        } else {
            env_file_scope_warnings.join(",")
        };
        actions.push(format!("use_chuang_scoped_env_file:{warning}"));
    }
    if has_legacy_names {
        actions.push("remove_legacy_feishu_env_names".to_string());
    }
    if !workspace_root_exists {
        actions.push("fix_chuang_agent_workspace_root".to_string());
    } else if !workspace_config_exists {
        actions.push("add_or_fix_workspace_config_toml".to_string());
    }
    if !connection_mode_ok {
        actions.push("set_chuang_feishu_connection_mode_to_websocket_or_webhook".to_string());
    }
    actions
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeishuEnvFileScope {
    is_chuang_scoped: bool,
    warnings: Vec<String>,
}

fn classify_feishu_env_file_scope(path: &PathBuf) -> FeishuEnvFileScope {
    let normalized = path.display().to_string().replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut warnings = Vec::new();

    if lower.ends_with("/.codex-im/.env") {
        warnings.push("env_file_looks_like_codex_im_default_env".to_string());
    }
    if lower.contains("codex-feishu") {
        warnings.push("env_file_looks_like_codex_feishu_bridge".to_string());
    }
    if lower.contains("hermes-gateway") || lower.contains("hermes-feishu") {
        warnings.push("env_file_looks_like_hermes_channel_env".to_string());
    }

    let explicitly_chuang = file_name.contains("chuang")
        || lower.contains("/chuang-agent/")
        || lower.contains("chuang-feishu");
    let is_chuang_scoped = explicitly_chuang && warnings.is_empty();
    if !explicitly_chuang {
        warnings.push("env_file_name_should_be_chuang_scoped".to_string());
    }

    FeishuEnvFileScope {
        is_chuang_scoped,
        warnings,
    }
}

fn parse_channel_feishu_check(args: &[String]) -> Result<ChannelFeishuCheckRequest, String> {
    let mut env_file = None;
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--env-file" => {
                env_file = Some(PathBuf::from(take_value(args, &mut index, "--env-file")?));
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }
    Ok(ChannelFeishuCheckRequest {
        env_file: env_file.ok_or_else(|| "channel feishu-check requires --env-file".to_string())?,
        output,
    })
}

fn parse_env_file(path: &PathBuf) -> Result<BTreeMap<String, String>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("feishu_env_read_failed: {e}"))?;
    let mut values = BTreeMap::new();
    for (line_index, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("feishu_env_invalid_line:{}", line_index + 1));
        };
        values.insert(
            key.trim().to_string(),
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        );
    }
    Ok(values)
}
