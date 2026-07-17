use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::actuator::{Actuator, ObserveTarget};
use chuang_agent::atomic_tool::{
    ga_atomic_tool_manifests, AtomicToolKind, AtomicToolManifest, AtomicToolStatus,
};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::plugin_registry::check_plugin_registry;
use chuang_agent::runtime_config::{ProviderConfig, RuntimeConfig};
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use chuang_agent::subagent_spawner::{
    ContextIsolation, QueuedSubagentSpawner, SpawnRequest, SubagentSpawner, SubagentToolPolicy,
};
use chuang_agent::tool_runtime::{ToolActionEnvelope, ToolLoopReport};
use chuang_agent::{common::AgentId, common::TaskId};

use crate::cli_args::{parse_cli_options, parse_status_output};
use crate::cli_output::{print_json, ControlOutputFormat};
use crate::cli_runtime::{kernel_config_from_runtime, run_with_options};
use crate::cli_types::{CliOptions, DoctorCheck, DoctorCliOutput, RunCliRequest};

pub(crate) fn doctor_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_cli_options(args)?;
    let doctor = run_doctor(&options.runtime)?;

    match output {
        ControlOutputFormat::Text => print_doctor(&doctor),
        ControlOutputFormat::Json => print_json(&doctor)?,
    }

    Ok(())
}

fn run_doctor(runtime: &RuntimeConfig) -> Result<DoctorCliOutput, String> {
    let mut checks = Vec::new();

    runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    checks.push(pass("config", "runtime config is valid"));

    let kernel = kernel_config_from_runtime(runtime)?;
    let status = build_chuang_mvp_status(runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    checks.push(pass(
        "provider_readiness",
        &format!(
            "state={} kind={} transport={} fallback_configured={} timeout_ms={} api_key_state={} placeholder_warnings={}",
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
            status.provider_readiness.placeholder_warning_count
        ),
    ));
    checks.push(pass("identity_memory", "identity snapshot loaded"));
    run_identity_experiences_check(runtime)?;
    checks.push(pass(
        "identity_experiences",
        "experiences.md entrypoint is available",
    ));
    if !status.memory_readiness.ok {
        return Err(format!(
            "doctor_memory_readiness_failed state={} blocked={}",
            status.memory_readiness.overall_state, status.memory_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "memory_readiness",
        &format!(
            "state={} layers={} ready={} partial={} deferred={} blocked={}",
            status.memory_readiness.overall_state,
            status.memory_readiness.layer_count,
            status.memory_readiness.ready_count,
            status.memory_readiness.partial_count,
            status.memory_readiness.deferred_count,
            status.memory_readiness.blocked_count
        ),
    ));
    if !status.channel_readiness.ok {
        return Err(format!(
            "doctor_channel_readiness_failed state={} blocked={}",
            status.channel_readiness.overall_state, status.channel_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "channel_readiness",
        &format!(
            "state={} layers={} ready={} partial={} deferred={} blocked={}",
            status.channel_readiness.overall_state,
            status.channel_readiness.layer_count,
            status.channel_readiness.ready_count,
            status.channel_readiness.partial_count,
            status.channel_readiness.deferred_count,
            status.channel_readiness.blocked_count
        ),
    ));
    if !status.subagent_readiness.ok {
        return Err(format!(
            "doctor_subagent_readiness_failed state={} blocked={}",
            status.subagent_readiness.overall_state, status.subagent_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "subagent_readiness",
        &format!(
            "state={} mode={} layers={} ready={} partial={} deferred={} blocked={} live_worker_available={} worker_runtime_state={} worker_runtime_reason={} worker_runtime_blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={}",
            status.subagent_readiness.overall_state,
            status.subagent_readiness.mode,
            status.subagent_readiness.layer_count,
            status.subagent_readiness.ready_count,
            status.subagent_readiness.partial_count,
            status.subagent_readiness.deferred_count,
            status.subagent_readiness.blocked_count,
            status.subagent_readiness.live_worker_available,
            status.subagent_readiness.worker_runtime_state,
            status.subagent_readiness.worker_runtime_reason,
            status.subagent_readiness.worker_runtime_blocked_reason,
            status.subagent_readiness.capability_route_state,
            status.subagent_readiness.capability_mismatch_blocks_live,
            status.subagent_readiness.capability_mismatch_reason
        ),
    ));
    let live_adapter_next_actions = status
        .live_adapter_gates
        .gates
        .iter()
        .map(|gate| format!("{}:{}", gate.name, gate.next_action))
        .collect::<Vec<_>>()
        .join(";");
    checks.push(pass(
        "live_adapter_preflight",
        &format!(
            "state={} gates={} enabled={} disabled={} next_actions={}",
            status.live_adapter_gates.overall_state,
            status.live_adapter_gates.gate_count,
            status.live_adapter_gates.enabled_count,
            status.live_adapter_gates.disabled_count,
            live_adapter_next_actions
        ),
    ));
    checks.push(pass(
        "live_readiness",
        &format!(
            "state={} local_ready_scope={} ga_local_mapped_only={} desktop_browser_live_gated={} browser_worker_frozen={} live_worker_available={} real_external_acceptance_pending={} provider_live_request_verified_by_status={} mapped_does_not_mean_live={} gated_does_not_mean_ready={} frozen_does_not_mean_ready={} ready_does_not_mean_live={}",
            status.live_readiness.overall_state,
            status.live_readiness.local_ready_scope,
            status.live_readiness.ga_local_mapped_only,
            status.live_readiness.desktop_browser_live_gated,
            status.live_readiness.browser_worker_frozen,
            status.live_readiness.live_worker_available,
            status.live_readiness.real_external_acceptance_pending,
            status.live_readiness.provider_live_request_verified_by_status,
            status.live_readiness.mapped_does_not_mean_live,
            status.live_readiness.gated_does_not_mean_ready,
            status.live_readiness.frozen_does_not_mean_ready,
            status.live_readiness.ready_does_not_mean_live
        ),
    ));
    if !status.external_ai_readiness.ok {
        return Err(format!(
            "doctor_external_ai_readiness_failed state={} blocked={}",
            status.external_ai_readiness.overall_state, status.external_ai_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "external_ai_readiness",
        &format!(
            "state={} layers={} ready={} partial={} deferred={} blocked={}",
            status.external_ai_readiness.overall_state,
            status.external_ai_readiness.layer_count,
            status.external_ai_readiness.ready_count,
            status.external_ai_readiness.partial_count,
            status.external_ai_readiness.deferred_count,
            status.external_ai_readiness.blocked_count
        ),
    ));
    if !status.local_contract_readiness.ok {
        return Err(format!(
            "doctor_local_contract_readiness_failed state={} blocked={}",
            status.local_contract_readiness.overall_state,
            status.local_contract_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "local_contract_readiness",
        &format!(
            "state={} contracts={} ready={} partial={} deferred={} blocked={} connects_real_external_services={} writes_core_memory={} executes_plugins={}",
            status.local_contract_readiness.overall_state,
            status.local_contract_readiness.contract_count,
            status.local_contract_readiness.ready_count,
            status.local_contract_readiness.partial_count,
            status.local_contract_readiness.deferred_count,
            status.local_contract_readiness.blocked_count,
            status.local_contract_readiness.connects_real_external_services,
            status.local_contract_readiness.writes_core_memory,
            status.local_contract_readiness.executes_plugins
        ),
    ));

    let mut slots = build_runtime_slots(runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    checks.push(pass("slots", "runtime slots build"));

    run_atomic_tool_manifest_check()?;
    checks.push(pass(
        "atomic_tools",
        "GenericAgent 9 atomic tool manifest is available",
    ));
    // Soft readiness only — CDP down must not fail doctor.
    checks.push(pass(
        "browser_cdp",
        &format!(
            "available={} state={} kind={} reason_code={} reason={} overall={} autostart_default=on",
            status.browser_readiness.browser_read_adapter_available,
            status.browser_readiness.browser_read_state,
            status.browser_readiness.browser_read_adapter_kind,
            status.browser_readiness.browser_read_reason_code,
            status.browser_readiness.browser_read_reason,
            status.browser_readiness.overall_state
        ),
    ));
    let rtk_bin = which_rtk_for_doctor();
    checks.push(pass(
        "shell_rtk",
        &format!(
            "rewrite_enabled={} binary={}",
            status.config.tool_shell_rtk_rewrite,
            rtk_bin
                .as_deref()
                .unwrap_or("<missing — install rtk or set RTK_BIN>")
        ),
    ));
    if !status.governance.ok {
        return Err("doctor_governance_readiness_failed".to_string());
    }
    checks.push(pass(
        "governance_readiness",
        &format!(
            "rules_loaded={} tool_surface_governed={} dangerous_shell={} dangerous_write={} goal_run_executes={}",
            status.governance.rules_loaded,
            status.governance.tool_surface_governed,
            status.governance.dangerous_shell_decision,
            status.governance.dangerous_write_decision,
            status.governance.goal_run_executes
        ),
    ));

    run_goal_mode_check()?;
    checks.push(pass(
        "goal_mode",
        &format!(
            "entrypoint={} kind={} context_source={} default_goal_id={} allowed_slots={} checkpoint_policy=progress_log:{} handoff:{} commit:{} final_report_policy=validation:{} next_steps:{} bypasses_governance={} adds_core_slot={}",
            status.goal_mode.cli_entrypoint,
            status.goal_mode.kind,
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
        ),
    ));
    if !status.goal_run.ok {
        return Err(format!(
            "doctor_goal_run_readiness_failed path={} error={}",
            status.goal_run.path,
            status
                .goal_run
                .read_error
                .as_deref()
                .unwrap_or("unknown goal run read error")
        ));
    }
    checks.push(pass(
        "goal_run_readiness",
        &format!(
            "goal_id={} plan_exists={} checkpoint_count={} worker_count={} validation_command_count={} checkpoint_log_complete={} last_checkpoint={} last_summary={} last_created_at={} last_completed_worker_ids={} last_validation_notes={} incomplete_reasons={}",
            status.goal_run.goal_id,
            status.goal_run.plan_exists,
            status.goal_run.checkpoint_count,
            status.goal_run.worker_count,
            status.goal_run.validation_command_count,
            status.goal_run.checkpoint_log_complete,
            status
                .goal_run
                .last_checkpoint_id
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_summary
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_created_at
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_completed_worker_ids
                .as_ref()
                .map(|values| values.join(","))
                .unwrap_or_else(|| "none".to_string()),
            status
                .goal_run
                .last_checkpoint_validation_notes
                .as_ref()
                .map(|values| values.join(" | "))
                .unwrap_or_else(|| "none".to_string()),
            format_text_list(&status.goal_run.incomplete_reasons)
        ),
    ));
    if !status.project_readiness.ok {
        return Err(format!(
            "doctor_project_readiness_failed state={} blocked={}",
            status.project_readiness.overall_state, status.project_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "project_readiness",
        &format!(
            "state={} ready={} partial={} deferred={} blocked={}",
            status.project_readiness.overall_state,
            status.project_readiness.ready_count,
            status.project_readiness.partial_count,
            status.project_readiness.deferred_count,
            status.project_readiness.blocked_count
        ),
    ));
    if !status.release_readiness.ok {
        return Err(format!(
            "doctor_release_readiness_failed state={} blocked={}",
            status.release_readiness.overall_state, status.release_readiness.blocked_count
        ));
    }
    checks.push(pass(
        "release_readiness",
        &format!(
            "name={} state={} ready={} partial={} deferred={} blocked={}",
            status.release_readiness.release_name,
            status.release_readiness.overall_state,
            status.release_readiness.ready_count,
            status.release_readiness.partial_count,
            status.release_readiness.deferred_count,
            status.release_readiness.blocked_count
        ),
    ));
    checks.push(pass(
        "third_test_candidate",
        &format!(
            "state={} local_gate_ready={} smoke_script={} marker={} requires_manual_live_check={} connects_real_external_services={} operator_env_blocks_100_percent={} real_live_ready={}",
            status.third_test_candidate.overall_state,
            status.third_test_candidate.local_gate_ready,
            status.third_test_candidate.smoke_script,
            status.third_test_candidate.marker,
            status.third_test_candidate.requires_manual_live_check,
            status.third_test_candidate.connects_real_external_services,
            status.third_test_candidate.operator_env_blocks_100_percent,
            status.third_test_candidate.real_live_ready
        ),
    ));

    slots
        .actuator
        .observe(ObserveTarget::Screen)
        .map_err(|e| format!("doctor_actuator_observe_failed: {}", e.message))?;
    checks.push(pass("actuator_smoke", "actuator observe completed"));

    slots
        .control_plane
        .try_list_units()
        .map_err(|e| format!("doctor_control_plane_list_failed: {e:?}"))?;
    checks.push(pass("control_plane_smoke", "control plane list completed"));

    run_isolated_runtime_smoke(runtime)?;
    checks.push(pass("runtime_smoke", "isolated runtime turn completed"));

    run_isolated_subagent_queue_smoke()?;
    checks.push(pass(
        "subagent_queue_smoke",
        "isolated queued dispatch completed",
    ));

    run_plugin_registry_check()?;
    checks.push(pass("plugin_registry", "plugin registry check completed"));

    Ok(DoctorCliOutput {
        ok: checks.iter().all(|check| check.ok),
        checks,
        status,
    })
}

fn run_atomic_tool_manifest_check() -> Result<(), String> {
    let manifests = ga_atomic_tool_manifests();
    if manifests.len() != 9 {
        return Err(format!(
            "doctor_atomic_tools_failed expected=9 actual={}",
            manifests.len()
        ));
    }

    let mapped = manifests
        .iter()
        .filter(|tool| tool.status == AtomicToolStatus::Mapped)
        .map(|tool| tool.kind)
        .collect::<Vec<_>>();
    let expected_mapped = vec![
        AtomicToolKind::Mouse,
        AtomicToolKind::Keyboard,
        AtomicToolKind::Screenshot,
        AtomicToolKind::Locate,
        AtomicToolKind::FileRead,
        AtomicToolKind::FileWrite,
        AtomicToolKind::CodeExecute,
        AtomicToolKind::Wait,
        AtomicToolKind::HumanSuspend,
    ];
    if mapped != expected_mapped {
        return Err(format!(
            "doctor_atomic_tools_failed mapped={mapped:?} expected={expected_mapped:?}"
        ));
    }

    let interface_only = manifests
        .iter()
        .filter(|tool| tool.status == AtomicToolStatus::InterfaceOnly)
        .map(|tool| tool.kind)
        .collect::<Vec<_>>();
    let expected_interface_only = Vec::<AtomicToolKind>::new();
    if interface_only != expected_interface_only {
        return Err(format!(
            "doctor_atomic_tools_failed interface_only={interface_only:?} expected={expected_interface_only:?}"
        ));
    }

    if AtomicToolManifest::schema_version() != 1
        || AtomicToolManifest::schema_fields()
            != [
                "kind",
                "name",
                "source",
                "status",
                "implementation",
                "description",
            ]
    {
        return Err("doctor_atomic_tools_failed invalid manifest schema".to_string());
    }

    if ToolActionEnvelope::schema_version() != 1
        || ToolActionEnvelope::schema_fields() != ["schema_version", "type", "call", "answer"]
        || ToolActionEnvelope::call_schema_fields()
            != [
                "tool",
                "path",
                "content",
                "x",
                "y",
                "text",
                "secret",
                "target",
                "app_name",
                "millis",
                "reason",
                "prompt",
                "patch",
                "command",
                "cwd",
                "query",
                "session_id",
                "limit",
                "task",
                "agent_name",
                "policy",
                "token_budget",
                "timeout_ms",
            ]
    {
        return Err("doctor_atomic_tools_failed invalid tool action schema".to_string());
    }

    if ToolLoopReport::schema_version() != 6
        || ToolLoopReport::schema_fields()
            != [
                "schema_version",
                "status",
                "workspace_root",
                "rounds",
                "call_count",
                "calls",
            ]
        || ToolLoopReport::call_schema_fields()
            != [
                "call",
                "tool_name",
                "atomic_tool_name",
                "ok",
                "summary",
                "decision",
                "duration_ms",
                "retryable",
                "target_path",
                "resolved_path",
                "cwd",
                "command",
                "entries",
                "output_bytes",
                "output_lines",
                "stderr_bytes",
                "stderr_lines",
                "output",
                "stdout",
                "stderr",
                "exit_code",
                "changed_files",
                "write_before_bytes",
                "write_after_bytes",
                "write_changed",
                "write_operation",
                "write_diff_preview",
                "write_diff_truncated",
                "failure_class",
                "output_redacted",
                "stdout_redacted",
                "stderr_redacted",
                "output_truncated",
                "stdout_truncated",
                "stderr_truncated",
            ]
    {
        return Err("doctor_atomic_tools_failed invalid tool report schema".to_string());
    }

    Ok(())
}

fn run_plugin_registry_check() -> Result<(), String> {
    let path = PathBuf::from("plugins/registry.example.json");
    if !path.exists() {
        return Ok(());
    }
    let check = check_plugin_registry(&path)?;
    if check.ok {
        Ok(())
    } else {
        Err(format!(
            "doctor_plugin_registry_failed path={} plugin_count={}",
            check.registry_path, check.plugin_count
        ))
    }
}

fn run_goal_mode_check() -> Result<(), String> {
    let goal = GoalSpec::mainline_mvp("doctor goal probe");
    goal.validate().map_err(|e| {
        format!(
            "doctor_goal_mode_failed field={} message={}",
            e.field, e.message
        )
    })?;
    let segment = goal.render_context_segment().map_err(|e| {
        format!(
            "doctor_goal_mode_failed field={} message={}",
            e.field, e.message
        )
    })?;
    if segment.content.contains("GOAL_SPEC")
        && segment.metadata.get("kind").map(String::as_str) == Some("goal_spec")
    {
        Ok(())
    } else {
        Err("doctor_goal_mode_failed invalid context segment".to_string())
    }
}

fn run_identity_experiences_check(runtime: &RuntimeConfig) -> Result<(), String> {
    let config = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let store = chuang_agent::hermes_memory::FileDualFileMemoryStore::open(config)
        .map_err(|e| format!("doctor_identity_experiences_failed: {e:?}"))?;
    let experiences_path = store.config().experiences_path();
    if experiences_path.file_name().and_then(|name| name.to_str()) != Some("experiences.md") {
        return Err(format!(
            "doctor_identity_experiences_failed unexpected_path={}",
            experiences_path.display()
        ));
    }
    if !experiences_path.exists() {
        return Err(format!(
            "doctor_identity_experiences_failed missing_path={}",
            experiences_path.display()
        ));
    }
    Ok(())
}

fn run_isolated_runtime_smoke(runtime: &RuntimeConfig) -> Result<(), String> {
    let mut smoke_runtime = runtime.clone();
    let root = unique_doctor_root()?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("doctor_workspace_create_failed: {error}"))?;
    smoke_runtime.db_path = root.join("memory.db");
    smoke_runtime.permission.workspace_root = root.clone();
    smoke_runtime.identity_memory =
        chuang_agent::runtime_config::IdentityMemoryConfig::HermesDualFile {
            root: root.join("identity"),
            user_max_chars: chuang_agent::hermes_memory::DEFAULT_USER_MEMORY_MAX_CHARS,
            memory_max_chars: chuang_agent::hermes_memory::DEFAULT_HOT_MEMORY_MAX_CHARS,
        };
    smoke_runtime.provider = ProviderConfig::Fake {
        provider_id: "doctor-fake".to_string(),
        model_name: "doctor-smoke".to_string(),
    };

    run_with_options(&RunCliRequest {
        options: CliOptions {
            runtime: smoke_runtime,
        },
        user_input: "doctor smoke".to_string(),
        workspace_root: Some(root),
        remember: false,
        session_id: None,
        remember_session: false,
        conversation_history: Vec::new(),
        remember_identity: false,
        remember_experience: false,
        dispatch_subagent: false,
        goal_spec: None,
        knowledge_context: None,
        live_guidance_path: None,
        progress_path: None,
    })
    .map(|_| ())
}

fn unique_doctor_root() -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "chuang-agent-doctor-{}-{nanos}",
        std::process::id()
    )))
}

fn run_isolated_subagent_queue_smoke() -> Result<(), String> {
    let queue_root = unique_doctor_root()?.join("queue");
    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(queue_root))
        .map_err(|e| format!("doctor_subagent_queue_open_failed: {e:?}"))?;
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(SpawnRequest {
            task_id: TaskId("doctor-subagent-smoke".to_string()),
            parent_agent_id: AgentId("doctor".to_string()),
            agent_name: "doctor-worker".to_string(),
            task: "doctor queued dispatch smoke".to_string(),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 128,
            idle_timeout_ms: 10_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .map_err(|e| format!("doctor_subagent_spawn_failed: {e:?}"))?;
    queue
        .flush_pending_dispatches(&spawner)
        .map_err(|e| format!("doctor_subagent_dispatch_write_failed: {e:?}"))?;
    let dispatch = queue
        .read_dispatch(&receipt.run_id)
        .map_err(|e| format!("doctor_subagent_dispatch_read_failed: {e:?}"))?
        .ok_or_else(|| "doctor_subagent_dispatch_missing".to_string())?;

    if dispatch.task_id.0 != "doctor-subagent-smoke" {
        return Err("doctor_subagent_dispatch_mismatch".to_string());
    }

    Ok(())
}

fn pass(name: &str, detail: &str) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        ok: true,
        detail: detail.to_string(),
    }
}

fn print_doctor(doctor: &DoctorCliOutput) {
    println!("doctor_ok: {}", doctor.ok);
    for check in &doctor.checks {
        println!(
            "doctor_check name={} ok={} detail={}",
            check.name, check.ok, check.detail
        );
    }
    println!("provider: {}", doctor.status.config.provider_kind);
    println!("model: {}", doctor.status.config.model_name);
    println!(
        "tool_shell_rtk_rewrite: {}",
        doctor.status.config.tool_shell_rtk_rewrite
    );
    println!(
        "browser_cdp: available={} state={} kind={} reason_code={} reason={}",
        doctor
            .status
            .browser_readiness
            .browser_read_adapter_available,
        doctor.status.browser_readiness.browser_read_state,
        doctor.status.browser_readiness.browser_read_adapter_kind,
        doctor.status.browser_readiness.browser_read_reason_code,
        doctor.status.browser_readiness.browser_read_reason
    );
    if !doctor
        .status
        .browser_readiness
        .browser_read_adapter_available
    {
        println!(
            "browser_cdp_next: chuang browser start   # 或依赖工具自动拉起；关自动: CHUANG_HEADLESS_AUTOSTART=0"
        );
    }
    if doctor.status.config.tool_shell_rtk_rewrite && which_rtk_for_doctor().is_none() {
        println!(
            "shell_rtk_next: install rtk on PATH or set RTK_BIN; or set tool_shell_rtk_rewrite=false"
        );
    }
    println!(
        "field_accept_next: chuang field-accept   # 本机 15 项快速验收；SKIP_LIVE=1 可跳过模型"
    );
    println!(
        "context_engine: {}",
        doctor.status.config.context_engine_kind
    );
    println!("subagent: {}", doctor.status.config.subagent_kind);
    println!(
        "subagent_live_worker: enabled={} adapter_kind={} status={} starts_worker={} available={} reason={}",
        doctor.status.config.subagent_live_worker.enabled,
        doctor.status.config.subagent_live_worker.adapter_kind,
        doctor.status.config.subagent_live_worker.status,
        doctor.status.config.subagent_live_worker.starts_worker,
        doctor.status.config.subagent_live_worker.available,
        doctor.status.config.subagent_live_worker.reason
    );
    println!("execution: {}", doctor.status.slots.execution);
    println!(
        "governance_readiness: ok={} kind={} rules_loaded={} tool_surface_governed={} goal_run_executes={}",
        doctor.status.governance.ok,
        doctor.status.governance.kind,
        doctor.status.governance.rules_loaded,
        doctor.status.governance.tool_surface_governed,
        doctor.status.governance.goal_run_executes
    );
    println!(
        "governance_decisions: read_only={} dangerous_write={} dangerous_shell={} secret_shell={}",
        doctor.status.governance.read_only_decision,
        doctor.status.governance.dangerous_write_decision,
        doctor.status.governance.dangerous_shell_decision,
        doctor.status.governance.secret_shell_decision
    );
    println!(
        "policy_tool_status: active_profile={} normal_local_action_default={} high_risk_boundary={} ga_tool_descriptors={}/{} missing={}",
        doctor.status.policy_tool_status.active_permission_profile,
        doctor.status.policy_tool_status.local_ga_normal_local_action_default,
        doctor
            .status
            .policy_tool_status
            .local_ga_high_risk_boundary_summary,
        doctor.status.policy_tool_status.ga_tool_descriptor_mapped_count,
        doctor.status.policy_tool_status.tool_descriptor_count,
        doctor.status.policy_tool_status.ga_tool_descriptor_missing.len()
    );
    println!(
        "runtime_report_surface: ok={} artifacts={} observability_fields={} artifact_locators={} observability={}",
        doctor.status.runtime_report_surface.ok,
        doctor.status.runtime_report_surface.artifact_count,
        doctor.status.runtime_report_surface.observability_field_count,
        format_name_list(&doctor.status.runtime_report_surface.artifact_locators),
        format_name_list(&doctor.status.runtime_report_surface.observability_fields)
    );
    println!(
        "provider_readiness: ok={} state={} kind={} transport={} fallback_configured={} timeout_ms={} api_key_state={} placeholder_warnings={}",
        doctor.status.provider_readiness.ok,
        doctor.status.provider_readiness.overall_state,
        doctor.status.provider_readiness.provider_kind,
        doctor.status.provider_readiness.transport,
        doctor.status.provider_readiness.fallback_configured,
        doctor
            .status
            .provider_readiness
            .request_timeout_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        doctor
            .status
            .provider_readiness
            .api_key_state
            .as_deref()
            .unwrap_or("none"),
        doctor.status.provider_readiness.placeholder_warning_count
    );
    println!(
        "provider_readiness_current: {}",
        doctor.status.provider_readiness.current
    );
    println!(
        "provider_readiness_next_action: {}",
        doctor.status.provider_readiness.next_action
    );
    println!(
        "runtime_capability_primer: {}",
        doctor.status.runtime_capability_primer
    );
    println!(
        "atomic_tools_ok: {} manifest_schema_version={} action_schema_version={} report_schema_version={}",
        doctor.status.atomic_tools.ok,
        doctor.status.atomic_tools.manifest_schema_version,
        doctor.status.atomic_tools.tool_action_schema_version,
        doctor.status.atomic_tools.tool_report_schema_version
    );
    println!(
        "atomic_tools_mapped: {}",
        format_name_list(&doctor.status.atomic_tools.mapped_atomic_tool_names)
    );
    println!(
        "atomic_tools_executable: {}",
        format_name_list(
            &doctor
                .status
                .atomic_tools
                .governed_executable_atomic_tool_names
        )
    );
    println!(
        "atomic_tools_interface_only: {} (local surface only; live adapters separate)",
        format_name_list(&doctor.status.atomic_tools.interface_only_atomic_tool_names)
    );
    println!(
        "atomic_tools_desktop_browser_interface_only: {} reason={}",
        format_name_list(
            &doctor
                .status
                .atomic_tools
                .desktop_browser_interface_only_atomic_tool_names
        ),
        doctor.status.atomic_tools.interface_only_reason
    );
    println!(
        "atomic_tools_desktop_browser_live_gated: {} required=adapter,live_gate,allowlist,audit_receipt",
        format_name_list(
            &doctor
                .status
                .atomic_tools
                .desktop_browser_live_gated_atomic_tool_names
        )
    );
    println!(
        "atomic_tools_self_check_entrypoints: {}",
        format_name_list(&doctor.status.atomic_tools.local_cli_self_check_entrypoints)
    );
    println!(
        "goal_mode: ok={} entrypoint={} kind={} context_source={} default_goal_id={} allowed_slots={} checkpoint_policy=progress_log:{} handoff:{} commit:{} final_report_policy=validation:{} next_steps:{} bypasses_governance={} adds_core_slot={}",
        doctor.status.goal_mode.ok,
        doctor.status.goal_mode.cli_entrypoint,
        doctor.status.goal_mode.kind,
        doctor.status.goal_mode.context_source,
        doctor.status.goal_mode.default_goal_id,
        format_name_list(&doctor.status.goal_mode.default_allowed_slots),
        doctor.status.goal_mode.checkpoint_policy.update_progress_log,
        doctor.status.goal_mode.checkpoint_policy.update_handoff,
        doctor.status.goal_mode.checkpoint_policy.commit_checkpoint,
        doctor.status.goal_mode.final_report_policy.include_validation,
        doctor.status.goal_mode.final_report_policy.include_next_steps,
        doctor.status.goal_mode.bypasses_governance,
        doctor.status.goal_mode.adds_core_slot
    );
    println!(
        "goal_run_ok: {} plan_exists={} goal_id={} checkpoints={} workers={} validation_commands={} path={}",
        doctor.status.goal_run.ok,
        doctor.status.goal_run.plan_exists,
        doctor.status.goal_run.goal_id,
        doctor.status.goal_run.checkpoint_count,
        doctor.status.goal_run.worker_count,
        doctor.status.goal_run.validation_command_count,
        doctor.status.goal_run.path
    );
    println!(
        "goal_run_checkpoint_log_complete: {}",
        doctor.status.goal_run.checkpoint_log_complete
    );
    println!(
        "goal_run_last_checkpoint: {}",
        doctor
            .status
            .goal_run
            .last_checkpoint_id
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "goal_run_last_checkpoint_summary: {}",
        doctor
            .status
            .goal_run
            .last_checkpoint_summary
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "goal_run_last_checkpoint_created_at: {}",
        doctor
            .status
            .goal_run
            .last_checkpoint_created_at
            .as_deref()
            .unwrap_or("none")
    );
    println!(
        "goal_run_last_checkpoint_completed_worker_ids: {}",
        doctor
            .status
            .goal_run
            .last_checkpoint_completed_worker_ids
            .as_ref()
            .map(|values| values.join(","))
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "goal_run_last_checkpoint_validation_notes: {}",
        doctor
            .status
            .goal_run
            .last_checkpoint_validation_notes
            .as_ref()
            .map(|values| values.join(" | "))
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "goal_run_incomplete_reasons: {}",
        format_text_list(&doctor.status.goal_run.incomplete_reasons)
    );
    println!(
        "project_readiness: ok={} state={} ready={} partial={} deferred={} blocked={}",
        doctor.status.project_readiness.ok,
        doctor.status.project_readiness.overall_state,
        doctor.status.project_readiness.ready_count,
        doctor.status.project_readiness.partial_count,
        doctor.status.project_readiness.deferred_count,
        doctor.status.project_readiness.blocked_count
    );
    println!(
        "local_contract_readiness: ok={} state={} contracts={} ready={} partial={} deferred={} blocked={} connects_real_external_services={} writes_core_memory={} executes_plugins={}",
        doctor.status.local_contract_readiness.ok,
        doctor.status.local_contract_readiness.overall_state,
        doctor.status.local_contract_readiness.contract_count,
        doctor.status.local_contract_readiness.ready_count,
        doctor.status.local_contract_readiness.partial_count,
        doctor.status.local_contract_readiness.deferred_count,
        doctor.status.local_contract_readiness.blocked_count,
        doctor
            .status
            .local_contract_readiness
            .connects_real_external_services,
        doctor.status.local_contract_readiness.writes_core_memory,
        doctor.status.local_contract_readiness.executes_plugins
    );
    for contract in &doctor.status.local_contract_readiness.contracts {
        if contract.name == "knowledge_context_preview"
            || contract.name == "external_knowledge_source_contracts"
        {
            println!(
                "local_contract_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
            );
        }
    }
    println!(
        "release_readiness: ok={} name={} state={} ready={} partial={} deferred={} blocked={}",
        doctor.status.release_readiness.ok,
        doctor.status.release_readiness.release_name,
        doctor.status.release_readiness.overall_state,
        doctor.status.release_readiness.ready_count,
        doctor.status.release_readiness.partial_count,
        doctor.status.release_readiness.deferred_count,
        doctor.status.release_readiness.blocked_count
    );
    println!(
        "release_acceptance: count={} ready={} partial={} deferred={} connects_real_external_services={} verifies_real_external_services={} uses_stub_or_local_fixtures={} writes_repo_files={}",
        doctor.status.release_readiness.acceptance_count,
        doctor.status.release_readiness.acceptance_ready_count,
        doctor.status.release_readiness.acceptance_partial_count,
        doctor.status.release_readiness.acceptance_deferred_count,
        doctor.status.release_readiness.connects_real_external_services,
        doctor.status.release_readiness.verifies_real_external_services,
        doctor.status.release_readiness.uses_stub_or_local_fixtures,
        doctor.status.release_readiness.writes_repo_files
    );
    println!(
        "third_test_candidate: ok={} state={} local_gate_ready={} smoke_script={} marker={} requires_manual_live_check={} connects_real_external_services={} operator_env_blocks_100_percent={} real_live_ready={}",
        doctor.status.third_test_candidate.ok,
        doctor.status.third_test_candidate.overall_state,
        doctor.status.third_test_candidate.local_gate_ready,
        doctor.status.third_test_candidate.smoke_script,
        doctor.status.third_test_candidate.marker,
        doctor.status.third_test_candidate.requires_manual_live_check,
        doctor.status.third_test_candidate.connects_real_external_services,
        doctor.status.third_test_candidate.operator_env_blocks_100_percent,
        doctor.status.third_test_candidate.real_live_ready
    );
    println!(
        "memory_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        doctor.status.memory_readiness.ok,
        doctor.status.memory_readiness.overall_state,
        doctor.status.memory_readiness.layer_count,
        doctor.status.memory_readiness.ready_count,
        doctor.status.memory_readiness.partial_count,
        doctor.status.memory_readiness.deferred_count,
        doctor.status.memory_readiness.blocked_count
    );
    for layer in &doctor.status.memory_readiness.layers {
        if layer.name == "external_knowledge" {
            println!(
                "memory_layer_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
            );
        }
    }
    println!(
        "channel_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        doctor.status.channel_readiness.ok,
        doctor.status.channel_readiness.overall_state,
        doctor.status.channel_readiness.layer_count,
        doctor.status.channel_readiness.ready_count,
        doctor.status.channel_readiness.partial_count,
        doctor.status.channel_readiness.deferred_count,
        doctor.status.channel_readiness.blocked_count
    );
    println!(
        "subagent_readiness: ok={} state={} mode={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} layers={} ready={} partial={} deferred={} blocked={} live_worker_available={} worker_runtime_state={} worker_runtime_blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={}",
        doctor.status.subagent_readiness.ok,
        doctor.status.subagent_readiness.overall_state,
        doctor.status.subagent_readiness.mode,
        doctor.status.subagent_readiness.local_contract_ready,
        doctor.status.subagent_readiness.local_contract_state,
        doctor.status.subagent_readiness.live_adapter_ready,
        doctor.status.subagent_readiness.live_adapter_state,
        doctor.status.subagent_readiness.layer_count,
        doctor.status.subagent_readiness.ready_count,
        doctor.status.subagent_readiness.partial_count,
        doctor.status.subagent_readiness.deferred_count,
        doctor.status.subagent_readiness.blocked_count,
        doctor.status.subagent_readiness.live_worker_available,
        doctor.status.subagent_readiness.worker_runtime_state,
        doctor.status.subagent_readiness.worker_runtime_blocked_reason,
        doctor.status.subagent_readiness.capability_route_state,
        doctor.status.subagent_readiness.capability_mismatch_blocks_live,
        doctor.status.subagent_readiness.capability_mismatch_reason
    );
    println!(
        "subagent_worker_runtime_reason: {}",
        doctor.status.subagent_readiness.worker_runtime_reason
    );
    println!(
        "subagent_model_tool_worker: available={} state={} reason={}",
        doctor.status.subagent_readiness.model_tool_worker_available,
        doctor.status.subagent_readiness.model_tool_worker_state,
        doctor.status.subagent_readiness.model_tool_worker_reason
    );
    println!(
        "subagent_capability_mismatch_reason: {}",
        doctor.status.subagent_readiness.capability_mismatch_reason
    );
    println!(
        "subagent_readiness_local_contract_reason: {}",
        doctor.status.subagent_readiness.local_contract_reason
    );
    println!(
        "subagent_readiness_live_adapter_reason: {}",
        doctor.status.subagent_readiness.live_adapter_reason
    );
    for layer in &doctor.status.subagent_readiness.layers {
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
        doctor.status.live_adapter_gates.ok,
        doctor.status.live_adapter_gates.overall_state,
        doctor.status.live_adapter_gates.gate_count,
        doctor.status.live_adapter_gates.enabled_count,
        doctor.status.live_adapter_gates.disabled_count
    );
    for gate in &doctor.status.live_adapter_gates.gates {
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
        "live_readiness: ok={} state={} local_ready_scope={} ga_local_mapped_only={} desktop_browser_live_gated={} browser_worker_frozen={} live_worker_available={} real_external_acceptance_pending={} provider_live_request_verified_by_status={} mapped_does_not_mean_live={} gated_does_not_mean_ready={} frozen_does_not_mean_ready={} ready_does_not_mean_live={}",
        doctor.status.live_readiness.ok,
        doctor.status.live_readiness.overall_state,
        doctor.status.live_readiness.local_ready_scope,
        doctor.status.live_readiness.ga_local_mapped_only,
        doctor.status.live_readiness.desktop_browser_live_gated,
        doctor.status.live_readiness.browser_worker_frozen,
        doctor.status.live_readiness.live_worker_available,
        doctor.status.live_readiness.real_external_acceptance_pending,
        doctor.status.live_readiness.provider_live_request_verified_by_status,
        doctor.status.live_readiness.mapped_does_not_mean_live,
        doctor.status.live_readiness.gated_does_not_mean_ready,
        doctor.status.live_readiness.frozen_does_not_mean_ready,
        doctor.status.live_readiness.ready_does_not_mean_live
    );
    println!(
        "external_ai_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        doctor.status.external_ai_readiness.ok,
        doctor.status.external_ai_readiness.overall_state,
        doctor.status.external_ai_readiness.layer_count,
        doctor.status.external_ai_readiness.ready_count,
        doctor.status.external_ai_readiness.partial_count,
        doctor.status.external_ai_readiness.deferred_count,
        doctor.status.external_ai_readiness.blocked_count
    );
    println!(
        "identity_bootstrap_present: soul={} story={} first_wake={} agents={}",
        doctor
            .status
            .kernel
            .identity_soul_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        doctor
            .status
            .kernel
            .identity_story_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        doctor
            .status
            .kernel
            .identity_first_wake_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        doctor
            .status
            .kernel
            .identity_agents_registry_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("control_plane: {}", doctor.status.slots.control_plane);
    if doctor.status.config.placeholder_warnings.is_empty() {
        println!("placeholder_warnings: none");
    } else {
        for warning in &doctor.status.config.placeholder_warnings {
            println!("placeholder_warning: {warning}");
        }
    }
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

fn which_rtk_for_doctor() -> Option<String> {
    if let Ok(path) = std::env::var("RTK_BIN") {
        let p = std::path::PathBuf::from(&path);
        if p.is_file() {
            return Some(path);
        }
    }
    if let Ok(output) = std::process::Command::new("sh")
        .args(["-c", "command -v rtk"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}
