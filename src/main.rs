use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

mod cli_args;
mod cli_output;
mod cli_runtime;
mod cli_subagent;
mod cli_types;

use chuang_agent::control_plane::{ControlPlane, ManagedUnit};
use chuang_agent::control_surface::{
    list_control_surface_units, run_control_surface_intent, ControlSurfaceError,
    ControlSurfaceRequest,
};
use chuang_agent::control_workflow::{build_decision_view, ControlWorkflowError};
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::runtime_config::RuntimeConfig;
use chuang_agent::slot_registry::build_runtime_slots;
use cli_args::*;
use cli_output::{
    print_config_summary, print_control_unit_view, print_control_view_with_format, print_json,
    print_runtime_result, print_status, usage, ControlOutputFormat,
};
use cli_runtime::{default_db_path, kernel_config_from_runtime, run_with_options};
use cli_subagent::subagent_command;
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
