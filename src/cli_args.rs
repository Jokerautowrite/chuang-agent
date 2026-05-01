use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::chuang_kernel::DEFAULT_MEMORY_WRITE_MAX_CHARS;
use chuang_agent::common::{AgentId, TaskId};
use chuang_agent::control_intent::{parse_control_intent, ControlIntentError, ControlIntentInput};
use chuang_agent::hermes_memory::DEFAULT_USER_MEMORY_MAX_CHARS;
use chuang_agent::provider_openai_compatible::ProviderTransport;
use chuang_agent::runtime_config::{
    IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig, RuntimeConfig, SubagentConfig,
    SubagentQueueConfig,
};
use chuang_agent::runtime_config_file::{load_runtime_config_file, RuntimeConfigFileError};
use chuang_agent::subagent_spawner::{ContextIsolation, RunId, SpawnRequest, SubagentToolPolicy};

use crate::cli_output::{usage, ControlOutputFormat};
use crate::cli_runtime::default_db_path;
use crate::cli_types::*;

pub(crate) fn parse_control_output(args: &[String]) -> Result<ControlOutputFormat, String> {
    let mut output = ControlOutputFormat::Text;
    for arg in args {
        match arg.as_str() {
            "--json" => output = ControlOutputFormat::Json,
            _ => return Err(usage()),
        }
    }
    Ok(output)
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
    let mut remember_identity = false;
    let mut dispatch_subagent = false;

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
            "--remember-identity" => {
                remember_identity = true;
                index += 1;
            }
            "--dispatch-subagent" => {
                dispatch_subagent = true;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(RunCliRequest {
        options,
        user_input: user_input.ok_or_else(usage)?,
        remember,
        remember_identity,
        dispatch_subagent,
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
            _ => return Err(usage()),
        }
    }

    let task = task.ok_or_else(|| "subagent dispatch requires --task".to_string())?;
    let options = parse_cli_options(&runtime_args)?;
    let task_id = TaskId(task_id.unwrap_or_else(default_subagent_task_id));
    let context_isolation = fork_parent_tokens
        .map(|max_parent_tokens| ContextIsolation::Forked { max_parent_tokens })
        .unwrap_or(ContextIsolation::Isolated);
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
        metadata: BTreeMap::from([("source".to_string(), "cli".to_string())]),
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
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut runner: Option<String> = None;

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
                    .ok_or_else(|| "subagent run-once requires value after --runner".to_string())?;
                runner = Some(value.clone());
                index += 2;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let runner = runner.unwrap_or_else(|| "fake".to_string());
    if runner != "fake" {
        return Err(format!(
            "unsupported subagent runner: {runner} (supported: fake)"
        ));
    }

    Ok(SubagentRunOnceCliRequest {
        options: parse_cli_options(&runtime_args)?,
        output,
        runner,
    })
}

pub(crate) fn parse_cli_options(args: &[String]) -> Result<CliOptions, String> {
    let config_path = find_config_path(args)?;
    let mut db_path: Option<PathBuf> = None;
    let mut provider_id: Option<String> = None;
    let mut provider_base_url: Option<String> = None;
    let mut provider_api_key: Option<String> = None;
    let mut provider_model: Option<String> = None;
    let mut provider_transport: Option<String> = None;
    let mut identity_memory_root: Option<PathBuf> = None;
    let mut subagent_kind: Option<String> = None;
    let mut subagent_queue_root: Option<PathBuf> = None;
    let mut context_max_tokens: Option<u16> = None;
    let mut context_reserve_system_tokens: Option<u16> = None;
    let mut context_min_working_tokens: Option<u16> = None;
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
            "--remember-identity" => index += 1,
            "--dispatch-subagent" => index += 1,
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
            "--context-max-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_tokens = Some(parse_u16_flag("--context-max-tokens", &value)?);
            }
            "--context-reserve-system-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_reserve_system_tokens =
                    Some(parse_u16_flag("--context-reserve-system-tokens", &value)?);
            }
            "--context-min-working-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_min_working_tokens =
                    Some(parse_u16_flag("--context-min-working-tokens", &value)?);
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
        load_runtime_config_file(&path).map_err(format_runtime_config_file_error)?
    } else if let Some(path) = default_config_path() {
        load_runtime_config_file(&path).map_err(format_runtime_config_file_error)?
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
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root"
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
