use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

mod cli_output;

use chuang_agent::chuang_kernel::{
    ChuangKernel, ChuangKernelConfig, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::common::{AgentId, ReportId, TaskId, Timestamp};
use chuang_agent::control_intent::{parse_control_intent, ControlIntentError, ControlIntentInput};
use chuang_agent::control_plane::{ControlPlane, ManagedUnit};
use chuang_agent::control_surface::{
    list_control_surface_units, run_control_surface_intent, ControlSurfaceError,
    ControlSurfaceRequest,
};
use chuang_agent::control_workflow::{build_decision_view, ControlWorkflowError};
use chuang_agent::hermes_memory::{
    DualFileMemoryStore, FileDualFileMemoryStore, HotMemoryEntry, DEFAULT_USER_MEMORY_MAX_CHARS,
};
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::memory_store::MemoryStore;
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::provider_openai_compatible::ProviderTransport;
use chuang_agent::runtime_config::{
    ConfigSummary, IdentityMemoryConfig, OpenAICompatibleConfig, ProviderConfig, RuntimeConfig,
    SubagentConfig, SubagentQueueConfig,
};
use chuang_agent::runtime_config_file::{load_runtime_config_file, RuntimeConfigFileError};
use chuang_agent::slot_registry::{build_provider_responder, build_runtime_slots};
use chuang_agent::subagent_queue::FileSubagentQueue;
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{
    ContextIsolation, QueuedSubagentSpawner, RunId, SpawnRequest, SubagentToolPolicy,
};
use cli_output::{
    print_config_summary, print_control_unit_view, print_control_view_with_format, print_json,
    print_runtime_result, print_status, usage, ControlOutputFormat,
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

const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../config.example.toml");

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
        Some("config") => config_command(&args[2..]),
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

fn config_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("check") => config_check_command(&args[1..]),
        Some("show") => config_show_command(&args[1..]),
        Some("init") => config_init_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn config_check_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_cli_options(args)?;
    let kernel = kernel_config_from_runtime(&options.runtime)?;
    build_chuang_mvp_status(&options.runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let result = ConfigCheckCliOutput {
        ok: true,
        source: effective_config_source(args)?.unwrap_or_else(|| "defaults".to_string()),
        summary: options.runtime.summary(),
    };

    match output {
        ControlOutputFormat::Text => {
            println!(
                "config_ok source={} provider={} model={} subagent={} queue_root={}",
                result.source,
                result.summary.provider_kind,
                result.summary.model_name,
                result.summary.subagent_kind,
                result.summary.subagent_queue_root
            );
        }
        ControlOutputFormat::Json => print_json(&result)?,
    }

    Ok(())
}

fn config_show_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_cli_options(args)?;
    options
        .runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let result = ConfigCheckCliOutput {
        ok: true,
        source: effective_config_source(args)?.unwrap_or_else(|| "defaults".to_string()),
        summary: options.runtime.summary(),
    };

    match output {
        ControlOutputFormat::Text => {
            print_config_summary(result.ok, &result.source, &result.summary)
        }
        ControlOutputFormat::Json => print_json(&result)?,
    }

    Ok(())
}

fn config_init_command(args: &[String]) -> Result<(), String> {
    let request = parse_config_init(args)?;
    if request.path.exists() {
        return Err(format!(
            "config_init_refused: path already exists: {}",
            request.path.display()
        ));
    }
    if let Some(parent) = request
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "config_init_parent_create_failed path={} error={e}",
                parent.display()
            )
        })?;
    }
    fs::write(&request.path, DEFAULT_CONFIG_TEMPLATE).map_err(|e| {
        format!(
            "config_init_write_failed path={} error={e}",
            request.path.display()
        )
    })?;

    let output = ConfigInitCliOutput {
        written: true,
        path: request.path.display().to_string(),
    };
    match request.output {
        ControlOutputFormat::Text => {
            println!("config_initialized path={}", output.path);
        }
        ControlOutputFormat::Json => print_json(&output)?,
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
        Some("list") => subagent_list_command(&args[1..]),
        Some("run-once") => subagent_run_once_command(&args[1..]),
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
    let dispatch_ids = unique_cli_subagent_ids(&request.spawn.agent_name)?;
    let receipt = spawner
        .spawn_with_ids(request.spawn, dispatch_ids.run_id, dispatch_ids.agent_id)
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

fn subagent_list_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_list(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let dispatches = queue
        .list_dispatches()
        .map_err(|e| format!("subagent_dispatch_list_failed: {e:?}"))?;
    let report_run_ids = queue
        .list_report_run_ids()
        .map_err(|e| format!("subagent_report_list_failed: {e:?}"))?;
    let report_lookup = report_run_ids
        .iter()
        .map(|run_id| run_id.0.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let items = dispatches
        .into_iter()
        .map(|dispatch| SubagentListItem {
            run_id: dispatch.run_id.0.clone(),
            agent_id: dispatch.agent_id.0,
            task_id: dispatch.task_id.0,
            agent_name: dispatch.agent_name,
            tool_policy: format!("{:?}", dispatch.tool_policy),
            has_report: report_lookup.contains(&dispatch.run_id.0),
        })
        .collect::<Vec<_>>();
    let output = SubagentListCliOutput {
        queue_root: request
            .options
            .runtime
            .subagent_queue
            .root
            .display()
            .to_string(),
        dispatch_count: items.len(),
        report_count: report_run_ids.len(),
        items,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_queue queue_root={} dispatch_count={} report_count={}",
                output.queue_root, output.dispatch_count, output.report_count
            );
            for item in &output.items {
                println!(
                    "run_id={} agent_id={} task_id={} policy={} has_report={}",
                    item.run_id, item.agent_id, item.task_id, item.tool_policy, item.has_report
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_run_once_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_run_once(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let dispatches = queue
        .list_dispatches()
        .map_err(|e| format!("subagent_dispatch_list_failed: {e:?}"))?;
    let report_run_ids = queue
        .list_report_run_ids()
        .map_err(|e| format!("subagent_report_list_failed: {e:?}"))?
        .into_iter()
        .map(|run_id| run_id.0)
        .collect::<std::collections::BTreeSet<_>>();
    let Some(dispatch) = dispatches
        .into_iter()
        .find(|dispatch| !report_run_ids.contains(&dispatch.run_id.0))
    else {
        let output = SubagentRunOnceCliOutput {
            runner: request.runner,
            ran: false,
            run_id: None,
            report_path: None,
            summary: "no pending dispatch".to_string(),
        };
        return match request.output {
            ControlOutputFormat::Text => {
                println!("subagent_run_once idle runner={}", output.runner);
                Ok(())
            }
            ControlOutputFormat::Json => print_json(&output),
        };
    };

    let report = build_fake_runner_report(&dispatch)?;
    let report_path = queue
        .write_report(&dispatch.run_id, &report)
        .map_err(|e| format!("subagent_report_write_failed: {e:?}"))?;
    let output = SubagentRunOnceCliOutput {
        runner: request.runner,
        ran: true,
        run_id: Some(dispatch.run_id.0),
        report_path: Some(report_path.display().to_string()),
        summary: report.summary,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_run_once runner={} run_id={} report_path={}",
                output.runner,
                output.run_id.as_deref().unwrap_or("none"),
                output.report_path.as_deref().unwrap_or("none")
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

    let provider = build_provider_responder(&request.options.runtime.provider)
        .map_err(|err| format!("config_invalid: {}: {}", err.field, err.message))?;
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
        .and_then(|turn| remember_turn_if_requested(&request.options, &mut kernel, turn, request))
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
struct CliSubagentIds {
    run_id: RunId,
    agent_id: AgentId,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubagentListCliRequest {
    options: CliOptions,
    output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SubagentListCliOutput {
    queue_root: String,
    dispatch_count: usize,
    report_count: usize,
    items: Vec<SubagentListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SubagentListItem {
    run_id: String,
    agent_id: String,
    task_id: String,
    agent_name: String,
    tool_policy: String,
    has_report: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SubagentRunOnceCliRequest {
    options: CliOptions,
    output: ControlOutputFormat,
    runner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SubagentRunOnceCliOutput {
    runner: String,
    ran: bool,
    run_id: Option<String>,
    report_path: Option<String>,
    summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ConfigCheckCliOutput {
    ok: bool,
    source: String,
    summary: ConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigInitCliRequest {
    output: ControlOutputFormat,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ConfigInitCliOutput {
    written: bool,
    path: String,
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
            flag if is_runtime_value_flag(flag) => index += 2,
            _ => return Err(usage()),
        }
    }
    Ok(output)
}

fn parse_config_init(args: &[String]) -> Result<ConfigInitCliRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut path = PathBuf::from("config.toml");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--path" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                path = PathBuf::from(value);
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    Ok(ConfigInitCliRequest { output, path })
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
            flag if is_runtime_value_flag(flag) => index += 2,
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
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
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
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
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

fn parse_subagent_list(args: &[String]) -> Result<SubagentListCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    Ok(SubagentListCliRequest {
        options: parse_cli_options(&runtime_args)?,
        output,
    })
}

fn parse_subagent_run_once(args: &[String]) -> Result<SubagentRunOnceCliRequest, String> {
    let mut runtime_args: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut runner: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--runner" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "subagent run-once requires value after --runner".to_string())?;
                runner = Some(value.clone());
                index += 2;
            }
            flag if is_runtime_value_flag(flag) => {
                copy_runtime_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let runner = runner.unwrap_or_else(|| "fake".to_string());
    if runner != "fake" {
        return Err(format!(
            "unsupported subagent runner: {runner} (supported: fake)"
        ));
    }

    Ok(SubagentRunOnceCliRequest {
        options: parse_cli_options(&runtime_args)?,
        output,
        runner,
    })
}

fn parse_cli_options(args: &[String]) -> Result<CliOptions, String> {
    let config_path = find_config_path(args)?;
    let mut db_path: Option<PathBuf> = None;
    let mut provider_id: Option<String> = None;
    let mut provider_base_url: Option<String> = None;
    let mut provider_api_key: Option<String> = None;
    let mut provider_model: Option<String> = None;
    let mut provider_transport: Option<String> = None;
    let mut identity_memory_root: Option<PathBuf> = None;
    let mut subagent_kind: Option<String> = None;
    let mut subagent_queue_root: Option<PathBuf> = None;
    let mut context_max_tokens: Option<u16> = None;
    let mut context_reserve_system_tokens: Option<u16> = None;
    let mut context_min_working_tokens: Option<u16> = None;
    let mut context_max_tool_results: Option<usize> = None;
    let mut context_max_memory_segments: Option<usize> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                skip_value_arg(args, &mut index)?;
            }
            "--db" => {
                db_path = Some(PathBuf::from(take_value_or_usage(args, &mut index)?));
            }
            "--json" => index += 1,
            "--input" => index += 2,
            "--remember" => index += 1,
            "--remember-identity" => index += 1,
            "--provider-base-url" => {
                provider_base_url = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-api-key" => {
                provider_api_key = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-model" => {
                provider_model = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-transport" => {
                provider_transport = Some(take_value_or_usage(args, &mut index)?);
            }
            "--provider-id" => {
                provider_id = Some(take_value_or_usage(args, &mut index)?);
            }
            "--identity-memory-root" => {
                identity_memory_root = Some(PathBuf::from(take_value_or_usage(args, &mut index)?));
            }
            "--subagent" => {
                subagent_kind = Some(take_value_or_usage(args, &mut index)?);
            }
            "--subagent-queue-root" => {
                subagent_queue_root = Some(PathBuf::from(take_value_or_usage(args, &mut index)?));
            }
            "--context-max-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_tokens = Some(parse_u16_flag("--context-max-tokens", &value)?);
            }
            "--context-reserve-system-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_reserve_system_tokens =
                    Some(parse_u16_flag("--context-reserve-system-tokens", &value)?);
            }
            "--context-min-working-tokens" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_min_working_tokens =
                    Some(parse_u16_flag("--context-min-working-tokens", &value)?);
            }
            "--context-max-tool-results" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_tool_results =
                    Some(parse_usize_flag("--context-max-tool-results", &value)?);
            }
            "--context-max-memory-segments" => {
                let value = take_value_or_usage(args, &mut index)?;
                context_max_memory_segments =
                    Some(parse_usize_flag("--context-max-memory-segments", &value)?);
            }
            _ => return Err(usage()),
        }
    }

    let mut runtime = if let Some(path) = config_path {
        load_runtime_config_file(&path).map_err(format_runtime_config_file_error)?
    } else if let Some(path) = default_config_path() {
        load_runtime_config_file(&path).map_err(format_runtime_config_file_error)?
    } else {
        RuntimeConfig::new(default_db_path())
    };
    if let Some(path) = db_path {
        runtime.db_path = path;
    }
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
    if let Some(value) = context_max_tokens {
        runtime.context_budget.max_tokens = value;
    }
    if let Some(value) = context_reserve_system_tokens {
        runtime.context_budget.reserve_system_tokens = value;
    }
    if let Some(value) = context_min_working_tokens {
        runtime.context_budget.min_working_tokens = value;
    }
    if let Some(value) = context_max_tool_results {
        runtime.context_budget.max_tool_results = value;
    }
    if let Some(value) = context_max_memory_segments {
        runtime.context_budget.max_memory_segments = value;
    }
    runtime.provider = match (provider_base_url, provider_api_key, provider_model) {
        (None, None, None) => runtime.provider,
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

fn is_runtime_value_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--db"
            | "--config"
            | "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport"
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root"
            | "--context-max-tokens"
            | "--context-reserve-system-tokens"
            | "--context-min-working-tokens"
            | "--context-max-tool-results"
            | "--context-max-memory-segments"
    )
}

fn copy_runtime_value_arg(
    args: &[String],
    index: &mut usize,
    runtime_args: &mut Vec<String>,
) -> Result<(), String> {
    let flag = args[*index].clone();
    let value = take_value_or_usage(args, index)?;
    runtime_args.push(flag);
    runtime_args.push(value);
    Ok(())
}

fn skip_value_arg(args: &[String], index: &mut usize) -> Result<(), String> {
    take_value_or_usage(args, index).map(|_| ())
}

fn take_value_or_usage(args: &[String], index: &mut usize) -> Result<String, String> {
    let value = args.get(*index + 1).ok_or_else(usage)?.clone();
    *index += 2;
    Ok(value)
}

fn find_config_path(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut index = 0;
    let mut config_path = None;
    while index < args.len() {
        match args[index].as_str() {
            "--config" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                config_path = Some(PathBuf::from(value));
                index += 2;
            }
            _ => index += 1,
        }
    }
    Ok(config_path)
}

fn default_config_path() -> Option<PathBuf> {
    let path = PathBuf::from("config.toml");
    path.is_file().then_some(path)
}

fn effective_config_source(args: &[String]) -> Result<Option<String>, String> {
    Ok(find_config_path(args)?
        .or_else(default_config_path)
        .map(|path| path.display().to_string()))
}

fn format_runtime_config_file_error(err: RuntimeConfigFileError) -> String {
    match err {
        RuntimeConfigFileError::ReadFailed { path } => {
            format!("config_read_failed path={}", path.display())
        }
        RuntimeConfigFileError::InvalidLine { line, content } => {
            format!("config_invalid_line line={line} content={content}")
        }
        RuntimeConfigFileError::InvalidValue { key, value } => {
            format!("config_invalid_value key={key} value={value}")
        }
        RuntimeConfigFileError::MissingEnv { name } => {
            format!("config_missing_env name={name}")
        }
    }
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

fn parse_usize_flag(flag: &str, raw: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|_| format!("{flag} must be a positive integer"))
}

fn default_subagent_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("cli-task-{nanos}")
}

fn unique_cli_subagent_ids(agent_name: &str) -> Result<CliSubagentIds, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    let pid = std::process::id();
    let safe_agent_name = agent_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let safe_agent_name = if safe_agent_name.is_empty() {
        "worker".to_string()
    } else {
        safe_agent_name
    };

    Ok(CliSubagentIds {
        run_id: RunId(format!("queued-cli-{pid}-{nanos}")),
        agent_id: AgentId(format!("{safe_agent_name}-{pid}-{nanos}")),
    })
}

fn build_fake_runner_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
) -> Result<SubagentReport, String> {
    let timestamp = current_rfc3339_timestamp()?;
    Ok(SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", dispatch.run_id.0)),
        task_id: dispatch.task_id.clone(),
        agent_id: dispatch.agent_id.clone(),
        parent_agent_id: Some(dispatch.parent_agent_id.clone()),
        status: ExecutionStatus::Success,
        started_at: Timestamp(timestamp.clone()),
        finished_at: Timestamp(timestamp),
        summary: format!(
            "fake runner completed {} with {:?} policy",
            dispatch.task_id.0, dispatch.tool_policy
        ),
        exit_code: Some(0),
        stdout_preview: Some(format!("task={}", dispatch.task)),
        stderr_preview: None,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("queued-subagent://{}", dispatch.run_id.0)),
        context_debug: None,
        truncated: false,
    })
}

fn current_rfc3339_timestamp() -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_secs();
    Ok(format_unix_rfc3339(now))
}

fn format_unix_rfc3339(seconds: u64) -> String {
    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds as i64, 0)
        .unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp should be valid")
        });
    datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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
