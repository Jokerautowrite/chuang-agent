use std::collections::BTreeMap;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::chuang_kernel::{
    ChuangKernel, ChuangKernelConfig, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::common::{AgentId, TaskId};
use chuang_agent::control_intent::{parse_control_intent, ControlIntentError, ControlIntentInput};
use chuang_agent::control_plane::{ControlPlane, ManagedUnit};
use chuang_agent::control_surface::{
    list_control_surface_units, run_control_surface_intent, ControlSurfaceError,
    ControlSurfaceRequest,
};
use chuang_agent::control_workflow::{
    build_decision_view, ControlUnitView, ControlWorkflowError, ControlWorkflowView,
};
use chuang_agent::hermes_memory::{
    DualFileMemoryStore, FileDualFileMemoryStore, HotMemoryEntry, DEFAULT_USER_MEMORY_MAX_CHARS,
};
use chuang_agent::kernel_status::{build_chuang_mvp_status, ChuangMvpStatus};
use chuang_agent::memory_store::MemoryStore;
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::ProviderTransport;
use chuang_agent::runtime_config::{
    IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig, RuntimeConfig, SubagentConfig,
    SubagentQueueConfig,
};
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::subagent_queue::FileSubagentQueue;
use chuang_agent::subagent_spawner::{
    ContextIsolation, QueuedSubagentSpawner, RunId, SpawnRequest, SubagentSpawner,
    SubagentToolPolicy,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    runtime: RuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunCliRequest {
    options: CliOptions,
    user_input: String,
    remember: bool,
    remember_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RememberedRecords {
    sqlite_record_id: Option<String>,
    identity_record_id: Option<String>,
}

fn main() {
    if let Err(message) = run_cli() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("run") => run_command(&args[2..]),
        Some("repl") => repl_command(&args[2..]),
        Some("status") => status_command(&args[2..]),
        Some("control") => control_command(&args[2..]),
        Some("subagent") => subagent_command(&args[2..]),
        _ => Err(usage()),
    }
}

fn run_command(args: &[String]) -> Result<(), String> {
    let request = parse_run_request(args)?;
    let (result, memory_records) = run_with_options(&request)?;
    print_runtime_result(&result);
    if let Some(record_id) = memory_records.sqlite_record_id {
        println!("memory_recorded: {record_id}");
    }
    if let Some(record_id) = memory_records.identity_record_id {
        println!("identity_memory_recorded: {record_id}");
    }
    Ok(())
}

fn repl_command(args: &[String]) -> Result<(), String> {
    let options = parse_cli_options(args)?;

    println!("chuang-agent repl ready (输入 exit 退出)");
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("stdin_read_failed: {e}"))?;
        let input = line.trim();
        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            break;
        }
        if input.is_empty() {
            continue;
        }

        let (result, _) = run_with_options(&RunCliRequest {
            options: options.clone(),
            user_input: input.to_string(),
            remember: false,
            remember_identity: false,
        })?;
        print_runtime_result(&result);
        writeln!(stdout, "---").map_err(|e| format!("stdout_write_failed: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    }

    Ok(())
}

fn status_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_cli_options(args)?;
    let kernel = kernel_config_from_runtime(&options.runtime)?;
    let status = build_chuang_mvp_status(&options.runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    match output {
        ControlOutputFormat::Text => print_status(&status),
        ControlOutputFormat::Json => print_json(&status)?,
    }

    Ok(())
}

fn control_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => control_list_command(&args[1..]),
        Some("apply") => control_apply_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn subagent_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("dispatch") => subagent_dispatch_command(&args[1..]),
        Some("report") => subagent_report_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn subagent_dispatch_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_dispatch(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let mut spawner = QueuedSubagentSpawner::new();
    let receipt = spawner
        .spawn(request.spawn)
        .map_err(|e| format!("subagent_spawn_failed: {e:?}"))?;
    let paths = queue
        .flush_pending_dispatches(&spawner)
        .map_err(|e| format!("subagent_dispatch_write_failed: {e:?}"))?;
    let dispatch_path = paths
        .first()
        .ok_or_else(|| "subagent_dispatch_write_failed: no dispatch was produced".to_string())?
        .clone();
    let output = SubagentDispatchCliOutput {
        run_id: receipt.run_id.0,
        agent_id: receipt.agent_id.0,
        task_id: request.task_id.0,
        dispatch_path: dispatch_path.display().to_string(),
        queue_root: request
            .options
            .runtime
            .subagent_queue
            .root
            .display()
            .to_string(),
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_dispatch_queued run_id={} agent_id={} task_id={} path={}",
                output.run_id, output.agent_id, output.task_id, output.dispatch_path
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_report_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_report(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let report = queue
        .read_report(&request.run_id)
        .map_err(|e| format!("subagent_report_read_failed: {e:?}"))?;
    let output = SubagentReportCliOutput {
        run_id: request.run_id.0.clone(),
        available: report.is_some(),
        report,
    };

    match request.output {
        ControlOutputFormat::Text => {
            if let Some(report) = &output.report {
                println!(
                    "subagent_report_available run_id={} status={:?} summary={}",
                    output.run_id, report.status, report.summary
                );
            } else {
                println!("subagent_report_missing run_id={}", output.run_id);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn control_list_command(args: &[String]) -> Result<(), String> {
    let output = parse_control_output(args)?;

    let options = CliOptions {
        runtime: RuntimeConfig::new(default_db_path()),
    };
    let slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    let views = list_control_surface_units(&slots.control_plane);
    match output {
        ControlOutputFormat::Text => {
            for unit in views {
                print_control_unit_view(&unit);
            }
        }
        ControlOutputFormat::Json => print_json(&views)?,
    }

    Ok(())
}

fn control_apply_command(args: &[String]) -> Result<(), String> {
    let request = parse_control_apply(args)?;
    let options = CliOptions {
        runtime: RuntimeConfig::new(default_db_path()),
    };
    let mut slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let unit_key = request
        .intent
        .unit_id
        .as_deref()
        .ok_or_else(|| "control apply requires --unit".to_string())?;
    let unit = find_control_unit(&slots.control_plane, unit_key)?;
    let result = match run_control_surface_intent(
        &mut slots.control_plane,
        &mut slots.governance,
        ControlSurfaceRequest {
            intent: request.intent,
            approved: request.approve,
        },
    ) {
        Ok(result) => result,
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::ApprovalRequired(decision))) => {
            print_control_view_with_format(&build_decision_view(&unit, &decision), request.output)?;
            return Err("control action requires --approve".to_string());
        }
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::NotAllowed(decision))) => {
            print_control_view_with_format(&build_decision_view(&unit, &decision), request.output)?;
            return Err("control action was not allowed by governance".to_string());
        }
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::Control(err))) => {
            return Err(format!("control_failed: {err:?}"))
        }
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::Governance(err))) => {
            return Err(format!("governance_failed: {}", err.message))
        }
        Err(ControlSurfaceError::Intent(err)) => return Err(control_intent_error_to_cli(err)),
    };

    print_control_view_with_format(&result.view, request.output)?;
    let receipt = result
        .receipt
        .ok_or_else(|| "control workflow returned no receipt".to_string())?;
    if request.output == ControlOutputFormat::Text {
        println!(
            "control_applied unit_id={} action={} previous={:?} next={:?} model={}",
            receipt.unit_id,
            receipt.action.as_str(),
            receipt.previous_status,
            receipt.next_status,
            receipt.model_name.as_deref().unwrap_or("none")
        );
    }

    Ok(())
}

fn find_control_unit<P: ControlPlane>(
    control_plane: &P,
    unit_id: &str,
) -> Result<ManagedUnit, String> {
    control_plane
        .list_units()
        .into_iter()
        .find(|unit| unit.unit_id == unit_id || unit.display_name == unit_id)
        .ok_or_else(|| format!("unknown control unit: {unit_id}"))
}

fn run_with_options(
    request: &RunCliRequest,
) -> Result<
    (
        chuang_agent::agent_runtime::RuntimeResult,
        RememberedRecords,
    ),
    String,
> {
    request
        .options
        .runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    match request.options.runtime.provider.build_openai_compatible() {
        Ok(Some(provider)) => {
            let mut store = SqliteMemoryStore::open(&request.options.runtime.db_path)
                .map_err(|e| format!("failed_to_open_db: {e:?}"))?;
            seed_default_memory_if_empty(&mut store)?;
            let mut kernel = ChuangKernel::with_responder(
                kernel_config_from_runtime(&request.options.runtime)?,
                store,
                provider,
            );
            kernel
                .run_turn(request.user_input.clone())
                .map_err(|e| format!("runtime_failed: {e:?}"))
                .and_then(|turn| {
                    remember_turn_if_requested(&request.options, &mut kernel, turn, request)
                })
        }
        Ok(None) => {
            let mut store = SqliteMemoryStore::open(&request.options.runtime.db_path)
                .map_err(|e| format!("failed_to_open_db: {e:?}"))?;
            seed_default_memory_if_empty(&mut store)?;
            let mut kernel =
                ChuangKernel::new(kernel_config_from_runtime(&request.options.runtime)?, store);
            kernel
                .run_turn(request.user_input.clone())
                .map_err(|e| format!("runtime_failed: {e:?}"))
                .and_then(|turn| {
                    remember_turn_if_requested(&request.options, &mut kernel, turn, request)
                })
        }
        Err(err) => Err(format!("config_invalid: {}: {}", err.field, err.message)),
    }
}

fn remember_turn_if_requested<S, R>(
    options: &CliOptions,
    kernel: &mut ChuangKernel<S, R>,
    turn: chuang_agent::chuang_kernel::ChuangKernelTurn,
    request: &RunCliRequest,
) -> Result<
    (
        chuang_agent::agent_runtime::RuntimeResult,
        RememberedRecords,
    ),
    String,
>
where
    S: MemoryStore,
    R: chuang_agent::responder::Responder,
{
    let mut records = RememberedRecords::default();

    if request.remember {
        records.sqlite_record_id = Some(
            kernel
                .remember_turn(&turn)
                .map_err(format_kernel_memory_error)?,
        );
    }

    if request.remember_identity {
        records.identity_record_id = Some(remember_identity_turn(options, &turn)?);
    }

    Ok((turn.result, records))
}

fn remember_identity_turn(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<String, String> {
    let dual_file_config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let mut store = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
    let entry_id = unique_identity_turn_id(&turn.turn_id)?;
    let content = format!(
        "user={}\nresponse={}\nsummary={}",
        turn.user_input, turn.result.response.body, turn.report.summary
    );
    store
        .append_memory(HotMemoryEntry {
            id: entry_id.clone(),
            content,
        })
        .map_err(format_identity_memory_error)?;
    Ok(entry_id)
}

fn unique_identity_turn_id(turn_id: &str) -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    Ok(format!(
        "identity-{}-{}-{}",
        turn_id,
        std::process::id(),
        nanos
    ))
}

fn format_identity_memory_error(err: chuang_agent::hermes_memory::DualFileMemoryError) -> String {
    match err {
        chuang_agent::hermes_memory::DualFileMemoryError::StorageUnavailable { path } => {
            format!("identity_memory_write_failed path={}", path.display())
        }
        chuang_agent::hermes_memory::DualFileMemoryError::DuplicateEntry { id } => {
            format!("identity_memory_duplicate_entry id={id}")
        }
        chuang_agent::hermes_memory::DualFileMemoryError::HardLimitExceeded {
            scope,
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "identity_memory_hard_limit_exceeded scope={scope:?} limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

fn format_kernel_memory_error(err: chuang_agent::chuang_kernel::ChuangKernelMemoryError) -> String {
    match err {
        chuang_agent::chuang_kernel::ChuangKernelMemoryError::Store(store_err) => {
            format!("memory_write_failed: {store_err:?}")
        }
        chuang_agent::chuang_kernel::ChuangKernelMemoryError::HardLimitExceeded {
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "memory_write_hard_limit_exceeded limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlApplyCliRequest {
    intent: ControlIntentInput,
    approve: bool,
    output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubagentDispatchCliRequest {
    options: CliOptions,
    output: ControlOutputFormat,
    task_id: TaskId,
    spawn: SpawnRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SubagentDispatchCliOutput {
    run_id: String,
    agent_id: String,
    task_id: String,
    dispatch_path: String,
    queue_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubagentReportCliRequest {
    options: CliOptions,
    output: ControlOutputFormat,
    run_id: RunId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SubagentReportCliOutput {
    run_id: String,
    available: bool,
    report: Option<chuang_agent::subagent_report::SubagentReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlOutputFormat {
    Text,
    Json,
}

fn parse_control_output(args: &[String]) -> Result<ControlOutputFormat, String> {
    let mut output = ControlOutputFormat::Text;
    for arg in args {
        match arg.as_str() {
            "--json" => output = ControlOutputFormat::Json,
            _ => return Err(usage()),
        }
    }
    Ok(output)
}

fn parse_status_output(args: &[String]) -> Result<ControlOutputFormat, String> {
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--db"
            | "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport"
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root" => index += 2,
            _ => return Err(usage()),
        }
    }
    Ok(output)
}

fn parse_control_apply(args: &[String]) -> Result<ControlApplyCliRequest, String> {
    let mut unit_id: Option<String> = None;
    let mut action: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut model_name: Option<String> = None;
    let mut approve = false;
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--unit" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --unit".to_string())?;
                unit_id = Some(value.clone());
                index += 2;
            }
            "--action" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --action".to_string())?;
                action = Some(value.clone());
                index += 2;
            }
            "--reason" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --reason".to_string())?;
                reason = Some(value.clone());
                index += 2;
            }
            "--model" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "control apply requires value after --model".to_string())?;
                model_name = Some(value.clone());
                index += 2;
            }
            "--approve" => {
                approve = true;
                index += 1;
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let intent = ControlIntentInput {
        unit_id,
        action,
        reason,
        model_name,
    };
    parse_control_intent(intent.clone()).map_err(control_intent_error_to_cli)?;

    Ok(ControlApplyCliRequest {
        intent,
        approve,
        output,
    })
}

fn control_intent_error_to_cli(error: ControlIntentError) -> String {
    match error {
        ControlIntentError::MissingUnit => "control apply requires --unit".to_string(),
        ControlIntentError::MissingAction => "control apply requires --action".to_string(),
        ControlIntentError::MissingReason => "control apply requires --reason".to_string(),
        ControlIntentError::MissingModel => "--model is required for change-model".to_string(),
        ControlIntentError::UnknownUnit(unit) => format!("unknown control unit: {unit}"),
        ControlIntentError::AmbiguousUnit(unit) => format!("ambiguous control unit: {unit}"),
        ControlIntentError::UnsupportedAction(action) => {
            format!("unsupported control action: {action}")
        }
    }
}

fn print_control_view(view: &ControlWorkflowView) {
    println!(
        "control_decision unit_id={} name={} decision={}",
        view.unit_id, view.display_name, view.decision
    );
    if view.audit_recorded {
        println!("control_audit: recorded");
    }
}

fn print_control_view_with_format(
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

fn print_control_unit_view(view: &ControlUnitView) {
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

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let rendered =
        serde_json::to_string_pretty(value).map_err(|e| format!("json_render_failed: {e}"))?;
    println!("{rendered}");
    Ok(())
}

fn print_status(status: &ChuangMvpStatus) {
    println!("kernel_agent_id: {}", status.kernel.agent_id);
    println!("kernel_turn_count: {}", status.kernel.turn_count);
    println!("provider: {}", status.config.provider_kind);
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
    println!("context_max_tokens: {}", status.config.context_max_tokens);
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

fn kernel_config_from_runtime(runtime: &RuntimeConfig) -> Result<ChuangKernelConfig, String> {
    let dual_file_config = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let identity_snapshot = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;

    Ok(ChuangKernelConfig {
        agent_id: "chuang-cli".to_string(),
        parent_agent_id: None,
        recall_limit: runtime.recall_limit,
        metadata: runtime.metadata.clone(),
        context_budget: Some(runtime.context_budget.clone()),
        memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        identity_snapshot: Some(identity_snapshot),
    })
}

fn parse_run_request(args: &[String]) -> Result<RunCliRequest, String> {
    let options = parse_cli_options(args)?;
    let mut user_input: Option<String> = None;
    let mut remember = false;
    let mut remember_identity = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--db" | "--identity-memory-root" | "--subagent" | "--subagent-queue-root" => {
                index += 2
            }
            "--input" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                user_input = Some(value.clone());
                index += 2;
            }
            "--remember" => {
                remember = true;
                index += 1;
            }
            "--remember-identity" => {
                remember_identity = true;
                index += 1;
            }
            "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport" => index += 2,
            _ => return Err(usage()),
        }
    }

    Ok(RunCliRequest {
        options,
        user_input: user_input.ok_or_else(usage)?,
        remember,
        remember_identity,
    })
}

fn parse_subagent_dispatch(args: &[String]) -> Result<SubagentDispatchCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut task: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut agent_name: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut token_budget: Option<u16> = None;
    let mut idle_timeout_ms: Option<u64> = None;
    let mut fork_parent_tokens: Option<u16> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--db"
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root"
            | "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                runtime_args.push(args[index].clone());
                runtime_args.push(value.clone());
                index += 2;
            }
            "--task" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent dispatch requires value after --task".to_string())?;
                task = Some(value.clone());
                index += 2;
            }
            "--task-id" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --task-id".to_string()
                })?;
                task_id = Some(value.clone());
                index += 2;
            }
            "--agent-name" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --agent-name".to_string()
                })?;
                agent_name = Some(value.clone());
                index += 2;
            }
            "--policy" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent dispatch requires value after --policy".to_string())?;
                policy = Some(value.clone());
                index += 2;
            }
            "--token-budget" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --token-budget".to_string()
                })?;
                token_budget = Some(parse_u16_flag("--token-budget", value)?);
                index += 2;
            }
            "--idle-timeout-ms" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --idle-timeout-ms".to_string()
                })?;
                idle_timeout_ms = Some(parse_u64_flag("--idle-timeout-ms", value)?);
                index += 2;
            }
            "--fork-parent-tokens" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    "subagent dispatch requires value after --fork-parent-tokens".to_string()
                })?;
                fork_parent_tokens = Some(parse_u16_flag("--fork-parent-tokens", value)?);
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let task = task.ok_or_else(|| "subagent dispatch requires --task".to_string())?;
    let options = parse_cli_options(&runtime_args)?;
    let task_id = TaskId(task_id.unwrap_or_else(default_subagent_task_id));
    let context_isolation = fork_parent_tokens
        .map(|max_parent_tokens| ContextIsolation::Forked { max_parent_tokens })
        .unwrap_or(ContextIsolation::Isolated);
    let spawn = SpawnRequest {
        task_id: task_id.clone(),
        parent_agent_id: AgentId("chuang-cli".to_string()),
        agent_name: agent_name.unwrap_or_else(|| "worker".to_string()),
        task,
        tool_policy: parse_subagent_tool_policy(policy.as_deref())?,
        context_isolation,
        token_budget: token_budget.unwrap_or(1024),
        idle_timeout_ms: idle_timeout_ms.unwrap_or(30_000),
        recursive_spawn: false,
        metadata: BTreeMap::from([("source".to_string(), "cli".to_string())]),
    };

    Ok(SubagentDispatchCliRequest {
        options,
        output,
        task_id,
        spawn,
    })
}

fn parse_subagent_report(args: &[String]) -> Result<SubagentReportCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut run_id: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--db"
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root"
            | "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                runtime_args.push(args[index].clone());
                runtime_args.push(value.clone());
                index += 2;
            }
            "--run-id" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent report requires value after --run-id".to_string())?;
                run_id = Some(value.clone());
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let options = parse_cli_options(&runtime_args)?;
    let run_id = RunId(run_id.ok_or_else(|| "subagent report requires --run-id".to_string())?);

    Ok(SubagentReportCliRequest {
        options,
        output,
        run_id,
    })
}

fn parse_cli_options(args: &[String]) -> Result<CliOptions, String> {
    let mut db_path: Option<PathBuf> = None;
    let mut provider_id: Option<String> = None;
    let mut provider_base_url: Option<String> = None;
    let mut provider_api_key: Option<String> = None;
    let mut provider_model: Option<String> = None;
    let mut provider_transport: Option<String> = None;
    let mut identity_memory_root: Option<PathBuf> = None;
    let mut subagent_kind: Option<String> = None;
    let mut subagent_queue_root: Option<PathBuf> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                db_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--json" => index += 1,
            "--input" => index += 2,
            "--remember" => index += 1,
            "--remember-identity" => index += 1,
            "--provider-base-url" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_base_url = Some(value.clone());
                index += 2;
            }
            "--provider-api-key" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_api_key = Some(value.clone());
                index += 2;
            }
            "--provider-model" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_model = Some(value.clone());
                index += 2;
            }
            "--provider-transport" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_transport = Some(value.clone());
                index += 2;
            }
            "--provider-id" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_id = Some(value.clone());
                index += 2;
            }
            "--identity-memory-root" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                identity_memory_root = Some(PathBuf::from(value));
                index += 2;
            }
            "--subagent" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                subagent_kind = Some(value.clone());
                index += 2;
            }
            "--subagent-queue-root" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                subagent_queue_root = Some(PathBuf::from(value));
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let mut runtime = RuntimeConfig::new(db_path.unwrap_or_else(default_db_path));
    if let Some(root) = identity_memory_root {
        runtime.identity_memory = IdentityMemoryConfig::HermesDualFile {
            root,
            user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
            memory_max_chars: DEFAULT_MEMORY_WRITE_MAX_CHARS,
        };
    }
    if let Some(kind) = subagent_kind {
        runtime.subagent = parse_subagent_config(&kind)?;
    }
    if let Some(root) = subagent_queue_root {
        runtime.subagent_queue = SubagentQueueConfig { root };
    }
    runtime.provider = match (provider_base_url, provider_api_key, provider_model) {
        (None, None, None) => ProviderConfig::Fake {
            provider_id: "fake-runtime".to_string(),
            model_name: "stub-responder".to_string(),
        },
        (Some(base_url), Some(api_key), Some(model_name)) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                provider_id: provider_id.unwrap_or_else(|| "openai-compatible-cli".to_string()),
                base_url,
                api_key,
                model_name,
                transport: parse_provider_transport(provider_transport.as_deref())?,
            })
        }
        _ => {
            return Err(
                "provider config requires base_url + api_key + model (optional: provider_id)"
                    .to_string(),
            )
        }
    };

    Ok(CliOptions { runtime })
}

fn print_runtime_result(result: &chuang_agent::agent_runtime::RuntimeResult) {
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

fn format_drop_reasons(reasons: &[chuang_agent::context_engine::DropReason]) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(|reason| format!("{}:{}", reason.segment_id, reason.reason.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_budget_exceeded_reasons(
    reasons: &[chuang_agent::context_engine::BudgetExceededReason],
) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(chuang_agent::context_engine::BudgetExceededReason::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_working_reservation(debug: &chuang_agent::agent_runtime::ContextDebugInfo) -> String {
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

fn parse_provider_transport(raw: Option<&str>) -> Result<ProviderTransport, String> {
    raw.unwrap_or("stub").parse()
}

fn parse_subagent_config(raw: &str) -> Result<SubagentConfig, String> {
    match raw {
        "fake" => Ok(SubagentConfig::Fake),
        "queued_external" => Ok(SubagentConfig::QueuedExternal),
        other => Err(format!(
            "unsupported subagent kind: {other} (supported: fake, queued_external)"
        )),
    }
}

fn parse_subagent_tool_policy(raw: Option<&str>) -> Result<SubagentToolPolicy, String> {
    match raw.unwrap_or("analyze") {
        "analyze" => Ok(SubagentToolPolicy::Analyze),
        "execute" => Ok(SubagentToolPolicy::Execute),
        "orchestrate" => Ok(SubagentToolPolicy::Orchestrate),
        other => Err(format!(
            "unsupported subagent policy: {other} (supported: analyze, execute, orchestrate)"
        )),
    }
}

fn parse_u16_flag(flag: &str, raw: &str) -> Result<u16, String> {
    raw.parse::<u16>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn parse_u64_flag(flag: &str, raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn default_subagent_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("cli-task-{nanos}")
}

fn seed_default_memory_if_empty(store: &mut SqliteMemoryStore) -> Result<(), String> {
    let existing = store
        .search(&chuang_agent::memory_store::MemoryQuery {
            text: None,
            metadata: BTreeMap::new(),
            limit: 1,
        })
        .map_err(|e| format!("seed_search_failed: {e:?}"))?;

    if !existing.is_empty() {
        return Ok(());
    }

    store
        .put(chuang_agent::memory_store::MemoryRecord {
            id: "boot-seed-1".to_string(),
            content: "创项目先跑起来，先闭环再优化。".to_string(),
            metadata: BTreeMap::from([("kind".to_string(), "goal".to_string())]),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            expires_at: None,
        })
        .map_err(|e| format!("seed_put_failed: {e:?}"))?;

    Ok(())
}

fn default_db_path() -> PathBuf {
    PathBuf::from("./data/chuang-agent.db")
}

fn usage() -> String {
    "usage: cargo run -- <run|repl|status|control|subagent> [--db PATH] [--identity-memory-root PATH] [--subagent fake|queued_external] [--subagent-queue-root PATH] [--input TEXT] [--remember] [--remember-identity] [--provider-base-url URL --provider-api-key KEY --provider-model MODEL [--provider-id ID]] | status [--json] | control list [--json] | control apply --unit ID --action start|stop|restart|change-model [--model MODEL] --reason TEXT [--approve] [--json] | subagent dispatch --task TEXT [--task-id ID] [--agent-name NAME] [--policy analyze|execute|orchestrate] [--token-budget N] [--idle-timeout-ms MS] [--fork-parent-tokens N] [--json] | subagent report --run-id ID [--json]".to_string()
}
