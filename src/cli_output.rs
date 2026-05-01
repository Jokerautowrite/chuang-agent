use chuang_agent::agent_runtime::{ContextDebugInfo, RuntimeResult};
use chuang_agent::context_engine::{BudgetExceededReason, DropReason};
use chuang_agent::control_workflow::{ControlUnitView, ControlWorkflowView};
use chuang_agent::kernel_status::ChuangMvpStatus;
use chuang_agent::runtime_config::ConfigSummary;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlOutputFormat {
    Text,
    Json,
}

pub fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let rendered =
        serde_json::to_string_pretty(value).map_err(|e| format!("json_render_failed: {e}"))?;
    println!("{rendered}");
    Ok(())
}

pub fn print_control_view(view: &ControlWorkflowView) {
    println!(
        "control_decision unit_id={} name={} decision={}",
        view.unit_id, view.display_name, view.decision
    );
    if view.audit_recorded {
        println!("control_audit: recorded");
    }
}

pub fn print_control_view_with_format(
    view: &ControlWorkflowView,
    output: ControlOutputFormat,
) -> Result<(), String> {
    match output {
        ControlOutputFormat::Text => {
            print_control_view(view);
            Ok(())
        }
        ControlOutputFormat::Json => print_json(view),
    }
}

pub fn print_control_unit_view(view: &ControlUnitView) {
    println!(
        "unit_id={} name={} kind={} status={} model={} channel={}",
        view.unit_id,
        view.display_name,
        view.kind,
        view.status,
        view.model_name.as_deref().unwrap_or("none"),
        view.channel
    );
}

pub fn print_status(status: &ChuangMvpStatus) {
    println!("kernel_agent_id: {}", status.kernel.agent_id);
    println!("kernel_turn_count: {}", status.kernel.turn_count);
    println!("provider: {}", status.config.provider_kind);
    println!("provider_slot: {}", status.slots.provider);
    println!("provider_id: {}", status.config.provider_id);
    println!("model: {}", status.config.model_name);
    println!("memory_db: {}", status.config.db_path);
    println!("identity_memory: {}", status.config.identity_memory_kind);
    println!(
        "identity_memory_root: {}",
        status.config.identity_memory_root
    );
    println!(
        "identity_memory_limits: user={} memory={}",
        status.config.identity_user_max_chars, status.config.identity_memory_max_chars
    );
    println!("recall_limit: {}", status.config.recall_limit);
    println!("context_engine: {}", status.config.context_engine_kind);
    println!("context_max_tokens: {}", status.config.context_max_tokens);
    println!(
        "context_budget: max={} reserve_system={} min_working={} max_tool_results={} max_memory_segments={}",
        status.config.context_max_tokens,
        status.config.context_reserve_system_tokens,
        status.config.context_min_working_tokens,
        status.config.context_max_tool_results,
        status.config.context_max_memory_segments
    );
    println!(
        "identity_snapshot_chars: user={} memory={}",
        status
            .kernel
            .identity_user_chars
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_memory_chars
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("governance: {}", status.slots.governance);
    println!("actuator: {}", status.slots.actuator);
    println!("subagent: {}", status.slots.subagent);
    println!("subagent_queue_root: {}", status.config.subagent_queue_root);
    println!("evolution: {}", status.slots.evolution);
    println!("control_plane: {}", status.slots.control_plane);
}

pub fn print_config_summary(ok: bool, source: &str, summary: &ConfigSummary) {
    println!("config_ok: {ok}");
    println!("config_source: {source}");
    println!("provider: {}", summary.provider_kind);
    println!("provider_id: {}", summary.provider_id);
    println!("model: {}", summary.model_name);
    println!("memory_db: {}", summary.db_path);
    println!("identity_memory_root: {}", summary.identity_memory_root);
    println!("subagent: {}", summary.subagent_kind);
    println!("subagent_queue_root: {}", summary.subagent_queue_root);
    println!(
        "context_budget: max={} reserve_system={} min_working={} max_tool_results={} max_memory_segments={}",
        summary.context_max_tokens,
        summary.context_reserve_system_tokens,
        summary.context_min_working_tokens,
        summary.context_max_tool_results,
        summary.context_max_memory_segments
    );
    if let Some(api_key_state) = &summary.api_key_state {
        println!("api_key: {api_key_state}");
    }
}

pub fn print_runtime_result(result: &RuntimeResult) {
    println!("model_name: {}", result.response.model_name);
    println!("body: {}", result.response.body);
    println!("trace: {}", result.response.trace);
    println!(
        "provider: {}",
        result
            .response
            .meta
            .provider
            .as_deref()
            .unwrap_or("unknown")
    );
    println!("recall_hits: {}", result.recall_hit_count);
    println!("recall_summary: {}", result.recall_summary);
    println!(
        "context_drop_reasons: {}",
        format_drop_reasons(&result.context_debug.drop_reasons)
    );
    println!(
        "context_working_reservation: {}",
        format_working_reservation(&result.context_debug)
    );
    println!(
        "context_budget_exceeded: {}",
        result.context_debug.budget_exceeded
    );
    println!(
        "context_budget_exceeded_reasons: {}",
        format_budget_exceeded_reasons(&result.context_debug.budget_exceeded_reasons)
    );

    for (key, value) in &result.response.meta.extra {
        println!("{key}: {value}");
    }
}

pub fn usage() -> String {
    "usage: cargo run -- <run|repl|status|config|control|subagent> [--config PATH] [--db PATH] [--identity-memory-root PATH] [--subagent fake|queued_external] [--subagent-queue-root PATH] [--context-max-tokens N] [--context-reserve-system-tokens N] [--context-min-working-tokens N] [--context-max-tool-results N] [--context-max-memory-segments N] [--input TEXT] [--remember] [--remember-identity] [--provider-base-url URL --provider-api-key KEY --provider-model MODEL [--provider-id ID]] | status [--json] | config init [--path PATH] [--json] | config check|show [--json] | control list [--json] | control apply --unit ID --action start|stop|restart|change-model [--model MODEL] --reason TEXT [--approve] [--json] | subagent dispatch --task TEXT [--task-id ID] [--agent-name NAME] [--policy analyze|execute|orchestrate] [--token-budget N] [--idle-timeout-ms MS] [--fork-parent-tokens N] [--json] | subagent report --run-id ID [--json] | subagent list [--json] | subagent run-once [--runner fake] [--json]".to_string()
}

fn format_drop_reasons(reasons: &[DropReason]) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(|reason| format!("{}:{}", reason.segment_id, reason.reason.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_budget_exceeded_reasons(reasons: &[BudgetExceededReason]) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(BudgetExceededReason::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_working_reservation(debug: &ContextDebugInfo) -> String {
    debug
        .working_reservation
        .as_ref()
        .map(|reservation| {
            format!(
                "reserved={}@{} reason={} dropped={}",
                reservation.reserved_segment_id,
                reservation.reserved_tokens,
                reservation.reason.as_str(),
                if reservation.dropped_segment_ids.is_empty() {
                    "none".to_string()
                } else {
                    reservation.dropped_segment_ids.join(",")
                }
            )
        })
        .unwrap_or_else(|| "none".to_string())
}
