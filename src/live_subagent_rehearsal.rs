use crate::live_adapter_gate::{evaluate_live_adapter_gate_with_lookup, LiveAdapterSlot};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSubagentRehearsalInput {
    pub runner: String,
    pub runner_command: String,
    pub allowed_runner_commands: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub worker_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentRehearsalReport {
    pub ok: bool,
    pub ready_for_live: bool,
    pub readonly: bool,
    pub starts_external_worker: bool,
    pub gate: LiveSubagentGateCheck,
    pub runner_allowlist: LiveSubagentRunnerAllowlistCheck,
    pub capability_routing: LiveSubagentCapabilityRoutingCheck,
    pub report_admission: LiveSubagentReportAdmissionCheck,
    pub forbidden_capabilities: LiveSubagentForbiddenCapabilityCheck,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentGateCheck {
    pub ok: bool,
    pub enabled: bool,
    pub env_value_state: String,
    pub required_env: String,
    pub audit_label: String,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentRunnerAllowlistCheck {
    pub ok: bool,
    pub runner: String,
    pub runner_command: String,
    pub allowed_runner_commands: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentCapabilityRoutingCheck {
    pub ok: bool,
    pub required_capabilities: Vec<String>,
    pub worker_capabilities: Vec<String>,
    pub missing_capabilities: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentReportAdmissionCheck {
    pub ok: bool,
    pub required: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentForbiddenCapabilityCheck {
    pub ok: bool,
    pub must_reject_capabilities: Vec<String>,
    pub requested_forbidden_capabilities: Vec<String>,
    pub reason: String,
}

pub fn rehearse_live_subagent_adapter(
    input: LiveSubagentRehearsalInput,
) -> LiveSubagentRehearsalReport {
    rehearse_live_subagent_adapter_with_lookup(input, |name| std::env::var(name).ok())
}

pub fn rehearse_live_subagent_adapter_with_lookup<F>(
    input: LiveSubagentRehearsalInput,
    lookup: F,
) -> LiveSubagentRehearsalReport
where
    F: Fn(&str) -> Option<String>,
{
    let gate = evaluate_live_adapter_gate_with_lookup(LiveAdapterSlot::SubagentRunner, lookup);
    let runner_allowlist = build_runner_allowlist_check(&input);
    let capability_routing = build_capability_routing_check(&input);
    let report_admission = LiveSubagentReportAdmissionCheck {
        ok: true,
        required: true,
        evidence:
            "run-once, run-loop, report, and collect expose ReportAdmission status and reason_code"
                .to_string(),
    };
    let forbidden_capabilities =
        build_forbidden_capability_check(&input, &gate.must_reject_capabilities);

    let gate_check = LiveSubagentGateCheck {
        ok: gate.enabled,
        enabled: gate.enabled,
        env_value_state: gate.env_value_state,
        required_env: gate.required_env.to_string(),
        audit_label: gate.audit_label.to_string(),
        reason: gate.reason,
        next_action: gate.next_action,
    };

    let ready_for_live = gate_check.ok
        && runner_allowlist.ok
        && capability_routing.ok
        && report_admission.ok
        && forbidden_capabilities.ok;
    let ok = runner_allowlist.ok
        && capability_routing.ok
        && report_admission.ok
        && forbidden_capabilities.ok;
    let next_action = if ready_for_live {
        "operator may run one approved live runner rehearsal with the exact allowlisted command and dispatch evidence".to_string()
    } else if !gate_check.enabled {
        "keep rehearsal read-only; set CHUANG_CODEX_RUNNER_ENABLE=1 only after operator approval of exact runner command, capabilities, and report admission evidence".to_string()
    } else {
        "fix failed preflight checks before any live runner process is started".to_string()
    };

    LiveSubagentRehearsalReport {
        ok,
        ready_for_live,
        readonly: true,
        starts_external_worker: false,
        gate: gate_check,
        runner_allowlist,
        capability_routing,
        report_admission,
        forbidden_capabilities,
        next_action,
    }
}

fn build_runner_allowlist_check(
    input: &LiveSubagentRehearsalInput,
) -> LiveSubagentRunnerAllowlistCheck {
    let ok = input.runner == "command"
        && !input.runner_command.trim().is_empty()
        && input
            .allowed_runner_commands
            .iter()
            .any(|allowed| allowed == &input.runner_command);
    let reason = if ok {
        "runner command exactly matches the explicit live runner allowlist".to_string()
    } else if input.runner != "command" {
        "live subagent rehearsal only accepts the command runner adapter boundary".to_string()
    } else if input.runner_command.trim().is_empty() {
        "runner command is required for live subagent rehearsal".to_string()
    } else {
        "runner command is not present in --allow-runner-command".to_string()
    };

    LiveSubagentRunnerAllowlistCheck {
        ok,
        runner: input.runner.clone(),
        runner_command: input.runner_command.clone(),
        allowed_runner_commands: input.allowed_runner_commands.clone(),
        reason,
    }
}

fn build_capability_routing_check(
    input: &LiveSubagentRehearsalInput,
) -> LiveSubagentCapabilityRoutingCheck {
    let missing_capabilities = input
        .required_capabilities
        .iter()
        .filter(|required| {
            !input
                .worker_capabilities
                .iter()
                .any(|capability| capability == *required)
        })
        .cloned()
        .collect::<Vec<_>>();
    let ok = missing_capabilities.is_empty();
    let reason = if ok {
        "worker capabilities satisfy dispatch required_capabilities".to_string()
    } else {
        "worker capabilities do not satisfy dispatch required_capabilities".to_string()
    };

    LiveSubagentCapabilityRoutingCheck {
        ok,
        required_capabilities: input.required_capabilities.clone(),
        worker_capabilities: input.worker_capabilities.clone(),
        missing_capabilities,
        reason,
    }
}

fn build_forbidden_capability_check(
    input: &LiveSubagentRehearsalInput,
    must_reject_capabilities: &[&'static str],
) -> LiveSubagentForbiddenCapabilityCheck {
    let mut requested_forbidden_capabilities = Vec::new();
    for capability in input
        .required_capabilities
        .iter()
        .chain(input.worker_capabilities.iter())
    {
        if is_forbidden_live_subagent_capability(capability)
            && !requested_forbidden_capabilities.contains(capability)
        {
            requested_forbidden_capabilities.push(capability.clone());
        }
    }
    let ok = requested_forbidden_capabilities.is_empty();
    let reason = if ok {
        "dangerous live subagent capabilities remain rejected".to_string()
    } else {
        "requested capabilities include live subagent capabilities that must remain rejected"
            .to_string()
    };

    LiveSubagentForbiddenCapabilityCheck {
        ok,
        must_reject_capabilities: must_reject_capabilities
            .iter()
            .map(|capability| capability.to_string())
            .collect(),
        requested_forbidden_capabilities,
        reason,
    }
}

fn is_forbidden_live_subagent_capability(capability: &str) -> bool {
    matches!(
        capability,
        "unscoped-external-worker-pool"
            | "unscoped_external_worker_pool"
            | "external-worker-pool"
            | "external_worker_pool"
            | "core-memory-write"
            | "core_memory_write"
            | "direct-core-memory-write"
            | "direct_core_memory_write"
            | "platform-login"
            | "platform_login"
            | "session-mutation"
            | "session_mutation"
            | "credential-mutation"
            | "credential_mutation"
    )
}
