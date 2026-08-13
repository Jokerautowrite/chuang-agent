//! `cli_external_ai` 模块。内部实现模块（无公开顶层项）。

use chuang_agent::external_ai_dispatch::{
    build_external_ai_dispatch, execute_external_ai_dispatch, parse_live_platform,
    ExternalAiDispatchError, ExternalAiDispatchRequest,
};
use chuang_agent::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use chuang_agent::runtime_config::{OpenAICompatibleConfig, ProviderApiEndpoint, ProviderConfig};
use chuang_agent::runtime_config_file::{
    load_runtime_config_file, load_runtime_config_file_with_options, RuntimeConfigFileError,
    RuntimeConfigFileOptions,
};

use crate::cli_output::{print_json, usage, ControlOutputFormat};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn external_ai_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("dispatch") => external_ai_dispatch_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn external_ai_dispatch_command(args: &[String]) -> Result<(), String> {
    let parsed = parse_external_ai_dispatch(args)?;
    let output = if parsed.request.dry_run {
        build_external_ai_dispatch(parsed.request).map_err(format_external_ai_error)?
    } else {
        let runtime = load_external_ai_runtime(&parsed.config_path)?;
        let mut provider_config = first_openai_compatible_provider(&runtime.provider)
            .ok_or_else(|| "external_ai_dispatch_invalid: provider: configured primary provider is not openai_compatible".to_string())?
            .clone();
        if let Some(model) = parse_live_platform(&parsed.request.platform)
            .map_err(format_external_ai_error)?
            .model
        {
            provider_config.model_name = model;
        }
        provider_config.endpoint = ProviderApiEndpoint::ChatCompletions;
        provider_config.request_timeout_ms = Some(parsed.request.timeout_ms);
        let adapter = build_provider_adapter(&provider_config);
        execute_external_ai_dispatch(parsed.request, &adapter).map_err(format_external_ai_error)?
    };
    match parse_output(args) {
        ControlOutputFormat::Text => {
            println!(
                "external_ai_dispatch adapter={} dry_run={} platform={} audit_id={}",
                output.adapter, output.dry_run, output.request.platform, output.result.audit_id
            );
            println!(
                "connects_real_service={} writes_memory={} quality={}",
                output.connects_real_service, output.writes_memory, output.result.quality
            );
            println!("{}", output.result.result.summary);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }
    Ok(())
}

struct ParsedExternalAiDispatch {
    request: ExternalAiDispatchRequest,
    config_path: PathBuf,
}

fn parse_external_ai_dispatch(args: &[String]) -> Result<ParsedExternalAiDispatch, String> {
    let mut platform = None;
    let mut task = None;
    let mut context = None;
    let mut session_hint = None;
    let mut timeout_ms = 60_000u64;
    let mut audit = true;
    let mut dry_run = false;
    let mut config_path = PathBuf::from("config.toml");

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--platform" => platform = Some(take_value(args, &mut index, "--platform")?),
            "--task" => task = Some(take_value(args, &mut index, "--task")?),
            "--context" => context = Some(take_value(args, &mut index, "--context")?),
            "--session-hint" => {
                session_hint = Some(take_value(args, &mut index, "--session-hint")?)
            }
            "--timeout-ms" => {
                let value = take_value(args, &mut index, "--timeout-ms")?;
                timeout_ms = value.parse::<u64>().map_err(|_| {
                    "external-ai dispatch requires numeric --timeout-ms".to_string()
                })?;
            }
            "--no-audit" => {
                audit = false;
                index += 1;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--config" => {
                config_path = PathBuf::from(take_value(args, &mut index, "--config")?);
            }
            "--json" => index += 1,
            _ => return Err(usage()),
        }
    }

    let mut request = ExternalAiDispatchRequest::new(
        platform.ok_or_else(|| "external-ai dispatch requires --platform".to_string())?,
        task.ok_or_else(|| "external-ai dispatch requires --task".to_string())?,
        context.ok_or_else(|| "external-ai dispatch requires --context".to_string())?,
    );
    request.session_hint = session_hint;
    request.timeout_ms = timeout_ms;
    request.audit = audit;
    request.dry_run = dry_run;
    Ok(ParsedExternalAiDispatch {
        request,
        config_path,
    })
}

fn build_provider_adapter(config: &OpenAICompatibleConfig) -> OpenAICompatibleProviderAdapter {
    OpenAICompatibleProviderAdapter::new(
        config.provider_id.clone(),
        config.base_url.clone(),
        config.api_key.clone(),
        config.model_name.clone(),
    )
    .with_transport(config.transport.clone())
    .with_endpoint(ProviderApiEndpoint::ChatCompletions)
    .with_reasoning_effort(config.reasoning_effort)
    .with_request_timeout_ms(config.request_timeout_ms.unwrap_or(60_000))
    .with_tls_ca_cert_path(config.tls_ca_cert_path.clone())
}

fn first_openai_compatible_provider(provider: &ProviderConfig) -> Option<&OpenAICompatibleConfig> {
    match provider {
        ProviderConfig::OpenAICompatible(config) => Some(config),
        ProviderConfig::Fallback { primary, .. } => first_openai_compatible_provider(primary),
        ProviderConfig::Fake { .. } => None,
        ProviderConfig::AnthropicCompatible(_) => None,
    }
}

fn load_external_ai_runtime(
    path: &Path,
) -> Result<chuang_agent::runtime_config::RuntimeConfig, String> {
    match load_runtime_config_file(path) {
        Ok(runtime) => Ok(runtime),
        Err(RuntimeConfigFileError::MissingEnv { .. }) => {
            let mut runtime = load_runtime_config_file_with_options(
                path,
                RuntimeConfigFileOptions::allow_missing_env(),
            )
            .map_err(format_config_error)?;
            let env_values = read_provider_env_file()?;
            materialize_provider_keys(&mut runtime.provider, &env_values)?;
            Ok(runtime)
        }
        Err(error) => Err(format_config_error(error)),
    }
}

fn read_provider_env_file() -> Result<BTreeMap<String, String>, String> {
    let path = std::env::var_os("CHUANG_PROVIDER_ENV_FILE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".config/chuang-agent/provider.env"))
        })
        .ok_or_else(|| {
            "external_ai_dispatch_invalid: provider_env: provider env file path is unavailable"
                .to_string()
        })?;
    let content = fs::read_to_string(&path).map_err(|_| {
        format!(
            "external_ai_dispatch_invalid: provider_env: cannot read {}",
            path.display()
        )
    })?;
    let mut values = BTreeMap::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            continue;
        }
        let value = raw_value.trim().trim_matches(|ch| ch == '\'' || ch == '"');
        values.insert(key.to_string(), value.to_string());
    }
    Ok(values)
}

fn materialize_provider_keys(
    provider: &mut ProviderConfig,
    env_values: &BTreeMap<String, String>,
) -> Result<(), String> {
    match provider {
        ProviderConfig::OpenAICompatible(config) => {
            if let Some(name) = config
                .api_key
                .strip_prefix("__MISSING_ENV:")
                .and_then(|value| value.strip_suffix("__"))
            {
                config.api_key = env_values.get(name).filter(|value| !value.is_empty()).cloned().ok_or_else(|| {
                    format!("external_ai_dispatch_invalid: provider_env: required variable {name} is not set")
                })?;
            }
        }
        ProviderConfig::AnthropicCompatible(config) => {
            if let Some(name) = config
                .api_key
                .strip_prefix("__MISSING_ENV:")
                .and_then(|value| value.strip_suffix("__"))
            {
                config.api_key = env_values.get(name).filter(|value| !value.is_empty()).cloned().ok_or_else(|| {
                    format!("external_ai_dispatch_invalid: provider_env: required variable {name} is not set")
                })?;
            }
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => {
            materialize_provider_keys(primary, env_values)?;
            materialize_provider_keys(fallback, env_values)?;
        }
        ProviderConfig::Fake { .. } => {}
    }
    Ok(())
}

fn format_config_error(error: RuntimeConfigFileError) -> String {
    match error {
        RuntimeConfigFileError::ReadFailed { path } => {
            format!(
                "external_ai_dispatch_invalid: config: cannot read {}",
                path.display()
            )
        }
        RuntimeConfigFileError::InvalidLine { line, .. } => {
            format!("external_ai_dispatch_invalid: config: invalid line {line}")
        }
        RuntimeConfigFileError::InvalidValue { key, value } => {
            format!("external_ai_dispatch_invalid: config: invalid {key}={value}")
        }
        RuntimeConfigFileError::MissingEnv { name } => {
            format!(
                "external_ai_dispatch_invalid: provider_env: required variable {name} is not set"
            )
        }
    }
}

fn parse_output(args: &[String]) -> ControlOutputFormat {
    if args.iter().any(|arg| arg == "--json") {
        ControlOutputFormat::Json
    } else {
        ControlOutputFormat::Text
    }
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn format_external_ai_error(error: ExternalAiDispatchError) -> String {
    format!(
        "external_ai_dispatch_invalid: {}: {}",
        error.field, error.message
    )
}
