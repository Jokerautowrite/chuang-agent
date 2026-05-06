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
    pub env_value_state: String,
    pub preflight_checks: Vec<&'static str>,
    pub must_reject_capabilities: Vec<&'static str>,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAdapterGateError {
    pub slot: LiveAdapterSlot,
    pub required_env: &'static str,
    pub audit_label: &'static str,
    pub must_reject_capabilities: Vec<&'static str>,
    pub reason: String,
    pub next_action: String,
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

    pub fn preflight_checks(&self) -> Vec<&'static str> {
        match self {
            Self::SubagentRunner => vec![
                "confirm CHUANG_CODEX_RUNNER_ENABLE=1 was set intentionally for this run",
                "verify runner command allowlist and capability routing before execution",
                "record dispatch id, worker id, and report admission evidence",
            ],
            Self::ControlApply => vec![
                "confirm CHUANG_REAL_CONTROL_ENABLE=1 was set intentionally for this run",
                "verify Chuang-only unit allowlist before any apply action",
                "require governance approval and receipt validation for unit/action/model",
            ],
            Self::ActuatorOperation => vec![
                "confirm CHUANG_REAL_ACTUATOR_ENABLE=1 was set intentionally for this run",
                "verify actuator action allowlist and target surface before execution",
                "require governance approval and audit receipt for every non-observe action",
            ],
        }
    }

    pub fn must_reject_capabilities(&self) -> Vec<&'static str> {
        match self {
            Self::SubagentRunner => vec![
                "unscoped external worker pool",
                "subagent direct core-memory write",
                "unapproved external platform login or session mutation",
            ],
            Self::ControlApply => vec![
                "arbitrary systemd unit or process control",
                "Codex or Hermes service control unless explicitly requested",
                "delete logs, queues, reports, memories, credentials, or claims",
            ],
            Self::ActuatorOperation => vec![
                "real desktop/browser operation without action allowlist",
                "profile, credential, or login-state mutation",
                "verification-code entry without operator-provided code and approval",
            ],
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
    let env_value = lookup(required_env);
    let enabled = env_value.as_deref() == Some("1");
    let env_value_state = match env_value.as_deref() {
        Some("1") => "enabled",
        Some(_) => "set_non_enabling",
        None => "unset",
    }
    .to_string();
    let preflight_checks = slot.preflight_checks();
    let must_reject_capabilities = slot.must_reject_capabilities();
    LiveAdapterGate {
        slot,
        name: slot.name(),
        required_env,
        audit_label: slot.audit_label(),
        enabled,
        default_enabled: false,
        env_value_state,
        preflight_checks,
        must_reject_capabilities,
        reason: if enabled {
            format!(
                "{required_env}=1 opens the live adapter preflight gate for {}; forbidden capabilities still remain rejected",
                slot.name()
            )
        } else {
            format!(
                "live adapter execution for {} is disabled by default; set {required_env}=1 only after operator approval",
                slot.name()
            )
        },
        next_action: if enabled {
            "run preflight review, verify allowlist/approval/audit receipt, and keep forbidden capabilities rejected".to_string()
        } else {
            "keep disabled until the operator approves exact live adapter targets and preflight evidence".to_string()
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
            must_reject_capabilities: gate.must_reject_capabilities,
            reason: gate.reason,
            next_action: gate.next_action,
        })
    }
}
