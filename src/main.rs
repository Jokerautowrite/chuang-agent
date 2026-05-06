use std::env;
use std::io::{self, BufRead, Write};

mod app_server;
mod cli_args;
mod cli_channel;
mod cli_config;
mod cli_console;
mod cli_control;
mod cli_doctor;
mod cli_experiment;
mod cli_external_ai;
mod cli_genesis;
mod cli_goal;
mod cli_memory;
mod cli_output;
mod cli_plugin;
mod cli_runtime;
mod cli_subagent;
mod cli_types;

use chuang_agent::kernel_status::build_chuang_mvp_status;
use cli_args::*;
use cli_channel::channel_command;
use cli_config::config_command;
use cli_console::console_command;
use cli_control::control_command;
use cli_doctor::doctor_command;
use cli_experiment::experiment_command;
use cli_external_ai::external_ai_command;
use cli_genesis::genesis_command;
use cli_goal::goal_command;
use cli_memory::memory_command;
use cli_output::{print_json, print_runtime_result, print_status, usage, ControlOutputFormat};
use cli_plugin::plugin_command;
use cli_runtime::{kernel_config_from_runtime, run_with_options};
use cli_subagent::subagent_command;
use cli_types::*;

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
        Some("doctor") => doctor_command(&args[2..]),
        Some("config") => config_command(&args[2..]),
        Some("channel") => channel_command(&args[2..]),
        Some("console") => console_command(&args[2..]),
        Some("control") => control_command(&args[2..]),
        Some("subagent") => subagent_command(&args[2..]),
        Some("genesis") => genesis_command(&args[2..]),
        Some("goal") => goal_command(&args[2..]),
        Some("memory") => memory_command(&args[2..]),
        Some("plugin") => plugin_command(&args[2..]),
        Some("experiment") => experiment_command(&args[2..]),
        Some("external-ai") => external_ai_command(&args[2..]),
        Some("app-server") => app_server::app_server_command(&args[2..]),
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
    if let Some(record_id) = memory_records.session_record_id {
        println!("session_memory_recorded: {record_id}");
    }
    if let Some(record_id) = memory_records.identity_record_id {
        println!("identity_memory_recorded: {record_id}");
    }
    if let Some(record_id) = memory_records.experience_record_id {
        println!("experience_memory_recorded: {record_id}");
    }
    if let Some(report_id) = memory_records.runtime_report_id {
        println!("runtime_report: {report_id}");
    }
    if let Some(run_id) = memory_records.subagent_dispatch_run_id {
        println!("subagent_dispatch_run_id: {run_id}");
    }
    if let Some(agent_id) = memory_records.subagent_dispatch_agent_id {
        println!("subagent_dispatch_agent_id: {agent_id}");
    }
    if let Some(task_id) = memory_records.subagent_dispatch_task_id {
        println!("subagent_dispatch_task_id: {task_id}");
    }
    Ok(())
}

fn repl_command(args: &[String]) -> Result<(), String> {
    let (options, verbose) = parse_repl_options(args)?;
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
            workspace_root: None,
            remember: false,
            session_id: None,
            remember_session: false,
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
        })?;
        if verbose {
            print_runtime_result(&result);
        } else {
            writeln!(stdout, "{}", result.response.body)
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        stdout
            .flush()
            .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    }

    Ok(())
}

fn parse_repl_options(args: &[String]) -> Result<(CliOptions, bool), String> {
    let mut verbose = false;
    let mut runtime_args: Vec<String> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--verbose" => {
                verbose = true;
                index += 1;
            }
            _ => {
                runtime_args.push(args[index].clone());
                index += 1;
            }
        }
    }

    Ok((parse_cli_options(&runtime_args)?, verbose))
}

fn status_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_status_cli_options(args)?;
    let kernel = kernel_config_from_runtime(&options.runtime)?;
    let status = build_chuang_mvp_status(&options.runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    match output {
        ControlOutputFormat::Text => print_status(&status),
        ControlOutputFormat::Json => print_json(&status)?,
    }

    Ok(())
}
