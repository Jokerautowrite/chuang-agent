//! `live_subagent_rehearsal` 模块。公开接口：struct LiveSubagentRehearsalInput, LiveSubagentRehearsalReport, LiveSubagentGateCheck, LiveSubagentRunnerAllowlistCheck, LiveSubagentCapabilityRoutingCheck, LiveSubagentReportAdmissionCheck, LiveSubagentForbiddenCapabilityCheck, LiveSubagentApprovalAuditPrerequisitesCheck；fn rehearse_live_subagent_adapter, rehearse_live_subagent_adapter_with_lookup。

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
    pub live_worker_available: bool,
    pub worker_runtime_state: String,
    pub worker_runtime_reason: String,
    pub adapter_entrypoint: String,
    pub gate_enabled: bool,
    pub runner_allowlist_ok: bool,
    pub capability_routing_ok: bool,
    pub report_admission_ok: bool,
    pub forbidden_capabilities_ok: bool,
    pub approval_audit_prerequisites_ok: bool,
    pub gate: LiveSubagentGateCheck,
    pub runner_allowlist: LiveSubagentRunnerAllowlistCheck,
    pub capability_routing: LiveSubagentCapabilityRoutingCheck,
    pub report_admission: LiveSubagentReportAdmissionCheck,
    pub forbidden_capabilities: LiveSubagentForbiddenCapabilityCheck,
    pub approval_audit_prerequisites: LiveSubagentApprovalAuditPrerequisitesCheck,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentGateCheck {
    pub ok: bool,
    pub enabled: bool,
    pub env_value_state: String,
    pub required_env: String,
    pub audit_label: String,
    pub default_enabled: bool,
    pub preflight_checks: Vec<String>,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentRunnerAllowlistCheck {
    pub ok: bool,
    pub runner: String,
    pub runner_command: String,
    pub allowed_runner_commands: Vec<String>,
    pub exact_match_required: bool,
    pub matched_runner_command: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentCapabilityRoutingCheck {
    pub ok: bool,
    pub required_capabilities: Vec<String>,
    pub worker_capabilities: Vec<String>,
    pub matched_capabilities: Vec<String>,
    pub missing_capabilities: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentReportAdmissionCheck {
    pub ok: bool,
    pub required: bool,
    pub covered_commands: Vec<String>,
    pub stable_reason_codes: Vec<String>,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentForbiddenCapabilityCheck {
    pub ok: bool,
    pub must_reject_capabilities: Vec<String>,
    pub requested_forbidden_capabilities: Vec<String>,
    pub checked_capability_sources: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSubagentApprovalAuditPrerequisitesCheck {
    pub ok: bool,
    pub explicit_operator_approval_required: bool,
    pub audit_receipt_required: bool,
    pub dispatch_evidence_required: bool,
    pub governance_approval_required: bool,
    pub prerequisites: Vec<String>,
    pub audit_label: String,
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
        covered_commands: vec![
            "run-once".to_string(),
            "run-loop".to_string(),
            "report".to_string(),
            "collect".to_string(),
        ],
        stable_reason_codes: vec![
            "report_validated".to_string(),
            "missing_required_field".to_string(),
            "invalid_json".to_string(),
            "invalid_timestamp_format".to_string(),
            "invalid_timestamp_order".to_string(),
            "command_protocol_report_rejected".to_string(),
        ],
        evidence:
            "run-once, run-loop, report, and collect expose ReportAdmission status and reason_code"
                .to_string(),
    };
    let forbidden_capabilities =
        build_forbidden_capability_check(&input, &gate.must_reject_capabilities);
    let approval_audit_prerequisites = build_approval_audit_prerequisites_check(gate.audit_label);

    let gate_check = LiveSubagentGateCheck {
        ok: gate.enabled,
        enabled: gate.enabled,
        env_value_state: gate.env_value_state,
        required_env: gate.required_env.to_string(),
        audit_label: gate.audit_label.to_string(),
        default_enabled: gate.default_enabled,
        preflight_checks: gate
            .preflight_checks
            .iter()
            .map(|check| check.to_string())
            .collect(),
        reason: gate.reason,
        next_action: gate.next_action,
    };

    let gate_enabled = gate_check.enabled;
    let runner_allowlist_ok = runner_allowlist.ok;
    let capability_routing_ok = capability_routing.ok;
    let report_admission_ok = report_admission.ok;
    let forbidden_capabilities_ok = forbidden_capabilities.ok;
    let approval_audit_prerequisites_ok = approval_audit_prerequisites.ok;
    let configured_but_dry_run = runner_allowlist_ok
        && capability_routing_ok
        && report_admission_ok
        && forbidden_capabilities_ok
        && approval_audit_prerequisites_ok;
    let ready_for_live = gate_enabled && configured_but_dry_run;
    let ok = configured_but_dry_run;
    let live_worker_available = false;
    let worker_runtime_state = if ready_for_live {
        "preflight_ready_no_worker_started"
    } else if configured_but_dry_run && !gate_enabled {
        "configured_but_gate_disabled"
    } else {
        "preflight_blocked"
    };
    let worker_runtime_reason = if ready_for_live {
        "read-only preflight checks pass, but this command does not start or mark a live worker available".to_string()
    } else if configured_but_dry_run && !gate_enabled {
        format!(
            "runner command and capability route are configured, but {} is not enabled; live_worker_available remains false",
            gate_check.required_env
        )
    } else {
        "one or more preflight checks failed; live_worker_available remains false".to_string()
    };
    let adapter_entrypoint = build_adapter_entrypoint(&input);
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
        live_worker_available,
        worker_runtime_state: worker_runtime_state.to_string(),
        worker_runtime_reason,
        adapter_entrypoint,
        gate_enabled,
        runner_allowlist_ok,
        capability_routing_ok,
        report_admission_ok,
        forbidden_capabilities_ok,
        approval_audit_prerequisites_ok,
        gate: gate_check,
        runner_allowlist,
        capability_routing,
        report_admission,
        forbidden_capabilities,
        approval_audit_prerequisites,
        next_action,
    }
}

fn build_adapter_entrypoint(input: &LiveSubagentRehearsalInput) -> String {
    let capabilities = if input.worker_capabilities.is_empty() {
        "--capability <declared-capability>".to_string()
    } else {
        input
            .worker_capabilities
            .iter()
            .map(|capability| format!("--capability {capability}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "subagent run-loop --runner {} --runner-command {} --approve-exec {}",
        input.runner, input.runner_command, capabilities
    )
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
        exact_match_required: true,
        matched_runner_command: ok.then(|| input.runner_command.clone()),
        reason,
    }
}

fn build_capability_routing_check(
    input: &LiveSubagentRehearsalInput,
) -> LiveSubagentCapabilityRoutingCheck {
    if input.required_capabilities.is_empty() {
        return LiveSubagentCapabilityRoutingCheck {
            ok: false,
            required_capabilities: input.required_capabilities.clone(),
            worker_capabilities: input.worker_capabilities.clone(),
            matched_capabilities: Vec::new(),
            missing_capabilities: Vec::new(),
            reason:
                "dispatch required_capabilities must be declared before live subagent rehearsal"
                    .to_string(),
        };
    }

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
    let matched_capabilities = input
        .required_capabilities
        .iter()
        .filter(|required| {
            input
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
        matched_capabilities,
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
        checked_capability_sources: vec![
            "dispatch required_capabilities".to_string(),
            "worker declared capabilities".to_string(),
        ],
        reason,
    }
}

fn build_approval_audit_prerequisites_check(
    audit_label: &str,
) -> LiveSubagentApprovalAuditPrerequisitesCheck {
    LiveSubagentApprovalAuditPrerequisitesCheck {
        ok: true,
        explicit_operator_approval_required: true,
        audit_receipt_required: true,
        dispatch_evidence_required: true,
        governance_approval_required: true,
        prerequisites: vec![
            "operator approves the exact runner command and target dispatch before live execution"
                .to_string(),
            "governance records why an external runner process may start".to_string(),
            "audit receipt includes dispatch id, worker id, runner command, capability route, and ReportAdmission result".to_string(),
            "runner remains bounded to the allowlisted command and declared capabilities".to_string(),
        ],
        audit_label: audit_label.to_string(),
        reason: "live runner execution still requires operator approval, governance evidence, and an audit receipt after this read-only preflight".to_string(),
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
