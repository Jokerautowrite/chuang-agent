use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::context_engine::{ContextBudget, ContextEngineKind};
use crate::hermes_memory::{
    DualFileMemoryConfig, DEFAULT_HOT_MEMORY_MAX_CHARS, DEFAULT_USER_MEMORY_MAX_CHARS,
};
use crate::provider_openai_compatible::ProviderTransport;
use crate::subagent_queue::FileSubagentQueueConfig;
use crate::tool_runtime::ShellRiskRules;
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
    pub identity_bootstrap: IdentityBootstrapConfig,
    pub rules: RulesConfig,
    pub governance: GovernanceConfig,
    pub tool_loop: ToolLoopConfig,
    pub actuator: ActuatorConfig,
    pub subagent: SubagentConfig,
    pub subagent_live_worker: SubagentLiveWorkerConfig,
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
    Fallback {
        primary: Box<ProviderConfig>,
        fallback: Box<ProviderConfig>,
        policy: ProviderFallbackPolicy,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAICompatibleConfig {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub transport: ProviderTransport,
    pub request_timeout_ms: Option<u64>,
    pub tls_ca_cert_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFallbackPolicy {
    pub on_retryable: bool,
    pub status_codes: Vec<u16>,
    pub error_classes: Vec<String>,
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
pub struct IdentityBootstrapConfig {
    pub root: PathBuf,
    pub soul_path: PathBuf,
    pub story_path: PathBuf,
    pub first_wake_path: PathBuf,
    pub agents_registry_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesConfig {
    pub root: PathBuf,
    pub core_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEngineConfig {
    DeterministicBudget,
    SummaryCompression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceConfig {
    StaticRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLoopConfig {
    pub max_rounds: usize,
    pub shell_timeout_ms: u64,
    pub shell_risk_rules: ShellRiskRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActuatorConfig {
    Fake,
    Command(ActuatorCommandConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActuatorCommandConfig {
    pub program: String,
    pub args: String,
    pub timeout_ms: u64,
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
pub struct SubagentLiveWorkerConfig {
    pub enabled: bool,
    pub adapter_kind: String,
    pub status: String,
    pub starts_worker: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionConfig {
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlPlaneConfig {
    FakeLocal,
    Command(ControlPlaneCommandConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPlaneCommandConfig {
    pub program: String,
    pub list_args: String,
    pub apply_args: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigSummary {
    pub provider_kind: String,
    pub provider_id: String,
    pub model_name: String,
    pub provider_tls_ca_cert_path: Option<String>,
    pub provider_request_timeout_ms: Option<u64>,
    pub provider_fallback_policy: Option<String>,
    pub governance_kind: String,
    pub actuator_kind: String,
    pub subagent_kind: String,
    pub subagent_live_worker: SubagentLiveWorkerSummary,
    pub subagent_queue_root: String,
    pub evolution_kind: String,
    pub control_plane_kind: String,
    pub control_command_timeout_ms: Option<u64>,
    pub actuator_command_timeout_ms: Option<u64>,
    pub identity_memory_kind: String,
    pub identity_memory_root: String,
    pub identity_experiences_path: String,
    pub identity_user_max_chars: usize,
    pub identity_memory_max_chars: usize,
    pub identity_root: String,
    pub soul_path: String,
    pub story_path: String,
    pub first_wake_path: String,
    pub agents_registry_path: String,
    pub rules_root: String,
    pub rules_core_path: String,
    pub tool_loop_max_rounds: usize,
    pub tool_shell_timeout_ms: u64,
    pub tool_shell_risk_rule_counts: String,
    pub db_path: String,
    pub recall_limit: usize,
    pub context_engine_kind: String,
    pub context_max_tokens: u16,
    pub context_reserve_system_tokens: u16,
    pub context_min_working_tokens: u16,
    pub context_max_tool_results: usize,
    pub context_max_memory_segments: usize,
    pub api_key_state: Option<String>,
    pub placeholder_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubagentLiveWorkerSummary {
    pub enabled: bool,
    pub adapter_kind: String,
    pub status: String,
    pub starts_worker: bool,
    pub available: bool,
    pub reason: String,
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
            identity_bootstrap: IdentityBootstrapConfig::new("./identity"),
            rules: RulesConfig::new("./rules"),
            governance: GovernanceConfig::StaticRule,
            tool_loop: ToolLoopConfig {
                max_rounds: 4,
                shell_timeout_ms: 30_000,
                shell_risk_rules: ShellRiskRules::default(),
            },
            actuator: ActuatorConfig::Fake,
            subagent: SubagentConfig::Fake,
            subagent_live_worker: SubagentLiveWorkerConfig::disabled(),
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
        self.identity_bootstrap.validate()?;
        self.rules.validate()?;
        self.governance.validate()?;
        self.tool_loop.validate()?;
        self.actuator.validate()?;
        self.subagent.validate()?;
        self.subagent_live_worker.validate()?;
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
            provider_tls_ca_cert_path: provider.tls_ca_cert_path,
            provider_request_timeout_ms: provider.request_timeout_ms,
            provider_fallback_policy: provider.fallback_policy,
            governance_kind: self.governance.kind().to_string(),
            actuator_kind: self.actuator.kind().to_string(),
            subagent_kind: self.subagent.kind().to_string(),
            subagent_live_worker: self.subagent_live_worker.summary(),
            subagent_queue_root: self.subagent_queue.root.display().to_string(),
            evolution_kind: self.evolution.kind().to_string(),
            control_plane_kind: self.control_plane.kind().to_string(),
            control_command_timeout_ms: self.control_plane.command_timeout_ms(),
            actuator_command_timeout_ms: self.actuator.command_timeout_ms(),
            identity_memory_kind: identity_memory.kind,
            identity_memory_root: identity_memory.root,
            identity_experiences_path: identity_memory.experiences_path,
            identity_user_max_chars: identity_memory.user_max_chars,
            identity_memory_max_chars: identity_memory.memory_max_chars,
            identity_root: self.identity_bootstrap.root.display().to_string(),
            soul_path: self.identity_bootstrap.soul_path.display().to_string(),
            story_path: self.identity_bootstrap.story_path.display().to_string(),
            first_wake_path: self
                .identity_bootstrap
                .first_wake_path
                .display()
                .to_string(),
            agents_registry_path: self
                .identity_bootstrap
                .agents_registry_path
                .display()
                .to_string(),
            rules_root: self.rules.root.display().to_string(),
            rules_core_path: self.rules.core_path.display().to_string(),
            tool_loop_max_rounds: self.tool_loop.max_rounds,
            tool_shell_timeout_ms: self.tool_loop.shell_timeout_ms,
            tool_shell_risk_rule_counts: self.tool_loop.shell_risk_rule_counts(),
            db_path: self.db_path.display().to_string(),
            recall_limit: self.recall_limit,
            context_engine_kind: self.context_engine.kind().to_string(),
            context_max_tokens: self.context_budget.max_tokens,
            context_reserve_system_tokens: self.context_budget.reserve_system_tokens,
            context_min_working_tokens: self.context_budget.min_working_tokens,
            context_max_tool_results: self.context_budget.max_tool_results,
            context_max_memory_segments: self.context_budget.max_memory_segments,
            api_key_state: provider.api_key_state,
            placeholder_warnings: self.placeholder_warnings(),
        }
    }

    fn placeholder_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.provider.uses_fake_responder() {
            warnings.push(
                "provider=fake is a local test responder; configure openai_compatible for real conversation"
                    .to_string(),
            );
        }
        if self.provider.uses_stub_transport() {
            warnings.push(
                "provider transport=stub only builds local preview responses; use native or curl for real calls"
                    .to_string(),
            );
        }
        if let ProviderConfig::OpenAICompatible(config) = &self.provider {
            if let Some(name) = config
                .api_key
                .strip_prefix("__MISSING_ENV:")
                .and_then(|value| value.strip_suffix("__"))
            {
                warnings.push(format!(
                    "provider api_key_env missing for {name}; status/config show are running in diagnostic mode"
                ));
            }
        }
        if matches!(self.actuator, ActuatorConfig::Fake) {
            warnings.push(
                "actuator=fake is a placeholder; no real desktop/browser operation adapter is configured"
                    .to_string(),
            );
        }
        if matches!(self.subagent, SubagentConfig::Fake) {
            warnings.push(
                "subagent=fake is a local test runner; use queued_external plus command runner for real workers"
                    .to_string(),
            );
        }
        if self.subagent_live_worker.enabled {
            warnings.push(
                "subagent_live_worker is status-only; live worker execution remains unavailable until an audited adapter is wired"
                    .to_string(),
            );
        }
        if matches!(self.control_plane, ControlPlaneConfig::FakeLocal) {
            warnings.push(
                "control_plane=fake_local is a placeholder; configure command control for real service control"
                    .to_string(),
            );
        }

        warnings
    }
}

impl RulesConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            core_path: root.join("core.md"),
            root,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty("rules.root", &self.root.display().to_string())?;
        require_non_empty("rules.core_path", &self.core_path.display().to_string())
    }
}

impl IdentityBootstrapConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            soul_path: root.join("SOUL.md"),
            story_path: root.join("STORY.md"),
            first_wake_path: root.join("FIRST_WAKE.md"),
            agents_registry_path: root.join("agents.toml"),
            root,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty("identity.root", &self.root.display().to_string())?;
        require_non_empty("identity.soul_path", &self.soul_path.display().to_string())?;
        require_non_empty(
            "identity.story_path",
            &self.story_path.display().to_string(),
        )?;
        require_non_empty(
            "identity.first_wake_path",
            &self.first_wake_path.display().to_string(),
        )?;
        require_non_empty(
            "identity.agents_registry_path",
            &self.agents_registry_path.display().to_string(),
        )
    }
}

impl ToolLoopConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_rounds == 0 {
            return Err(ConfigError {
                field: "tool_loop.max_rounds".to_string(),
                message: "tool loop max_rounds must be greater than zero".to_string(),
            });
        }
        if self.max_rounds > 32 {
            return Err(ConfigError {
                field: "tool_loop.max_rounds".to_string(),
                message: "tool loop max_rounds must not exceed 32".to_string(),
            });
        }
        if self.shell_timeout_ms == 0 {
            return Err(ConfigError {
                field: "tool_loop.shell_timeout_ms".to_string(),
                message: "tool shell_timeout_ms must be greater than zero".to_string(),
            });
        }
        if self.shell_timeout_ms > 600_000 {
            return Err(ConfigError {
                field: "tool_loop.shell_timeout_ms".to_string(),
                message: "tool shell_timeout_ms must not exceed 600000".to_string(),
            });
        }
        Ok(())
    }

    pub fn shell_risk_rule_counts(&self) -> String {
        format!(
            "delete_or_cleanup={},service_change={},network_change={},secret_access={}",
            self.shell_risk_rules.delete_or_cleanup.len(),
            self.shell_risk_rules.service_change.len(),
            self.shell_risk_rules.network_change.len(),
            self.shell_risk_rules.secret_access.len()
        )
    }
}

impl ProviderConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Fake { .. } => "fake",
            Self::OpenAICompatible(_) => "openai_compatible",
            Self::Fallback { .. } => "fallback",
        }
    }

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
            Self::Fallback {
                primary,
                fallback,
                policy,
            } => {
                primary.validate()?;
                fallback.validate()?;
                policy.validate()
            }
        }
    }

    fn summary_parts(&self) -> ProviderSummaryParts {
        match self {
            Self::Fake {
                provider_id,
                model_name,
            } => ProviderSummaryParts {
                kind: self.kind().to_string(),
                provider_id: provider_id.clone(),
                model_name: model_name.clone(),
                tls_ca_cert_path: None,
                api_key_state: None,
                request_timeout_ms: None,
                fallback_policy: None,
            },
            Self::OpenAICompatible(config) => ProviderSummaryParts {
                kind: self.kind().to_string(),
                provider_id: config.provider_id.clone(),
                model_name: config.model_name.clone(),
                tls_ca_cert_path: config
                    .tls_ca_cert_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                api_key_state: Some(mask_key_state(&config.api_key)),
                request_timeout_ms: config.request_timeout_ms,
                fallback_policy: None,
            },
            Self::Fallback {
                primary,
                fallback,
                policy,
            } => {
                let primary = primary.summary_parts();
                let fallback = fallback.summary_parts();
                ProviderSummaryParts {
                    kind: self.kind().to_string(),
                    provider_id: format!("{}->{}", primary.provider_id, fallback.provider_id),
                    model_name: format!("{}->{}", primary.model_name, fallback.model_name),
                    tls_ca_cert_path: primary.tls_ca_cert_path.or(fallback.tls_ca_cert_path),
                    api_key_state: Some(format!(
                        "primary:{} fallback:{}",
                        primary.api_key_state.unwrap_or_else(|| "none".to_string()),
                        fallback.api_key_state.unwrap_or_else(|| "none".to_string())
                    )),
                    request_timeout_ms: primary.request_timeout_ms.or(fallback.request_timeout_ms),
                    fallback_policy: Some(policy.summary()),
                }
            }
        }
    }

    fn uses_stub_transport(&self) -> bool {
        match self {
            Self::Fake { .. } => false,
            Self::OpenAICompatible(OpenAICompatibleConfig {
                transport: ProviderTransport::Stub,
                ..
            }) => true,
            Self::OpenAICompatible(_) => false,
            Self::Fallback {
                primary, fallback, ..
            } => primary.uses_stub_transport() || fallback.uses_stub_transport(),
        }
    }

    fn uses_fake_responder(&self) -> bool {
        match self {
            Self::Fake { .. } => true,
            Self::OpenAICompatible(_) => false,
            Self::Fallback {
                primary, fallback, ..
            } => primary.uses_fake_responder() || fallback.uses_fake_responder(),
        }
    }
}

impl Default for ProviderFallbackPolicy {
    fn default() -> Self {
        Self {
            on_retryable: true,
            status_codes: vec![401, 402],
            error_classes: Vec::new(),
        }
    }
}

impl ProviderFallbackPolicy {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self
            .status_codes
            .iter()
            .any(|status| *status < 100 || *status > 599)
        {
            return Err(ConfigError {
                field: "fallback.status_codes".to_string(),
                message: "fallback status codes must be HTTP status codes from 100 to 599"
                    .to_string(),
            });
        }
        if self
            .error_classes
            .iter()
            .any(|class| class.trim().is_empty())
        {
            return Err(ConfigError {
                field: "fallback.error_classes".to_string(),
                message: "fallback error classes must not be empty".to_string(),
            });
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        let status_codes = if self.status_codes.is_empty() {
            "none".to_string()
        } else {
            self.status_codes
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let error_classes = if self.error_classes.is_empty() {
            "none".to_string()
        } else {
            self.error_classes.join(",")
        };
        format!(
            "retryable={} status_codes={} error_classes={}",
            self.on_retryable, status_codes, error_classes
        )
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
                experiences_path: root
                    .join(crate::hermes_memory::DEFAULT_EXPERIENCES_MEMORY_FILE)
                    .display()
                    .to_string(),
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
            Self::SummaryCompression => "summary_compression",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }

    pub fn to_context_engine_kind(&self) -> ContextEngineKind {
        match self {
            Self::DeterministicBudget => ContextEngineKind::DeterministicBudget,
            Self::SummaryCompression => ContextEngineKind::SummaryCompression,
        }
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
            Self::Command(_) => "command",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::Fake => Ok(()),
            Self::Command(config) => {
                require_non_empty("actuator.program", &config.program)?;
                require_non_empty("actuator.args", &config.args)?;
                require_positive_u64("actuator.timeout_ms", config.timeout_ms)
            }
        }
    }

    pub fn command_timeout_ms(&self) -> Option<u64> {
        match self {
            Self::Fake => None,
            Self::Command(config) => Some(config.timeout_ms),
        }
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

impl SubagentLiveWorkerConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            adapter_kind: "none".to_string(),
            status: "disabled".to_string(),
            starts_worker: false,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty("subagent_live_worker.adapter_kind", &self.adapter_kind)?;
        require_non_empty("subagent_live_worker.status", &self.status)?;
        if self.starts_worker {
            return Err(ConfigError {
                field: "subagent_live_worker.starts_worker".to_string(),
                message: "subagent_live_worker is status-only and must not start a worker"
                    .to_string(),
            });
        }
        Ok(())
    }

    pub fn summary(&self) -> SubagentLiveWorkerSummary {
        let available = false;
        let reason = if self.enabled {
            format!(
                "subagent_live_worker config is enabled for adapter_kind={}, but this build exposes status only and does not start workers",
                self.adapter_kind
            )
        } else {
            "subagent_live_worker disabled by default; no live worker is started".to_string()
        };
        SubagentLiveWorkerSummary {
            enabled: self.enabled,
            adapter_kind: self.adapter_kind.clone(),
            status: self.status.clone(),
            starts_worker: self.starts_worker,
            available,
            reason,
        }
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
            Self::Command(_) => "command",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::FakeLocal => Ok(()),
            Self::Command(config) => {
                require_non_empty("control.program", &config.program)?;
                require_non_empty("control.list_args", &config.list_args)?;
                require_non_empty("control.apply_args", &config.apply_args)?;
                require_positive_u64("control.timeout_ms", config.timeout_ms)
            }
        }
    }

    pub fn command_timeout_ms(&self) -> Option<u64> {
        match self {
            Self::FakeLocal => None,
            Self::Command(config) => Some(config.timeout_ms),
        }
    }
}

impl OpenAICompatibleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        require_non_empty("provider.provider_id", &self.provider_id)?;
        require_non_empty("provider.base_url", &self.base_url)?;
        require_non_empty("provider.api_key", &self.api_key)?;
        require_non_empty("provider.model_name", &self.model_name)?;
        if let Some(path) = &self.tls_ca_cert_path {
            require_non_empty("provider.tls_ca_path", &path.display().to_string())?;
            if !path.exists() {
                return Err(ConfigError {
                    field: "provider.tls_ca_path".to_string(),
                    message: format!("provider.tls_ca_path does not exist: {}", path.display()),
                });
            }
        }
        if let Some(timeout_ms) = self.request_timeout_ms {
            require_positive_u64("provider.request_timeout_ms", timeout_ms)?;
        }

        Ok(())
    }
}

struct ProviderSummaryParts {
    kind: String,
    provider_id: String,
    model_name: String,
    tls_ca_cert_path: Option<String>,
    request_timeout_ms: Option<u64>,
    api_key_state: Option<String>,
    fallback_policy: Option<String>,
}

struct IdentityMemorySummaryParts {
    kind: String,
    root: String,
    experiences_path: String,
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

fn require_positive_u64(field: &str, value: u64) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError {
            field: field.to_string(),
            message: format!("{field} must be greater than zero"),
        });
    }

    Ok(())
}

fn mask_key_state(api_key: &str) -> String {
    if let Some(name) = api_key
        .strip_prefix("__MISSING_ENV:")
        .and_then(|value| value.strip_suffix("__"))
    {
        return format!("<missing:{name}>");
    }
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
