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
            "state={} mode={} layers={} ready={} partial={} deferred={} blocked={}",
            status.subagent_readiness.overall_state,
            status.subagent_readiness.mode,
            status.subagent_readiness.layer_count,
            status.subagent_readiness.ready_count,
            status.subagent_readiness.partial_count,
            status.subagent_readiness.deferred_count,
            status.subagent_readiness.blocked_count
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

    let mut slots = build_runtime_slots(runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    checks.push(pass("slots", "runtime slots build"));

    run_atomic_tool_manifest_check()?;
    checks.push(pass(
        "atomic_tools",
        "GenericAgent 9 atomic tool manifest is available",
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
        "lightweight goal context wrapper is available",
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
            "goal_id={} plan_exists={} checkpoint_count={}",
            status.goal_run.goal_id, status.goal_run.plan_exists, status.goal_run.checkpoint_count
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
        AtomicToolKind::FileRead,
        AtomicToolKind::FileWrite,
        AtomicToolKind::CodeExecute,
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
    let expected_interface_only = vec![
        AtomicToolKind::Mouse,
        AtomicToolKind::Keyboard,
        AtomicToolKind::Screenshot,
        AtomicToolKind::Locate,
        AtomicToolKind::Wait,
        AtomicToolKind::HumanSuspend,
    ];
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
                "millis",
                "patch",
                "command",
                "cwd",
                "query",
                "session_id",
                "limit",
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
    smoke_runtime.db_path = root.join("memory.db");
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
        remember_identity: false,
        remember_experience: false,
        dispatch_subagent: false,
        goal_spec: None,
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
        "context_engine: {}",
        doctor.status.config.context_engine_kind
    );
    println!("subagent: {}", doctor.status.config.subagent_kind);
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
        "atomic_tools_interface_only: {}",
        format_name_list(&doctor.status.atomic_tools.interface_only_atomic_tool_names)
    );
    println!(
        "goal_mode_ok: {} entrypoint={} kind={}",
        doctor.status.goal_mode.ok,
        doctor.status.goal_mode.cli_entrypoint,
        doctor.status.goal_mode.kind
    );
    println!(
        "goal_run_ok: {} plan_exists={} goal_id={} checkpoints={} path={}",
        doctor.status.goal_run.ok,
        doctor.status.goal_run.plan_exists,
        doctor.status.goal_run.goal_id,
        doctor.status.goal_run.checkpoint_count,
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
        "memory_readiness: ok={} state={} layers={} ready={} partial={} deferred={} blocked={}",
        doctor.status.memory_readiness.ok,
        doctor.status.memory_readiness.overall_state,
        doctor.status.memory_readiness.layer_count,
        doctor.status.memory_readiness.ready_count,
        doctor.status.memory_readiness.partial_count,
        doctor.status.memory_readiness.deferred_count,
        doctor.status.memory_readiness.blocked_count
    );
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
        "subagent_readiness: ok={} state={} mode={} local_contract_ready={} live_adapter_ready={} layers={} ready={} partial={} deferred={} blocked={}",
        doctor.status.subagent_readiness.ok,
        doctor.status.subagent_readiness.overall_state,
        doctor.status.subagent_readiness.mode,
        doctor.status.subagent_readiness.local_contract_ready,
        doctor.status.subagent_readiness.live_adapter_ready,
        doctor.status.subagent_readiness.layer_count,
        doctor.status.subagent_readiness.ready_count,
        doctor.status.subagent_readiness.partial_count,
        doctor.status.subagent_readiness.deferred_count,
        doctor.status.subagent_readiness.blocked_count
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
