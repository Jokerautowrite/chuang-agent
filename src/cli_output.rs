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
    if let Some(path) = &status.config.provider_tls_ca_cert_path {
        println!("provider_tls_ca_path: {path}");
    }
    if let Some(policy) = &status.config.provider_fallback_policy {
        println!("provider_fallback_policy: {policy}");
    }
    println!("memory_db: {}", status.config.db_path);
    println!("identity_memory: {}", status.config.identity_memory_kind);
    println!(
        "identity_memory_root: {}",
        status.config.identity_memory_root
    );
    println!("identity_root: {}", status.config.identity_root);
    println!("soul_path: {}", status.config.soul_path);
    println!("story_path: {}", status.config.story_path);
    println!("first_wake_path: {}", status.config.first_wake_path);
    println!(
        "agents_registry_path: {}",
        status.config.agents_registry_path
    );
    println!("rules_root: {}", status.config.rules_root);
    println!("rules_core_path: {}", status.config.rules_core_path);
    println!(
        "tool_loop_max_rounds: {}",
        status.config.tool_loop_max_rounds
    );
    println!(
        "tool_shell_timeout_ms: {}",
        status.config.tool_shell_timeout_ms
    );
    println!(
        "tool_shell_risk_rule_counts: {}",
        status.config.tool_shell_risk_rule_counts
    );
    println!(
        "atomic_tools: source={} ok={} total={} mapped={} interface_only={} action_schema_version={} report_schema_version={}",
        status.atomic_tools.source,
        status.atomic_tools.ok,
        status.atomic_tools.total_count,
        status.atomic_tools.mapped_count,
        status.atomic_tools.interface_only_count,
        status.atomic_tools.tool_action_schema_version,
        status.atomic_tools.tool_report_schema_version
    );
    for tool in &status.atomic_tools.manifests {
        println!(
            "atomic_tool name={} status={} implementation={}",
            tool.name,
            tool.status.as_str(),
            tool.implementation.unwrap_or("none")
        );
    }
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
    println!(
        "identity_bootstrap_chars: soul={} story={} first_wake={} agents={}",
        status
            .kernel
            .identity_soul_chars
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_story_chars
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_first_wake_chars
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_agents_registry_chars
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("governance: {}", status.slots.governance);
    println!("execution: {}", status.slots.execution);
    println!("actuator: {}", status.slots.actuator);
    if let Some(timeout_ms) = status.config.actuator_command_timeout_ms {
        println!("actuator_command_timeout_ms: {timeout_ms}");
    }
    println!("subagent: {}", status.slots.subagent);
    println!("subagent_queue_root: {}", status.config.subagent_queue_root);
    println!("evolution: {}", status.slots.evolution);
    println!("control_plane: {}", status.slots.control_plane);
    if let Some(timeout_ms) = status.config.control_command_timeout_ms {
        println!("control_command_timeout_ms: {timeout_ms}");
    }
    println!(
        "plugin_registry: available={} ok={} path={} plugin_count={} enabled_count={} issue_count={}",
        status.plugin_registry.available,
        status.plugin_registry.ok,
        status.plugin_registry.registry_path,
        status.plugin_registry.plugin_count,
        status.plugin_registry.enabled_count,
        status.plugin_registry.issue_count
    );
    print_placeholder_warnings(&status.config.placeholder_warnings);
}

pub fn print_config_summary(ok: bool, source: &str, summary: &ConfigSummary) {
    println!("config_ok: {ok}");
    println!("config_source: {source}");
    println!("provider: {}", summary.provider_kind);
    println!("provider_id: {}", summary.provider_id);
    println!("model: {}", summary.model_name);
    if let Some(path) = &summary.provider_tls_ca_cert_path {
        println!("provider_tls_ca_path: {path}");
    }
    if let Some(policy) = &summary.provider_fallback_policy {
        println!("provider_fallback_policy: {policy}");
    }
    println!("memory_db: {}", summary.db_path);
    println!("identity_memory_root: {}", summary.identity_memory_root);
    println!("identity_root: {}", summary.identity_root);
    println!("soul_path: {}", summary.soul_path);
    println!("story_path: {}", summary.story_path);
    println!("first_wake_path: {}", summary.first_wake_path);
    println!("agents_registry_path: {}", summary.agents_registry_path);
    println!("rules_root: {}", summary.rules_root);
    println!("rules_core_path: {}", summary.rules_core_path);
    println!("tool_loop_max_rounds: {}", summary.tool_loop_max_rounds);
    println!("tool_shell_timeout_ms: {}", summary.tool_shell_timeout_ms);
    println!(
        "tool_shell_risk_rule_counts: {}",
        summary.tool_shell_risk_rule_counts
    );
    println!("actuator: {}", summary.actuator_kind);
    if let Some(timeout_ms) = summary.actuator_command_timeout_ms {
        println!("actuator_command_timeout_ms: {timeout_ms}");
    }
    println!("subagent: {}", summary.subagent_kind);
    println!("subagent_queue_root: {}", summary.subagent_queue_root);
    println!("control_plane: {}", summary.control_plane_kind);
    if let Some(timeout_ms) = summary.control_command_timeout_ms {
        println!("control_command_timeout_ms: {timeout_ms}");
    }
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
    print_placeholder_warnings(&summary.placeholder_warnings);
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
    println!("context_engine: {}", result.context_engine_kind);
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
    "usage: cargo run -- <run|repl|status|doctor|config|channel|console|control|subagent|genesis|memory|plugin|experiment|app-server> [--config PATH] [--db PATH] [--identity-memory-root PATH] [--subagent fake|queued_external] [--subagent-queue-root PATH] [--context-engine deterministic_budget|summary_compression] [--context-max-tokens N] [--context-reserve-system-tokens N] [--context-min-working-tokens N] [--context-max-tool-results N] [--context-max-memory-segments N] [--input TEXT] [--remember] [--session-id ID] [--remember-session] [--remember-identity] [--dispatch-subagent] [--provider-base-url URL --provider-api-key KEY --provider-model MODEL [--provider-id ID] [--provider-transport stub|http|native|curl]] | status|doctor [--json] | config init [--path PATH] [--json] | config check|show [--json] | channel simulate --workspace-root PATH --message-id ID --sender-id ID --text TEXT [--thread-id ID] [--channel NAME] [--json] | channel feishu-check --env-file PATH [--json] | console snapshot [--json] | control list [--json] | control apply --unit ID --action start|stop|restart|change-model [--model MODEL] --reason TEXT [--approve] [--json] | subagent dispatch --task TEXT [--task-id ID] [--agent-name NAME] [--policy analyze|execute|orchestrate] [--token-budget N] [--idle-timeout-ms MS] [--fork-parent-tokens N] [--requires-capability NAME] [--json] | subagent report --run-id ID [--json] | subagent collect --run-id ID [--json] | subagent release-claim --run-id ID --reason TEXT [--json] | subagent list [--json] | subagent run-once [--runner fake|command] [--capability NAME] [--runner-command PATH] [--runner-arg ARG] [--approve-exec] [--json] | subagent run-loop [--max-runs N] [--max-concurrency 1] [--runner fake|command] [--capability NAME] [--runner-command PATH] [--runner-arg ARG] [--approve-exec] [--json] | genesis ask --prompt TEXT (--approve-exec|--dry-run) [--program autocli] [--profile-dir PATH] [--cdp-port N] [--timeout-ms N] [--json] | memory identity show [--json] | memory identity append --id ID --content TEXT [--json] | memory identity write-user --content TEXT --approve-overwrite [--json] | memory identity write-memory --content TEXT --approve-overwrite [--json] | plugin list|check [--registry PATH] [--json] | app-server health [--workspace-root PATH] [--json] | experiment plan --goal TEXT --success TEXT [--time-budget-minutes N] [--root PATH] [--json] | experiment complete --experiment-id ID --outcome success|failure|inconclusive --summary TEXT --next TEXT [--root PATH] [--json] | experiment list [--root PATH] [--json] | experiment show --experiment-id ID [--root PATH] [--json]".to_string()
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

fn print_placeholder_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        println!("placeholder_warnings: none");
        return;
    }

    for warning in warnings {
        println!("placeholder_warning: {warning}");
    }
}
