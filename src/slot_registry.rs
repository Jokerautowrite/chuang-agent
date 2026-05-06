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
use crate::genesis_actuator::{AutoCliGenesisActuator, GenesisConfig, SystemGenesisCommandRunner};
use crate::genesis_actuator::{
    GenesisActuator, GenesisAskRequest, GenesisAskResponse, GenesisCommandSpec, GenesisError,
};
use crate::governance::{
    Governance, GovernanceError, MarkdownRuleSet, ProposedAction, RiskDecision,
    StaticRuleGovernance,
};
use crate::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use crate::responder::{
    FakeResponder, Responder, ResponderOutput, ResponderProvider, ResponderRequest,
};
use crate::runtime_config::{
    ActuatorConfig, ConfigError, ControlPlaneConfig, EvolutionConfig, GovernanceConfig,
    ProviderConfig, ProviderFallbackPolicy, RuntimeConfig, SubagentConfig,
};
use crate::skill_evolver::{
    EvolutionError, EvolutionReceipt, EvolutionScope, NoopEvolver, RuntimeEvent, SkillEvolver,
    SkillId, SkillProposal, ValidationReport,
};
use crate::subagent_queue::{FileSubagentQueue, FileSubagentQueueError};
use crate::subagent_spawner::{
    FakeSubagentSpawner, KillReason, QueuedSubagentSpawner, RunId, SpawnReceipt, SpawnRequest,
    SubagentError, SubagentSpawner,
};
use crate::tool_runtime::{ExecutionSlot, ToolExecutionConfig};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RuntimeSlots {
    pub provider: ProviderSlot,
    pub governance: GovernanceSlot,
    pub execution: ExecutionSlot,
    pub actuator: ActuatorSlot,
    pub subagent: SubagentRuntimeSlot,
    pub evolution: EvolutionSlot,
    pub control_plane: ControlPlaneSlot,
}

#[derive(Debug, Clone)]
pub enum ProviderSlot {
    Fake(FakeResponder),
    OpenAICompatible(OpenAICompatibleProviderAdapter),
    Fallback {
        primary: Box<ProviderSlot>,
        fallback: Box<ProviderSlot>,
        policy: ProviderFallbackPolicy,
    },
}

#[derive(Debug, Clone)]
pub enum GovernanceSlot {
    StaticRule(StaticRuleGovernance),
}

#[derive(Debug, Clone)]
pub enum ActuatorSlot {
    Fake(FakeActuator),
    Command(CommandActuator),
}

#[derive(Debug, Clone)]
pub enum EvolutionSlot {
    Noop(NoopEvolver),
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
            Ok(GovernanceSlot::StaticRule(
                StaticRuleGovernance::with_rules(rules),
            ))
        }
    }
}

fn build_execution(config: &RuntimeConfig) -> ExecutionSlot {
    ExecutionSlot::generic_agent_mvp(ToolExecutionConfig {
        shell_timeout_ms: config.tool_loop.shell_timeout_ms,
        shell_risk_rules: config.tool_loop.shell_risk_rules.clone(),
        memory: None,
        actuator: None,
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
        } => Ok(SubagentRuntimeSlot::QueuedExternal {
            spawner: QueuedSubagentSpawner::new(),
            queue: FileSubagentQueue::open(config.subagent_queue.build_file_queue_config()?)
                .map_err(subagent_queue_config_error)?,
        }),
    }
}

fn build_evolution(config: &EvolutionConfig) -> Result<EvolutionSlot, ConfigError> {
    match config {
        EvolutionConfig::Noop => Ok(EvolutionSlot::Noop(NoopEvolver::new())),
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
                    primary_output
                        .meta
                        .extra
                        .insert("provider_fallback_used".to_string(), "false".to_string());
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
                if let Some(status_code) = primary_output.meta.extra.get("status_code") {
                    fallback_output.meta.extra.insert(
                        "provider_fallback_primary_status_code".to_string(),
                        status_code.clone(),
                    );
                }
                if let Some(error_class) = primary_output.meta.extra.get("provider_error_class") {
                    fallback_output.meta.extra.insert(
                        "provider_fallback_primary_error_class".to_string(),
                        error_class.clone(),
                    );
                }
                if let Some(reason_code) = primary_output
                    .meta
                    .extra
                    .get("provider_failure_reason_code")
                {
                    fallback_output.meta.extra.insert(
                        "provider_fallback_primary_failure_reason_code".to_string(),
                        reason_code.clone(),
                    );
                }
                if let Some(category) = primary_output.meta.extra.get("provider_failure_category") {
                    fallback_output.meta.extra.insert(
                        "provider_fallback_primary_failure_category".to_string(),
                        category.clone(),
                    );
                }
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
            Self::QueuedExternal { spawner, queue } => {
                let receipt = spawner.spawn(request)?;
                let dispatch = spawner
                    .pending_dispatches()
                    .into_iter()
                    .find(|dispatch| dispatch.run_id == receipt.run_id)
                    .ok_or_else(|| {
                        SubagentError::InvalidRequest(format!(
                            "queued dispatch missing for run_id={}",
                            receipt.run_id.0
                        ))
                    })?;
                queue
                    .write_dispatch(&dispatch)
                    .map_err(subagent_queue_runtime_error)?;
                Ok(receipt)
            }
        }
    }

    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.steer(run_id, message),
            Self::QueuedExternal { spawner, .. } => spawner.steer(run_id, message),
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
        }
    }

    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.propose(scope),
        }
    }

    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.validate(proposal),
        }
    }

    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError> {
        match self {
            Self::Noop(evolver) => evolver.solidify(proposal),
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
