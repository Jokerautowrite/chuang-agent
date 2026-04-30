use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::hermes_memory::{DEFAULT_HOT_MEMORY_MAX_CHARS, DEFAULT_USER_MEMORY_MAX_CHARS};
use crate::responder::ProviderTransport;
use crate::runtime_config::{
    IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig, RuntimeConfig, SubagentConfig,
    SubagentQueueConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeConfigFileError {
    ReadFailed { path: PathBuf },
    InvalidLine { line: usize, content: String },
    InvalidValue { key: String, value: String },
    MissingEnv { name: String },
}

pub fn load_runtime_config_file(
    path: impl AsRef<Path>,
) -> Result<RuntimeConfig, RuntimeConfigFileError> {
    let path = path.as_ref();
    let content = fs::read_to_string(path).map_err(|_| RuntimeConfigFileError::ReadFailed {
        path: path.to_path_buf(),
    })?;
    parse_runtime_config_file(&content)
}

pub fn parse_runtime_config_file(content: &str) -> Result<RuntimeConfig, RuntimeConfigFileError> {
    let values = parse_simple_toml(content)?;
    let mut config = RuntimeConfig::new(PathBuf::from(
        values
            .get("db_path")
            .cloned()
            .unwrap_or_else(|| "./data/chuang-agent.db".to_string()),
    ));

    if let Some(value) = values.get("recall_limit") {
        config.recall_limit = parse_usize("recall_limit", value)?;
    }
    if let Some(value) = values.get("identity_memory_root") {
        config.identity_memory = IdentityMemoryConfig::HermesDualFile {
            root: PathBuf::from(value),
            user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
            memory_max_chars: DEFAULT_HOT_MEMORY_MAX_CHARS,
        };
    }
    if let Some(value) = values.get("subagent") {
        config.subagent = parse_subagent(value)?;
    }
    if let Some(value) = values.get("subagent_queue_root") {
        config.subagent_queue = SubagentQueueConfig {
            root: PathBuf::from(value),
        };
    }
    if let Some(value) = values.get("context.max_tokens") {
        config.context_budget.max_tokens = parse_u16("context.max_tokens", value)?;
    }
    if let Some(value) = values.get("context.reserve_system_tokens") {
        config.context_budget.reserve_system_tokens =
            parse_u16("context.reserve_system_tokens", value)?;
    }
    if let Some(value) = values.get("context.min_working_tokens") {
        config.context_budget.min_working_tokens = parse_u16("context.min_working_tokens", value)?;
    }
    if let Some(value) = values.get("context.max_tool_results") {
        config.context_budget.max_tool_results = parse_usize("context.max_tool_results", value)?;
    }
    if let Some(value) = values.get("context.max_memory_segments") {
        config.context_budget.max_memory_segments =
            parse_usize("context.max_memory_segments", value)?;
    }

    if values.contains_key("provider.kind") {
        config.provider = parse_provider(&values)?;
    }

    Ok(config)
}

fn parse_simple_toml(content: &str) -> Result<BTreeMap<String, String>, RuntimeConfigFileError> {
    let mut section = String::new();
    let mut values = BTreeMap::new();

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            if section.is_empty() {
                return Err(RuntimeConfigFileError::InvalidLine {
                    line: line_number,
                    content: raw_line.to_string(),
                });
            }
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(RuntimeConfigFileError::InvalidLine {
                line: line_number,
                content: raw_line.to_string(),
            });
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(RuntimeConfigFileError::InvalidLine {
                line: line_number,
                content: raw_line.to_string(),
            });
        }
        let full_key = if section.is_empty() {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        values.insert(full_key, parse_value(value.trim()));
    }

    Ok(values)
}

fn parse_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

fn parse_provider(
    values: &BTreeMap<String, String>,
) -> Result<ProviderConfig, RuntimeConfigFileError> {
    let kind = values
        .get("provider.kind")
        .map(String::as_str)
        .unwrap_or("fake");
    match kind {
        "fake" => Ok(ProviderConfig::Fake {
            provider_id: values
                .get("provider.id")
                .cloned()
                .unwrap_or_else(|| "fake-runtime".to_string()),
            model_name: values
                .get("provider.model")
                .cloned()
                .unwrap_or_else(|| "stub-responder".to_string()),
        }),
        "openai_compatible" => {
            let api_key_env = required(values, "provider.api_key_env")?;
            let api_key = std::env::var(&api_key_env)
                .map_err(|_| RuntimeConfigFileError::MissingEnv { name: api_key_env })?;
            Ok(ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                provider_id: values
                    .get("provider.id")
                    .cloned()
                    .unwrap_or_else(|| "openai-compatible-config".to_string()),
                base_url: required(values, "provider.base_url")?,
                api_key,
                model_name: required(values, "provider.model")?,
                transport: values
                    .get("provider.transport")
                    .map(|value| value.parse::<ProviderTransport>())
                    .transpose()
                    .map_err(|_| RuntimeConfigFileError::InvalidValue {
                        key: "provider.transport".to_string(),
                        value: values
                            .get("provider.transport")
                            .cloned()
                            .unwrap_or_default(),
                    })?
                    .unwrap_or(ProviderTransport::Stub),
            }))
        }
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "provider.kind".to_string(),
            value: other.to_string(),
        }),
    }
}

fn required(
    values: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, RuntimeConfigFileError> {
    values
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| RuntimeConfigFileError::InvalidValue {
            key: key.to_string(),
            value: String::new(),
        })
}

fn parse_subagent(raw: &str) -> Result<SubagentConfig, RuntimeConfigFileError> {
    match raw {
        "fake" => Ok(SubagentConfig::Fake),
        "queued_external" => Ok(SubagentConfig::QueuedExternal),
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "subagent".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_u16(key: &str, raw: &str) -> Result<u16, RuntimeConfigFileError> {
    raw.parse::<u16>()
        .map_err(|_| RuntimeConfigFileError::InvalidValue {
            key: key.to_string(),
            value: raw.to_string(),
        })
}

fn parse_usize(key: &str, raw: &str) -> Result<usize, RuntimeConfigFileError> {
    raw.parse::<usize>()
        .map_err(|_| RuntimeConfigFileError::InvalidValue {
            key: key.to_string(),
            value: raw.to_string(),
        })
}
