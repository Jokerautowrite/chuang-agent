use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

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
mod cli_skill;
mod cli_subagent;
mod cli_types;

use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::tool_loop_meta::ToolLoopMeta;
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
use cli_skill::skill_command;
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
        Some("skill") => skill_command(&args[2..]),
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
    let interactive = stdin.is_terminal() && stdout.is_terminal();
    let show_trace = true;

    if interactive {
        print_repl_banner(&mut stdout, &options)?;
        return repl_interactive_loop(options, verbose, show_trace, &mut stdout);
    }

    let mut stdin_lock = stdin.lock();
    loop {
        let mut line = String::new();
        let bytes = stdin_lock
            .read_line(&mut line)
            .map_err(|e| format!("stdin_read_failed: {e}"))?;
        if bytes == 0 {
            break;
        }
        let input = line.trim();
        if input.eq_ignore_ascii_case("exit")
            || input.eq_ignore_ascii_case("quit")
            || input.eq_ignore_ascii_case("/exit")
            || input.eq_ignore_ascii_case("/quit")
        {
            if interactive {
                writeln!(stdout, "bye.").map_err(|e| format!("stdout_write_failed: {e}"))?;
            }
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
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: None,
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

fn repl_interactive_loop(
    options: CliOptions,
    mut verbose: bool,
    mut show_trace: bool,
    stdout: &mut io::Stdout,
) -> Result<(), String> {
    let summary = options.runtime.summary();
    let mut turn_count = 0usize;
    let mut pending_guidance: Vec<String> = Vec::new();
    let mut running: Option<RunningTurn> = None;
    let mut last_tick_second: Option<u64> = None;
    let (input_sender, input_receiver) = mpsc::channel::<Option<String>>();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    if input_sender.send(Some(line)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = input_sender.send(None);
                    return;
                }
            }
        }
        let _ = input_sender.send(None);
    });
    print_repl_prompt(stdout, false, pending_guidance.len())?;

    loop {
        if print_running_tick(stdout, running.as_ref(), &mut last_tick_second)? {
            print_repl_prompt(stdout, running.is_some(), pending_guidance.len())?;
        }
        if poll_running_turn(
            stdout,
            &mut running,
            &mut turn_count,
            show_trace,
            verbose,
            &mut pending_guidance,
        )? {
            last_tick_second = None;
            print_repl_prompt(stdout, running.is_some(), pending_guidance.len())?;
        }

        let input = match input_receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match process_repl_input(
            &input,
            &options,
            &summary,
            &mut verbose,
            &mut show_trace,
            &mut running,
            &mut pending_guidance,
            stdout,
        )? {
            InputAction::Continue => {
                print_repl_prompt(stdout, running.is_some(), pending_guidance.len())?;
            }
            InputAction::Exit => break,
        }
    }

    Ok(())
}

enum InputAction {
    Continue,
    Exit,
}

fn process_repl_input(
    raw_input: &str,
    options: &CliOptions,
    summary: &chuang_agent::runtime_config::ConfigSummary,
    verbose: &mut bool,
    show_trace: &mut bool,
    running: &mut Option<RunningTurn>,
    pending_guidance: &mut Vec<String>,
    stdout: &mut io::Stdout,
) -> Result<InputAction, String> {
    let input = raw_input.trim();
    if input.eq_ignore_ascii_case("exit")
        || input.eq_ignore_ascii_case("quit")
        || input.eq_ignore_ascii_case("/exit")
        || input.eq_ignore_ascii_case("/quit")
    {
        if running.is_some() {
            writeln!(
                stdout,
                "task still running; close the terminal to force quit, or wait for completion."
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
            return Ok(InputAction::Continue);
        }
        writeln!(stdout, "bye.").map_err(|e| format!("stdout_write_failed: {e}"))?;
        return Ok(InputAction::Exit);
    }
    if input.is_empty() {
        return Ok(InputAction::Continue);
    }

    if input.starts_with('/') {
        handle_repl_command(input, verbose, show_trace, options, stdout)?;
        return Ok(InputAction::Continue);
    }

    if let Some(note) = input.strip_prefix('!') {
        let note = note.trim();
        if note.is_empty() {
            writeln!(stdout, "guidance ignored: empty note")
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        } else if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, note)?;
            writeln!(stdout, "guidance injected into current turn")
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        } else {
            pending_guidance.push(note.to_string());
            writeln!(stdout, "guidance queued: {}", pending_guidance.len())
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        return Ok(InputAction::Continue);
    }

    if running.is_some() {
        if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, input)?;
        }
        writeln!(
            stdout,
            "guidance injected into current turn. Prefix with ! next time to make this explicit."
        )
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
        return Ok(InputAction::Continue);
    }

    let user_input = merge_repl_guidance(input, pending_guidance);
    pending_guidance.clear();
    print_repl_section_rule(stdout, "RUNNING")?;
    writeln!(stdout, "task      {}", compact_preview(&user_input, 120))
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "provider  {} / {}",
        summary.provider_id, summary.model_name
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "workspace {}",
        env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "input     type !text while running to inject guidance at the next safe point"
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    *running = Some(spawn_repl_turn(options.clone(), user_input));
    Ok(InputAction::Continue)
}

fn print_running_tick(
    stdout: &mut io::Stdout,
    running: Option<&RunningTurn>,
    last_tick_second: &mut Option<u64>,
) -> Result<bool, String> {
    let Some(turn) = running else {
        *last_tick_second = None;
        return Ok(false);
    };
    let elapsed = turn.started_at.elapsed().as_secs();
    if elapsed == 0 || elapsed % 5 != 0 || *last_tick_second == Some(elapsed) {
        return Ok(false);
    }
    *last_tick_second = Some(elapsed);
    writeln!(
        stdout,
        "\n[working {:>3}s] waiting for model/tools; input remains live for !guidance",
        elapsed
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    Ok(true)
}

fn poll_running_turn(
    stdout: &mut io::Stdout,
    running: &mut Option<RunningTurn>,
    turn_count: &mut usize,
    show_trace: bool,
    verbose: bool,
    pending_guidance: &mut Vec<String>,
) -> Result<bool, String> {
    if let Some(mut turn) = running.take() {
        match turn.receiver.try_recv() {
            Ok(result) => {
                turn.result = Some(result);
                finish_running_turn(
                    stdout,
                    turn,
                    turn_count,
                    show_trace,
                    verbose,
                    pending_guidance,
                )?;
                return Ok(true);
            }
            Err(TryRecvError::Empty) => {
                *running = Some(turn);
            }
            Err(TryRecvError::Disconnected) => {
                turn.result = Some(Err("repl_turn_disconnected".to_string()));
                finish_running_turn(
                    stdout,
                    turn,
                    turn_count,
                    show_trace,
                    verbose,
                    pending_guidance,
                )?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

struct RunningTurn {
    started_at: Instant,
    input_preview: String,
    receiver: mpsc::Receiver<Result<chuang_agent::agent_runtime::RuntimeResult, String>>,
    handle: thread::JoinHandle<()>,
    result: Option<Result<chuang_agent::agent_runtime::RuntimeResult, String>>,
    guidance_path: PathBuf,
}

fn spawn_repl_turn(options: CliOptions, user_input: String) -> RunningTurn {
    let started_at = Instant::now();
    let input_preview = compact_preview(&user_input, 80);
    let guidance_path = env::temp_dir().join(format!(
        "chuang-repl-guidance-{}-{}.txt",
        std::process::id(),
        started_at.elapsed().as_nanos()
    ));
    let (sender, receiver) = mpsc::channel();
    let request_guidance_path = guidance_path.clone();
    let handle = thread::spawn(move || {
        let result = run_with_options(&RunCliRequest {
            options,
            user_input,
            workspace_root: None,
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history: Vec::new(),
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: Some(request_guidance_path),
        })
        .map(|(result, _)| result);
        let _ = sender.send(result);
    });
    RunningTurn {
        started_at,
        input_preview,
        receiver,
        handle,
        result: None,
        guidance_path,
    }
}

fn finish_running_turn(
    stdout: &mut io::Stdout,
    turn: RunningTurn,
    turn_count: &mut usize,
    show_trace: bool,
    verbose: bool,
    pending_guidance: &mut [String],
) -> Result<(), String> {
    let elapsed_ms = turn.started_at.elapsed().as_millis();
    let result = match turn.result {
        Some(result) => result,
        None => turn
            .receiver
            .recv()
            .map_err(|e| format!("repl_turn_receive_failed: {e}"))?,
    };
    let _ = turn.handle.join();
    match result {
        Ok(result) => {
            *turn_count += 1;
            print_repl_result(
                stdout,
                &result,
                elapsed_ms,
                *turn_count,
                show_trace,
                &turn.input_preview,
                pending_guidance.len(),
            )?;
            if verbose {
                print_runtime_result(&result);
            }
        }
        Err(error) => {
            writeln!(
                stdout,
                "╰─ failed elapsed={}ms input={}",
                elapsed_ms, turn.input_preview
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
            writeln!(stdout, "{error}").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn print_repl_prompt(
    stdout: &mut io::Stdout,
    running: bool,
    guidance_count: usize,
) -> Result<(), String> {
    writeln!(stdout).map_err(|e| format!("stdout_write_failed: {e}"))?;
    if running {
        write!(stdout, "╭─ input [running: !guidance]")
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
    } else {
        write!(stdout, "╭─ input").map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    if guidance_count > 0 {
        write!(stdout, " +{guidance_count}").map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    writeln!(stdout).map_err(|e| format!("stdout_write_failed: {e}"))?;
    write!(stdout, "╰─ chuang › ").map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn merge_repl_guidance(input: &str, guidance: &[String]) -> String {
    if guidance.is_empty() {
        return input.to_string();
    }
    let mut merged = String::new();
    merged.push_str(input);
    merged.push_str("\n\n[operator-guidance]\n");
    for (index, note) in guidance.iter().enumerate() {
        merged.push_str(&format!("{}. {}\n", index + 1, note));
    }
    merged
        .push_str("Treat operator-guidance as the creator's latest requirements for this turn.\n");
    merged
}

fn compact_preview(input: &str, max_chars: usize) -> String {
    let text = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut preview = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    preview.push('…');
    preview
}

fn append_live_guidance(path: &PathBuf, note: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("guidance_dir_create_failed: {e}"))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("guidance_open_failed: {e}"))?;
    writeln!(file, "{}", note.trim()).map_err(|e| format!("guidance_write_failed: {e}"))
}

fn print_repl_banner(stdout: &mut io::Stdout, options: &CliOptions) -> Result<(), String> {
    let summary = options.runtime.summary();
    write!(stdout, "\x1b[2J\x1b[H").map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "╭────────────────────────────────────────────────────────────╮\n│ Chuang Terminal                                            │\n│ provider {:<20} model {:<19} │\n│ commands /help /status /trace /notrace /verbose /quiet /exit │\n│ running input stays live: type !text to guide current task  │\n╰────────────────────────────────────────────────────────────╯\n\nVisible trace shows audited runtime/tool evidence, not hidden model reasoning.",
        summary.provider_id, summary.model_name
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn handle_repl_command(
    input: &str,
    verbose: &mut bool,
    show_trace: &mut bool,
    options: &CliOptions,
    stdout: &mut io::Stdout,
) -> Result<(), String> {
    match input {
        "/help" | "/?" => {
            writeln!(
                stdout,
                "\nCommands\n  /help      show this help\n  /status    show runtime status summary\n  /trace     show visible execution trace\n  /notrace   hide visible execution trace\n  /verbose   print full runtime metadata after each turn\n  /quiet     show concise answers only\n  /clear     clear the screen\n  /exit      leave the terminal\n\nMid-task guidance\n  !text      inject creator guidance into the current task at the next safe point\n  plain text while running is also injected into the current task\n  !text while idle is queued for the next submitted task\n\nDisplay\n  The input box stays visible between turns.\n  Running tasks print a small progress tick every 5s.\n  Results are split into trace, tool events, answer, and report id.\n  Hidden model reasoning is not printed.\n"
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/status" => {
            let kernel = kernel_config_from_runtime(&options.runtime)?;
            let status = build_chuang_mvp_status(&options.runtime, &kernel)
                .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
            writeln!(
                stdout,
                "\nStatus\n  provider: {} / {}\n  readiness: {}\n  tools: mapped={} executable={}\n  subagent: {}\n  memory: {}\n",
                status.config.provider_id,
                status.config.model_name,
                status.provider_readiness.overall_state,
                status.atomic_tools.mapped_count,
                status.atomic_tools.governed_executable_atomic_tool_names.len(),
                status.slots.subagent,
                status.config.identity_memory_root
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/verbose" => {
            *verbose = true;
            writeln!(stdout, "verbose: on").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/trace" => {
            *show_trace = true;
            writeln!(stdout, "trace: on").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/notrace" => {
            *show_trace = false;
            writeln!(stdout, "trace: off").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/quiet" => {
            *verbose = false;
            writeln!(stdout, "verbose: off").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/clear" => {
            write!(stdout, "\x1b[2J\x1b[H").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/exit" | "/quit" => {}
        _ => {
            writeln!(stdout, "unknown command: {input}. Try /help.")
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn print_repl_result(
    stdout: &mut io::Stdout,
    result: &chuang_agent::agent_runtime::RuntimeResult,
    elapsed_ms: u128,
    turn_count: usize,
    show_trace: bool,
    input_preview: &str,
    pending_guidance_count: usize,
) -> Result<(), String> {
    let meta = &result.response.meta.extra;
    let tool_meta = ToolLoopMeta::from_extra(meta)?;
    let tool_status = meta
        .get("tool_loop_status")
        .map(String::as_str)
        .unwrap_or("none");
    print_repl_section_rule(stdout, "DONE")?;
    writeln!(
        stdout,
        "turn      {}\nelapsed   {}ms\ntools     {}\nprotocol  {}\nstatus    {}",
        turn_count,
        elapsed_ms,
        tool_meta.tool_call_count,
        tool_meta.tool_protocol_error_count,
        tool_status
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(stdout, "input     {input_preview}")
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    if pending_guidance_count > 0 {
        writeln!(
            stdout,
            "   queued idle guidance: {pending_guidance_count} note(s) will apply to the next submitted task"
        )
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    if show_trace {
        print_visible_trace(stdout, result, elapsed_ms, &tool_meta, tool_status)?;
    }
    if !tool_meta.tool_events.is_empty() {
        print_repl_section_rule(stdout, "TOOL EVENTS")?;
    }
    for event in &tool_meta.tool_events {
        if let Some(line) = format_tool_event(event) {
            writeln!(stdout, "- {line}").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    print_repl_section_rule(stdout, "ANSWER")?;
    writeln!(stdout, "{}", result.response.body.trim())
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    if let Some(report_id) = meta.get("runtime_report_id") {
        print_repl_section_rule(stdout, "REPORT")?;
        writeln!(stdout, "{report_id}").map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn print_visible_trace(
    stdout: &mut io::Stdout,
    result: &chuang_agent::agent_runtime::RuntimeResult,
    elapsed_ms: u128,
    tool_meta: &ToolLoopMeta,
    tool_status: &str,
) -> Result<(), String> {
    print_repl_section_rule(stdout, "TRACE")?;
    writeln!(
        stdout,
        "context   engine={} tokens={} recall_hits={} dropped={}",
        result.context_engine_kind,
        result.packed_token_count,
        result.recall_hit_count,
        result.dropped_segment_ids.len()
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "model     {} finish={}",
        result.response.model_name,
        result
            .response
            .meta
            .finish_reason
            .as_deref()
            .unwrap_or("none")
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "runtime   elapsed={}ms tools={} protocol_errors={} status={}",
        elapsed_ms, tool_meta.tool_call_count, tool_meta.tool_protocol_error_count, tool_status
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    Ok(())
}

fn print_repl_section_rule(stdout: &mut io::Stdout, title: &str) -> Result<(), String> {
    writeln!(
        stdout,
        "\n── {title} ─────────────────────────────────────────────"
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))
}

fn format_tool_event(event: &serde_json::Value) -> Option<String> {
    let kind = event.get("kind").and_then(|value| value.as_str())?;
    match kind {
        "tool_call" => {
            let tool = event
                .get("tool_name")
                .and_then(|value| value.as_str())
                .unwrap_or("tool");
            let decision = event
                .get("decision")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let ok = event
                .get("ok")
                .and_then(|value| value.as_bool())
                .map(|value| if value { "ok" } else { "failed" })
                .unwrap_or("unknown");
            let summary = event
                .get("summary")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            Some(format!("tool {tool}: {ok} decision={decision} {summary}"))
        }
        "protocol_error" => {
            let code = event
                .get("protocol_error_code")
                .and_then(|value| value.as_str())
                .unwrap_or("protocol_error");
            Some(format!("protocol: {code}"))
        }
        _ => Some(format!("event: {kind}")),
    }
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
