use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAdapterSlot {
    SubagentRunner,
    ControlApply,
    ActuatorOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAdapterGate {
    pub slot: LiveAdapterSlot,
    pub name: &'static str,
    pub required_env: &'static str,
    pub audit_label: &'static str,
    pub enabled: bool,
    pub default_enabled: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAdapterGateError {
    pub slot: LiveAdapterSlot,
    pub required_env: &'static str,
    pub audit_label: &'static str,
    pub reason: String,
}

impl LiveAdapterSlot {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SubagentRunner => "subagent_runner",
            Self::ControlApply => "control_apply",
            Self::ActuatorOperation => "actuator_operation",
        }
    }

    pub fn required_env(&self) -> &'static str {
        match self {
            Self::SubagentRunner => "CHUANG_CODEX_RUNNER_ENABLE",
            Self::ControlApply => "CHUANG_REAL_CONTROL_ENABLE",
            Self::ActuatorOperation => "CHUANG_REAL_ACTUATOR_ENABLE",
        }
    }

    pub fn audit_label(&self) -> &'static str {
        match self {
            Self::SubagentRunner => "subagent.runner.live",
            Self::ControlApply => "control.apply.live",
            Self::ActuatorOperation => "actuator.operation.live",
        }
    }
}

pub fn evaluate_live_adapter_gate(slot: LiveAdapterSlot) -> LiveAdapterGate {
    evaluate_live_adapter_gate_with_lookup(slot, |name| env::var(name).ok())
}

pub fn require_live_adapter_enabled(
    slot: LiveAdapterSlot,
) -> Result<LiveAdapterGate, LiveAdapterGateError> {
    require_live_adapter_enabled_with_lookup(slot, |name| env::var(name).ok())
}

pub fn evaluate_live_adapter_gate_with_lookup<F>(
    slot: LiveAdapterSlot,
    lookup: F,
) -> LiveAdapterGate
where
    F: Fn(&str) -> Option<String>,
{
    let required_env = slot.required_env();
    let enabled = lookup(required_env).as_deref() == Some("1");
    LiveAdapterGate {
        slot,
        name: slot.name(),
        required_env,
        audit_label: slot.audit_label(),
        enabled,
        default_enabled: false,
        reason: if enabled {
            format!(
                "{required_env}=1 enables live adapter execution for {}",
                slot.name()
            )
        } else {
            format!(
                "live adapter execution for {} is disabled by default; set {required_env}=1 only after operator approval",
                slot.name()
            )
        },
    }
}

pub fn require_live_adapter_enabled_with_lookup<F>(
    slot: LiveAdapterSlot,
    lookup: F,
) -> Result<LiveAdapterGate, LiveAdapterGateError>
where
    F: Fn(&str) -> Option<String>,
{
    let gate = evaluate_live_adapter_gate_with_lookup(slot, lookup);
    if gate.enabled {
        Ok(gate)
    } else {
        Err(LiveAdapterGateError {
            slot,
            required_env: gate.required_env,
            audit_label: gate.audit_label,
            reason: gate.reason,
        })
    }
}
