//! `cli_genesis` 模块。内部实现模块（无公开顶层项）。

use chuang_agent::common::{AgentId, AuditRecord, TaskId, Timestamp};
use chuang_agent::genesis_actuator::{GenesisActuator, GenesisAskRequest, GenesisConfig};
use chuang_agent::governance::{
    risk_decision_label, ActionKind, Governance, ProposedAction, RiskDecision, StaticRuleGovernance,
};
use chuang_agent::slot_registry::build_genesis_actuator;

use crate::cli_args::parse_genesis_ask;
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_types::{GenesisAskCliOutput, GenesisDryRunCliOutput};

pub(crate) fn genesis_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("ask") => genesis_ask_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn genesis_ask_command(args: &[String]) -> Result<(), String> {
    let request = parse_genesis_ask(args)?;
    let mut config = GenesisConfig::new(request.profile_dir);
    config.program = request.program;
    config.cdp_port = request.cdp_port;
    config.timeout_ms = request.timeout_ms;

    if request.dry_run {
        let actuator = build_genesis_actuator(config);
        let output = GenesisDryRunCliOutput {
            primary: actuator.primary_spec(&request.prompt),
            fallback: actuator.fallback_spec(&request.prompt),
        };
        return match request.output {
            ControlOutputFormat::Text => {
                println!("genesis_dry_run: true");
                println!(
                    "primary: {} {}",
                    output.primary.program,
                    output.primary.args.join(" ")
                );
                println!(
                    "fallback: {} {}",
                    output.fallback.program,
                    output.fallback.args.join(" ")
                );
                Ok(())
            }
            ControlOutputFormat::Json => print_json(&output),
        };
    }

    if !request.approve_exec {
        return Err("genesis_ask_requires_approve_exec: pass --approve-exec".to_string());
    }
    let action = ProposedAction {
        action_id: "genesis:ask".to_string(),
        kind: ActionKind::ExternalSend,
        target: "genesis.web_ai".to_string(),
        summary: "query external web AI through Genesis Actuator".to_string(),
    };
    let mut governance = StaticRuleGovernance::new();
    let decision = governance
        .classify(&action)
        .map_err(|error| format!("genesis_governance_failed: {}", error.message))?;
    if matches!(decision, RiskDecision::NeedsApproval { .. }) && !request.approve_exec {
        return Err("genesis action requires --approve-exec".to_string());
    }
    if !matches!(
        decision,
        RiskDecision::Allowed { .. } | RiskDecision::NeedsApproval { .. }
    ) {
        return Err(format!(
            "genesis action was not allowed by governance: {}",
            risk_decision_label(&decision)
        ));
    }

    let prompt = request.prompt;
    let mut actuator = build_genesis_actuator(config);
    let response = actuator
        .ask(GenesisAskRequest {
            prompt: prompt.clone(),
        })
        .map_err(|error| format!("genesis_ask_failed: {error:?}"))?;
    governance
        .audit(AuditRecord {
            operation: "genesis.ask".to_string(),
            agent_id: AgentId("genesis-actuator".to_string()),
            task_id: TaskId("genesis:ask".to_string()),
            delta_bytes: response.answer.len() as i64,
            reason: format!(
                "approved={}; prompt_chars={}; channel={}",
                request.approve_exec,
                prompt.chars().count(),
                response.channel.as_str()
            ),
            timestamp: current_rfc3339_timestamp(),
        })
        .map_err(|error| format!("genesis_audit_failed: {}", error.message))?;
    let output = GenesisAskCliOutput {
        response,
        governance_decision: risk_decision_label(&decision),
        audit_recorded: true,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "genesis_governance_decision: {}",
                output.governance_decision
            );
            println!("genesis_audit_recorded: {}", output.audit_recorded);
            println!("genesis_channel: {}", output.response.channel.as_str());
            println!("answer: {}", output.response.answer);
            if let Some(repair) = output.response.primary_repair {
                println!("primary_repair_required: {}", repair.requires_approval);
                println!("primary_repair_reason: {}", repair.reason);
                println!(
                    "primary_repair_recommended_action: {}",
                    repair.recommended_action
                );
            }
            Ok(())
        }
        ControlOutputFormat::Json => print_json(&output),
    }
}

fn current_rfc3339_timestamp() -> Timestamp {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    Timestamp(now)
}
