use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::channel_adapter::{
    app_server_turn_start_request, ChannelInboundMessage, ChannelOutboundMessage,
};
use serde::Serialize;
use serde_json::Value;

use crate::app_server::build_runtime_for_workspace;
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_runtime::run_with_options;
use crate::cli_types::{CliOptions, RunCliRequest};
use chuang_agent::tool_loop_meta::ToolLoopMeta;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ChannelSimulateOutput {
    inbound: ChannelInboundMessage,
    app_server_request: Value,
    outbound: ChannelOutboundMessage,
    model_name: String,
    finish_reason: Option<String>,
    tool_call_count: usize,
    tool_protocol_error_count: usize,
    tool_trace: String,
    tool_report: Option<Value>,
    tool_calls: Vec<Value>,
    tool_protocol_errors: Vec<Value>,
    tool_events: Vec<Value>,
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
    let has_legacy_names = values.keys().any(|key| {
        matches!(
            key.as_str(),
            "FEISHU_APP_ID" | "FEISHU_APP_SECRET" | "FEISHU_BOT_ID" | "HERMES_FEISHU_APP_ID"
        )
    });
    let output = FeishuCheckOutput {
        ok: missing.is_empty() && !has_legacy_names,
        env_file: request.env_file.display().to_string(),
        workspace_root: values
            .get("CHUANG_AGENT_WORKSPACE_ROOT")
            .cloned()
            .unwrap_or_default(),
        connection_mode: values
            .get("CHUANG_FEISHU_CONNECTION_MODE")
            .cloned()
            .unwrap_or_else(|| "websocket".to_string()),
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
        has_legacy_names,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("feishu_check_ok: {}", output.ok);
            println!("env_file: {}", output.env_file);
            println!("workspace_root: {}", output.workspace_root);
            println!("connection_mode: {}", output.connection_mode);
            if output.missing.is_empty() {
                println!("missing: none");
            } else {
                println!("missing: {}", output.missing.join(","));
            }
            if !output.optional_vars.is_empty() {
                println!("optional_vars: {}", output.optional_vars.len());
            }
            println!("has_legacy_names: {}", output.has_legacy_names);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn channel_simulate_command(args: &[String]) -> Result<(), String> {
    let request = parse_channel_simulate(args)?;
    let app_server_request = app_server_turn_start_request(1, &request.inbound)?;
    let runtime = build_runtime_for_workspace(&request.inbound.workspace_root)?;
    let thread_id = request
        .inbound
        .thread_id
        .clone()
        .unwrap_or_else(|| format!("channel-{}", request.inbound.message_id));
    let (result, _) = run_with_options(&RunCliRequest {
        options: CliOptions { runtime },
        user_input: request.inbound.text.clone(),
        workspace_root: Some(PathBuf::from(&request.inbound.workspace_root)),
        remember: false,
        session_id: Some(thread_id.clone()),
        remember_session: true,
        remember_identity: false,
        dispatch_subagent: false,
    })?;
    let tool_meta = ToolLoopMeta::from_extra(&result.response.meta.extra)?;
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
        tool_calls: tool_meta.tool_calls,
        tool_protocol_errors: tool_meta.tool_protocol_errors,
        tool_events: tool_meta.tool_events,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("channel: {}", output.outbound.channel);
            println!("message_id: {}", output.outbound.message_id);
            println!(
                "thread_id: {}",
                output.outbound.thread_id.as_deref().unwrap_or("none")
            );
            println!("model: {}", output.model_name);
            if let Some(reason) = &output.finish_reason {
                println!("finish_reason: {reason}");
            }
            println!("tool_call_count: {}", output.tool_call_count);
            println!(
                "tool_protocol_error_count: {}",
                output.tool_protocol_error_count
            );
            println!("reply: {}", output.outbound.text);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
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
    env_file: String,
    workspace_root: String,
    connection_mode: String,
    required_vars: BTreeMap<String, String>,
    optional_vars: BTreeMap<String, String>,
    missing: Vec<String>,
    has_legacy_names: bool,
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
