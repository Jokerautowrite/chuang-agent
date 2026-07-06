use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
use chuang_agent::terminal_event::{StepStatus, TerminalEvent};
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

const REPL_ANSWER_PREVIEW_CHARS: usize = 2400;
const REPL_TEXT_WRAP_WIDTH: usize = 78;
const REPL_HISTORY_MAX_TURNS: usize = 8;
const REPL_BANNER_INNER_WIDTH: usize = 74;
static REPL_TURN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
            progress_path: None,
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
    let mut conversation_history: Vec<ConversationHistoryItem> = Vec::new();
    let mut running: Option<RunningTurn> = None;
    let mut last_tick_second: Option<u64> = None;
    let mut progress_cursor = ProgressCursor::default();
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
        poll_progress_events(stdout, running.as_ref(), &mut progress_cursor)?;
        print_running_tick(stdout, running.as_ref(), &mut last_tick_second)?;
        if poll_running_turn(
            stdout,
            &mut running,
            &mut turn_count,
            &mut conversation_history,
            &progress_cursor.displays,
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
            &conversation_history,
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
    conversation_history: &[ConversationHistoryItem],
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
        handle_repl_command(
            input,
            verbose,
            show_trace,
            options,
            conversation_history,
            stdout,
        )?;
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
    let history = recent_repl_conversation_history(conversation_history, REPL_HISTORY_MAX_TURNS);
    *running = Some(spawn_repl_turn(options.clone(), user_input, history));
    Ok(InputAction::Continue)
}

#[derive(Default)]
struct ProgressCursor {
    bytes_read: u64,
    visible_count: usize,
    displays: Vec<ProgressDisplay>,
}

fn poll_progress_events(
    stdout: &mut io::Stdout,
    running: Option<&RunningTurn>,
    cursor: &mut ProgressCursor,
) -> Result<bool, String> {
    let Some(turn) = running else {
        cursor.bytes_read = 0;
        cursor.visible_count = 0;
        cursor.displays.clear();
        return Ok(false);
    };
    let content = match fs::read_to_string(&turn.progress_path) {
        Ok(content) => content,
        Err(_) => return Ok(false),
    };
    let start = cursor.bytes_read.min(content.len() as u64) as usize;
    let new_content = &content[start..];
    if new_content.trim().is_empty() {
        return Ok(false);
    }
    if cursor.visible_count == 0 {
        writeln!(stdout).map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    for line in new_content.lines().filter(|line| !line.trim().is_empty()) {
        if let Some(display) = format_progress_event(line) {
            cursor.visible_count += 1;
            print_progress_display_line(stdout, cursor.visible_count, &display)?;
            cursor.displays.push(display);
        }
    }
    cursor.bytes_read = content.len() as u64;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    Ok(true)
}

fn format_progress_event(line: &str) -> Option<ProgressDisplay> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if let Some(event) = value.get("event") {
        let event: TerminalEvent = serde_json::from_value(event.clone()).ok()?;
        return format_terminal_event(&event);
    }
    let kind = value.get("kind").and_then(|value| value.as_str())?;
    let details = value
        .get("details")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    match kind {
        "turn_started" => details
            .get("input_preview")
            .and_then(|value| value.as_str())
            .map(|text| {
                ProgressDisplay::Step(format!("turn started: {}", compact_preview(text, 48)))
            }),
        "model_started" => Some(ProgressDisplay::Step(format!(
            "model round {} running",
            details
                .get("round")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
        ))),
        "model_finished" => Some(ProgressDisplay::Step(format!(
            "model round {} ok chars={}",
            details
                .get("round")
                .and_then(|value| value.as_u64())
                .unwrap_or(0),
            details
                .get("chars")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
        ))),
        "tool_started" => Some(ProgressDisplay::Tool(format!(
            "[{}] running",
            details
                .get("tool")
                .and_then(|value| value.as_str())
                .unwrap_or("tool")
        ))),
        "tool_finished" => Some(ProgressDisplay::Tool(format!(
            "[{}] {} {}",
            details
                .get("tool")
                .and_then(|value| value.as_str())
                .unwrap_or("tool"),
            if details
                .get("ok")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                "ok"
            } else {
                "failed"
            },
            compact_preview(
                details
                    .get("summary")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
                46
            )
        ))),
        "protocol_error" => Some(ProgressDisplay::Tool(format!(
            "protocol {}",
            details
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("error")
        ))),
        "guidance_injected" => Some(ProgressDisplay::Step(format!(
            "guidance injected chars={}",
            details
                .get("chars")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
        ))),
        _ => Some(ProgressDisplay::Step(kind.to_string())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProgressDisplay {
    Step(String),
    Tool(String),
}

fn format_terminal_event(event: &TerminalEvent) -> Option<ProgressDisplay> {
    match event {
        TerminalEvent::TurnStarted { input_preview, .. } => Some(ProgressDisplay::Step(format!(
            "started: {}",
            compact_preview(input_preview, 48)
        ))),
        TerminalEvent::StepStarted { title, detail } => Some(ProgressDisplay::Step(format!(
            "{}{}",
            readable_step_title(title, true),
            format_optional_detail(detail.as_deref())
        ))),
        TerminalEvent::StepFinished {
            title,
            status,
            detail,
        } => Some(ProgressDisplay::Step(format!(
            "{} {}{}",
            readable_step_title(title, false),
            format_step_status(*status),
            format_optional_detail(detail.as_deref())
        ))),
        TerminalEvent::ModelStarted { round } => {
            Some(ProgressDisplay::Step(format!("thinking round {round}")))
        }
        TerminalEvent::ModelFinished {
            round,
            finish,
            chars,
        } => Some(ProgressDisplay::Step(format!(
            "model replied round {round} finish={finish} chars={chars}"
        ))),
        TerminalEvent::ToolStarted { tool, summary, .. } => Some(ProgressDisplay::Tool(format!(
            "using {tool}{}",
            format_optional_detail(summary.as_deref())
        ))),
        TerminalEvent::ToolFinished {
            tool,
            ok,
            decision: _,
            summary,
            ..
        } => Some(ProgressDisplay::Tool(format!(
            "{tool}: {} {}",
            if *ok { "ok" } else { "failed" },
            compact_preview(summary, 54)
        ))),
        TerminalEvent::ProtocolError { code, .. } => {
            Some(ProgressDisplay::Tool(format!("[protocol] {code}")))
        }
        TerminalEvent::GuidanceInjected { chars, .. } => Some(ProgressDisplay::Step(format!(
            "operator guidance injected chars={chars}"
        ))),
        TerminalEvent::AnswerReady {
            chars,
            truncated,
            snapshot_path,
        } => Some(ProgressDisplay::Step(format!(
            "answer ready chars={chars} truncated={truncated}{}",
            format_optional_detail(snapshot_path.as_deref())
        ))),
    }
}

fn readable_step_title(title: &str, running: bool) -> String {
    match title {
        "prepare context" if running => "preparing context".to_string(),
        "prepare context" => "context ready".to_string(),
        _ if running => format!("{title} running"),
        _ => title.to_string(),
    }
}

fn format_step_status(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Ok => "ok",
        StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
    }
}

fn format_optional_detail(detail: Option<&str>) -> String {
    detail
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("  {}", compact_preview(value, 64)))
        .unwrap_or_default()
}

fn print_progress_display_line(
    stdout: &mut io::Stdout,
    index: usize,
    display: &ProgressDisplay,
) -> Result<(), String> {
    let (label, text) = match display {
        ProgressDisplay::Step(text) => ("thinking / steps", text),
        ProgressDisplay::Tool(text) => ("tool stream", text),
    };
    writeln!(
        stdout,
        "│ {:>3} │ {:<18} │ {}",
        index,
        label,
        compact_preview(text, 80)
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))
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
    if elapsed == 0 || elapsed % 15 != 0 || *last_tick_second == Some(elapsed) {
        return Ok(false);
    }
    *last_tick_second = Some(elapsed);
    writeln!(
        stdout,
        "\n[working {:>3}s] still running; type !note to guide this turn",
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
    conversation_history: &mut Vec<ConversationHistoryItem>,
    progress_displays: &[ProgressDisplay],
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
                    conversation_history,
                    progress_displays,
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
                    conversation_history,
                    progress_displays,
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
    user_input: String,
    receiver: mpsc::Receiver<Result<chuang_agent::agent_runtime::RuntimeResult, String>>,
    handle: thread::JoinHandle<()>,
    result: Option<Result<chuang_agent::agent_runtime::RuntimeResult, String>>,
    guidance_path: PathBuf,
    progress_path: PathBuf,
}

fn spawn_repl_turn(
    options: CliOptions,
    user_input: String,
    conversation_history: Vec<ConversationHistoryItem>,
) -> RunningTurn {
    let started_at = Instant::now();
    let input_preview = compact_preview(&user_input, 80);
    let request_user_input = user_input.clone();
    let turn_nonce = repl_turn_nonce();
    let guidance_path = env::temp_dir().join(format!(
        "chuang-repl-guidance-{}-{}.txt",
        std::process::id(),
        turn_nonce
    ));
    let progress_path = env::temp_dir().join(format!(
        "chuang-repl-progress-{}-{}.jsonl",
        std::process::id(),
        turn_nonce
    ));
    let (sender, receiver) = mpsc::channel();
    let request_guidance_path = guidance_path.clone();
    let request_progress_path = progress_path.clone();
    let handle = thread::spawn(move || {
        let result = run_with_options(&RunCliRequest {
            options,
            user_input: request_user_input,
            workspace_root: None,
            remember: false,
            session_id: None,
            remember_session: false,
            conversation_history,
            remember_identity: false,
            remember_experience: false,
            dispatch_subagent: false,
            goal_spec: None,
            knowledge_context: None,
            live_guidance_path: Some(request_guidance_path),
            progress_path: Some(request_progress_path),
        })
        .map(|(result, _)| result);
        let _ = sender.send(result);
    });
    RunningTurn {
        started_at,
        input_preview,
        user_input,
        receiver,
        handle,
        result: None,
        guidance_path,
        progress_path,
    }
}

fn repl_turn_nonce() -> String {
    let sequence = REPL_TURN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{timestamp}-{sequence}")
}

fn finish_running_turn(
    stdout: &mut io::Stdout,
    turn: RunningTurn,
    turn_count: &mut usize,
    conversation_history: &mut Vec<ConversationHistoryItem>,
    progress_displays: &[ProgressDisplay],
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
            record_repl_conversation_turn(
                conversation_history,
                &turn.user_input,
                &result.response.body,
            );
            print_repl_result(
                stdout,
                &result,
                elapsed_ms,
                *turn_count,
                show_trace,
                &turn.input_preview,
                pending_guidance.len(),
                progress_displays,
            )?;
            if verbose {
                print_runtime_result(&result);
            }
        }
        Err(error) => {
            print_repl_failure(
                stdout,
                &turn.input_preview,
                elapsed_ms,
                &error,
                progress_displays,
            )?;
        }
    }
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn print_repl_failure(
    stdout: &mut io::Stdout,
    input_preview: &str,
    elapsed_ms: u128,
    error: &str,
    progress_displays: &[ProgressDisplay],
) -> Result<(), String> {
    print_repl_section_rule(stdout, "FAILED")?;
    writeln!(stdout, "input     {input_preview}")
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(stdout, "elapsed   {elapsed_ms}ms")
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(stdout, "reason    {}", readable_runtime_error(error))
        .map_err(|e| format!("stdout_write_failed: {e}"))?;

    print_repl_section_rule(stdout, "WHAT HAPPENED")?;
    let mut count = 0usize;
    for display in progress_displays
        .iter()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        count += 1;
        match display {
            ProgressDisplay::Step(text) | ProgressDisplay::Tool(text) => {
                writeln!(stdout, "{}. {text}", count)
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
            }
        }
    }
    if count == 0 {
        writeln!(stdout, "1. no visible runtime events were captured")
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
    }

    print_repl_section_rule(stdout, "NEXT")?;
    writeln!(
        stdout,
        "- Ask again with a narrower goal, or type a correction with ! while the task is running."
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "- If this repeats, increase tool_max_rounds or improve the model's FINAL discipline."
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))
}

fn readable_runtime_error(error: &str) -> String {
    if error.contains("tool_loop_exhausted") {
        return "tool loop reached its round limit before the model produced FINAL".to_string();
    }
    error.to_string()
}

fn recent_repl_conversation_history(
    history: &[ConversationHistoryItem],
    max_turns: usize,
) -> Vec<ConversationHistoryItem> {
    let max_items = max_turns.saturating_mul(2);
    if max_items == 0 {
        return Vec::new();
    }
    let start = history.len().saturating_sub(max_items);
    let mut recent = history[start..].to_vec();
    if recent.first().is_some_and(|item| item.role != "user") {
        recent.remove(0);
    }
    recent
}

fn record_repl_conversation_turn(
    history: &mut Vec<ConversationHistoryItem>,
    user_input: &str,
    assistant_text: &str,
) {
    history.push(ConversationHistoryItem {
        role: "user".to_string(),
        text: user_input.to_string(),
    });
    history.push(ConversationHistoryItem {
        role: "assistant".to_string(),
        text: assistant_text.to_string(),
    });
    let recent = recent_repl_conversation_history(history, REPL_HISTORY_MAX_TURNS);
    *history = recent;
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

fn fixed_cell(input: &str, width: usize) -> String {
    let preview = compact_preview(input, width);
    format!("{preview:<width$}")
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
    writeln!(stdout, "╭{}╮", "─".repeat(REPL_BANNER_INNER_WIDTH))
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    for line in CHUANG_BANNER_LINES {
        writeln!(stdout, "{}", banner_center_row(line))
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    writeln!(
        stdout,
        "{}",
        banner_center_row("local agent OS / terminal workspace")
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(stdout, "├{}┤", "─".repeat(REPL_BANNER_INNER_WIDTH))
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "{}",
        banner_row(&format!(
            "provider: {}    model: {}",
            summary.provider_id, summary.model_name
        ))
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "{}",
        banner_row("visible: thinking/steps    tools: live stream    answer: scrollback")
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "{}",
        banner_row("commands: /help /status /history /trace /notrace /verbose /quiet /exit")
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(stdout, "╰{}╯", "─".repeat(REPL_BANNER_INNER_WIDTH))
        .map_err(|e| format!("stdout_write_failed: {e}"))?;
    writeln!(
        stdout,
        "\nVisible thinking shows audited runtime/tool progress, not hidden model reasoning."
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn banner_row(text: &str) -> String {
    let text = banner_preview(text, REPL_BANNER_INNER_WIDTH.saturating_sub(2));
    let padding = REPL_BANNER_INNER_WIDTH.saturating_sub(text.chars().count() + 1);
    format!("│ {text}{}│", " ".repeat(padding))
}

fn banner_center_row(text: &str) -> String {
    let text = banner_preview(text, REPL_BANNER_INNER_WIDTH);
    let text_width = text.chars().count();
    let total_padding = REPL_BANNER_INNER_WIDTH.saturating_sub(text_width);
    let left = total_padding / 2;
    let right = total_padding.saturating_sub(left);
    format!("│{}{}{}│", " ".repeat(left), text, " ".repeat(right))
}

fn banner_preview(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut preview = input
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    preview.push('…');
    preview
}

const CHUANG_BANNER_LINES: &[&str] = &[
    " ██████╗██╗  ██╗██╗   ██╗ █████╗ ███╗   ██╗ ██████╗ ",
    "██╔════╝██║  ██║██║   ██║██╔══██╗████╗  ██║██╔════╝ ",
    "██║     ███████║██║   ██║███████║██╔██╗ ██║██║  ███╗",
    "██║     ██╔══██║██║   ██║██╔══██║██║╚██╗██║██║   ██║",
    "╚██████╗██║  ██║╚██████╔╝██║  ██║██║ ╚████║╚██████╔╝",
    " ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝ ",
];

fn handle_repl_command(
    input: &str,
    verbose: &mut bool,
    show_trace: &mut bool,
    options: &CliOptions,
    conversation_history: &[ConversationHistoryItem],
    stdout: &mut io::Stdout,
) -> Result<(), String> {
    match input {
        "/help" | "/?" => {
            writeln!(
                stdout,
                "\nCommands\n  /help      show this help\n  /status    show runtime status summary\n  /history   show recent REPL conversation context\n  /trace     show visible execution trace\n  /notrace   hide visible execution trace\n  /verbose   print full runtime metadata after each turn\n  /quiet     show concise answers only\n  /clear     clear the screen\n  /exit      leave the terminal\n\nMid-task guidance\n  !text      inject creator guidance into the current task at the next safe point\n  plain text while running is also injected into the current task\n  !text while idle is queued for the next submitted task\n\nDisplay\n  Recent REPL conversation is carried into the next turn for continuation prompts.\n  The input box stays visible between turns.\n  Running tasks show THINKING / STEPS and TOOL STREAM progress.\n  Results are split into trace, tool stream, answer, and report id.\n  Hidden model reasoning is not printed.\n"
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/history" => {
            print_repl_section_rule(stdout, "HISTORY")?;
            if conversation_history.is_empty() {
                writeln!(stdout, "no completed REPL turns yet")
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
            } else {
                for item in conversation_history {
                    writeln!(
                        stdout,
                        "{}: {}",
                        item.role,
                        compact_preview(&item.text, 160)
                    )
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
                }
            }
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
    progress_displays: &[ProgressDisplay],
) -> Result<(), String> {
    let meta = &result.response.meta.extra;
    let tool_meta = ToolLoopMeta::from_extra(meta)?;
    let tool_status = meta
        .get("tool_loop_status")
        .map(String::as_str)
        .unwrap_or("none");
    let elapsed_cell = format!("{elapsed_ms}ms");
    let report_id = result
        .response
        .meta
        .extra
        .get("runtime_report_id")
        .map(String::as_str)
        .unwrap_or("pending");
    print_repl_section_rule(stdout, "DONE")?;
    writeln!(
        stdout,
        "┌─ left status ─────────────┬─ center scrollback ───────────────┬─ right tool stream ───────┐\n│ turn      {:<15} │ elapsed   {:<20} │ tools     {:<14} │\n│ status    {:<15} │ protocol  {:<20} │ report    {:<14} │\n└───────────────────────────┴──────────────────────────────────┴──────────────────────────┘",
        turn_count,
        fixed_cell(&elapsed_cell, 20),
        tool_meta.tool_call_count,
        fixed_cell(tool_status, 15),
        tool_meta.tool_protocol_error_count,
        fixed_cell(report_id, 14)
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
    print_repl_section_rule(stdout, "THINKING / STEPS")?;
    let mut step_count = 0usize;
    for display in progress_displays {
        if let ProgressDisplay::Step(text) = display {
            step_count += 1;
            writeln!(stdout, "{}. {}", step_count, text)
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    if step_count == 0 {
        writeln!(stdout, "1. runtime completed without visible step events")
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    writeln!(
        stdout,
        "- hidden model reasoning is not printed; this is audited execution progress"
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    if !tool_meta.tool_events.is_empty() {
        print_repl_section_rule(stdout, "TOOL STREAM")?;
    }
    for event in &tool_meta.tool_events {
        if let Some(line) = format_tool_event(event) {
            writeln!(stdout, "- {line}").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    print_repl_section_rule(stdout, "ANSWER")?;
    print_repl_answer(stdout, result.response.body.trim(), turn_count)?;
    if let Some(report_id) = meta.get("runtime_report_id") {
        print_repl_section_rule(stdout, "REPORT")?;
        writeln!(stdout, "{report_id}").map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn print_repl_answer(
    stdout: &mut io::Stdout,
    answer: &str,
    turn_count: usize,
) -> Result<(), String> {
    let answer_chars = answer.chars().count();
    if answer_chars <= REPL_ANSWER_PREVIEW_CHARS {
        print_wrapped_text(stdout, answer, REPL_TEXT_WRAP_WIDTH)?;
        return Ok(());
    }

    let snapshot_path = write_repl_answer_snapshot(answer, turn_count)?;
    let preview = multiline_preview(answer, REPL_ANSWER_PREVIEW_CHARS);
    print_wrapped_text(stdout, &preview, REPL_TEXT_WRAP_WIDTH)?;
    writeln!(
        stdout,
        "\n[answer truncated: showing {} of {} chars; full saved to {}]",
        REPL_ANSWER_PREVIEW_CHARS,
        answer_chars,
        snapshot_path.display()
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))
}

fn write_repl_answer_snapshot(answer: &str, turn_count: usize) -> Result<PathBuf, String> {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let path = env::temp_dir().join(format!(
        "chuang-repl-answer-{}-{}-{}.txt",
        std::process::id(),
        turn_count,
        ts_ms
    ));
    fs::write(&path, answer).map_err(|e| format!("answer_snapshot_write_failed: {e}"))?;
    Ok(path)
}

fn multiline_preview(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut preview = input
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    preview.push('…');
    preview
}

fn print_wrapped_text(stdout: &mut io::Stdout, text: &str, width: usize) -> Result<(), String> {
    for line in text.lines() {
        if line.is_empty() {
            writeln!(stdout).map_err(|e| format!("stdout_write_failed: {e}"))?;
            continue;
        }
        for wrapped in wrap_line_by_chars(line, width) {
            writeln!(stdout, "{wrapped}").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    if text.ends_with('\n') {
        writeln!(stdout).map_err(|e| format!("stdout_write_failed: {e}"))?;
    }
    Ok(())
}

fn wrap_line_by_chars(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for ch in line.chars() {
        if current_len >= width {
            rows.push(current);
            current = String::new();
            current_len = 0;
        }
        current.push(ch);
        current_len += 1;
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repl_progress_event_formats_live_tool_stream_items() {
        let started = serde_json::json!({
            "kind": "turn_started",
            "details": {
                "input_preview": "看一下 git 状态并报告当前分支"
            }
        })
        .to_string();
        let tool_done = serde_json::json!({
            "kind": "tool_finished",
            "details": {
                "tool": "code_execute",
                "ok": true,
                "summary": "command exited with status 0"
            }
        })
        .to_string();
        let protocol = serde_json::json!({
            "kind": "protocol_error",
            "details": {
                "code": "plain_text_response"
            }
        })
        .to_string();

        assert_eq!(
            format_progress_event(&started),
            Some(ProgressDisplay::Step(
                "turn started: 看一下 git 状态并报告当前分支".to_string()
            ))
        );
        assert_eq!(
            format_progress_event(&tool_done),
            Some(ProgressDisplay::Tool(
                "[code_execute] ok command exited with status 0".to_string()
            ))
        );
        assert_eq!(
            format_progress_event(&protocol),
            Some(ProgressDisplay::Tool(
                "protocol plain_text_response".to_string()
            ))
        );
    }

    #[test]
    fn repl_progress_event_formats_typed_terminal_events() {
        let step = serde_json::json!({
            "schema_version": 2,
            "event": {
                "kind": "step_finished",
                "title": "prepare context",
                "status": "ok",
                "detail": "segments=2"
            }
        })
        .to_string();
        let tool = serde_json::json!({
            "schema_version": 2,
            "event": {
                "kind": "tool_finished",
                "round": 1,
                "tool": "code_execute",
                "ok": true,
                "decision": "allow",
                "summary": "command exited with status 0"
            }
        })
        .to_string();

        assert_eq!(
            format_progress_event(&step),
            Some(ProgressDisplay::Step(
                "context ready ok  segments=2".to_string()
            ))
        );
        assert_eq!(
            format_progress_event(&tool),
            Some(ProgressDisplay::Tool(
                "code_execute: ok command exited with status 0".to_string()
            ))
        );
    }

    #[test]
    fn repl_answer_preview_caps_long_scrollback_without_losing_marker() {
        let long_answer = "a".repeat(REPL_ANSWER_PREVIEW_CHARS + 20);
        let preview = multiline_preview(&long_answer, REPL_ANSWER_PREVIEW_CHARS);

        assert_eq!(preview.chars().count(), REPL_ANSWER_PREVIEW_CHARS);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn repl_answer_wraps_long_lines_for_terminal_readability() {
        let rows = wrap_line_by_chars("abcdefghijkl", 5);

        assert_eq!(rows, vec!["abcde", "fghij", "kl"]);
        assert!(rows.iter().all(|row| row.chars().count() <= 5));
    }

    #[test]
    fn repl_history_keeps_recent_turn_pairs_for_continuation() {
        let mut history = Vec::new();
        for index in 0..10 {
            record_repl_conversation_turn(
                &mut history,
                &format!("user-{index}"),
                &format!("assistant-{index}"),
            );
        }

        assert_eq!(history.len(), REPL_HISTORY_MAX_TURNS * 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].text, "user-2");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].text, "assistant-2");
        assert_eq!(
            history.last().expect("history should have last").text,
            "assistant-9"
        );
    }

    #[test]
    fn repl_history_limit_zero_returns_no_continuation_context() {
        let history = vec![
            ConversationHistoryItem {
                role: "user".to_string(),
                text: "first".to_string(),
            },
            ConversationHistoryItem {
                role: "assistant".to_string(),
                text: "reply".to_string(),
            },
        ];

        assert!(recent_repl_conversation_history(&history, 0).is_empty());
    }

    #[test]
    fn repl_history_window_starts_on_user_boundary() {
        let history = vec![
            ConversationHistoryItem {
                role: "assistant".to_string(),
                text: "orphaned-reply".to_string(),
            },
            ConversationHistoryItem {
                role: "user".to_string(),
                text: "next".to_string(),
            },
            ConversationHistoryItem {
                role: "assistant".to_string(),
                text: "next-reply".to_string(),
            },
        ];

        let recent = recent_repl_conversation_history(&history, 2);

        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].role, "user");
        assert_eq!(recent[0].text, "next");
        assert_eq!(recent[1].role, "assistant");
    }

    #[test]
    fn repl_banner_and_help_advertise_history_command() {
        let help = "\nCommands\n  /help      show this help\n  /status    show runtime status summary\n  /history   show recent REPL conversation context\n";
        let banner =
            banner_row("commands: /help /status /history /trace /notrace /verbose /quiet /exit");
        let title = banner_center_row("CHUANG");

        assert!(help.contains("/history"));
        assert!(banner.contains("/history"));
        assert!(title.contains("CHUANG"));
        assert_eq!(banner.chars().count(), REPL_BANNER_INNER_WIDTH + 2);
        assert_eq!(title.chars().count(), REPL_BANNER_INNER_WIDTH + 2);
    }

    #[test]
    fn repl_banner_rows_keep_fixed_width_for_variable_provider_names() {
        let row = banner_row("provider: very-long-provider-name    model: gpt-5.5");

        assert!(row.starts_with('│'));
        assert!(row.ends_with('│'));
        assert_eq!(row.chars().count(), REPL_BANNER_INNER_WIDTH + 2);
    }

    #[test]
    fn repl_chuang_banner_art_keeps_fixed_width() {
        for line in CHUANG_BANNER_LINES {
            let row = banner_center_row(line);

            assert!(row.starts_with('│'));
            assert!(row.ends_with('│'));
            assert_eq!(row.chars().count(), REPL_BANNER_INNER_WIDTH + 2);
        }
    }

    #[test]
    fn repl_turn_nonce_uses_wall_clock_for_temp_file_uniqueness() {
        let first = repl_turn_nonce();
        let second = repl_turn_nonce();

        assert_ne!(first, second);
        assert!(first.contains('-'));
        assert!(second.contains('-'));
    }
}
