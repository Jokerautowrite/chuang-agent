use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::hermes_memory::{DEFAULT_HOT_MEMORY_MAX_CHARS, DEFAULT_USER_MEMORY_MAX_CHARS};
use crate::knowledge_read::{KnowledgeReadConfig, KnowledgeReadSourceConfig};
use crate::provider_openai_compatible::ProviderTransport;
use crate::runtime_config::{
    ActuatorCommandConfig, ActuatorConfig, ContextEngineConfig, ControlPlaneCommandConfig,
    ControlPlaneConfig, IdentityBootstrapConfig, IdentityMemoryConfig, OpenAICompatibleConfig,
    ProviderConfig, ProviderFallbackPolicy, RulesConfig, RuntimeConfig, SubagentConfig,
    SubagentLiveWorkerConfig, SubagentQueueConfig,
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
    load_runtime_config_file_with_options(path, RuntimeConfigFileOptions::strict())
}

pub fn load_runtime_config_file_with_options(
    path: impl AsRef<Path>,
    options: RuntimeConfigFileOptions,
) -> Result<RuntimeConfig, RuntimeConfigFileError> {
    let path = path.as_ref();
    let content = fs::read_to_string(path).map_err(|_| RuntimeConfigFileError::ReadFailed {
        path: path.to_path_buf(),
    })?;
    parse_runtime_config_file_with_options(&content, options)
}

pub fn parse_runtime_config_file(content: &str) -> Result<RuntimeConfig, RuntimeConfigFileError> {
    parse_runtime_config_file_with_options(content, RuntimeConfigFileOptions::strict())
}

pub fn parse_runtime_config_file_with_options(
    content: &str,
    options: RuntimeConfigFileOptions,
) -> Result<RuntimeConfig, RuntimeConfigFileError> {
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
    if let Some(value) = get_any(&values, &["tool_loop.max_rounds", "tool_max_rounds"]) {
        config.tool_loop.max_rounds = parse_usize("tool_loop.max_rounds", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &["tool_loop.shell_timeout_ms", "tool_shell_timeout_ms"],
    ) {
        config.tool_loop.shell_timeout_ms = parse_u64("tool_loop.shell_timeout_ms", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &[
            "tool_loop.risk.delete_or_cleanup",
            "tool_shell_risk_delete_or_cleanup",
        ],
    ) {
        config.tool_loop.shell_risk_rules.delete_or_cleanup =
            parse_pattern_list("tool_loop.risk.delete_or_cleanup", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &[
            "tool_loop.risk.service_change",
            "tool_shell_risk_service_change",
        ],
    ) {
        config.tool_loop.shell_risk_rules.service_change =
            parse_pattern_list("tool_loop.risk.service_change", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &[
            "tool_loop.risk.network_change",
            "tool_shell_risk_network_change",
        ],
    ) {
        config.tool_loop.shell_risk_rules.network_change =
            parse_pattern_list("tool_loop.risk.network_change", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &[
            "tool_loop.risk.secret_access",
            "tool_shell_risk_secret_access",
        ],
    ) {
        config.tool_loop.shell_risk_rules.secret_access =
            parse_pattern_list("tool_loop.risk.secret_access", value)?;
    }
    if let Some(value) = values.get("identity_memory_root") {
        config.identity_memory = IdentityMemoryConfig::HermesDualFile {
            root: PathBuf::from(value),
            user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
            memory_max_chars: DEFAULT_HOT_MEMORY_MAX_CHARS,
        };
    }
    if let Some(value) = get_any(&values, &["identity.root", "identity_root"]) {
        config.identity_bootstrap = IdentityBootstrapConfig::new(value);
    }
    if let Some(value) = get_any(&values, &["identity.soul_path", "soul_path"]) {
        config.identity_bootstrap.soul_path = PathBuf::from(value);
    }
    if let Some(value) = get_any(&values, &["identity.story_path", "story_path"]) {
        config.identity_bootstrap.story_path = PathBuf::from(value);
    }
    if let Some(value) = get_any(&values, &["identity.first_wake_path", "first_wake_path"]) {
        config.identity_bootstrap.first_wake_path = PathBuf::from(value);
    }
    if let Some(value) = get_any(
        &values,
        &["identity.agents_registry_path", "agents_registry_path"],
    ) {
        config.identity_bootstrap.agents_registry_path = PathBuf::from(value);
    }
    if let Some(value) = get_any(&values, &["rules.root", "rules_root"]) {
        config.rules = RulesConfig::new(value);
    }
    if let Some(value) = get_any(&values, &["rules.core_path", "rules_core_path"]) {
        config.rules.core_path = PathBuf::from(value);
    }
    if let Some(value) = values.get("subagent") {
        config.subagent = parse_subagent(value)?;
    }
    if has_any(
        &values,
        &[
            "subagent_live_worker.enabled",
            "subagent_live_worker.adapter_kind",
            "subagent_live_worker.status",
            "subagent_live_worker.starts_worker",
            "subagent_live_worker_enabled",
            "subagent_live_worker_adapter_kind",
            "subagent_live_worker_status",
            "subagent_live_worker_starts_worker",
        ],
    ) {
        config.subagent_live_worker = parse_subagent_live_worker(&values)?;
    }
    if values.contains_key("actuator.kind") || values.contains_key("actuator") {
        config.actuator = parse_actuator(&values)?;
    }
    if let Some(value) = values.get("subagent_queue_root") {
        config.subagent_queue = SubagentQueueConfig {
            root: PathBuf::from(value),
        };
    }
    if values.contains_key("control.kind") || values.contains_key("control") {
        config.control_plane = parse_control_plane(&values)?;
    }
    if has_any(
        &values,
        &[
            "external_knowledge.wiki.endpoint",
            "external_knowledge.wiki.token_env",
            "external_knowledge.wiki.timeout_ms",
            "external_knowledge.gbrain.endpoint",
            "external_knowledge.gbrain.token_env",
            "external_knowledge.gbrain.timeout_ms",
        ],
    ) {
        config.external_knowledge = parse_external_knowledge(&values)?;
    }
    if let Some(value) = get_any(&values, &["context.engine", "context_engine"]) {
        config.context_engine = parse_context_engine(value)?;
    }
    if let Some(value) = get_any(&values, &["evolution.kind", "evolution"]) {
        config.evolution = parse_evolution(value)?;
    }
    if let Some(value) = get_any(&values, &["context.max_tokens", "context_max_tokens"]) {
        config.context_budget.max_tokens = parse_u32("context.max_tokens", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &[
            "context.reserve_system_tokens",
            "context_reserve_system_tokens",
        ],
    ) {
        config.context_budget.reserve_system_tokens =
            parse_u32("context.reserve_system_tokens", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &["context.min_working_tokens", "context_min_working_tokens"],
    ) {
        config.context_budget.min_working_tokens = parse_u32("context.min_working_tokens", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &["context.max_tool_results", "context_max_tool_results"],
    ) {
        config.context_budget.max_tool_results = parse_usize("context.max_tool_results", value)?;
    }
    if let Some(value) = get_any(
        &values,
        &["context.max_memory_segments", "context_max_memory_segments"],
    ) {
        config.context_budget.max_memory_segments =
            parse_usize("context.max_memory_segments", value)?;
    }

    if values.contains_key("provider.kind") || values.contains_key("provider") {
        config.provider = parse_provider(&values, options)?;
    }

    Ok(config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfigFileOptions {
    pub allow_missing_env: bool,
}

impl RuntimeConfigFileOptions {
    pub fn strict() -> Self {
        Self {
            allow_missing_env: false,
        }
    }

    pub fn allow_missing_env() -> Self {
        Self {
            allow_missing_env: true,
        }
    }
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

fn parse_pattern_list(key: &str, value: &str) -> Result<Vec<String>, RuntimeConfigFileError> {
    let patterns = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        return Err(RuntimeConfigFileError::InvalidValue {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    Ok(patterns)
}

fn parse_provider(
    values: &BTreeMap<String, String>,
    options: RuntimeConfigFileOptions,
) -> Result<ProviderConfig, RuntimeConfigFileError> {
    let primary = parse_primary_provider(values, options)?;
    if values.contains_key("fallback_provider") || values.contains_key("fallback.provider.kind") {
        let fallback = parse_fallback_provider(values, options)?;
        let policy = parse_fallback_policy(values)?;
        return Ok(ProviderConfig::Fallback {
            primary: Box::new(primary),
            fallback: Box::new(fallback),
            policy,
        });
    }

    Ok(primary)
}

fn parse_primary_provider(
    values: &BTreeMap<String, String>,
    options: RuntimeConfigFileOptions,
) -> Result<ProviderConfig, RuntimeConfigFileError> {
    let kind = get_any(values, &["provider.kind", "provider"])
        .map(String::as_str)
        .unwrap_or("fake");
    match kind {
        "fake" => Ok(ProviderConfig::Fake {
            provider_id: get_any(values, &["provider.id", "provider_id"])
                .cloned()
                .unwrap_or_else(|| "fake-runtime".to_string()),
            model_name: get_any(values, &["provider.model", "model"])
                .cloned()
                .unwrap_or_else(|| "stub-responder".to_string()),
        }),
        "openai_compatible" => {
            let api_key_env = required_any(values, &["provider.api_key_env", "api_key_env"])?;
            let api_key = resolve_api_key_env(&api_key_env, options)?;
            Ok(ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                provider_id: get_any(values, &["provider.id", "provider_id"])
                    .cloned()
                    .unwrap_or_else(|| "openai-compatible-config".to_string()),
                base_url: required_any(values, &["provider.base_url", "base_url"])?,
                api_key,
                model_name: required_any(values, &["provider.model", "model"])?,
                transport: get_any(values, &["provider.transport", "transport"])
                    .map(|value| value.parse::<ProviderTransport>())
                    .transpose()
                    .map_err(|_| RuntimeConfigFileError::InvalidValue {
                        key: "provider.transport".to_string(),
                        value: get_any(values, &["provider.transport", "transport"])
                            .cloned()
                            .unwrap_or_default(),
                    })?
                    .unwrap_or(ProviderTransport::Stub),
                request_timeout_ms: get_any(
                    values,
                    &["provider.request_timeout_ms", "provider_timeout_ms"],
                )
                .map(|value| parse_u64("provider.request_timeout_ms", value))
                .transpose()?,
                tls_ca_cert_path: get_any(values, &["provider.tls_ca_path", "tls_ca_path"])
                    .map(PathBuf::from),
            }))
        }
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "provider.kind".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_fallback_provider(
    values: &BTreeMap<String, String>,
    options: RuntimeConfigFileOptions,
) -> Result<ProviderConfig, RuntimeConfigFileError> {
    let kind = get_any(values, &["fallback.provider.kind", "fallback_provider"])
        .map(String::as_str)
        .unwrap_or("fake");
    match kind {
        "fake" => Ok(ProviderConfig::Fake {
            provider_id: get_any(values, &["fallback.provider.id", "fallback_provider_id"])
                .cloned()
                .unwrap_or_else(|| "fallback-fake-runtime".to_string()),
            model_name: get_any(values, &["fallback.provider.model", "fallback_model"])
                .cloned()
                .unwrap_or_else(|| "fallback-stub-responder".to_string()),
        }),
        "openai_compatible" => {
            let api_key_env = required_any(
                values,
                &["fallback.provider.api_key_env", "fallback_api_key_env"],
            )?;
            let api_key = resolve_api_key_env(&api_key_env, options)?;
            Ok(ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                provider_id: get_any(values, &["fallback.provider.id", "fallback_provider_id"])
                    .cloned()
                    .unwrap_or_else(|| "fallback-openai-compatible".to_string()),
                base_url: required_any(
                    values,
                    &["fallback.provider.base_url", "fallback_base_url"],
                )?,
                api_key,
                model_name: required_any(values, &["fallback.provider.model", "fallback_model"])?,
                transport: get_any(
                    values,
                    &["fallback.provider.transport", "fallback_transport"],
                )
                .map(|value| value.parse::<ProviderTransport>())
                .transpose()
                .map_err(|_| RuntimeConfigFileError::InvalidValue {
                    key: "fallback.provider.transport".to_string(),
                    value: get_any(
                        values,
                        &["fallback.provider.transport", "fallback_transport"],
                    )
                    .cloned()
                    .unwrap_or_default(),
                })?
                .unwrap_or(ProviderTransport::Stub),
                request_timeout_ms: get_any(
                    values,
                    &[
                        "fallback.provider.request_timeout_ms",
                        "fallback_provider_timeout_ms",
                    ],
                )
                .map(|value| parse_u64("fallback.provider.request_timeout_ms", value))
                .transpose()?,
                tls_ca_cert_path: get_any(
                    values,
                    &["fallback.provider.tls_ca_path", "fallback_tls_ca_path"],
                )
                .map(PathBuf::from),
            }))
        }
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "fallback.provider.kind".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_fallback_policy(
    values: &BTreeMap<String, String>,
) -> Result<ProviderFallbackPolicy, RuntimeConfigFileError> {
    Ok(ProviderFallbackPolicy {
        on_retryable: get_any(
            values,
            &[
                "fallback.on_retryable",
                "fallback_on_retryable",
                "provider.fallback_on_retryable",
            ],
        )
        .map(|value| parse_bool("fallback.on_retryable", value))
        .transpose()?
        .unwrap_or(true),
        status_codes: get_any(
            values,
            &[
                "fallback.status_codes",
                "fallback_status_codes",
                "provider.fallback_status_codes",
            ],
        )
        .map(|value| parse_status_codes("fallback.status_codes", value))
        .transpose()?
        .unwrap_or_else(|| vec![401, 402]),
        error_classes: get_any(
            values,
            &[
                "fallback.error_classes",
                "fallback_error_classes",
                "provider.fallback_error_classes",
            ],
        )
        .map(|value| parse_csv_strings(value))
        .unwrap_or_default(),
    })
}

fn resolve_api_key_env(
    api_key_env: &str,
    options: RuntimeConfigFileOptions,
) -> Result<String, RuntimeConfigFileError> {
    match std::env::var(api_key_env) {
        Ok(value) => Ok(value),
        Err(_) if options.allow_missing_env => Ok(format!("__MISSING_ENV:{api_key_env}__")),
        Err(_) => Err(RuntimeConfigFileError::MissingEnv {
            name: api_key_env.to_string(),
        }),
    }
}

fn get_any<'a>(values: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a String> {
    keys.iter().find_map(|key| values.get(*key))
}

fn has_any(values: &BTreeMap<String, String>, keys: &[&str]) -> bool {
    keys.iter().any(|key| values.contains_key(*key))
}

fn required_any(
    values: &BTreeMap<String, String>,
    keys: &[&str],
) -> Result<String, RuntimeConfigFileError> {
    get_any(values, keys)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| RuntimeConfigFileError::InvalidValue {
            key: keys.first().copied().unwrap_or("unknown").to_string(),
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

fn parse_subagent_live_worker(
    values: &BTreeMap<String, String>,
) -> Result<SubagentLiveWorkerConfig, RuntimeConfigFileError> {
    let starts_worker = get_any(
        values,
        &[
            "subagent_live_worker.starts_worker",
            "subagent_live_worker_starts_worker",
        ],
    )
    .map(|value| parse_bool("subagent_live_worker.starts_worker", value))
    .transpose()?
    .unwrap_or(false);
    if starts_worker {
        return Err(RuntimeConfigFileError::InvalidValue {
            key: "subagent_live_worker.starts_worker".to_string(),
            value: "true".to_string(),
        });
    }

    Ok(SubagentLiveWorkerConfig {
        enabled: get_any(
            values,
            &[
                "subagent_live_worker.enabled",
                "subagent_live_worker_enabled",
            ],
        )
        .map(|value| parse_bool("subagent_live_worker.enabled", value))
        .transpose()?
        .unwrap_or(false),
        adapter_kind: get_any(
            values,
            &[
                "subagent_live_worker.adapter_kind",
                "subagent_live_worker_adapter_kind",
            ],
        )
        .cloned()
        .unwrap_or_else(|| "none".to_string()),
        status: get_any(
            values,
            &["subagent_live_worker.status", "subagent_live_worker_status"],
        )
        .cloned()
        .unwrap_or_else(|| "disabled".to_string()),
        starts_worker,
    })
}

fn parse_actuator(
    values: &BTreeMap<String, String>,
) -> Result<ActuatorConfig, RuntimeConfigFileError> {
    let kind = get_any(values, &["actuator.kind", "actuator"])
        .map(String::as_str)
        .unwrap_or("fake");
    match kind {
        "fake" => Ok(ActuatorConfig::Fake),
        "command" => Ok(ActuatorConfig::Command(ActuatorCommandConfig {
            program: required_any(values, &["actuator.program", "actuator_program"])?,
            args: required_any(values, &["actuator.args", "actuator_args"])?,
            timeout_ms: get_any(values, &["actuator.timeout_ms", "actuator_timeout_ms"])
                .map(|value| parse_u64("actuator.timeout_ms", value))
                .transpose()?
                .unwrap_or(30_000),
        })),
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "actuator.kind".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_control_plane(
    values: &BTreeMap<String, String>,
) -> Result<ControlPlaneConfig, RuntimeConfigFileError> {
    let kind = get_any(values, &["control.kind", "control"])
        .map(String::as_str)
        .unwrap_or("fake_local");
    match kind {
        "fake_local" => Ok(ControlPlaneConfig::FakeLocal),
        "command" => Ok(ControlPlaneConfig::Command(ControlPlaneCommandConfig {
            program: required_any(values, &["control.program", "program"])?,
            list_args: required_any(values, &["control.list_args", "list_args"])?,
            apply_args: required_any(values, &["control.apply_args", "apply_args"])?,
            timeout_ms: get_any(values, &["control.timeout_ms", "control_timeout_ms"])
                .map(|value| parse_u64("control.timeout_ms", value))
                .transpose()?
                .unwrap_or(30_000),
        })),
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "control.kind".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_external_knowledge(
    values: &BTreeMap<String, String>,
) -> Result<KnowledgeReadConfig, RuntimeConfigFileError> {
    Ok(KnowledgeReadConfig {
        wiki: parse_external_knowledge_source(values, "wiki")?,
        gbrain: parse_external_knowledge_source(values, "gbrain")?,
    })
}

fn parse_external_knowledge_source(
    values: &BTreeMap<String, String>,
    source: &str,
) -> Result<KnowledgeReadSourceConfig, RuntimeConfigFileError> {
    let endpoint_key = format!("external_knowledge.{source}.endpoint");
    let token_env_key = format!("external_knowledge.{source}.token_env");
    let timeout_key = format!("external_knowledge.{source}.timeout_ms");
    Ok(KnowledgeReadSourceConfig {
        endpoint: get_any(values, &[&endpoint_key]).cloned(),
        token_env: get_any(values, &[&token_env_key]).cloned(),
        timeout_ms: get_any(values, &[&timeout_key])
            .map(|value| parse_u64(&timeout_key, value))
            .transpose()?,
    })
}

fn parse_evolution(
    raw: &str,
) -> Result<crate::runtime_config::EvolutionConfig, RuntimeConfigFileError> {
    match raw {
        "noop" => Ok(crate::runtime_config::EvolutionConfig::Noop),
        "dry_run" => Ok(crate::runtime_config::EvolutionConfig::DryRun),
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "evolution.kind".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_context_engine(raw: &str) -> Result<ContextEngineConfig, RuntimeConfigFileError> {
    match raw {
        "deterministic_budget" => Ok(ContextEngineConfig::DeterministicBudget),
        "summary_compression" => Ok(ContextEngineConfig::SummaryCompression),
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: "context.engine".to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_u32(key: &str, raw: &str) -> Result<u32, RuntimeConfigFileError> {
    raw.parse::<u32>()
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

fn parse_u64(key: &str, raw: &str) -> Result<u64, RuntimeConfigFileError> {
    raw.parse::<u64>()
        .map_err(|_| RuntimeConfigFileError::InvalidValue {
            key: key.to_string(),
            value: raw.to_string(),
        })
}

fn parse_bool(key: &str, raw: &str) -> Result<bool, RuntimeConfigFileError> {
    match raw {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(RuntimeConfigFileError::InvalidValue {
            key: key.to_string(),
            value: other.to_string(),
        }),
    }
}

fn parse_status_codes(key: &str, raw: &str) -> Result<Vec<u16>, RuntimeConfigFileError> {
    let mut codes = Vec::new();
    for token in raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let code = token
            .parse::<u16>()
            .map_err(|_| RuntimeConfigFileError::InvalidValue {
                key: key.to_string(),
                value: token.to_string(),
            })?;
        if !(100..=599).contains(&code) {
            return Err(RuntimeConfigFileError::InvalidValue {
                key: key.to_string(),
                value: token.to_string(),
            });
        }
        codes.push(code);
    }
    Ok(codes)
}

fn parse_csv_strings(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}
