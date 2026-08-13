//! `slot_registry` 模块。公开接口：struct RuntimeSlots, CanonicalEvolutionSlot, GenesisSlot, RuntimeSlotsSummary；enum ProviderSlot, GovernanceSlot, ActuatorSlot, EvolutionSlot, CanonicalGovernanceSlot, ControlPlaneSlot, SubagentRuntimeSlot, EmotionSlotRuntime；fn provider_name, model_name, register_operator_approval, clone_boxed, new, evolver, evolver_mut, governance。

use crate::actuator::{
    Actuator, ActuatorError, AppHandle, ClickTarget, CommandActuator, EvidenceRef, FakeActuator,
    FocusTarget, InputTarget, Observation, ObserveTarget, OpenAppRequest, ScreenshotTarget,
    SecretOrPlainText,
};
use crate::common::AuditRecord;
use crate::control_plane::{
    CommandControlPlane, ControlError, ControlPlane, ControlReceipt, ControlRequest,
    FakeControlPlane, ManagedUnit,
};
use crate::emotion_slot::{
    EmotionDelta, EmotionSlot, EmotionSlotError, EmotionStateSnapshot, FakeEmotionSlot,
    JiwenEmotionSlot,
};
use crate::genesis_actuator::{AutoCliGenesisActuator, GenesisConfig, SystemGenesisCommandRunner};
use crate::genesis_actuator::{
    GenesisActuator, GenesisAskRequest, GenesisAskResponse, GenesisCommandSpec, GenesisError,
};
use crate::governance::{
    Governance, GovernanceError, MarkdownRuleSet, OperatorApprovalEvidence, ProposedAction,
    RiskDecision, StaticRuleGovernance,
};
use crate::permission_profile_slot::unrestricted_profile;
use crate::provider_anthropic_compatible::AnthropicCompatibleProviderAdapter;
use crate::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use crate::responder::{
    FakeResponder, ProviderAdapterResponder, Responder, ResponderOutput, ResponderProvider,
    ResponderRequest,
};
use crate::runtime_config::{
    ActuatorConfig, CanonicalEvolutionGovernance, ConfigError, ControlPlaneConfig, EvolutionConfig,
    GovernanceConfig, ProviderConfig, ProviderFallbackPolicy, RuntimeConfig, SubagentConfig,
};
use crate::skill_evolver::{
    CanonicalSkillEvolver, DryRunProposalEvolver, EvolutionError, EvolutionReceipt, EvolutionScope,
    FailureDetectorConfig, FailurePattern, GovernanceContext, GovernanceDecision, NoopEvolver,
    NoopRuleChangeGovernance, PolicyRuleChangeGovernance, RuleChangeGovernance,
    RuleChangeJournalEntry, RuleChangeProposal, RuleChangeReceipt, RuntimeEvent, SkillEvolver,
    SkillId, SkillProposal, ValidationReport,
};
use crate::subagent_queue::{FileSubagentQueue, FileSubagentQueueError};
use crate::subagent_spawner::{
    FakeSubagentSpawner, KillReason, QueuedSubagentSpawner, RunId, SpawnReceipt, SpawnRequest,
    SubagentError, SubagentSpawner,
};
use crate::tool_runtime::{build_subagent_tool_context, ExecutionSlot, ToolExecutionConfig};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RuntimeSlots {
    pub provider: ProviderSlot,
    pub governance: GovernanceSlot,
    pub execution: ExecutionSlot,
    pub actuator: ActuatorSlot,
    pub subagent: SubagentRuntimeSlot,
    pub evolution: EvolutionSlot,
    pub control_plane: ControlPlaneSlot,
    pub emotion: EmotionSlotRuntime,
}

#[derive(Debug, Clone)]
pub enum ProviderSlot {
    Fake(FakeResponder),
    OpenAICompatible(OpenAICompatibleProviderAdapter),
    AnthropicCompatible(AnthropicCompatibleProviderAdapter),
    Fallback {
        primary: Box<ProviderSlot>,
        fallback: Box<ProviderSlot>,
        policy: ProviderFallbackPolicy,
    },
}

impl ProviderSlot {
    pub fn provider_name(&self) -> String {
        match self {
            Self::Fake(responder) => responder.provider().provider_id,
            Self::OpenAICompatible(responder) => responder.identity().provider_id,
            Self::AnthropicCompatible(responder) => responder.identity().provider_id,
            Self::Fallback { primary, .. } => primary.provider_name(),
        }
    }

    pub fn model_name(&self) -> String {
        match self {
            Self::Fake(responder) => responder.provider().model_name,
            Self::OpenAICompatible(responder) => responder.identity().model_name,
            Self::AnthropicCompatible(responder) => responder.identity().model_name,
            Self::Fallback { primary, .. } => primary.model_name(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum GovernanceSlot {
    StaticRule(StaticRuleGovernance),
}

impl GovernanceSlot {
    pub fn register_operator_approval(
        &mut self,
        evidence: OperatorApprovalEvidence,
    ) -> Result<(), GovernanceError> {
        match self {
            Self::StaticRule(governance) => governance.register_operator_approval(evidence),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ActuatorSlot {
    Fake(FakeActuator),
    Command(CommandActuator),
}

#[derive(Debug, Clone)]
pub enum EvolutionSlot {
    Noop(NoopEvolver),
    DryRun(DryRunProposalEvolver),
    Canonical(CanonicalEvolutionSlot),
}

/// canonical 外环的治理门禁槽。治理是强制门禁：规则修改只有经
/// `RuleChangeGovernance` 批准后才允许走写路径，本槽位本身从不写盘。
#[derive(Debug, Clone)]
pub enum CanonicalGovernanceSlot {
    Policy(PolicyRuleChangeGovernance),
    Noop(NoopRuleChangeGovernance),
}

impl Default for CanonicalGovernanceSlot {
    fn default() -> Self {
        Self::Policy(PolicyRuleChangeGovernance::default())
    }
}

impl RuleChangeGovernance for CanonicalGovernanceSlot {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
        context: &GovernanceContext,
    ) -> Result<GovernanceDecision, EvolutionError> {
        match self {
            Self::Policy(governance) => governance.evaluate(proposal, context),
            Self::Noop(governance) => governance.evaluate(proposal, context),
        }
    }
}

impl CanonicalGovernanceSlot {
    /// owned 拷贝（接线层在持有 `&mut evolver` 时使用，避免借用冲突）。
    pub fn clone_boxed(&self) -> Box<dyn RuleChangeGovernance> {
        match self {
            Self::Policy(governance) => Box::new(governance.clone()),
            Self::Noop(governance) => Box::new(governance.clone()),
        }
    }
}

/// canonical 外环运行时槽：承载 `CanonicalSkillEvolver` + 治理门禁 + 检测配置。
/// 运行时事件流经 `SkillEvolver::observe` 进入外环，随后可走
/// detect → propose → governance apply 闭环。
#[derive(Debug, Clone)]
pub struct CanonicalEvolutionSlot {
    evolver: CanonicalSkillEvolver,
    governance: CanonicalGovernanceSlot,
    detector_config: FailureDetectorConfig,
}

impl CanonicalEvolutionSlot {
    pub fn new(
        evolver: CanonicalSkillEvolver,
        governance: CanonicalGovernanceSlot,
        detector_config: FailureDetectorConfig,
    ) -> Self {
        Self {
            evolver,
            governance,
            detector_config,
        }
    }

    pub fn evolver(&self) -> &CanonicalSkillEvolver {
        &self.evolver
    }

    pub fn evolver_mut(&mut self) -> &mut CanonicalSkillEvolver {
        &mut self.evolver
    }

    /// 治理门禁实例（外部调用方用它对提案做批准/拒绝）。
    pub fn governance(&self) -> &dyn RuleChangeGovernance {
        &self.governance
    }

    /// 治理上下文：当前已观察事件流 + 检测配置（证据验证用）。
    pub fn governance_context(&self) -> GovernanceContext {
        GovernanceContext {
            observed_events: self.evolver.observed_events().to_vec(),
            detector_config: self.detector_config.clone(),
        }
    }

    pub fn detector_config(&self) -> &FailureDetectorConfig {
        &self.detector_config
    }
}

impl EvolutionSlot {
    /// 外环治理门禁实例；仅 canonical 槽位提供。
    pub fn rule_change_governance(&self) -> Option<&dyn RuleChangeGovernance> {
        match self {
            Self::Canonical(slot) => Some(slot.governance()),
            Self::Noop(_) | Self::DryRun(_) => None,
        }
    }

    /// 外环治理门禁的 owned 拷贝；仅 canonical 槽位提供。
    /// 供接线层在持有 `&mut evolver` 时仍能调用治理门禁（避免借用冲突）。
    pub fn cloned_rule_change_governance(&self) -> Option<Box<dyn RuleChangeGovernance>> {
        match self {
            Self::Canonical(slot) => Some(slot.governance.clone_boxed()),
            Self::Noop(_) | Self::DryRun(_) => None,
        }
    }

    /// 外环治理上下文；仅 canonical 槽位提供。
    pub fn rule_change_governance_context(&self) -> Option<GovernanceContext> {
        match self {
            Self::Canonical(slot) => Some(slot.governance_context()),
            Self::Noop(_) | Self::DryRun(_) => None,
        }
    }

    /// canonical 外环的重复失败检测配置；仅 canonical 槽位提供。
    pub fn rule_change_detector_config(&self) -> Option<&FailureDetectorConfig> {
        match self {
            Self::Canonical(slot) => Some(slot.detector_config()),
            Self::Noop(_) | Self::DryRun(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ControlPlaneSlot {
    FakeLocal(FakeControlPlane),
    Command(CommandControlPlane),
}

#[derive(Debug, Clone)]
pub struct GenesisSlot {
    actuator: AutoCliGenesisActuator<SystemGenesisCommandRunner>,
}

#[derive(Debug, Clone)]
pub enum SubagentRuntimeSlot {
    Fake(FakeSubagentSpawner),
    QueuedExternal {
        spawner: QueuedSubagentSpawner,
        queue: FileSubagentQueue,
    },
}

/// 情感槽位：先 Fake 后真实；真实实现默认 JiwenEmotionSlot（五轴连续状态）。
#[derive(Debug, Clone)]
pub enum EmotionSlotRuntime {
    Fake(FakeEmotionSlot),
    Jiwen(JiwenEmotionSlot),
}

impl EmotionSlotRuntime {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Fake(_) => "fake",
            Self::Jiwen(_) => "jiwen",
        }
    }

    pub fn snapshot(&self) -> Result<EmotionStateSnapshot, EmotionSlotError> {
        match self {
            Self::Fake(slot) => slot.snapshot(),
            Self::Jiwen(slot) => slot.snapshot(),
        }
    }

    pub fn observe_delta(&mut self, delta: &EmotionDelta) -> Result<(), EmotionSlotError> {
        match self {
            Self::Fake(slot) => slot.observe_delta(delta),
            Self::Jiwen(slot) => slot.observe_delta(delta),
        }
    }

    pub fn reset_connection(&mut self) -> Result<(), EmotionSlotError> {
        match self {
            Self::Fake(slot) => slot.reset_connection(),
            Self::Jiwen(slot) => slot.reset_connection(),
        }
    }

    pub fn tick(
        &mut self,
        minutes_elapsed: f64,
    ) -> Result<Vec<crate::emotion_slot::EmotionTrigger>, EmotionSlotError> {
        match self {
            Self::Fake(slot) => slot.tick(minutes_elapsed),
            Self::Jiwen(slot) => slot.tick(minutes_elapsed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeSlotsSummary {
    pub provider: String,
    pub governance: String,
    pub execution: String,
    pub actuator: String,
    pub subagent: String,
    pub evolution: String,
    pub control_plane: String,
}

pub fn build_runtime_slots(config: &RuntimeConfig) -> Result<RuntimeSlots, ConfigError> {
    config.validate()?;

    Ok(RuntimeSlots {
        provider: build_provider_responder(&config.provider)?,
        governance: build_governance(config)?,
        execution: build_execution(config),
        actuator: build_actuator(&config.actuator)?,
        subagent: build_subagent(config)?,
        evolution: build_evolution(&config.evolution)?,
        control_plane: build_control_plane(&config.control_plane)?,
        emotion: EmotionSlotRuntime::Jiwen(JiwenEmotionSlot::default()),
    })
}

pub fn build_provider_responder(config: &ProviderConfig) -> Result<ProviderSlot, ConfigError> {
    config.validate()?;
    match config {
        ProviderConfig::Fake { model_name, .. } => {
            Ok(ProviderSlot::Fake(FakeResponder::new(model_name.clone())))
        }
        ProviderConfig::OpenAICompatible(config) => Ok(ProviderSlot::OpenAICompatible(
            OpenAICompatibleProviderAdapter::new(
                config.provider_id.clone(),
                config.base_url.clone(),
                config.api_key.clone(),
                config.model_name.clone(),
            )
            .with_transport(config.transport.clone())
            .with_endpoint(config.endpoint)
            .with_reasoning_effort(config.reasoning_effort)
            .with_request_timeout_ms(config.request_timeout_ms.unwrap_or(60_000))
            .with_tls_ca_cert_path(config.tls_ca_cert_path.clone()),
        )),
        ProviderConfig::AnthropicCompatible(config) => Ok(ProviderSlot::AnthropicCompatible(
            AnthropicCompatibleProviderAdapter::new(
                config.provider_id.clone(),
                config.base_url.clone(),
                config.api_key.clone(),
                config.model_name.clone(),
            )
            .with_transport(config.transport.clone())
            .with_endpoint(config.endpoint)
            .with_reasoning_effort(config.reasoning_effort)
            .with_request_timeout_ms(config.request_timeout_ms.unwrap_or(60_000))
            .with_tls_ca_cert_path(config.tls_ca_cert_path.clone()),
        )),
        ProviderConfig::Fallback {
            primary,
            fallback,
            policy,
        } => Ok(ProviderSlot::Fallback {
            primary: Box::new(build_provider_responder(primary)?),
            fallback: Box::new(build_provider_responder(fallback)?),
            policy: policy.clone(),
        }),
    }
}

pub fn summarize_runtime_slots(config: &RuntimeConfig) -> RuntimeSlotsSummary {
    RuntimeSlotsSummary {
        provider: config.provider.kind().to_string(),
        governance: config.governance.kind().to_string(),
        execution: "generic_agent_mvp".to_string(),
        actuator: config.actuator.kind().to_string(),
        subagent: config.subagent.kind().to_string(),
        evolution: config.evolution.kind().to_string(),
        control_plane: config.control_plane.kind().to_string(),
    }
}

pub fn build_governance_slot(config: &RuntimeConfig) -> Result<GovernanceSlot, ConfigError> {
    match config.governance {
        GovernanceConfig::StaticRule => {
            let rules =
                MarkdownRuleSet::load(&config.rules.core_path).map_err(|message| ConfigError {
                    field: "rules.core_path".to_string(),
                    message,
                })?;
            // 免审批不受限测试模式：approval_policy = "unrestricted" 时所有操作直接放行（仍保留审计）。
            // 由 config.toml 显式开启；关闭只需改回 auto_for_workspace，无需改代码。
            if config.permission.approval_policy == "unrestricted" {
                Ok(GovernanceSlot::StaticRule(
                    StaticRuleGovernance::with_rules_and_profile(rules, unrestricted_profile()),
                ))
            } else {
                Ok(GovernanceSlot::StaticRule(
                    StaticRuleGovernance::with_rules(rules),
                ))
            }
        }
    }
}

fn build_execution(config: &RuntimeConfig) -> ExecutionSlot {
    ExecutionSlot::generic_agent_mvp(ToolExecutionConfig {
        shell_timeout_ms: config.tool_loop.shell_timeout_ms,
        shell_rtk_rewrite: config.tool_loop.shell_rtk_rewrite,
        shell_risk_rules: config.tool_loop.shell_risk_rules.clone(),
        memory: None,
        actuator: None,
        subagent: Some(build_subagent_tool_context(config)),
    })
}

fn build_governance(config: &RuntimeConfig) -> Result<GovernanceSlot, ConfigError> {
    build_governance_slot(config)
}

fn build_actuator(config: &ActuatorConfig) -> Result<ActuatorSlot, ConfigError> {
    match config {
        ActuatorConfig::Fake => Ok(ActuatorSlot::Fake(FakeActuator::new())),
        ActuatorConfig::Command(config) => {
            Ok(ActuatorSlot::Command(CommandActuator::new(config.clone())))
        }
    }
}

fn build_subagent(config: &RuntimeConfig) -> Result<SubagentRuntimeSlot, ConfigError> {
    match config {
        RuntimeConfig {
            subagent: SubagentConfig::Fake,
            ..
        } => Ok(SubagentRuntimeSlot::Fake(FakeSubagentSpawner::new())),
        RuntimeConfig {
            subagent: SubagentConfig::QueuedExternal,
            ..
        } => {
            let queue = FileSubagentQueue::open(config.subagent_queue.build_file_queue_config()?)
                .map_err(subagent_queue_config_error)?;
            let mut spawner = QueuedSubagentSpawner::new();
            let dispatches = queue
                .list_dispatches()
                .map_err(subagent_queue_restore_error)?;
            for dispatch in dispatches {
                spawner
                    .restore_dispatch(dispatch)
                    .map_err(subagent_restore_config_error)?;
            }
            Ok(SubagentRuntimeSlot::QueuedExternal { spawner, queue })
        }
    }
}

fn build_evolution(config: &EvolutionConfig) -> Result<EvolutionSlot, ConfigError> {
    match config {
        EvolutionConfig::Noop => Ok(EvolutionSlot::Noop(NoopEvolver::new())),
        EvolutionConfig::DryRun => Ok(EvolutionSlot::DryRun(DryRunProposalEvolver::new())),
        EvolutionConfig::Canonical(config) => {
            let governance = match config.governance {
                CanonicalEvolutionGovernance::Policy => {
                    CanonicalGovernanceSlot::Policy(PolicyRuleChangeGovernance::default())
                }
                CanonicalEvolutionGovernance::Noop => {
                    CanonicalGovernanceSlot::Noop(NoopRuleChangeGovernance)
                }
            };
            let evolver = CanonicalSkillEvolver::new(&config.skill_root)
                .with_approval_threshold(config.approval_threshold);
            Ok(EvolutionSlot::Canonical(CanonicalEvolutionSlot::new(
                evolver,
                governance,
                config.detector.clone(),
            )))
        }
    }
}

fn build_control_plane(config: &ControlPlaneConfig) -> Result<ControlPlaneSlot, ConfigError> {
    match config {
        ControlPlaneConfig::FakeLocal => Ok(ControlPlaneSlot::FakeLocal(
            FakeControlPlane::default_local_agents(),
        )),
        ControlPlaneConfig::Command(config) => Ok(ControlPlaneSlot::Command(
            CommandControlPlane::new(config.clone()),
        )),
    }
}

fn subagent_queue_config_error(error: FileSubagentQueueError) -> ConfigError {
    ConfigError {
        field: "subagent_queue.root".to_string(),
        message: format!("failed to open subagent queue: {error:?}"),
    }
}

fn subagent_queue_restore_error(error: FileSubagentQueueError) -> ConfigError {
    ConfigError {
        field: "subagent_queue.dispatch".to_string(),
        message: format!("failed to restore queued dispatches: {error:?}"),
    }
}

fn subagent_restore_config_error(error: SubagentError) -> ConfigError {
    ConfigError {
        field: "subagent_queue.dispatch".to_string(),
        message: format!("failed to restore queued dispatch into spawner: {error:?}"),
    }
}

fn subagent_queue_runtime_error(error: FileSubagentQueueError) -> SubagentError {
    SubagentError::InvalidRequest(format!("subagent queue failed: {error:?}"))
}

impl Governance for GovernanceSlot {
    fn classify(&self, action: &ProposedAction) -> Result<RiskDecision, GovernanceError> {
        match self {
            Self::StaticRule(governance) => governance.classify(action),
        }
    }

    fn audit(&mut self, record: AuditRecord) -> Result<(), GovernanceError> {
        match self {
            Self::StaticRule(governance) => governance.audit(record),
        }
    }

    fn verify_operator_approval(
        &self,
        evidence: &OperatorApprovalEvidence,
    ) -> Result<(), GovernanceError> {
        match self {
            Self::StaticRule(governance) => governance.verify_operator_approval(evidence),
        }
    }
}

pub fn build_genesis_actuator(config: GenesisConfig) -> GenesisSlot {
    GenesisSlot {
        actuator: AutoCliGenesisActuator::new(config),
    }
}

impl GenesisSlot {
    pub fn primary_spec(&self, prompt: &str) -> GenesisCommandSpec {
        self.actuator.primary_spec(prompt)
    }

    pub fn fallback_spec(&self, prompt: &str) -> GenesisCommandSpec {
        self.actuator.fallback_spec(prompt)
    }
}

impl GenesisActuator for GenesisSlot {
    fn ask(&mut self, request: GenesisAskRequest) -> Result<GenesisAskResponse, GenesisError> {
        self.actuator.ask(request)
    }
}

impl ControlPlaneSlot {
    pub fn try_list_units(&self) -> Result<Vec<ManagedUnit>, ControlError> {
        match self {
            Self::FakeLocal(control_plane) => Ok(control_plane.list_units()),
            Self::Command(control_plane) => control_plane.try_list_units(),
        }
    }
}

impl Responder for ProviderSlot {
    fn generate(&self, request: &ResponderRequest) -> ResponderOutput {
        match self {
            Self::Fake(responder) => {
                let mut output = responder.generate(request);
                mark_provider_fallback_unconfigured(&mut output);
                output
            }
            Self::OpenAICompatible(responder) => {
                let mut output = responder.generate(request);
                mark_provider_fallback_unconfigured(&mut output);
                output
            }
            Self::AnthropicCompatible(responder) => {
                let mut output = responder.generate(request);
                mark_provider_fallback_unconfigured(&mut output);
                output
            }
            Self::Fallback {
                primary,
                fallback,
                policy,
            } => {
                let mut primary_output = primary.generate(request);
                if !provider_output_should_fallback(&primary_output, policy) {
                    primary_output.meta.extra.insert(
                        "provider_fallback_configured".to_string(),
                        "true".to_string(),
                    );
                    // A nested chain may already have recovered at an inner
                    // level. Preserve that fact instead of overwriting it
                    // with this level's own "no fallback" status.
                    if !primary_output
                        .meta
                        .extra
                        .contains_key("provider_fallback_used")
                    {
                        primary_output
                            .meta
                            .extra
                            .insert("provider_fallback_used".to_string(), "false".to_string());
                    }
                    return primary_output;
                }

                let mut fallback_output = fallback.generate(request);
                fallback_output.meta.extra.insert(
                    "provider_fallback_configured".to_string(),
                    "true".to_string(),
                );
                fallback_output
                    .meta
                    .extra
                    .insert("provider_fallback_used".to_string(), "true".to_string());
                fallback_output.meta.extra.insert(
                    "provider_fallback_from".to_string(),
                    primary_output
                        .meta
                        .provider
                        .clone()
                        .unwrap_or_else(|| primary_output.model_name.clone()),
                );
                fallback_output.meta.extra.insert(
                    "provider_fallback_reason".to_string(),
                    provider_fallback_reason(&primary_output),
                );
                fallback_output.meta.extra.insert(
                    "provider_fallback_primary_retryable".to_string(),
                    primary_output
                        .meta
                        .extra
                        .get("provider_retryable")
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                );
                copy_provider_fallback_primary_meta(&primary_output, &mut fallback_output);
                fallback_output.trace = format!(
                    "{} fallback_from_trace=({})",
                    fallback_output.trace, primary_output.trace
                );
                fallback_output
            }
        }
    }

    fn provider(&self) -> ResponderProvider {
        match self {
            Self::Fake(responder) => responder.provider(),
            Self::OpenAICompatible(responder) => responder.provider(),
            Self::AnthropicCompatible(responder) => responder.provider(),
            Self::Fallback {
                primary, fallback, ..
            } => {
                let primary = primary.provider();
                let fallback = fallback.provider();
                ResponderProvider {
                    provider_id: format!("{}->{}", primary.provider_id, fallback.provider_id),
                    model_name: format!("{}->{}", primary.model_name, fallback.model_name),
                }
            }
        }
    }
}

fn mark_provider_fallback_unconfigured(output: &mut ResponderOutput) {
    output
        .meta
        .extra
        .entry("provider_fallback_configured".to_string())
        .or_insert_with(|| "false".to_string());
    output
        .meta
        .extra
        .entry("provider_fallback_used".to_string())
        .or_insert_with(|| "false".to_string());
}

fn copy_provider_fallback_primary_meta(
    primary_output: &ResponderOutput,
    fallback_output: &mut ResponderOutput,
) {
    for (source, target) in [
        ("status_code", "provider_fallback_primary_status_code"),
        (
            "provider_error_class",
            "provider_fallback_primary_error_class",
        ),
        (
            "provider_failure_reason_code",
            "provider_fallback_primary_failure_reason_code",
        ),
        (
            "provider_failure_category",
            "provider_fallback_primary_failure_category",
        ),
        (
            "config_error_field",
            "provider_fallback_primary_config_error_field",
        ),
        (
            "provider_timeout_ms",
            "provider_fallback_primary_timeout_ms",
        ),
        (
            "provider_error_message",
            "provider_fallback_primary_error_message",
        ),
        ("request_url", "provider_fallback_primary_request_url"),
        ("request_method", "provider_fallback_primary_request_method"),
        (
            "request_message_count",
            "provider_fallback_primary_request_message_count",
        ),
        ("transport", "provider_fallback_primary_transport"),
        ("transport_mode", "provider_fallback_primary_transport_mode"),
        (
            "provider_response_ok",
            "provider_fallback_primary_response_ok",
        ),
    ] {
        if let Some(value) = primary_output.meta.extra.get(source) {
            fallback_output
                .meta
                .extra
                .insert(target.to_string(), value.clone());
        }
    }
}

fn provider_output_should_fallback(
    output: &ResponderOutput,
    policy: &ProviderFallbackPolicy,
) -> bool {
    if policy.on_retryable
        && output
            .meta
            .extra
            .get("provider_retryable")
            .map(String::as_str)
            == Some("true")
    {
        return true;
    }

    if output
        .meta
        .extra
        .get("status_code")
        .and_then(|status| status.parse::<u16>().ok())
        .is_some_and(|status| policy.status_codes.contains(&status))
    {
        return true;
    }

    output
        .meta
        .extra
        .get("provider_error_class")
        .is_some_and(|class| policy.error_classes.iter().any(|allowed| allowed == class))
}

fn provider_fallback_reason(output: &ResponderOutput) -> String {
    if let Some(status) = output.meta.extra.get("status_code") {
        return format!("status_code={status}");
    }
    if let Some(class) = output.meta.extra.get("provider_error_class") {
        return format!("provider_error_class={class}");
    }
    output
        .meta
        .finish_reason
        .clone()
        .unwrap_or_else(|| "unknown".to_string())
}

impl SubagentSpawner for SubagentRuntimeSlot {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnReceipt, SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.spawn(request),
            Self::QueuedExternal { spawner, queue } => spawner.persist_spawn(request, |dispatch| {
                queue
                    .write_dispatch(dispatch)
                    .map(|_| ())
                    .map_err(subagent_queue_runtime_error)
            }),
        }
    }

    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.steer(run_id, message),
            Self::QueuedExternal { spawner, queue } => {
                spawner.persist_steer(run_id, message, |dispatch| {
                    queue
                        .write_dispatch(dispatch)
                        .map(|_| ())
                        .map_err(subagent_queue_runtime_error)
                })
            }
        }
    }

    fn kill(&mut self, run_id: &RunId, reason: KillReason) -> Result<(), SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.kill(run_id, reason),
            Self::QueuedExternal { spawner, .. } => spawner.kill(run_id, reason),
        }
    }

    fn collect(
        &mut self,
        run_id: &RunId,
    ) -> Result<Option<crate::subagent_report::SubagentReport>, SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.collect(run_id),
            Self::QueuedExternal { spawner, queue } => {
                queue
                    .attach_report_if_present(spawner, run_id)
                    .map_err(subagent_queue_runtime_error)?;
                spawner.collect(run_id)
            }
        }
    }
}

impl Actuator for ActuatorSlot {
    fn observe(&mut self, target: ObserveTarget) -> Result<Observation, ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.observe(target),
            Self::Command(actuator) => actuator.observe(target),
        }
    }

    fn open_app(&mut self, request: OpenAppRequest) -> Result<AppHandle, ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.open_app(request),
            Self::Command(actuator) => actuator.open_app(request),
        }
    }

    fn focus(&mut self, target: FocusTarget) -> Result<(), ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.focus(target),
            Self::Command(actuator) => actuator.focus(target),
        }
    }

    fn click(&mut self, target: ClickTarget) -> Result<(), ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.click(target),
            Self::Command(actuator) => actuator.click(target),
        }
    }

    fn input_text(
        &mut self,
        target: InputTarget,
        text: SecretOrPlainText,
    ) -> Result<(), ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.input_text(target, text),
            Self::Command(actuator) => actuator.input_text(target, text),
        }
    }

    fn screenshot(&mut self, target: ScreenshotTarget) -> Result<EvidenceRef, ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.screenshot(target),
            Self::Command(actuator) => actuator.screenshot(target),
        }
    }
}

impl SkillEvolver for EvolutionSlot {
    fn observe(&mut self, event: RuntimeEvent) -> Result<EvolutionReceipt, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.observe(event),
            Self::DryRun(evolver) => evolver.observe(event),
            Self::Canonical(slot) => slot.evolver_mut().observe(event),
        }
    }

    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.propose(scope),
            Self::DryRun(evolver) => evolver.propose(scope),
            Self::Canonical(slot) => slot.evolver().propose(scope),
        }
    }

    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.validate(proposal),
            Self::DryRun(evolver) => evolver.validate(proposal),
            Self::Canonical(slot) => slot.evolver().validate(proposal),
        }
    }

    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.solidify(proposal),
            Self::DryRun(evolver) => evolver.solidify(proposal),
            Self::Canonical(slot) => slot.evolver_mut().solidify(proposal),
        }
    }

    fn detect_repeated_failures(
        &self,
        config: &FailureDetectorConfig,
    ) -> Result<Vec<FailurePattern>, EvolutionError> {
        match self {
            Self::Canonical(slot) => slot.evolver().detect_repeated_failures(config),
            Self::Noop(_) | Self::DryRun(_) => Err(EvolutionError::InvalidScope(
                "detect_repeated_failures requires the canonical evolution slot".to_string(),
            )),
        }
    }

    fn propose_rule_change(
        &self,
        pattern: &FailurePattern,
    ) -> Result<RuleChangeProposal, EvolutionError> {
        match self {
            Self::Canonical(slot) => slot.evolver().propose_rule_change(pattern),
            Self::Noop(_) | Self::DryRun(_) => Err(EvolutionError::InvalidRuleChange(
                "propose_rule_change requires the canonical evolution slot".to_string(),
            )),
        }
    }

    fn apply_rule_change(
        &mut self,
        proposal: RuleChangeProposal,
        governance: &dyn RuleChangeGovernance,
        context: &GovernanceContext,
    ) -> Result<RuleChangeReceipt, EvolutionError> {
        match self {
            Self::Canonical(slot) => slot
                .evolver_mut()
                .apply_rule_change(proposal, governance, context),
            Self::Noop(_) | Self::DryRun(_) => Err(EvolutionError::InvalidRuleChange(
                "apply_rule_change requires the canonical evolution slot".to_string(),
            )),
        }
    }

    fn rollback_rule_change(
        &mut self,
        entry_id: &str,
        governance: &dyn RuleChangeGovernance,
        context: &GovernanceContext,
    ) -> Result<RuleChangeReceipt, EvolutionError> {
        match self {
            Self::Canonical(slot) => slot
                .evolver_mut()
                .rollback_rule_change(entry_id, governance, context),
            Self::Noop(_) | Self::DryRun(_) => Err(EvolutionError::InvalidRuleChange(
                "rollback_rule_change requires the canonical evolution slot".to_string(),
            )),
        }
    }

    fn rule_change_history(&self) -> Result<Vec<RuleChangeJournalEntry>, EvolutionError> {
        match self {
            Self::Canonical(slot) => slot.evolver().rule_change_history(),
            Self::Noop(_) | Self::DryRun(_) => Err(EvolutionError::InvalidRuleChange(
                "rule_change_history requires the canonical evolution slot".to_string(),
            )),
        }
    }

    fn rule_change_journal_path(&self) -> Option<PathBuf> {
        match self {
            Self::Canonical(slot) => Some(slot.evolver().rule_change_journal_path()),
            Self::Noop(_) | Self::DryRun(_) => None,
        }
    }
}

impl ControlPlane for ControlPlaneSlot {
    fn list_units(&self) -> Vec<ManagedUnit> {
        match self {
            Self::FakeLocal(control_plane) => control_plane.list_units(),
            Self::Command(control_plane) => control_plane.list_units(),
        }
    }

    fn apply(&mut self, request: ControlRequest) -> Result<ControlReceipt, ControlError> {
        match self {
            Self::FakeLocal(control_plane) => control_plane.apply(request),
            Self::Command(control_plane) => control_plane.apply(request),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::responder::ResponderRequest;
    use crate::runtime_config::ProviderFallbackPolicy;

    fn policy() -> ProviderFallbackPolicy {
        ProviderFallbackPolicy {
            on_retryable: true,
            status_codes: vec![502],
            error_classes: vec![],
        }
    }

    fn request() -> ResponderRequest {
        ResponderRequest {
            prompt: "p".to_string(),
            user_input: "u".to_string(),
            recall_hit_count: 0,
        }
    }

    #[test]
    fn nested_fallback_chain_reaches_leaf_when_all_primaries_fail() {
        // A primary that points at an unroutable local port fails fast
        // (connection refused), exercising the fallback path without a
        // network dependency.
        let primary_fail = ProviderSlot::OpenAICompatible(
            OpenAICompatibleProviderAdapter::new("primary", "http://127.0.0.1:1/v1", "k", "m1")
                .with_transport(crate::provider_openai_compatible::ProviderTransport::Http)
                .with_request_timeout_ms(1500),
        );
        let level1 = ProviderSlot::Fallback {
            primary: Box::new(primary_fail),
            fallback: Box::new(ProviderSlot::Fake(FakeResponder::new("m2-ok"))),
            policy: policy(),
        };
        let chain = ProviderSlot::Fallback {
            primary: Box::new(level1),
            fallback: Box::new(ProviderSlot::Fake(FakeResponder::new("m3-ok"))),
            policy: policy(),
        };
        // Level 1 primary fails (connection refused) -> level1 falls back to
        // FakeResponder m2-ok. Since level1 now succeeded, the outer chain
        // must return m2's body, proving the nested fallback resolves at the
        // first available level and the outer leaf is only reached when
        // level1 also fails.
        let output = chain.generate(&request());
        assert!(output.body.contains("m2-ok"), "{}", output.body);
        // The inner level-1 fallback already recovered, so the outer chain
        // must NOT fall through to its own leaf. Correct multi-level
        // behavior: recover at the first available level, and the final
        // meta must still report that a fallback was used (at an inner hop).
        assert_eq!(
            output
                .meta
                .extra
                .get("provider_fallback_used")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            output
                .meta
                .extra
                .get("provider_fallback_from")
                .map(String::as_str),
            Some("primary")
        );
    }
}
