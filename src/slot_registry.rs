use crate::actuator::FakeActuator;
use crate::control_plane::FakeControlPlane;
use crate::governance::StaticRuleGovernance;
use crate::runtime_config::{
    ActuatorConfig, ConfigError, ControlPlaneConfig, EvolutionConfig, GovernanceConfig,
    RuntimeConfig, SubagentConfig,
};
use crate::skill_evolver::NoopEvolver;
use crate::subagent_spawner::FakeSubagentSpawner;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RuntimeSlots {
    pub governance: StaticRuleGovernance,
    pub actuator: FakeActuator,
    pub subagent: FakeSubagentSpawner,
    pub evolution: NoopEvolver,
    pub control_plane: FakeControlPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeSlotsSummary {
    pub governance: String,
    pub actuator: String,
    pub subagent: String,
    pub evolution: String,
    pub control_plane: String,
}

pub fn build_runtime_slots(config: &RuntimeConfig) -> Result<RuntimeSlots, ConfigError> {
    config.validate()?;

    Ok(RuntimeSlots {
        governance: build_governance(&config.governance)?,
        actuator: build_actuator(&config.actuator)?,
        subagent: build_subagent(&config.subagent)?,
        evolution: build_evolution(&config.evolution)?,
        control_plane: build_control_plane(&config.control_plane)?,
    })
}

pub fn summarize_runtime_slots(config: &RuntimeConfig) -> RuntimeSlotsSummary {
    RuntimeSlotsSummary {
        governance: config.governance.kind().to_string(),
        actuator: config.actuator.kind().to_string(),
        subagent: config.subagent.kind().to_string(),
        evolution: config.evolution.kind().to_string(),
        control_plane: config.control_plane.kind().to_string(),
    }
}

fn build_governance(config: &GovernanceConfig) -> Result<StaticRuleGovernance, ConfigError> {
    match config {
        GovernanceConfig::StaticRule => Ok(StaticRuleGovernance::new()),
    }
}

fn build_actuator(config: &ActuatorConfig) -> Result<FakeActuator, ConfigError> {
    match config {
        ActuatorConfig::Fake => Ok(FakeActuator::new()),
    }
}

fn build_subagent(config: &SubagentConfig) -> Result<FakeSubagentSpawner, ConfigError> {
    match config {
        SubagentConfig::Fake => Ok(FakeSubagentSpawner::new()),
    }
}

fn build_evolution(config: &EvolutionConfig) -> Result<NoopEvolver, ConfigError> {
    match config {
        EvolutionConfig::Noop => Ok(NoopEvolver::new()),
    }
}

fn build_control_plane(config: &ControlPlaneConfig) -> Result<FakeControlPlane, ConfigError> {
    match config {
        ControlPlaneConfig::FakeLocal => Ok(FakeControlPlane::default_local_agents()),
    }
}
