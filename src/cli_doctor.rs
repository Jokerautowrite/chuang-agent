use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::actuator::{Actuator, ObserveTarget};
use chuang_agent::atomic_tool::{ga_atomic_tool_manifests, AtomicToolKind, AtomicToolStatus};
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

    let mut slots = build_runtime_slots(runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    checks.push(pass("slots", "runtime slots build"));

    run_atomic_tool_manifest_check()?;
    checks.push(pass(
        "atomic_tools",
        "GenericAgent 9 atomic tool manifest is available",
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

    if ToolActionEnvelope::schema_version() != 1
        || ToolActionEnvelope::schema_fields() != ["schema_version", "type", "call", "answer"]
        || ToolActionEnvelope::call_schema_fields() != ["tool", "path", "content", "command", "cwd"]
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
        "atomic_tools_ok: {} action_schema_version={} report_schema_version={}",
        doctor.status.atomic_tools.ok,
        doctor.status.atomic_tools.tool_action_schema_version,
        doctor.status.atomic_tools.tool_report_schema_version
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
