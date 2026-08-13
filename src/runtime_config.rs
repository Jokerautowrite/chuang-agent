//! `runtime_config` 模块。公开接口：struct RuntimeConfig, OpenAICompatibleConfig, AnthropicCompatibleConfig, ProviderFallbackPolicy, IdentityBootstrapConfig, RulesConfig, PermissionRuntimeConfig, ToolLoopConfig, ActuatorCommandConfig, ContextCompactionConfig；enum ProviderConfig, ProviderApiEndpoint, AnthropicApiEndpoint, IdentityMemoryConfig, ContextEngineConfig, GovernanceConfig, ActuatorConfig, SubagentConfig, CanonicalEvolutionGovernance；fn as_str, new, validate, summary, shell_risk_rule_counts, kind, build_dual_file_config, to_context_engine_kind, context_compaction_config；const DEFAULT_WORKSPACE_ROOT, TOOL_LOOP_MAX_ROUNDS_CAP。

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::context_engine::{ContextBudget, ContextEngineKind};
use crate::hermes_memory::{
    DualFileMemoryConfig, DEFAULT_HOT_MEMORY_MAX_CHARS, DEFAULT_USER_MEMORY_MAX_CHARS,
};
use crate::knowledge_read::KnowledgeReadConfig;
use crate::provider_openai_compatible::{ProviderTransport, ReasoningEffort};
use crate::skill_evolver::FailureDetectorConfig;
use crate::subagent_queue::FileSubagentQueueConfig;
use crate::tool_runtime::ShellRiskRules;
use serde::{Deserialize, Serialize};

/// 默认工作区根（仅作为 RuntimeConfig::new 的初始哨兵值；
/// 实际生效值由 app_server 按 base_dir / 环境变量归一化）。
pub const DEFAULT_WORKSPACE_ROOT: &str = "/home/user/projects/chuang-agent";
/// 工具循环最大轮数上限（防失控保护）。
/// 32 轮跑长任务不够，放宽到 256；config 中 max_rounds 超过此值仍会被拒绝。
pub const TOOL_LOOP_MAX_ROUNDS_CAP: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub db_path: PathBuf,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: ContextBudget,
    pub context_engine: ContextEngineConfig,
    /// 压缩时保留的最近完整对话轮数。
    pub context_recent_turns: usize,
    pub provider: ProviderConfig,
    /// 视觉模型（用于识图兜底：主模型不支持视觉时，用它把图片描述成文字）。
    /// 形如 "sub2/mimo-v2.5"，走 opencodex 路由。
    pub vision_model: Option<String>,
    pub identity_memory: IdentityMemoryConfig,
    pub identity_bootstrap: IdentityBootstrapConfig,
    pub rules: RulesConfig,
    pub governance: GovernanceConfig,
    pub permission: PermissionRuntimeConfig,
    pub tool_loop: ToolLoopConfig,
    pub actuator: ActuatorConfig,
    pub subagent: SubagentConfig,
    pub subagent_live_worker: SubagentLiveWorkerConfig,
    pub subagent_queue: SubagentQueueConfig,
    pub evolution: EvolutionConfig,
    pub control_plane: ControlPlaneConfig,
    pub external_knowledge: KnowledgeReadConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfig {
    Fake {
        provider_id: String,
        model_name: String,
    },
    OpenAICompatible(OpenAICompatibleConfig),
    AnthropicCompatible(AnthropicCompatibleConfig),
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
    /// API endpoint shape used for this provider. `Responses` matches the
    /// OpenAI Responses API (`/responses`); `ChatCompletions` matches the
    /// classic chat-completions API (`/chat/completions`) used by most
    /// OpenAI-compatible gateways (e.g. example-provider / ccswitch for
    /// deepseek-v4-flash). Defaults to Responses for backwards compatibility.
    pub endpoint: ProviderApiEndpoint,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub request_timeout_ms: Option<u64>,
    pub tls_ca_cert_path: Option<PathBuf>,
}

/// Anthropic Messages API（/v1/messages）兼容 provider 配置。
/// 与 `OpenAICompatibleConfig` 平行：认证走 `x-api-key` +
/// `anthropic-version: 2023-06-01`，system prompt 是顶层 `system` 字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicCompatibleConfig {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub transport: ProviderTransport,
    /// Anthropic API endpoint shape. `Messages` matches `/v1/messages`
    /// (the only Messages API endpoint today); kept as an enum so future
    /// Anthropic endpoint shapes (e.g. `/v1/messages/count_tokens`) can be
    /// added without changing the config schema.
    pub endpoint: AnthropicApiEndpoint,
    /// Accepted for config parity with openai_compatible. Anthropic 原生扩展
    /// 思考走 `thinking` 字段（需模型支持），默认不据此生成请求体字段。
    pub reasoning_effort: Option<ReasoningEffort>,
    pub request_timeout_ms: Option<u64>,
    pub tls_ca_cert_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnthropicApiEndpoint {
    Messages,
}

impl AnthropicApiEndpoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Messages => "messages",
        }
    }
}

impl Default for AnthropicApiEndpoint {
    fn default() -> Self {
        Self::Messages
    }
}

impl std::str::FromStr for AnthropicApiEndpoint {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "messages" | "v1_messages" | "anthropic_messages" => Ok(Self::Messages),
            other => Err(format!(
                "unsupported anthropic endpoint: {other} (supported: messages)"
            )),
        }
    }
}

impl std::fmt::Display for AnthropicApiEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderApiEndpoint {
    Responses,
    ChatCompletions,
}

impl ProviderApiEndpoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat_completions",
        }
    }
}

impl Default for ProviderApiEndpoint {
    fn default() -> Self {
        Self::Responses
    }
}

impl std::str::FromStr for ProviderApiEndpoint {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "responses" | "openai_responses" => Ok(Self::Responses),
            "chat_completions" | "chat" | "chatcompletions" => Ok(Self::ChatCompletions),
            other => Err(format!(
                "unsupported provider endpoint: {other} (supported: responses, chat_completions)"
            )),
        }
    }
}

impl std::fmt::Display for ProviderApiEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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

/// 上下文压缩熔断配置（Claude Code 5：连续 3 次 autocompact 失败停止重试）。
/// 真值源：RuntimeConfig.metadata 透传键（config 的 [metadata] 段）：
/// - `context_compaction_breaker_threshold`：连续失败 N 次熔断（默认 3）；
/// - `context_compaction_breaker_cooldown_secs`：熔断冷却秒数，冷却后自动复位（默认 60）。
/// 未配置时用默认值；引擎侧默认与之一致（summary_compression 模块常量）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCompactionConfig {
    pub breaker_threshold: usize,
    pub breaker_cooldown_secs: u64,
}

pub const DEFAULT_CONTEXT_COMPACTION_BREAKER_THRESHOLD: usize = 3;
pub const DEFAULT_CONTEXT_COMPACTION_BREAKER_COOLDOWN_SECS: u64 = 60;

impl Default for ContextCompactionConfig {
    fn default() -> Self {
        Self {
            breaker_threshold: DEFAULT_CONTEXT_COMPACTION_BREAKER_THRESHOLD,
            breaker_cooldown_secs: DEFAULT_CONTEXT_COMPACTION_BREAKER_COOLDOWN_SECS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceConfig {
    StaticRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuntimeConfig {
    pub profile: String,
    pub approval_policy: String,
    pub workspace_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLoopConfig {
    pub max_rounds: usize,
    pub shell_timeout_ms: u64,
    /// Auto-prefix supported shell tools with `rtk` for compact tool output.
    pub shell_rtk_rewrite: bool,
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

/// canonical 外环的规则修改治理门禁槽。
/// `policy` = 确定性证据门禁（证据必须能在已观察事件流中验证，否则拒绝）；
/// `noop` = 永不批准（安全默认，绝不写盘）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalEvolutionGovernance {
    Policy,
    Noop,
}

impl Default for CanonicalEvolutionGovernance {
    fn default() -> Self {
        Self::Policy
    }
}

impl std::str::FromStr for CanonicalEvolutionGovernance {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "policy" => Ok(Self::Policy),
            "noop" => Ok(Self::Noop),
            other => Err(format!(
                "unsupported evolution governance: {other} (supported: policy, noop)"
            )),
        }
    }
}

/// canonical 外环配置。所有字段都有默认值：配置只写 `evolution = "canonical"`
/// 也能解析。serde 向后兼容：新字段缺省时回落到 `Default`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CanonicalEvolutionConfig {
    /// 技能/规则落盘根目录（canonical markdown 格式；默认与 governance 规则根同源）。
    pub skill_root: PathBuf,
    /// 自批准分数线 0-100（低于阈值不写盘）。
    pub approval_threshold: u16,
    /// 重复失败检测配置（窗口 / 最低重复次数 / 失败事件类型）。
    pub detector: FailureDetectorConfig,
    /// 规则修改治理门禁槽（默认 policy：确定性证据门禁）。
    pub governance: CanonicalEvolutionGovernance,
    /// 是否在每个 agent turn 结束后自动驱动 evolver 外环
    /// （detect → propose → governance → apply）。默认关闭，向后兼容：
    /// 配置只写 `evolution = "canonical"` 时不会突然开始自动写规则。
    pub auto_outer_loop: bool,
}

impl Default for CanonicalEvolutionConfig {
    fn default() -> Self {
        Self {
            skill_root: PathBuf::from("./rules"),
            approval_threshold: 75,
            detector: FailureDetectorConfig::default(),
            governance: CanonicalEvolutionGovernance::default(),
            auto_outer_loop: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionConfig {
    Noop,
    DryRun,
    Canonical(CanonicalEvolutionConfig),
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
    pub provider_reasoning_effort: Option<String>,
    pub provider_fallback_policy: Option<String>,
    pub governance_kind: String,
    pub permission_profile: String,
    pub approval_policy: String,
    pub permission_workspace_root: String,
    pub actuator_kind: String,
    pub subagent_kind: String,
    pub subagent_live_worker: SubagentLiveWorkerSummary,
    pub subagent_queue_root: String,
    pub evolution_kind: String,
    pub control_plane_kind: String,
    pub control_command_timeout_ms: Option<u64>,
    pub external_knowledge_wiki_endpoint: Option<String>,
    pub external_knowledge_wiki_token_env: Option<String>,
    pub external_knowledge_wiki_timeout_ms: Option<u64>,
    pub external_knowledge_gbrain_endpoint: Option<String>,
    pub external_knowledge_gbrain_token_env: Option<String>,
    pub external_knowledge_gbrain_timeout_ms: Option<u64>,
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
    pub tool_shell_rtk_rewrite: bool,
    pub tool_shell_risk_rule_counts: String,
    pub db_path: String,
    pub recall_limit: usize,
    pub context_engine_kind: String,
    pub context_recent_turns: usize,
    pub context_max_tokens: u32,
    pub context_reserve_system_tokens: u32,
    pub context_min_working_tokens: u32,
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
            context_recent_turns: 10,
            provider: ProviderConfig::Fake {
                provider_id: "fake-runtime".to_string(),
                model_name: "stub-responder".to_string(),
            },
            vision_model: None,
            identity_memory: IdentityMemoryConfig::HermesDualFile {
                // 与 identity_bootstrap 同根：创自己的工作区，不用 hermes 遗留路径
                root: PathBuf::from("./identity"),
                user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
                memory_max_chars: DEFAULT_HOT_MEMORY_MAX_CHARS,
            },
            identity_bootstrap: IdentityBootstrapConfig::new("./identity"),
            rules: RulesConfig::new("./rules"),
            governance: GovernanceConfig::StaticRule,
            permission: PermissionRuntimeConfig {
                profile: "full_local_workspace".to_string(),
                approval_policy: "auto_for_workspace".to_string(),
                workspace_root: PathBuf::from(DEFAULT_WORKSPACE_ROOT),
            },
            tool_loop: ToolLoopConfig {
                max_rounds: 4,
                shell_timeout_ms: 120_000,
                shell_rtk_rewrite: true,
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
            external_knowledge: KnowledgeReadConfig::disabled(),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.recall_limit == 0 {
            return Err(ConfigError {
                field: "recall_limit".to_string(),
                message: "recall_limit must be greater than zero".to_string(),
            });
        }
        if self.permission.profile != "full_local_workspace" {
            return Err(ConfigError {
                field: "permission.profile".to_string(),
                message: "permission profile must be full_local_workspace".to_string(),
            });
        }
        if !matches!(
            self.permission.approval_policy.as_str(),
            "auto_for_workspace" | "unrestricted"
        ) {
            return Err(ConfigError {
                field: "permission.approval_policy".to_string(),
                message: "approval policy must be auto_for_workspace or unrestricted".to_string(),
            });
        }
        if self.permission.workspace_root.as_os_str().is_empty() {
            return Err(ConfigError {
                field: "permission.workspace_root".to_string(),
                message: "permission workspace_root must not be empty".to_string(),
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
        if self.context_recent_turns == 0 {
            return Err(ConfigError {
                field: "context.recent_turns".to_string(),
                message: "context recent_turns must be greater than zero".to_string(),
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

    /// knowledge_context（GBrain 直连 API 通道）显式开关：metadata
    /// `knowledge_context=1` 时启用（与 emotion_brain 同款约定）。
    pub fn knowledge_context_enabled(&self) -> bool {
        self.metadata
            .get("knowledge_context")
            .map(|value| value == "1")
            .unwrap_or(false)
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
            provider_reasoning_effort: provider.reasoning_effort,
            provider_fallback_policy: provider.fallback_policy,
            governance_kind: self.governance.kind().to_string(),
            permission_profile: self.permission.profile.clone(),
            approval_policy: self.permission.approval_policy.clone(),
            permission_workspace_root: self.permission.workspace_root.display().to_string(),
            actuator_kind: self.actuator.kind().to_string(),
            subagent_kind: self.subagent.kind().to_string(),
            subagent_live_worker: self.subagent_live_worker.summary(),
            subagent_queue_root: self.subagent_queue.root.display().to_string(),
            evolution_kind: self.evolution.kind().to_string(),
            control_plane_kind: self.control_plane.kind().to_string(),
            control_command_timeout_ms: self.control_plane.command_timeout_ms(),
            external_knowledge_wiki_endpoint: self
                .external_knowledge
                .wiki
                .endpoint
                .as_ref()
                .map(|value| value.to_string()),
            external_knowledge_wiki_token_env: self
                .external_knowledge
                .wiki
                .token_env
                .as_ref()
                .map(|value| value.to_string()),
            external_knowledge_wiki_timeout_ms: self.external_knowledge.wiki.timeout_ms,
            external_knowledge_gbrain_endpoint: self
                .external_knowledge
                .gbrain
                .endpoint
                .as_ref()
                .map(|value| value.to_string()),
            external_knowledge_gbrain_token_env: self
                .external_knowledge
                .gbrain
                .token_env
                .as_ref()
                .map(|value| value.to_string()),
            external_knowledge_gbrain_timeout_ms: self.external_knowledge.gbrain.timeout_ms,
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
            tool_shell_rtk_rewrite: self.tool_loop.shell_rtk_rewrite,
            tool_shell_risk_rule_counts: self.tool_loop.shell_risk_rule_counts(),
            db_path: self.db_path.display().to_string(),
            recall_limit: self.recall_limit,
            context_engine_kind: self.context_engine.kind().to_string(),
            context_recent_turns: self.context_recent_turns,
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
        if let Some(name) = missing_provider_api_key_env(&self.provider) {
            warnings.push(format!(
                "provider api_key_env missing for {name}; status/config show are running in diagnostic mode"
            ));
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
        push_external_knowledge_warning(
            &mut warnings,
            "wiki",
            &self.external_knowledge.wiki,
        );
        push_external_knowledge_warning(
            &mut warnings,
            "gbrain",
            &self.external_knowledge.gbrain,
        );

        warnings
    }

    /// 上下文压缩熔断配置：优先读 [metadata] 透传键，未配置/非法时回退默认值。
    pub fn context_compaction_config(&self) -> ContextCompactionConfig {
        let breaker_threshold = self
            .metadata
            .get("context_compaction_breaker_threshold")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value >= 1)
            .unwrap_or(DEFAULT_CONTEXT_COMPACTION_BREAKER_THRESHOLD);
        let breaker_cooldown_secs = self
            .metadata
            .get("context_compaction_breaker_cooldown_secs")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_CONTEXT_COMPACTION_BREAKER_COOLDOWN_SECS);
        ContextCompactionConfig {
            breaker_threshold,
            breaker_cooldown_secs,
        }
    }
}

fn push_external_knowledge_warning(
    warnings: &mut Vec<String>,
    source: &str,
    config: &crate::knowledge_read::KnowledgeReadSourceConfig,
) {
    let endpoint_configured = config
        .endpoint
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let token_env_configured = config
        .token_env
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !endpoint_configured && !token_env_configured {
        return;
    }
    let token_available = config
        .token_env
        .as_ref()
        .map(|name| {
            std::env::var(name)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if endpoint_configured && token_env_configured && token_available {
        return;
    }
    let missing = if !endpoint_configured {
        "endpoint"
    } else if !token_env_configured {
        "token_env"
    } else {
        "token (export the env named by external_knowledge.{source}.token_env)"
    };
    warnings.push(format!(
        "external_knowledge.{source} is partially configured; live read remains unavailable until {missing} is set"
    ));
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
        if self.max_rounds > TOOL_LOOP_MAX_ROUNDS_CAP {
            return Err(ConfigError {
                field: "tool_loop.max_rounds".to_string(),
                message: format!("tool loop max_rounds must not exceed {TOOL_LOOP_MAX_ROUNDS_CAP}"),
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
            "delete_or_cleanup={},privilege_escalation={},service_change={},network_change={},secret_access={}",
            self.shell_risk_rules.delete_or_cleanup.len(),
            self.shell_risk_rules.privilege_escalation.len(),
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
            Self::AnthropicCompatible(_) => "anthropic_compatible",
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
            Self::AnthropicCompatible(config) => config.validate(),
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
                reasoning_effort: None,
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
                reasoning_effort: config.reasoning_effort.map(|effort| effort.to_string()),
                fallback_policy: None,
            },
            Self::AnthropicCompatible(config) => ProviderSummaryParts {
                kind: self.kind().to_string(),
                provider_id: config.provider_id.clone(),
                model_name: config.model_name.clone(),
                tls_ca_cert_path: config
                    .tls_ca_cert_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                api_key_state: Some(mask_key_state(&config.api_key)),
                request_timeout_ms: config.request_timeout_ms,
                reasoning_effort: config.reasoning_effort.map(|effort| effort.to_string()),
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
                    reasoning_effort: primary.reasoning_effort.or(fallback.reasoning_effort),
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
            Self::AnthropicCompatible(AnthropicCompatibleConfig {
                transport: ProviderTransport::Stub,
                ..
            }) => true,
            Self::AnthropicCompatible(_) => false,
            Self::Fallback {
                primary, fallback, ..
            } => primary.uses_stub_transport() || fallback.uses_stub_transport(),
        }
    }

    fn uses_fake_responder(&self) -> bool {
        match self {
            Self::Fake { .. } => true,
            Self::OpenAICompatible(_) => false,
            Self::AnthropicCompatible(_) => false,
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
            Self::DryRun => "dry_run",
            Self::Canonical(_) => "canonical",
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::Noop | Self::DryRun => Ok(()),
            Self::Canonical(config) => {
                require_non_empty(
                    "evolution.skill_root",
                    &config.skill_root.display().to_string(),
                )?;
                if config.approval_threshold == 0 || config.approval_threshold > 100 {
                    return Err(ConfigError {
                        field: "evolution.approval_threshold".to_string(),
                        message: "approval_threshold must be in 1..=100".to_string(),
                    });
                }
                if config.detector.min_repeats == 0 {
                    return Err(ConfigError {
                        field: "evolution.detector.min_repeats".to_string(),
                        message: "min_repeats must be greater than zero".to_string(),
                    });
                }
                if config.detector.failure_kinds.is_empty() {
                    return Err(ConfigError {
                        field: "evolution.detector.failure_kinds".to_string(),
                        message: "failure_kinds must not be empty".to_string(),
                    });
                }
                Ok(())
            }
        }
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

impl AnthropicCompatibleConfig {
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
    reasoning_effort: Option<String>,
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

/// 沿 provider 链（嵌套 Fallback）找第一个未解析的 api_key_env 占位符名。
fn missing_provider_api_key_env(provider: &ProviderConfig) -> Option<String> {
    match provider {
        ProviderConfig::Fake { .. } => None,
        ProviderConfig::OpenAICompatible(config) => missing_env_name(&config.api_key),
        ProviderConfig::AnthropicCompatible(config) => missing_env_name(&config.api_key),
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => missing_provider_api_key_env(primary)
            .or_else(|| missing_provider_api_key_env(fallback)),
    }
}

fn missing_env_name(api_key: &str) -> Option<String> {
    api_key
        .strip_prefix("__MISSING_ENV:")
        .and_then(|value| value.strip_suffix("__"))
        .map(str::to_string)
}

pub fn default_context_budget() -> ContextBudget {
    ContextBudget {
        max_tokens: 272000,
        reserve_system_tokens: 4096,
        min_working_tokens: 1,
        max_tool_results: 5,
        max_memory_segments: 5,
    }
}
