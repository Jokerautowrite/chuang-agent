use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::chuang_kernel::DEFAULT_MEMORY_WRITE_MAX_CHARS;
use chuang_agent::common::{AgentId, TaskId};
use chuang_agent::control_intent::{parse_control_intent, ControlIntentError, ControlIntentInput};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::hermes_memory::DEFAULT_USER_MEMORY_MAX_CHARS;
use chuang_agent::provider_openai_compatible::ProviderTransport;
use chuang_agent::runtime_config::{
    ContextEngineConfig, IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig,
    RuntimeConfig, SubagentConfig, SubagentQueueConfig,
};
use chuang_agent::runtime_config_file::{
    load_runtime_config_file_with_options, RuntimeConfigFileError, RuntimeConfigFileOptions,
};
use chuang_agent::subagent_spawner::{ContextIsolation, RunId, SpawnRequest, SubagentToolPolicy};

use crate::cli_output::{usage, ControlOutputFormat};
use crate::cli_runtime::default_db_path;
use crate::cli_types::*;

pub(crate) fn parse_control_output(args: &[String]) -> Result<ControlOutputFormat, String> {
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => index += 2,
            "--unit" | "--action" | "--reason" | "--model" => index += 2,
            "--approve" => index += 1,
            _ => return Err(usage()),
        }
    }
    Ok(output)
}

pub(crate) fn parse_control_runtime_options(args: &[String]) -> Result<CliOptions, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            "--json" | "--approve" => index += 1,
            "--unit" | "--action" | "--reason" | "--model" => index += 2,
            _ => return Err(usage()),
        }
    }
    parse_cli_options(&runtime_args)
}

pub(crate) fn parse_status_output(args: &[String]) -> Result<ControlOutputFormat, String> {
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => index += 2,
            _ => return Err(usage()),
        }
    }
    Ok(output)
}

pub(crate) fn parse_status_cli_options(args: &[String]) -> Result<CliOptions, String> {
    parse_cli_options_with_options(args, CliParseOptions::allow_missing_env())
}

pub(crate) fn parse_config_init(args: &[String]) -> Result<ConfigInitCliRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut path = PathBuf::from("config.toml");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--path" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                path = PathBuf::from(value);
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    Ok(ConfigInitCliRequest { output, path })
}

pub(crate) fn parse_genesis_ask(args: &[String]) -> Result<GenesisAskCliRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut prompt: Option<String> = None;
    let mut program = "autocli".to_string();
    let mut profile_dir = PathBuf::from("./deepseek_profile");
    let mut cdp_port = 9222u16;
    let mut timeout_ms = 30_000u64;
    let mut approve_exec = false;
    let mut dry_run = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--prompt" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "genesis ask requires value after --prompt".to_string())?;
                prompt = Some(value.clone());
                index += 2;
            }
            "--program" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "genesis ask requires value after --program".to_string())?;
                program = value.clone();
                index += 2;
            }
            "--profile-dir" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "genesis ask requires value after --profile-dir".to_string())?;
                profile_dir = PathBuf::from(value);
                index += 2;
            }
            "--cdp-port" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "genesis ask requires value after --cdp-port".to_string())?;
                cdp_port = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --cdp-port: {value}"))?;
                index += 2;
            }
            "--timeout-ms" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "genesis ask requires value after --timeout-ms".to_string())?;
                timeout_ms = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms: {value}"))?;
                index += 2;
            }
            "--approve-exec" => {
                approve_exec = true;
                index += 1;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let prompt = prompt.ok_or_else(|| "genesis ask requires --prompt".to_string())?;
    if prompt.trim().is_empty() {
        return Err("genesis ask requires non-empty --prompt".to_string());
    }
    if program.trim().is_empty() {
        return Err("genesis ask requires non-empty --program".to_string());
    }
    if !approve_exec && !dry_run {
        return Err("genesis_ask_requires_approve_exec: pass --approve-exec".to_string());
    }

    Ok(GenesisAskCliRequest {
        output,
        prompt,
        program,
        profile_dir,
        cdp_port,
        timeout_ms,
        approve_exec,
        dry_run,
    })
}

pub(crate) fn parse_control_apply(args: &[String]) -> Result<ControlApplyCliRequest, String> {
    let mut unit_id: Option<String> = None;
    let mut action: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut model_name: Option<String> = None;
    let mut approve = false;
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            flag if is_runtime_value_flag(flag) => index += 2,
            "--unit" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --unit".to_string())?;
                unit_id = Some(value.clone());
                index += 2;
            }
            "--action" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --action".to_string())?;
                action = Some(value.clone());
                index += 2;
            }
            "--reason" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --reason".to_string())?;
                reason = Some(value.clone());
                index += 2;
            }
            "--model" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --model".to_string())?;
                model_name = Some(value.clone());
                index += 2;
            }
            "--approve" => {
                approve = true;
                index += 1;
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let intent = ControlIntentInput {
        unit_id,
        action,
        reason,
        model_name,
    };
    parse_control_intent(intent.clone()).map_err(control_intent_error_to_cli)?;

    Ok(ControlApplyCliRequest {
        intent,
        approve,
        output,
    })
}

pub(crate) fn control_intent_error_to_cli(error: ControlIntentError) -> String {
    match error {
        ControlIntentError::MissingUnit => "control apply requires --unit".to_string(),
        ControlIntentError::MissingAction => "control apply requires --action".to_string(),
        ControlIntentError::MissingReason => "control apply requires --reason".to_string(),
        ControlIntentError::MissingModel => "--model is required for change-model".to_string(),
        ControlIntentError::UnknownUnit(unit) => format!("unknown control unit: {unit}"),
        ControlIntentError::AmbiguousUnit(unit) => format!("ambiguous control unit: {unit}"),
        ControlIntentError::UnsupportedAction(action) => {
            format!("unsupported control action: {action}")
        }
    }
}

pub(crate) fn parse_run_request(args: &[String]) -> Result<RunCliRequest, String> {
    let options = parse_cli_options(args)?;
    let mut user_input: Option<String> = None;
    let mut remember = false;
    let mut session_id: Option<String> = None;
    let mut remember_session = false;
    let mut remember_identity = false;
    let mut remember_experience = false;
    let mut dispatch_subagent = false;
    let mut goal_spec: Option<GoalSpec> = None;
    let mut knowledge_context_root: Option<PathBuf> = None;
    let mut knowledge_context_query: Option<String> = None;
    let mut knowledge_context_limit = 3usize;
    let mut knowledge_context_enabled = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            flag if is_runtime_value_flag(flag) => index += 2,
            "--input" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                user_input = Some(value.clone());
                index += 2;
            }
            "--remember" => {
                remember = true;
                index += 1;
            }
            "--session-id" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                if value.trim().is_empty() {
                    return Err("--session-id requires non-empty value".to_string());
                }
                session_id = Some(value.clone());
                index += 2;
            }
            "--remember-session" => {
                remember_session = true;
                index += 1;
            }
            "--remember-identity" => {
                remember_identity = true;
                index += 1;
            }
            "--remember-experience" => {
                remember_experience = true;
                index += 1;
            }
            "--dispatch-subagent" => {
                dispatch_subagent = true;
                index += 1;
            }
            "--goal" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                if value.trim().is_empty() {
                    return Err("--goal requires non-empty value".to_string());
                }
                goal_spec = Some(GoalSpec::mainline_mvp(value.clone()));
                index += 2;
            }
            "--knowledge-context-root" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                knowledge_context_root = Some(PathBuf::from(value));
                index += 2;
            }
            "--knowledge-context-query" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                if value.trim().is_empty() {
                    return Err("--knowledge-context-query requires non-empty value".to_string());
                }
                knowledge_context_query = Some(value.clone());
                index += 2;
            }
            "--knowledge-context-limit" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                knowledge_context_limit = value
                    .parse::<usize>()
                    .map_err(|_| "--knowledge-context-limit requires numeric value".to_string())?;
                if knowledge_context_limit == 0 {
                    return Err("--knowledge-context-limit must be greater than zero".to_string());
                }
                index += 2;
            }
            "--enable-knowledge-context-preview" => {
                knowledge_context_enabled = true;
                index += 1;
            }
            // Accepted by run_command (split_run_verbosity); ignore if present here.
            "--verbose" => {
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let knowledge_context = if knowledge_context_enabled {
        Some(KnowledgeContextCliRequest {
            root: knowledge_context_root.ok_or_else(|| {
                "--enable-knowledge-context-preview requires --knowledge-context-root".to_string()
            })?,
            query: knowledge_context_query.ok_or_else(|| {
                "--enable-knowledge-context-preview requires --knowledge-context-query".to_string()
            })?,
            limit: knowledge_context_limit,
            enabled: true,
        })
    } else {
        if knowledge_context_root.is_some()
            || knowledge_context_query.is_some()
            || knowledge_context_limit != 3
        {
            return Err(
                "knowledge context preview is disabled by default; pass --enable-knowledge-context-preview"
                    .to_string(),
            );
        }
        None
    };

    Ok(RunCliRequest {
        options,
        user_input: user_input.ok_or_else(usage)?,
        workspace_root: None,
        remember,
        session_id,
        remember_session,
        conversation_history: Vec::new(),
        remember_identity,
        remember_experience,
        dispatch_subagent,
        goal_spec,
        knowledge_context,
        live_guidance_path: None,
        progress_path: None,
    })
}

pub(crate) fn parse_subagent_dispatch(
    args: &[String],
) -> Result<SubagentDispatchCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut task: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut agent_name: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut token_budget: Option<u16> = None;
    let mut idle_timeout_ms: Option<u64> = None;
    let mut fork_parent_tokens: Option<u16> = None;
    let mut required_capabilities: Vec<String> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            "--task" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent dispatch requires value after --task".to_string())?;
                task = Some(value.clone());
                index += 2;
            }
            "--task-id" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --task-id".to_string()
                })?;
                task_id = Some(value.clone());
                index += 2;
            }
            "--agent-name" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --agent-name".to_string()
                })?;
                agent_name = Some(value.clone());
                index += 2;
            }
            "--policy" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent dispatch requires value after --policy".to_string())?;
                policy = Some(value.clone());
                index += 2;
            }
            "--token-budget" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --token-budget".to_string()
                })?;
                token_budget = Some(parse_u16_flag("--token-budget", value)?);
                index += 2;
            }
            "--idle-timeout-ms" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --idle-timeout-ms".to_string()
                })?;
                idle_timeout_ms = Some(parse_u64_flag("--idle-timeout-ms", value)?);
                index += 2;
            }
            "--fork-parent-tokens" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --fork-parent-tokens".to_string()
                })?;
                fork_parent_tokens = Some(parse_u16_flag("--fork-parent-tokens", value)?);
                index += 2;
            }
            "--requires-capability" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --requires-capability".to_string()
                })?;
                push_unique_capability(
                    &mut required_capabilities,
                    normalize_capability_flag("--requires-capability", value)?,
                );
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let task = task.ok_or_else(|| "subagent dispatch requires --task".to_string())?;
    let options = parse_cli_options(&runtime_args)?;
    let task_id = TaskId(task_id.unwrap_or_else(default_subagent_task_id));
    let context_isolation = fork_parent_tokens
        .map(|max_parent_tokens| ContextIsolation::Forked { max_parent_tokens })
        .unwrap_or(ContextIsolation::Isolated);
    let mut metadata = BTreeMap::from([("source".to_string(), "cli".to_string())]);
    if !required_capabilities.is_empty() {
        metadata.insert(
            "required_capabilities".to_string(),
            required_capabilities.join(","),
        );
    }

    let spawn = SpawnRequest {
        task_id: task_id.clone(),
        parent_agent_id: AgentId("chuang-cli".to_string()),
        agent_name: agent_name.unwrap_or_else(|| "worker".to_string()),
        task,
        tool_policy: parse_subagent_tool_policy(policy.as_deref())?,
        context_isolation,
        token_budget: token_budget.unwrap_or(1024),
        idle_timeout_ms: idle_timeout_ms.unwrap_or(30_000),
        recursive_spawn: false,
        metadata,
    };

    Ok(SubagentDispatchCliRequest {
        options,
        output,
        task_id,
        spawn,
    })
}

pub(crate) fn parse_subagent_report(args: &[String]) -> Result<SubagentReportCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut run_id: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            "--run-id" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent report requires value after --run-id".to_string())?;
                run_id = Some(value.clone());
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let options = parse_cli_options(&runtime_args)?;
    let run_id = RunId(run_id.ok_or_else(|| "subagent report requires --run-id".to_string())?);

    Ok(SubagentReportCliRequest {
        options,
        output,
        run_id,
    })
}

pub(crate) fn parse_subagent_collect(args: &[String]) -> Result<SubagentReportCliRequest, String> {
    parse_subagent_report(args)
}

pub(crate) fn parse_subagent_release_claim(
    args: &[String],
) -> Result<SubagentReleaseClaimCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut run_id: Option<String> = None;
    let mut reason: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            "--run-id" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent release-claim requires value after --run-id".to_string()
                })?;
                run_id = Some(value.clone());
                index += 2;
            }
            "--reason" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent release-claim requires value after --reason".to_string()
                })?;
                reason = Some(value.clone());
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let reason = reason.ok_or_else(|| "subagent release-claim requires --reason".to_string())?;
    if reason.trim().is_empty() {
        return Err("subagent release-claim requires non-empty --reason".to_string());
    }

    Ok(SubagentReleaseClaimCliRequest {
        options: parse_cli_options(&runtime_args)?,
        output,
        run_id: RunId(
            run_id.ok_or_else(|| "subagent release-claim requires --run-id".to_string())?,
        ),
        reason,
    })
}

pub(crate) fn parse_subagent_list(args: &[String]) -> Result<SubagentListCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    Ok(SubagentListCliRequest {
        options: parse_cli_options(&runtime_args)?,
        output,
    })
}

pub(crate) fn parse_subagent_run_once(
    args: &[String],
) -> Result<SubagentRunOnceCliRequest, String> {
    let parsed = parse_subagent_runner_args("subagent run-once", args, false)?;
    Ok(SubagentRunOnceCliRequest {
        options: parsed.options,
        output: parsed.output,
        runner: parsed.runner,
        runner_command: parsed.runner_command,
        runner_args: parsed.runner_args,
        worker_capabilities: parsed.worker_capabilities,
        approve_exec: parsed.approve_exec,
    })
}

pub(crate) fn parse_subagent_run_loop(
    args: &[String],
) -> Result<SubagentRunLoopCliRequest, String> {
    let parsed = parse_subagent_runner_args("subagent run-loop", args, true)?;
    Ok(SubagentRunLoopCliRequest {
        options: parsed.options,
        output: parsed.output,
        runner: parsed.runner,
        runner_command: parsed.runner_command,
        runner_args: parsed.runner_args,
        worker_capabilities: parsed.worker_capabilities,
        approve_exec: parsed.approve_exec,
        max_runs: parsed.max_runs.unwrap_or(10),
        max_concurrency: parsed.max_concurrency.unwrap_or(1),
        require_live_gate: parsed.require_live_gate,
        allowed_runner_commands: parsed.allowed_runner_commands,
    })
}

pub(crate) fn parse_subagent_live_preflight(
    args: &[String],
) -> Result<SubagentLivePreflightCliRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut runner: Option<String> = None;
    let mut runner_command: Option<String> = None;
    let mut allowed_runner_commands: Vec<String> = Vec::new();
    let mut required_capabilities: Vec<String> = Vec::new();
    let mut worker_capabilities: Vec<String> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--runner" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent live-preflight requires value after --runner".to_string()
                })?;
                runner = Some(value.clone());
                index += 2;
            }
            "--runner-command" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent live-preflight requires value after --runner-command".to_string()
                })?;
                runner_command = Some(value.clone());
                index += 2;
            }
            "--allow-runner-command" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent live-preflight requires value after --allow-runner-command"
                        .to_string()
                })?;
                if value.trim().is_empty() {
                    return Err("--allow-runner-command must not be empty".to_string());
                }
                if !allowed_runner_commands.contains(value) {
                    allowed_runner_commands.push(value.clone());
                }
                index += 2;
            }
            "--requires-capability" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent live-preflight requires value after --requires-capability".to_string()
                })?;
                push_unique_capability(
                    &mut required_capabilities,
                    normalize_capability_flag("--requires-capability", value)?,
                );
                index += 2;
            }
            "--capability" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent live-preflight requires value after --capability".to_string()
                })?;
                push_unique_capability(
                    &mut worker_capabilities,
                    normalize_capability_flag("--capability", value)?,
                );
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let runner = runner.unwrap_or_else(|| "command".to_string());
    if runner != "command" {
        return Err("subagent live-preflight only supports --runner command".to_string());
    }
    let runner_command = runner_command
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| "subagent live-preflight requires --runner-command".to_string())?;
    if allowed_runner_commands.is_empty() {
        return Err("subagent live-preflight requires --allow-runner-command".to_string());
    }

    Ok(SubagentLivePreflightCliRequest {
        output,
        runner,
        runner_command,
        allowed_runner_commands,
        required_capabilities,
        worker_capabilities,
    })
}

struct ParsedSubagentRunnerArgs {
    options: CliOptions,
    output: ControlOutputFormat,
    runner: String,
    runner_command: Option<String>,
    runner_args: Vec<String>,
    worker_capabilities: Vec<String>,
    approve_exec: bool,
    max_runs: Option<usize>,
    max_concurrency: Option<usize>,
    require_live_gate: bool,
    allowed_runner_commands: Vec<String>,
}

fn parse_subagent_runner_args(
    command_name: &str,
    args: &[String],
    allow_max_runs: bool,
) -> Result<ParsedSubagentRunnerArgs, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut runner: Option<String> = None;
    let mut runner_command: Option<String> = None;
    let mut runner_args: Vec<String> = Vec::new();
    let mut worker_capabilities: Vec<String> = Vec::new();
    let mut approve_exec = false;
    let mut max_runs: Option<usize> = None;
    let mut max_concurrency: Option<usize> = None;
    let mut require_live_gate = false;
    let mut allowed_runner_commands: Vec<String> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--runner" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{command_name} requires value after --runner"))?;
                runner = Some(value.clone());
                index += 2;
            }
            "--runner-command" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    format!("{command_name} requires value after --runner-command")
                })?;
                runner_command = Some(value.clone());
                index += 2;
            }
            "--runner-arg" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{command_name} requires value after --runner-arg"))?;
                runner_args.push(value.clone());
                index += 2;
            }
            "--capability" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{command_name} requires value after --capability"))?;
                push_unique_capability(
                    &mut worker_capabilities,
                    normalize_capability_flag("--capability", value)?,
                );
                index += 2;
            }
            "--max-runs" if allow_max_runs => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{command_name} requires value after --max-runs"))?;
                let parsed = parse_usize_flag("--max-runs", value)?;
                if parsed == 0 {
                    return Err("--max-runs must be greater than zero".to_string());
                }
                max_runs = Some(parsed);
                index += 2;
            }
            "--require-live-gate" if allow_max_runs => {
                require_live_gate = true;
                index += 1;
            }
            "--require-live-gate" => {
                return Err("--require-live-gate is only supported by subagent run-loop".to_string())
            }
            "--allow-runner-command" if allow_max_runs => {
                let value = args.get(index + 1).ok_or_else(|| {
                    format!("{command_name} requires value after --allow-runner-command")
                })?;
                if value.trim().is_empty() {
                    return Err("--allow-runner-command must not be empty".to_string());
                }
                allowed_runner_commands.push(value.clone());
                index += 2;
            }
            "--allow-runner-command" => {
                return Err(
                    "--allow-runner-command is only supported by subagent run-loop".to_string(),
                )
            }
            "--max-concurrency" if allow_max_runs => {
                let value = args.get(index + 1).ok_or_else(|| {
                    format!("{command_name} requires value after --max-concurrency")
                })?;
                let parsed = parse_usize_flag("--max-concurrency", value)?;
                if parsed == 0 {
                    return Err("--max-concurrency must be greater than zero".to_string());
                }
                if parsed > 8 {
                    return Err(
                        "--max-concurrency above 8 is not supported by the MVP worker loop"
                            .to_string(),
                    );
                }
                max_concurrency = Some(parsed);
                index += 2;
            }
            "--max-concurrency" => {
                return Err("--max-concurrency is only supported by subagent run-loop".to_string())
            }
            "--max-runs" => {
                return Err("--max-runs is only supported by subagent run-loop".to_string())
            }
            "--approve-exec" => {
                approve_exec = true;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let runner = runner.unwrap_or_else(|| "fake".to_string());
    match runner.as_str() {
        "fake" => {
            if runner_command.is_some() || !runner_args.is_empty() || approve_exec {
                return Err(
                    "subagent fake runner does not accept command execution flags".to_string(),
                );
            }
        }
        "command" => {
            if !approve_exec {
                return Err("command_runner_requires_approve_exec: pass --approve-exec".to_string());
            }
            if runner_command
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("command_runner_requires_runner_command".to_string());
            }
        }
        _ => {
            return Err(format!(
                "unsupported subagent runner: {runner} (supported: fake, command)"
            ));
        }
    }

    Ok(ParsedSubagentRunnerArgs {
        options: parse_cli_options(&runtime_args)?,
        output,
        runner,
        runner_command,
        runner_args,
        worker_capabilities,
        approve_exec,
        max_runs,
        max_concurrency,
        require_live_gate,
        allowed_runner_commands,
    })
}

fn normalize_capability_flag(flag_name: &str, value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(format!("{flag_name} must not be empty"));
    }
    if normalized.contains(',') {
        return Err(format!("{flag_name} must not contain comma"));
    }
    Ok(normalized)
}

fn push_unique_capability(capabilities: &mut Vec<String>, capability: String) {
    if !capabilities.contains(&capability) {
        capabilities.push(capability);
    }
}

pub(crate) fn parse_cli_options(args: &[String]) -> Result<CliOptions, String> {
    parse_cli_options_with_options(args, CliParseOptions::strict())
}

fn parse_cli_options_with_options(
    args: &[String],
    options: CliParseOptions,
) -> Result<CliOptions, String> {
    let config_path = find_config_path(args)?;
    let mut db_path: Option<PathBuf> = None;
    let mut provider_id: Option<String> = None;
    let mut provider_base_url: Option<String> = None;
    let mut provider_api_key: Option<String> = None;
    let mut provider_model: Option<String> = None;
    let mut provider_transport: Option<String> = None;
    let mut provider_request_timeout_ms: Option<u64> = None;
    let mut identity_memory_root: Option<PathBuf> = None;
    let mut subagent_kind: Option<String> = None;
    let mut subagent_queue_root: Option<PathBuf> = None;
    let mut context_engine: Option<String> = None;
    let mut context_max_tokens: Option<u32> = None;
    let mut context_reserve_system_tokens: Option<u32> = None;
    let mut context_min_working_tokens: Option<u32> = None;
    let mut context_max_tool_results: Option<usize> = None;
    let mut context_max_memory_segments: Option<usize> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                skip_value_arg(args, &mut index)?;
            }
            "--db" => {
                db_path = Some(PathBuf::from(take_value_or_usage(args, &mut index)?));
            }
            "--json" => index += 1,
            "--input" => index += 2,
            "--remember" => index += 1,
            "--session-id" => index += 2,
            "--remember-session" => index += 1,
            "--remember-identity" => index += 1,
            "--remember-experience" => index += 1,
            "--dispatch-subagent" => index += 1,
            "--goal" => index += 2,
            "--knowledge-context-root" => index += 2,
            "--knowledge-context-query" => index += 2,
            "--knowledge-context-limit" => index += 2,
            "--enable-knowledge-context-preview" => index += 1,
            "--provider-base-url" => {
                provider_base_url = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-api-key" => {
                provider_api_key = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-model" => {
                provider_model = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-transport" => {
                provider_transport = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-request-timeout-ms" => {
                let value = take_value_or_usage(args, &mut index)?;
                provider_request_timeout_ms =
                    Some(parse_u64_flag("--provider-request-timeout-ms", &value)?);
            }
            "--provider-id" => {
                provider_id = Some(take_value_or_usage(args, &mut index)?);
            }
            "--identity-memory-root" => {
                identity_memory_root = Some(PathBuf::from(take_value_or_usage(args, &mut index)?));
            }
            "--subagent" => {
                subagent_kind = Some(take_value_or_usage(args, &mut index)?);
            }
            "--subagent-queue-root" => {
                subagent_queue_root = Some(PathBuf::from(take_value_or_usage(args, &mut index)?));
            }
            "--context-engine" => {
                context_engine = Some(take_value_or_usage(args, &mut index)?);
            }
            "--context-max-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_tokens = Some(parse_u32_flag("--context-max-tokens", &value)?);
            }
            "--context-reserve-system-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_reserve_system_tokens =
                    Some(parse_u32_flag("--context-reserve-system-tokens", &value)?);
            }
            "--context-min-working-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_min_working_tokens =
                    Some(parse_u32_flag("--context-min-working-tokens", &value)?);
            }
            "--context-max-tool-results" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_tool_results =
                    Some(parse_usize_flag("--context-max-tool-results", &value)?);
            }
            "--context-max-memory-segments" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_memory_segments =
                    Some(parse_usize_flag("--context-max-memory-segments", &value)?);
            }
            _ => return Err(usage()),
        }
    }

    let mut runtime = if let Some(path) = config_path {
        load_runtime_config_file_with_options(&path, options.runtime_config_file)
            .map_err(format_runtime_config_file_error)?
    } else if let Some(path) = default_config_path() {
        load_runtime_config_file_with_options(&path, options.runtime_config_file)
            .map_err(format_runtime_config_file_error)?
    } else {
        RuntimeConfig::new(default_db_path())
    };
    if let Some(path) = db_path {
        runtime.db_path = path;
    }
    if let Some(root) = identity_memory_root {
        runtime.identity_memory = IdentityMemoryConfig::HermesDualFile {
            root,
            user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
            memory_max_chars: DEFAULT_MEMORY_WRITE_MAX_CHARS,
        };
    }
    if let Some(kind) = subagent_kind {
        runtime.subagent = parse_subagent_config(&kind)?;
    }
    if let Some(root) = subagent_queue_root {
        runtime.subagent_queue = SubagentQueueConfig { root };
    }
    if let Some(value) = context_engine {
        runtime.context_engine = parse_context_engine_config(&value)?;
    }
    if let Some(value) = context_max_tokens {
        runtime.context_budget.max_tokens = value;
    }
    if let Some(value) = context_reserve_system_tokens {
        runtime.context_budget.reserve_system_tokens = value;
    }
    if let Some(value) = context_min_working_tokens {
        runtime.context_budget.min_working_tokens = value;
    }
    if let Some(value) = context_max_tool_results {
        runtime.context_budget.max_tool_results = value;
    }
    if let Some(value) = context_max_memory_segments {
        runtime.context_budget.max_memory_segments = value;
    }
    runtime.provider = match (provider_base_url, provider_api_key, provider_model) {
        (None, None, None) => runtime.provider,
        (Some(base_url), Some(api_key), Some(model_name)) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                provider_id: provider_id.unwrap_or_else(|| "openai-compatible-cli".to_string()),
                base_url,
                api_key,
                model_name,
                transport: parse_provider_transport(provider_transport.as_deref())?,
                reasoning_effort: None,
                request_timeout_ms: provider_request_timeout_ms,
                tls_ca_cert_path: None,
            })
        }
        _ => {
            return Err(
                "provider config requires base_url + api_key + model (optional: provider_id)"
                    .to_string(),
            )
        }
    };

    Ok(CliOptions { runtime })
}

#[derive(Clone, Copy)]
struct CliParseOptions {
    runtime_config_file: RuntimeConfigFileOptions,
}

impl CliParseOptions {
    fn strict() -> Self {
        Self {
            runtime_config_file: RuntimeConfigFileOptions::strict(),
        }
    }

    fn allow_missing_env() -> Self {
        Self {
            runtime_config_file: RuntimeConfigFileOptions::allow_missing_env(),
        }
    }
}

pub(crate) fn effective_config_source(args: &[String]) -> Result<Option<String>, String> {
    Ok(find_config_path(args)?
        .or_else(default_config_path)
        .map(|path| path.display().to_string()))
}

fn is_runtime_value_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--db"
            | "--config"
            | "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport"
            | "--provider-request-timeout-ms"
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root"
            | "--context-engine"
            | "--context-max-tokens"
            | "--context-reserve-system-tokens"
            | "--context-min-working-tokens"
            | "--context-max-tool-results"
            | "--context-max-memory-segments"
    )
}

fn copy_runtime_value_arg(
    args: &[String],
    index: &mut usize,
    runtime_args: &mut Vec<String>,
) -> Result<(), String> {
    let flag = args[*index].clone();
    let value = take_value_or_usage(args, index)?;
    runtime_args.push(flag);
    runtime_args.push(value);
    Ok(())
}

fn skip_value_arg(args: &[String], index: &mut usize) -> Result<(), String> {
    take_value_or_usage(args, index).map(|_| ())
}

fn take_value_or_usage(args: &[String], index: &mut usize) -> Result<String, String> {
    let value = args.get(*index + 1).ok_or_else(usage)?.clone();
    *index += 2;
    Ok(value)
}

fn find_config_path(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut index = 0;
    let mut config_path = None;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                config_path = Some(PathBuf::from(value));
                index += 2;
            }
            _ => index += 1,
        }
    }
    Ok(config_path)
}

fn default_config_path() -> Option<PathBuf> {
    let path = PathBuf::from("config.toml");
    path.is_file().then_some(path)
}

fn format_runtime_config_file_error(err: RuntimeConfigFileError) -> String {
    match err {
        RuntimeConfigFileError::ReadFailed { path } => {
            format!("config_read_failed path={}", path.display())
        }
        RuntimeConfigFileError::InvalidLine { line, content } => {
            format!("config_invalid_line line={line} content={content}")
        }
        RuntimeConfigFileError::InvalidValue { key, value } => {
            format!("config_invalid_value key={key} value={value}")
        }
        RuntimeConfigFileError::MissingEnv { name } => {
            format!("config_missing_env name={name}")
        }
    }
}

fn parse_provider_transport(raw: Option<&str>) -> Result<ProviderTransport, String> {
    raw.unwrap_or("stub").parse()
}

fn parse_subagent_config(raw: &str) -> Result<SubagentConfig, String> {
    match raw {
        "fake" => Ok(SubagentConfig::Fake),
        "queued_external" => Ok(SubagentConfig::QueuedExternal),
        other => Err(format!(
            "unsupported subagent kind: {other} (supported: fake, queued_external)"
        )),
    }
}

fn parse_context_engine_config(raw: &str) -> Result<ContextEngineConfig, String> {
    match raw {
        "deterministic_budget" => Ok(ContextEngineConfig::DeterministicBudget),
        "summary_compression" => Ok(ContextEngineConfig::SummaryCompression),
        _ => Err(format!("unsupported context engine: {raw}")),
    }
}

fn parse_subagent_tool_policy(raw: Option<&str>) -> Result<SubagentToolPolicy, String> {
    match raw.unwrap_or("analyze") {
        "analyze" => Ok(SubagentToolPolicy::Analyze),
        "execute" => Ok(SubagentToolPolicy::Execute),
        "orchestrate" => Ok(SubagentToolPolicy::Orchestrate),
        other => Err(format!(
            "unsupported subagent policy: {other} (supported: analyze, execute, orchestrate)"
        )),
    }
}

fn parse_u16_flag(flag: &str, raw: &str) -> Result<u16, String> {
    raw.parse::<u16>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn parse_u32_flag(flag: &str, raw: &str) -> Result<u32, String> {
    raw.parse::<u32>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn parse_u64_flag(flag: &str, raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn parse_usize_flag(flag: &str, raw: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn default_subagent_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("cli-task-{nanos}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_run_request_accepts_goal_objective() {
        let config_path = fake_config_path("goal-accepts");
        let args = vec![
            "--config".to_string(),
            config_path.display().to_string(),
            "--input".to_string(),
            "继续推进主线".to_string(),
            "--goal".to_string(),
            "补齐 goal CLI 入口".to_string(),
        ];

        let request = parse_run_request(&args).expect("run request should parse");

        assert_eq!(request.user_input, "继续推进主线");
        let goal = request.goal_spec.expect("goal spec should be present");
        assert_eq!(goal.goal_id, "mainline-mvp");
        assert_eq!(goal.objective, "补齐 goal CLI 入口");
    }

    #[test]
    fn parse_run_request_rejects_empty_goal() {
        let config_path = fake_config_path("goal-empty");
        let args = vec![
            "--config".to_string(),
            config_path.display().to_string(),
            "--input".to_string(),
            "继续推进主线".to_string(),
            "--goal".to_string(),
            " ".to_string(),
        ];

        let error = parse_run_request(&args).expect_err("empty goal should fail");

        assert_eq!(error, "--goal requires non-empty value");
    }

    #[test]
    fn parse_run_request_accepts_explicit_knowledge_context_preview() {
        let config_path = fake_config_path("knowledge-context");
        let args = vec![
            "--config".to_string(),
            config_path.display().to_string(),
            "--input".to_string(),
            "检查外脑".to_string(),
            "--enable-knowledge-context-preview".to_string(),
            "--knowledge-context-root".to_string(),
            "/tmp/knowledge".to_string(),
            "--knowledge-context-query".to_string(),
            "marker".to_string(),
            "--knowledge-context-limit".to_string(),
            "2".to_string(),
        ];

        let request = parse_run_request(&args).expect("run request should parse");

        let knowledge = request
            .knowledge_context
            .expect("knowledge context should be present");
        assert!(knowledge.enabled);
        assert_eq!(knowledge.root, PathBuf::from("/tmp/knowledge"));
        assert_eq!(knowledge.query, "marker");
        assert_eq!(knowledge.limit, 2);
    }

    #[test]
    fn parse_run_request_rejects_knowledge_context_without_enable_flag() {
        let config_path = fake_config_path("knowledge-context-disabled");
        let args = vec![
            "--config".to_string(),
            config_path.display().to_string(),
            "--input".to_string(),
            "检查外脑".to_string(),
            "--knowledge-context-root".to_string(),
            "/tmp/knowledge".to_string(),
            "--knowledge-context-query".to_string(),
            "marker".to_string(),
        ];

        let error = parse_run_request(&args).expect_err("disabled preview should fail");

        assert_eq!(
            error,
            "knowledge context preview is disabled by default; pass --enable-knowledge-context-preview"
        );
    }

    fn fake_config_path(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "chuang-agent-cli-args-{name}-{}",
            default_subagent_task_id()
        ));
        fs::create_dir_all(&root).expect("temp root should be created");
        let path = root.join("config.toml");
        fs::write(
            &path,
            format!(
                "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"test\"\nmodel = \"stub\"\n",
                root.join("memory.db").display(),
                root.join("identity").display()
            ),
        )
        .expect("fake config should be written");
        path
    }
}
