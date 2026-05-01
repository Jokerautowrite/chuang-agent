use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

mod cli_args;
mod cli_output;
mod cli_runtime;
mod cli_types;

use chuang_agent::common::{AgentId, ReportId, Timestamp};
use chuang_agent::control_plane::{ControlPlane, ManagedUnit};
use chuang_agent::control_surface::{
    list_control_surface_units, run_control_surface_intent, ControlSurfaceError,
    ControlSurfaceRequest,
};
use chuang_agent::control_workflow::{build_decision_view, ControlWorkflowError};
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::runtime_config::RuntimeConfig;
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::subagent_queue::FileSubagentQueue;
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{QueuedSubagentSpawner, RunId};
use cli_args::*;
use cli_output::{
    print_config_summary, print_control_unit_view, print_control_view_with_format, print_json,
    print_runtime_result, print_status, usage, ControlOutputFormat,
};
use cli_runtime::{default_db_path, kernel_config_from_runtime, run_with_options};
use cli_types::*;

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
