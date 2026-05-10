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
    if let Some(timeout_ms) = status.config.provider_request_timeout_ms {
        println!("provider_request_timeout_ms: {timeout_ms}");
    }
    if let Some(policy) = &status.config.provider_fallback_policy {
        println!("provider_fallback_policy: {policy}");
    }
    println!(
        "provider_readiness: ok={} state={} kind={} transport={} fallback_configured={} timeout_ms={} api_key_state={} placeholder_warnings={} provider_env_file_state={}",
        status.provider_readiness.ok,
        status.provider_readiness.overall_state,
        status.provider_readiness.provider_kind,
        status.provider_readiness.transport,
        status.provider_readiness.fallback_configured,
        status
            .provider_readiness
            .request_timeout_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .provider_readiness
            .api_key_state
            .as_deref()
            .unwrap_or("none"),
        status.provider_readiness.placeholder_warning_count,
        status.provider_readiness.provider_env_file_state
    );
    println!(
        "provider_readiness_current: {}",
        status.provider_readiness.current
    );
    println!(
        "provider_readiness_next_action: {}",
        status.provider_readiness.next_action
    );
    println!("memory_db: {}", status.config.db_path);
    println!("identity_memory: {}", status.config.identity_memory_kind);
    println!(
        "identity_memory_root: {}",
        status.config.identity_memory_root
    );
    println!(
        "identity_experiences_path: {}",
        status.config.identity_experiences_path
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
        "runtime_capability_primer: {}",
        status.runtime_capability_primer
    );
    println!(
        "atomic_tools: source={} ok={} total={} mapped={} interface_only={} manifest_schema_version={} action_schema_version={} report_schema_version={}",
        status.atomic_tools.source,
        status.atomic_tools.ok,
        status.atomic_tools.total_count,
        status.atomic_tools.mapped_count,
        status.atomic_tools.interface_only_count,
        status.atomic_tools.manifest_schema_version,
        status.atomic_tools.tool_action_schema_version,
        status.atomic_tools.tool_report_schema_version
    );
    println!(
        "atomic_tools_mapped: {}",
        format_name_list(&status.atomic_tools.mapped_atomic_tool_names)
    );
    println!(
        "atomic_tools_executable: {}",
        format_name_list(&status.atomic_tools.governed_executable_atomic_tool_names)
    );
    println!(
        "atomic_tools_interface_only: {} (local surface only; live adapters separate)",
        format_name_list(&status.atomic_tools.interface_only_atomic_tool_names)
    );
    println!(
        "atomic_tools_desktop_browser_interface_only: {} reason={}",
        format_name_list(
            &status
                .atomic_tools
                .desktop_browser_interface_only_atomic_tool_names
        ),
        status.atomic_tools.interface_only_reason
    );
    println!(
        "atomic_tools_desktop_browser_live_gated: {} required=adapter,live_gate,allowlist,audit_receipt",
        format_name_list(
            &status
                .atomic_tools
                .desktop_browser_live_gated_atomic_tool_names
        )
    );
    println!(
        "desktop_read: ready={} tools={} boundary={}",
        status.browser_readiness.desktop_read_observation_ready,
        format_name_list(&status.browser_readiness.desktop_read_tools),
        status.browser_readiness.desktop_read_boundary
    );
    println!(
        "browser_read: available={} state={} adapter_kind={} browser_read_reason_code={} browser_read_reason={} capabilities={} browser_read_boundary={} desktop_read_boundary={} current={} next_action={}",
        status.browser_readiness.browser_read_adapter_available,
        status.browser_readiness.browser_read_state,
        status.browser_readiness.browser_read_adapter_kind,
        status.browser_readiness.browser_read_reason_code,
        status.browser_readiness.browser_read_reason,
        format_name_list(&status.browser_readiness.browser_read_capabilities),
        status.browser_readiness.browser_read_boundary,
        status.browser_readiness.desktop_read_boundary,
        status.browser_readiness.current,
        status.browser_readiness.next_action
    );
    println!(
        "knowledge_read: available={} state={} adapter_kind={} live_reason_code={} live_reason={} sources={} live_boundary={} local_preview_boundary={} local_preview_is_separate={} connects_real_service={} writes_automatically={} current={} next_action={}",
        status.knowledge_readiness.live_adapter_available,
        status.knowledge_readiness.live_adapter_state,
        status.knowledge_readiness.live_adapter_kind,
        status.knowledge_readiness.live_reason_code,
        status.knowledge_readiness.live_reason,
        format_name_list(&status.knowledge_readiness.live_sources),
        status.knowledge_readiness.live_boundary,
        status.knowledge_readiness.local_preview_boundary,
        status.knowledge_readiness.local_preview_is_separate,
        status.knowledge_readiness.connects_real_service,
        status.knowledge_readiness.writes_automatically,
        status.knowledge_readiness.current,
        status.knowledge_readiness.next_action
    );
    println!(
        "knowledge_read_preflight: endpoint_wiki={} endpoint_gbrain={} token_env_wiki={} token_env_gbrain={}",
        status
            .config
            .external_knowledge_wiki_endpoint
            .as_deref()
            .unwrap_or("none"),
        status
            .config
            .external_knowledge_gbrain_endpoint
            .as_deref()
            .unwrap_or("none"),
        status
            .config
            .external_knowledge_wiki_token_env
            .as_deref()
            .unwrap_or("none"),
        status
            .config
            .external_knowledge_gbrain_token_env
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "atomic_tools_self_check_entrypoints: {}",
        format_name_list(&status.atomic_tools.local_cli_self_check_entrypoints)
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
    println!(
        "identity_bootstrap_present: soul={} story={} first_wake={} agents={}",
        status
            .kernel
            .identity_soul_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_story_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_first_wake_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        status
            .kernel
            .identity_agents_registry_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("governance: {}", status.slots.governance);
    println!(
        "governance_readiness: ok={} kind={} rules_loaded={} tool_surface_governed={} goal_run_executes={}",
        status.governance.ok,
        status.governance.kind,
        status.governance.rules_loaded,
        status.governance.tool_surface_governed,
        status.governance.goal_run_executes
    );
    println!(
        "governance_rules: path={} rule_count={} fingerprint={}",
        status.governance.rules_core_path,
        status.governance.rule_count,
        status.governance.rules_fingerprint
    );
    println!(
        "governance_decisions: read_only={} dangerous_write={} dangerous_shell={} secret_shell={}",
        status.governance.read_only_decision,
        status.governance.dangerous_write_decision,
        status.governance.dangerous_shell_decision,
        status.governance.secret_shell_decision
    );
    println!("execution: {}", status.slots.execution);
    println!("actuator: {}", status.slots.actuator);
    if let Some(timeout_ms) = status.config.actuator_command_timeout_ms {
        println!("actuator_command_timeout_ms: {timeout_ms}");
    }
    println!("subagent: {}", status.slots.subagent);
    println!(
        "subagent_live_worker: enabled={} adapter_kind={} status={} starts_worker={} available={} reason={}",
        status.config.subagent_live_worker.enabled,
        status.config.subagent_live_worker.adapter_kind,
        status.config.subagent_live_worker.status,
        status.config.subagent_live_worker.starts_worker,
        status.config.subagent_live_worker.available,
        status.config.subagent_live_worker.reason
    );
    println!("subagent_queue_root: {}", status.config.subagent_queue_root);
    println!("evolution: {}", status.slots.evolution);
    println!("control_plane: {}", status.slots.control_plane);
    if let Some(timeout_ms) = status.config.control_command_timeout_ms {
        println!("control_command_timeout_ms: {timeout_ms}");
    }
    println!(
        "goal_run: ok={} plan_exists={} goal_id={} checkpoints={} workers={} validation_commands={} path={}",
        status.goal_run.ok,
        status.goal_run.plan_exists,
        status.goal_run.goal_id,
        status.goal_run.checkpoint_count,
        status.goal_run.worker_count,
        status.goal_run.validation_command_count,
        status.goal_run.path
    );
    println!(
        "goal_mode: ok={} kind={} cli_entrypoint={} context_source={} default_goal_id={} allowed_slots={} checkpoint_policy=progress_log:{} handoff:{} commit:{} final_report_policy=validation:{} next_steps:{} bypasses_governance={} adds_core_slot={}",
        status.goal_mode.ok,
        status.goal_mode.kind,
        status.goal_mode.cli_entrypoint,
        status.goal_mode.context_source,
        status.goal_mode.default_goal_id,
        format_name_list(&status.goal_mode.default_allowed_slots),
        status.goal_mode.checkpoint_policy.update_progress_log,
        status.goal_mode.checkpoint_policy.update_handoff,
        status.goal_mode.checkpoint_policy.commit_checkpoint,
        status.goal_mode.final_report_policy.include_validation,
        status.goal_mode.final_report_policy.include_next_steps,
        status.goal_mode.bypasses_governance,
        status.goal_mode.adds_core_slot
    );
    println!(
        "goal_run_checkpoint_log_complete: {}",
        status.goal_run.checkpoint_log_complete
    );
    println!(
        "goal_run_last_checkpoint: {}",
        status
            .goal_run
            .last_checkpoint_id
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "goal_run_last_checkpoint_summary: {}",
        status
            .goal_run
            .last_checkpoint_summary
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "goal_run_last_checkpoint_created_at: {}",
        status
            .goal_run
            .last_checkpoint_created_at
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "goal_run_last_checkpoint_completed_worker_ids: {}",
        status
            .goal_run
            .last_checkpoint_completed_worker_ids
            .as_ref()
            .map(|values| values.join(","))
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "goal_run_last_checkpoint_validation_notes: {}",
        status
            .goal_run
            .last_checkpoint_validation_notes
            .as_ref()
            .map(|values| values.join(" | "))
            .unwrap_or_else(|| "none".to_string())
    );
    if let Some(read_error) = &status.goal_run.read_error {
        println!("goal_run_read_error: {read_error}");
    }
    println!(
        "goal_run_incomplete_reasons: {}",
        format_text_list(&status.goal_run.incomplete_reasons)
    );
    println!(
        "plugin_registry: available={} ok={} path={} plugin_count={} enabled_count={} issue_count={} evidence_available={} check_only={} executes_plugins={} reads_secret={} connects_external_service={} writes_files={} capability_count={} capabilities={}",
        status.plugin_registry.available,
        status.plugin_registry.ok,
        status.plugin_registry.registry_path,
        status.plugin_registry.plugin_count,
        status.plugin_registry.enabled_count,
        status.plugin_registry.issue_count,
        status.plugin_registry.evidence_available,
        status.plugin_registry.check_only,
        status.plugin_registry.executes_plugins,
        status.plugin_registry.reads_secret,
        status.plugin_registry.connects_external_service,
        status.plugin_registry.writes_files,
        status.plugin_registry.capability_count,
        format_name_list(&status.plugin_registry.capabilities)
    );
    println!(
        "local_contract_readiness: ok={} state={} contracts={} ready={} partial={} deferred={} blocked={} connects_real_external_services={} writes_core_memory={} executes_plugins={}",
        status.local_contract_readiness.ok,
        status.local_contract_readiness.overall_state,
        status.local_contract_readiness.contract_count,
        status.local_contract_readiness.ready_count,
        status.local_contract_readiness.partial_count,
        status.local_contract_readiness.deferred_count,
        status.local_contract_readiness.blocked_count,
        status.local_contract_readiness.connects_real_external_services,
        status.local_contract_readiness.writes_core_memory,
        status.local_contract_readiness.executes_plugins
    );
    for contract in &status.local_contract_readiness.contracts {
        println!(
            "local_contract name={} state={} boundary={} read_only={} dry_run={} connects_real_service={} writes_core_memory={} writes_repo_files={} executes_plugins={} next={}",
            contract.name,
            contract.state,
            contract.boundary,
            contract.read_only,
            contract.dry_run,
            contract.connects_real_service,
            contract.writes_core_memory,
            contract.writes_repo_files,
            contract.executes_plugins,
            contract.next_action
        );
        if contract.name == "knowledge_context_preview"
            || contract.name == "external_knowledge_source_contracts"
        {
            println!(
                "local_contract_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
            );
        }
    }
    println!(
        "project_readiness: ok={} state={} ready={} partial={} deferred={} blocked={}",
        status.project_readiness.ok,
        status.project_readiness.overall_state,
        status.project_readiness.ready_count,
        status.project_readiness.partial_count,
        status.project_readiness.deferred_count,
        status.project_readiness.blocked_count
    );
    for module in &status.project_readiness.modules {
        println!(
            "project_module name={} state={} boundary={} next={}",
            module.name, module.state, module.core_boundary, module.next_action
        );
    }
    println!(
        "memory_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        status.memory_readiness.ok,
        status.memory_readiness.overall_state,
        status.memory_readiness.layer_count,
        status.memory_readiness.ready_count,
        status.memory_readiness.partial_count,
        status.memory_readiness.deferred_count,
        status.memory_readiness.blocked_count
    );
    for layer in &status.memory_readiness.layers {
        println!(
            "memory_layer name={} state={} storage={} writes_automatically={} next={}",
            layer.name, layer.state, layer.storage, layer.writes_automatically, layer.next_action
        );
        if layer.name == "external_knowledge" {
            println!(
                "memory_layer_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
            );
        }
    }
    println!(
        "memory_maintenance_receipt: available={} readable={} state={} receipts={} latest_entry_id={} latest_source_record_id={} latest_approval_source={} latest_approved_at={} latest_provenance_preserved={}",
        status.memory_maintenance_receipt.available,
        status.memory_maintenance_receipt.readable,
        status.memory_maintenance_receipt.state,
        status.memory_maintenance_receipt.receipt_count,
        status
            .memory_maintenance_receipt
            .latest_entry_id
            .as_deref()
            .unwrap_or("none"),
        status
            .memory_maintenance_receipt
            .latest_source_record_id
            .as_deref()
            .unwrap_or("none"),
        status
            .memory_maintenance_receipt
            .latest_approval_source
            .as_deref()
            .unwrap_or("none"),
        status
            .memory_maintenance_receipt
            .latest_approved_at
            .as_deref()
            .unwrap_or("none"),
        status.memory_maintenance_receipt.latest_provenance_preserved
    );
    if let Some(note) = &status.memory_maintenance_receipt.latest_approval_note {
        println!("memory_maintenance_receipt_note: {note}");
    }
    if let Some(error) = &status.memory_maintenance_receipt.error {
        println!("memory_maintenance_receipt_error: {error}");
    }
    println!(
        "channel_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        status.channel_readiness.ok,
        status.channel_readiness.overall_state,
        status.channel_readiness.layer_count,
        status.channel_readiness.ready_count,
        status.channel_readiness.partial_count,
        status.channel_readiness.deferred_count,
        status.channel_readiness.blocked_count
    );
    for layer in &status.channel_readiness.layers {
        println!(
            "channel_layer name={} state={} boundary={} next={}",
            layer.name, layer.state, layer.boundary, layer.next_action
        );
    }
    println!(
        "subagent_readiness: ok={} state={} mode={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} layers={} ready={} partial={} deferred={} blocked={} live_worker_available={} worker_runtime_state={} worker_runtime_blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={}",
        status.subagent_readiness.ok,
        status.subagent_readiness.overall_state,
        status.subagent_readiness.mode,
        status.subagent_readiness.local_contract_ready,
        status.subagent_readiness.local_contract_state,
        status.subagent_readiness.live_adapter_ready,
        status.subagent_readiness.live_adapter_state,
        status.subagent_readiness.layer_count,
        status.subagent_readiness.ready_count,
        status.subagent_readiness.partial_count,
        status.subagent_readiness.deferred_count,
        status.subagent_readiness.blocked_count,
        status.subagent_readiness.live_worker_available,
        status.subagent_readiness.worker_runtime_state,
        status.subagent_readiness.worker_runtime_blocked_reason,
        status.subagent_readiness.capability_route_state,
        status.subagent_readiness.capability_mismatch_blocks_live,
        status.subagent_readiness.capability_mismatch_reason
    );
    println!(
        "subagent_worker_runtime_reason: {}",
        status.subagent_readiness.worker_runtime_reason
    );
    println!(
        "subagent_capability_mismatch_reason: {}",
        status.subagent_readiness.capability_mismatch_reason
    );
    println!(
        "subagent_readiness_local_contract_reason: {}",
        status.subagent_readiness.local_contract_reason
    );
    println!(
        "subagent_readiness_live_adapter_reason: {}",
        status.subagent_readiness.live_adapter_reason
    );
    for layer in &status.subagent_readiness.layers {
        println!(
            "subagent_layer name={} state={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} live_worker_available={} worker_runtime_state={} blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} boundary={} local_contract_reason={} live_adapter_reason={} capability_mismatch_reason={} next={}",
            layer.name,
            layer.state,
            layer.local_contract_ready,
            layer.local_contract_state,
            layer.live_adapter_ready,
            layer.live_adapter_state,
            layer.live_worker_available,
            layer.worker_runtime_state,
            layer.blocked_reason,
            layer.capability_route_state,
            layer.capability_mismatch_blocks_live,
            layer.boundary,
            layer.local_contract_reason,
            layer.live_adapter_reason,
            layer.capability_mismatch_reason,
            layer.next_action
        );
    }
    println!(
        "live_adapter_gates: ok={} state={} gates={} enabled={} disabled={}",
        status.live_adapter_gates.ok,
        status.live_adapter_gates.overall_state,
        status.live_adapter_gates.gate_count,
        status.live_adapter_gates.enabled_count,
        status.live_adapter_gates.disabled_count
    );
    for gate in &status.live_adapter_gates.gates {
        println!(
            "live_adapter_gate name={} state={} enabled={} default_enabled={} env_value_state={} required_env={} audit_label={} preflight={} must_reject={} reason={} next={}",
            gate.name,
            gate.state,
            gate.enabled,
            gate.default_enabled,
            gate.env_value_state,
            gate.required_env,
            gate.audit_label,
            format_text_list(&gate.preflight_checks),
            format_text_list(&gate.must_reject_capabilities),
            gate.reason,
            gate.next_action
        );
    }
    println!(
        "external_ai_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        status.external_ai_readiness.ok,
        status.external_ai_readiness.overall_state,
        status.external_ai_readiness.layer_count,
        status.external_ai_readiness.ready_count,
        status.external_ai_readiness.partial_count,
        status.external_ai_readiness.deferred_count,
        status.external_ai_readiness.blocked_count
    );
    println!(
        "release_readiness: ok={} name={} state={} ready={} partial={} deferred={} blocked={}",
        status.release_readiness.ok,
        status.release_readiness.release_name,
        status.release_readiness.overall_state,
        status.release_readiness.ready_count,
        status.release_readiness.partial_count,
        status.release_readiness.deferred_count,
        status.release_readiness.blocked_count
    );
    println!(
        "release_acceptance: count={} ready={} partial={} deferred={} connects_real_external_services={} verifies_real_external_services={} uses_stub_or_local_fixtures={} writes_repo_files={}",
        status.release_readiness.acceptance_count,
        status.release_readiness.acceptance_ready_count,
        status.release_readiness.acceptance_partial_count,
        status.release_readiness.acceptance_deferred_count,
        status.release_readiness.connects_real_external_services,
        status.release_readiness.verifies_real_external_services,
        status.release_readiness.uses_stub_or_local_fixtures,
        status.release_readiness.writes_repo_files
    );
    for item in &status.release_readiness.acceptance {
        println!(
            "release_acceptance_item name={} state={} boundary={} read_only={} connects_real_service={} writes_repo_files={}",
            item.name,
            item.state,
            item.boundary,
            item.read_only,
            item.connects_real_service,
            item.writes_repo_files
        );
    }
    println!(
        "third_test_candidate: ok={} state={} local_gate_ready={} smoke_script={} marker={} requires_manual_live_check={} connects_real_external_services={} operator_env_blocks_100_percent={} real_live_ready={}",
        status.third_test_candidate.ok,
        status.third_test_candidate.overall_state,
        status.third_test_candidate.local_gate_ready,
        status.third_test_candidate.smoke_script,
        status.third_test_candidate.marker,
        status.third_test_candidate.requires_manual_live_check,
        status.third_test_candidate.connects_real_external_services,
        status.third_test_candidate.operator_env_blocks_100_percent,
        status.third_test_candidate.real_live_ready
    );
    if !status.config.placeholder_warnings.is_empty() {
        println!(
            "placeholder_warnings: {}",
            format_name_list(&status.config.placeholder_warnings)
        );
    }
    for layer in &status.external_ai_readiness.layers {
        println!(
            "external_ai_layer name={} state={} boundary={} next={}",
            layer.name, layer.state, layer.boundary, layer.next_action
        );
    }
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
    if let Some(timeout_ms) = summary.provider_request_timeout_ms {
        println!("provider_request_timeout_ms: {timeout_ms}");
    }
    if let Some(policy) = &summary.provider_fallback_policy {
        println!("provider_fallback_policy: {policy}");
    }
    println!("memory_db: {}", summary.db_path);
    println!("identity_memory_root: {}", summary.identity_memory_root);
    println!(
        "identity_experiences_path: {}",
        summary.identity_experiences_path
    );
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
    println!(
        "subagent_live_worker: enabled={} adapter_kind={} status={} starts_worker={} available={} reason={}",
        summary.subagent_live_worker.enabled,
        summary.subagent_live_worker.adapter_kind,
        summary.subagent_live_worker.status,
        summary.subagent_live_worker.starts_worker,
        summary.subagent_live_worker.available,
        summary.subagent_live_worker.reason
    );
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

fn format_name_list(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(",")
    }
}

fn format_text_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(" | ")
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
    "usage: cargo run -- <run|repl|status|doctor|config|channel|console|control|subagent|genesis|goal|memory|plugin|skill|experiment|external-ai|app-server> [--config PATH] [--db PATH] [--identity-memory-root PATH] [--subagent fake|queued_external] [--subagent-queue-root PATH] [--context-engine deterministic_budget|summary_compression] [--context-max-tokens N] [--context-reserve-system-tokens N] [--context-min-working-tokens N] [--context-max-tool-results N] [--context-max-memory-segments N] [--input TEXT] [--remember] [--session-id ID] [--remember-session] [--remember-identity] [--remember-experience] [--dispatch-subagent] [--goal TEXT] [--enable-knowledge-context-preview --knowledge-context-root PATH --knowledge-context-query TEXT [--knowledge-context-limit N]] [--provider-base-url URL --provider-api-key KEY --provider-model MODEL [--provider-id ID] [--provider-transport stub|http|native|curl] [--provider-request-timeout-ms MS]] | repl [--verbose] | status|doctor [--json] | config init [--path PATH] [--json] | config check|show [--json] | channel simulate --workspace-root PATH --message-id ID --sender-id ID --text TEXT [--thread-id ID] [--goal TEXT] [--channel NAME] [--json] | channel feishu-check --env-file PATH [--json] | console snapshot [--json] | control list [--json] | control apply --unit ID --action start|stop|restart|change-model [--model MODEL] --reason TEXT [--approve] [--json] | subagent dispatch --task TEXT [--task-id ID] [--agent-name NAME] [--policy analyze|execute|orchestrate] [--token-budget N] [--idle-timeout-ms MS] [--fork-parent-tokens N] [--requires-capability NAME] [--json] | subagent report --run-id ID [--json] | subagent collect --run-id ID [--json] | subagent release-claim --run-id ID --reason TEXT [--json] | subagent list [--json] | subagent run-once [--runner fake|command] [--capability NAME] [--runner-command PATH] [--runner-arg ARG] [--approve-exec] [--json] | subagent run-loop [--max-runs N] [--max-concurrency 1..8] [--runner fake|command] [--capability NAME] [--runner-command PATH] [--runner-arg ARG] [--approve-exec] [--json] | subagent live-preflight --runner-command PATH --allow-runner-command PATH [--requires-capability NAME] [--capability NAME] [--json] | goal plan --objective TEXT [--root PATH] [--goal-id ID] [--max-subtasks N] [--json] | goal show [--root PATH] [--goal-id ID] [--json] | goal checkpoint [--from-collect --subagent-queue-root PATH] --summary TEXT --completed-worker-id ID --validation-note TEXT [--completed-worker-id ID ...] [--validation-note TEXT ...] [--root PATH] [--goal-id ID] [--json] | goal dispatch [--root PATH] [--goal-id ID] [--subagent-queue-root PATH] [--parent-agent-id ID] [--json] | goal collect [--root PATH] [--goal-id ID] [--subagent-queue-root PATH] [--json] | goal step [--root PATH] [--goal-id ID] [--subagent-queue-root PATH] [--max-runs N] [--max-concurrency 1..8] [--runner fake|command] [--capability NAME] [--runner-command PATH] [--runner-arg ARG] [--approve-exec] [--json] | genesis ask --prompt TEXT (--approve-exec|--dry-run) [--program autocli] [--profile-dir PATH] [--cdp-port N] [--timeout-ms N] [--json] | external-ai dispatch --platform NAME --task TEXT --context TEXT --dry-run [--session-hint ID] [--timeout-ms N] [--json] | memory identity show [--json] | memory identity append --id ID --content TEXT [--json] | memory identity append-experience --id ID --content TEXT [--json] | memory identity write-user --content TEXT --approve-overwrite [--json] | memory identity write-memory --content TEXT --approve-overwrite [--json] | memory session search --query TEXT [--session-id ID] [--limit N] [--json] | memory lim extract --query TEXT [--session-id ID] [--limit N] [--json] | memory maintenance report --query TEXT [--session-id ID] [--limit N] [--json] | memory maintenance apply --query TEXT [--session-id ID] [--limit N] [--candidate-id ID] [--approve-writeback] [--json] | memory knowledge status [--json] | memory knowledge search --root PATH --query TEXT [--limit N] [--json] | memory knowledge preview-context --root PATH --query TEXT [--limit N] [--json] | memory knowledge source-contract --source wiki|gbrain [--json] | plugin list|check [--registry PATH] [--json] | skill propose --event-id ID --task-id ID --summary TEXT [--kind KIND] [--metadata key=value] [--agent-id ID] [--task-kind KIND] [--max-proposals N] [--json] | skill approve --event-id ID --task-id ID --summary TEXT [--kind KIND] [--metadata key=value] [--agent-id ID] [--task-kind KIND] [--max-proposals N] [--approval-source TEXT] [--approved-at TEXT] [--approval-note TEXT] [--approval-threshold N] [--skills-root PATH] [--json] | skill judge --event-id ID --task-id ID --summary TEXT [--kind KIND] [--metadata key=value] [--agent-id ID] [--task-kind KIND] [--max-proposals N] [--approval-source TEXT] [--approved-at TEXT] [--approval-note TEXT] [--approval-threshold N] [--skills-root PATH] [--json] | skill solidify --event-id ID --task-id ID --summary TEXT [--kind KIND] [--metadata key=value] [--agent-id ID] [--task-kind KIND] [--max-proposals N] [--approval-source TEXT] [--approved-at TEXT] [--approval-note TEXT] [--approval-threshold N] [--skills-root PATH] [--json] | skill retire|deprecate --skill-id ID --reason TEXT [--status deprecated|retired] [--retired-at TEXT] [--skills-root PATH] [--json] | skill monitor [--skills-root PATH] [--json] | skill rollback --skill-id ID --reason TEXT [--rollback-at TEXT] [--skills-root PATH] [--json] | app-server health [--workspace-root PATH] [--diagnostic] [--json] | experiment plan --goal TEXT --success TEXT [--time-budget-minutes N] [--root PATH] [--json] | experiment complete --experiment-id ID --outcome success|failure|inconclusive --summary TEXT --next TEXT [--root PATH] [--json] | experiment list [--root PATH] [--json] | experiment show --experiment-id ID [--root PATH] [--json]".to_string()
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
