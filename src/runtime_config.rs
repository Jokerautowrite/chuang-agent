use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::context_engine::ContextBudget;
use crate::hermes_memory::{
    DualFileMemoryConfig, DEFAULT_HOT_MEMORY_MAX_CHARS, DEFAULT_USER_MEMORY_MAX_CHARS,
};
use crate::responder::{OpenAICompatibleProviderAdapter, ProviderTransport};
use crate::subagent_queue::FileSubagentQueueConfig;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub db_path: PathBuf,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: ContextBudget,
    pub context_engine: ContextEngineConfig,
    pub provider: ProviderConfig,
    pub identity_memory: IdentityMemoryConfig,
    pub governance: GovernanceConfig,
    pub actuator: ActuatorConfig,
    pub subagent: SubagentConfig,
    pub subagent_queue: SubagentQueueConfig,
    pub evolution: EvolutionConfig,
    pub control_plane: ControlPlaneConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfig {
    Fake {
        provider_id: String,
        model_name: String,
    },
    OpenAICompatible(OpenAICompatibleConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAICompatibleConfig {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub transport: ProviderTransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityMemoryConfig {
    HermesDualFile {
        root: PathBuf,
        user_max_chars: usize,
        memory_max_chars: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEngineConfig {
    DeterministicBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceConfig {
    StaticRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActuatorConfig {
    Fake,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentConfig {
    Fake,
    QueuedExternal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentQueueConfig {
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionConfig {
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlPlaneConfig {
    FakeLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigSummary {
    pub provider_kind: String,
    pub provider_id: String,
    pub model_name: String,
    pub governance_kind: String,
    pub actuator_kind: String,
    pub subagent_kind: String,
    pub subagent_queue_root: String,
    pub evolution_kind: String,
    pub control_plane_kind: String,
    pub identity_memory_kind: String,
    pub identity_memory_root: String,
    pub identity_user_max_chars: usize,
    pub identity_memory_max_chars: usize,
    pub db_path: String,
    pub recall_limit: usize,
    pub context_engine_kind: String,
    pub context_max_tokens: u16,
    pub context_reserve_system_tokens: u16,
    pub context_min_working_tokens: u16,
    pub context_max_tool_results: usize,
    pub context_max_memory_segments: usize,
    pub api_key_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub field: String,
    pub message: String,
}

impl RuntimeConfig {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            recall_limit: 5,
            metadata: BTreeMap::new(),
            context_budget: default_context_budget(),
            context_engine: ContextEngineConfig::DeterministicBudget,
            provider: ProviderConfig::Fake {
                provider_id: "fake-runtime".to_string(),
                model_name: "stub-responder".to_string(),
            },
            identity_memory: IdentityMemoryConfig::HermesDualFile {
                root: PathBuf::from("./data/hermes-memory"),
                user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
                memory_max_chars: DEFAULT_HOT_MEMORY_MAX_CHARS,
            },
            governance: GovernanceConfig::StaticRule,
            actuator: ActuatorConfig::Fake,
            subagent: SubagentConfig::Fake,
            subagent_queue: SubagentQueueConfig {
                root: PathBuf::from("./data/subagent-queue"),
            },
            evolution: EvolutionConfig::Noop,
            control_plane: ControlPlaneConfig::FakeLocal,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.recall_limit == 0 {
            return Err(ConfigError {
                field: "recall_limit".to_string(),
                message: "recall_limit must be greater than zero".to_string(),
            });
        }

        if self.context_budget.max_tokens == 0 {
            return Err(ConfigError {
                field: "context.max_tokens".to_string(),
                message: "context max_tokens must be greater than zero".to_string(),
            });
        }
        if self.context_budget.reserve_system_tokens > self.context_budget.max_tokens {
            return Err(ConfigError {
                field: "context.reserve_system_tokens".to_string(),
                message: "context reserve_system_tokens must not exceed max_tokens".to_string(),
            });
        }

        self.provider.validate()?;
        self.context_engine.validate()?;
        self.identity_memory.validate()?;
        self.governance.validate()?;
        self.actuator.validate()?;
        self.subagent.validate()?;
        self.subagent_queue.validate()?;
        self.evolution.validate()?;
        self.control_plane.validate()
    }

    pub fn summary(&self) -> ConfigSummary {
        let provider = self.provider.summary_parts();
        let identity_memory = self.identity_memory.summary_parts();
        ConfigSummary {
            provider_kind: provider.kind,
            provider_id: provider.provider_id,
            model_name: provider.model_name,
            governance_kind: self.governance.kind().to_string(),
            actuator_kind: self.actuator.kind().to_string(),
            subagent_kind: self.subagent.kind().to_string(),
            subagent_queue_root: self.subagent_queue.root.display().to_string(),
            evolution_kind: self.evolution.kind().to_string(),
            control_plane_kind: self.control_plane.kind().to_string(),
            identity_memory_kind: identity_memory.kind,
            identity_memory_root: identity_memory.root,
            identity_user_max_chars: identity_memory.user_max_chars,
            identity_memory_max_chars: identity_memory.memory_max_chars,
            db_path: self.db_path.display().to_string(),
            recall_limit: self.recall_limit,
            context_engine_kind: self.context_engine.kind().to_string(),
            context_max_tokens: self.context_budget.max_tokens,
            context_reserve_system_tokens: self.context_budget.reserve_system_tokens,
            context_min_working_tokens: self.context_budget.min_working_tokens,
            context_max_tool_results: self.context_budget.max_tool_results,
            context_max_memory_segments: self.context_budget.max_memory_segments,
            api_key_state: provider.api_key_state,
        }
    }
}

impl ProviderConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::Fake {
                provider_id,
                model_name,
            } => {
                require_non_empty("provider.provider_id", provider_id)?;
                require_non_empty("provider.model_name", model_name)
            }
            Self::OpenAICompatible(config) => config.validate(),
        }
    }

    pub fn build_openai_compatible(
        &self,
    ) -> Result<Option<OpenAICompatibleProviderAdapter>, ConfigError> {
        match self {
            Self::Fake { .. } => Ok(None),
            Self::OpenAICompatible(config) => {
                config.validate()?;
                Ok(Some(
                    OpenAICompatibleProviderAdapter::new(
                        config.provider_id.clone(),
                        config.base_url.clone(),
                        config.api_key.clone(),
                        config.model_name.clone(),
                    )
                    .with_transport(config.transport.clone()),
                ))
            }
        }
    }

    fn summary_parts(&self) -> ProviderSummaryParts {
        match self {
            Self::Fake {
                provider_id,
                model_name,
            } => ProviderSummaryParts {
                kind: "fake".to_string(),
                provider_id: provider_id.clone(),
                model_name: model_name.clone(),
                api_key_state: None,
            },
            Self::OpenAICompatible(config) => ProviderSummaryParts {
                kind: "openai_compatible".to_string(),
                provider_id: config.provider_id.clone(),
                model_name: config.model_name.clone(),
                api_key_state: Some(mask_key_state(&config.api_key)),
            },
        }
    }
}

impl IdentityMemoryConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::HermesDualFile { .. } => "hermes_dual_file",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::HermesDualFile {
                root,
                user_max_chars,
                memory_max_chars,
            } => {
                require_non_empty("identity_memory.root", &root.display().to_string())?;
                require_positive_usize("identity_memory.user_max_chars", *user_max_chars)?;
                require_positive_usize("identity_memory.memory_max_chars", *memory_max_chars)
            }
        }
    }

    pub fn build_dual_file_config(&self) -> Result<DualFileMemoryConfig, ConfigError> {
        self.validate()?;
        match self {
            Self::HermesDualFile {
                root,
                user_max_chars,
                memory_max_chars,
            } => {
                let mut config = DualFileMemoryConfig::new(root);
                config.user_max_chars = *user_max_chars;
                config.memory_max_chars = *memory_max_chars;
                Ok(config)
            }
        }
    }

    fn summary_parts(&self) -> IdentityMemorySummaryParts {
        match self {
            Self::HermesDualFile {
                root,
                user_max_chars,
                memory_max_chars,
            } => IdentityMemorySummaryParts {
                kind: self.kind().to_string(),
                root: root.display().to_string(),
                user_max_chars: *user_max_chars,
                memory_max_chars: *memory_max_chars,
            },
        }
    }
}

impl ContextEngineConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::DeterministicBudget => "deterministic_budget",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

impl GovernanceConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::StaticRule => "static_rule",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

impl ActuatorConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Fake => "fake",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

impl SubagentConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Fake => "fake",
            Self::QueuedExternal => "queued_external",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

impl SubagentQueueConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty("subagent_queue.root", &self.root.display().to_string())
    }

    pub fn build_file_queue_config(&self) -> Result<FileSubagentQueueConfig, ConfigError> {
        self.validate()?;
        Ok(FileSubagentQueueConfig::new(&self.root))
    }
}

impl EvolutionConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Noop => "noop",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

impl ControlPlaneConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::FakeLocal => "fake_local",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

impl OpenAICompatibleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty("provider.provider_id", &self.provider_id)?;
        require_non_empty("provider.base_url", &self.base_url)?;
        require_non_empty("provider.api_key", &self.api_key)?;
        require_non_empty("provider.model_name", &self.model_name)
    }
}

struct ProviderSummaryParts {
    kind: String,
    provider_id: String,
    model_name: String,
    api_key_state: Option<String>,
}

struct IdentityMemorySummaryParts {
    kind: String,
    root: String,
    user_max_chars: usize,
    memory_max_chars: usize,
}

fn require_non_empty(field: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError {
            field: field.to_string(),
            message: format!("{field} must not be empty"),
        });
    }

    Ok(())
}

fn require_positive_usize(field: &str, value: usize) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError {
            field: field.to_string(),
            message: format!("{field} must be greater than zero"),
        });
    }

    Ok(())
}

fn mask_key_state(api_key: &str) -> String {
    if api_key.is_empty() {
        "<missing>".to_string()
    } else {
        "<set>".to_string()
    }
}

pub fn default_context_budget() -> ContextBudget {
    ContextBudget {
        max_tokens: 512,
        reserve_system_tokens: 32,
        min_working_tokens: 1,
        max_tool_results: 5,
        max_memory_segments: 5,
    }
}
