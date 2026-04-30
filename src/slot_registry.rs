use crate::actuator::{
    Actuator, ActuatorError, AppHandle, ClickTarget, EvidenceRef, FakeActuator, FocusTarget,
    InputTarget, Observation, ObserveTarget, OpenAppRequest, ScreenshotTarget, SecretOrPlainText,
};
use crate::common::AuditRecord;
use crate::control_plane::{
    ControlError, ControlPlane, ControlReceipt, ControlRequest, FakeControlPlane, ManagedUnit,
};
use crate::governance::{
    Governance, GovernanceError, ProposedAction, RiskDecision, StaticRuleGovernance,
};
use crate::provider_openai_compatible::OpenAICompatibleProviderAdapter;
use crate::responder::{
    FakeResponder, Responder, ResponderOutput, ResponderProvider, ResponderRequest,
};
use crate::runtime_config::{
    ActuatorConfig, ConfigError, ControlPlaneConfig, EvolutionConfig, GovernanceConfig,
    ProviderConfig, RuntimeConfig, SubagentConfig,
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
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RuntimeSlots {
    pub provider: ProviderSlot,
    pub governance: GovernanceSlot,
    pub actuator: ActuatorSlot,
    pub subagent: SubagentRuntimeSlot,
    pub evolution: EvolutionSlot,
    pub control_plane: ControlPlaneSlot,
}

#[derive(Debug, Clone)]
pub enum ProviderSlot {
    Fake(FakeResponder),
    OpenAICompatible(OpenAICompatibleProviderAdapter),
}

#[derive(Debug, Clone)]
pub enum GovernanceSlot {
    StaticRule(StaticRuleGovernance),
}

#[derive(Debug, Clone)]
pub enum ActuatorSlot {
    Fake(FakeActuator),
}

#[derive(Debug, Clone)]
pub enum EvolutionSlot {
    Noop(NoopEvolver),
}

#[derive(Debug, Clone)]
pub enum ControlPlaneSlot {
    FakeLocal(FakeControlPlane),
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
    pub actuator: String,
    pub subagent: String,
    pub evolution: String,
    pub control_plane: String,
}

pub fn build_runtime_slots(config: &RuntimeConfig) -> Result<RuntimeSlots, ConfigError> {
    config.validate()?;

    Ok(RuntimeSlots {
        provider: build_provider_responder(&config.provider)?,
        governance: build_governance(&config.governance)?,
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
            .with_transport(config.transport.clone()),
        )),
    }
}

pub fn summarize_runtime_slots(config: &RuntimeConfig) -> RuntimeSlotsSummary {
    RuntimeSlotsSummary {
        provider: config.provider.kind().to_string(),
        governance: config.governance.kind().to_string(),
        actuator: config.actuator.kind().to_string(),
        subagent: config.subagent.kind().to_string(),
        evolution: config.evolution.kind().to_string(),
        control_plane: config.control_plane.kind().to_string(),
    }
}

fn build_governance(config: &GovernanceConfig) -> Result<GovernanceSlot, ConfigError> {
    match config {
        GovernanceConfig::StaticRule => Ok(GovernanceSlot::StaticRule(StaticRuleGovernance::new())),
    }
}

fn build_actuator(config: &ActuatorConfig) -> Result<ActuatorSlot, ConfigError> {
    match config {
        ActuatorConfig::Fake => Ok(ActuatorSlot::Fake(FakeActuator::new())),
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

impl Responder for ProviderSlot {
    fn generate(&self, request: &ResponderRequest) -> ResponderOutput {
        match self {
            Self::Fake(responder) => responder.generate(request),
            Self::OpenAICompatible(responder) => responder.generate(request),
        }
    }

    fn provider(&self) -> ResponderProvider {
        match self {
            Self::Fake(responder) => responder.provider(),
            Self::OpenAICompatible(responder) => responder.provider(),
        }
    }
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
        }
    }

    fn open_app(&mut self, request: OpenAppRequest) -> Result<AppHandle, ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.open_app(request),
        }
    }

    fn focus(&mut self, target: FocusTarget) -> Result<(), ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.focus(target),
        }
    }

    fn click(&mut self, target: ClickTarget) -> Result<(), ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.click(target),
        }
    }

    fn input_text(
        &mut self,
        target: InputTarget,
        text: SecretOrPlainText,
    ) -> Result<(), ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.input_text(target, text),
        }
    }

    fn screenshot(&mut self, target: ScreenshotTarget) -> Result<EvidenceRef, ActuatorError> {
        match self {
            Self::Fake(actuator) => actuator.screenshot(target),
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
        }
    }

    fn apply(&mut self, request: ControlRequest) -> Result<ControlReceipt, ControlError> {
        match self {
            Self::FakeLocal(control_plane) => control_plane.apply(request),
        }
    }
}
