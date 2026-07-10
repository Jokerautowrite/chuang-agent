use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod app_server;
mod cli_approval;
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

use chuang_agent::display_projector::{
    DisplayEvent, DisplayEventKind, DisplayProjectionOptions, DisplayProjector, DisplayProminence,
    DisplayState,
};
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::secret_redaction::redact_sensitive_text;
use chuang_agent::terminal_event::TerminalEvent;
use chuang_agent::tool_loop_meta::ToolLoopMeta;
use cli_approval::{approval_command, resume_local_tty_approval};
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
const REPL_META_WRAP_WIDTH: usize = 92;
const REPL_ACTIVITY_VISIBLE_LIMIT: usize = 14;
static REPL_TURN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_BLUE: &str = "\x1b[38;5;75m";
const ANSI_RED: &str = "\x1b[38;5;203m";
const ANSI_GREEN: &str = "\x1b[38;5;114m";
const ANSI_YELLOW: &str = "\x1b[38;5;222m";
const ANSI_CYAN: &str = "\x1b[38;5;117m";
const ANSI_GRAY: &str = "\x1b[38;5;245m";

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
        Some("approval") => approval_command(&args[2..]),
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
    let show_trace = false;

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
    let mut progress_cursor = ProgressCursor::default();
    let mut stats = ReplSessionStats::from_summary(&summary);
    let mut pending_approval: Option<ReplPendingApproval> = None;
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
    print_repl_prompt(
        stdout,
        false,
        pending_guidance.len(),
        &stats,
        pending_approval.is_some(),
    )?;

    loop {
        poll_progress_events(stdout, running.as_ref(), &mut progress_cursor)?;
        if poll_running_turn(
            stdout,
            &mut running,
            &mut turn_count,
            &mut conversation_history,
            &progress_cursor.displays,
            show_trace,
            verbose,
            &mut pending_guidance,
            &mut stats,
            &mut pending_approval,
        )? {
            print_repl_prompt(
                stdout,
                running.is_some(),
                pending_guidance.len(),
                &stats,
                pending_approval.is_some(),
            )?;
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
            &mut pending_approval,
            &mut stats,
            stdout,
        )? {
            InputAction::Continue => {
                print_repl_prompt(
                    stdout,
                    running.is_some(),
                    pending_guidance.len(),
                    &stats,
                    pending_approval.is_some(),
                )?;
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
    pending_approval: &mut Option<ReplPendingApproval>,
    stats: &mut ReplSessionStats,
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

    if input.eq_ignore_ascii_case("/stop") {
        if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, "[chuang-control] stop")?;
            writeln!(
                stdout,
                "{}■{} 已请求停止，将在当前安全点结束任务。",
                ANSI_YELLOW, ANSI_RESET
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        } else {
            writeln!(stdout, "{}当前没有运行中的任务。{}", ANSI_DIM, ANSI_RESET)
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        return Ok(InputAction::Continue);
    }

    if let Some(approval) = pending_approval.as_ref() {
        match input {
            "1" | "y" | "Y" | "yes" | "YES" => {
                let outcome = resume_local_tty_approval(
                    &options.runtime,
                    &approval.workspace_root,
                    &approval.pending_file,
                )?;
                writeln!(
                    stdout,
                    "{}✓ 已批准一次{}  {}",
                    ANSI_GREEN,
                    ANSI_RESET,
                    humanize_approval_record(&outcome.record)
                )
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
                pending_approval.take();
                let continuation = format!(
                    "继续刚才的任务。用户已在本地终端明确批准并完成了待审批操作。安全回执：{}。请基于这个结果继续，不要重复执行同一操作。",
                    humanize_approval_record(&outcome.record)
                );
                let history =
                    recent_repl_conversation_history(conversation_history, REPL_HISTORY_MAX_TURNS);
                *running = Some(spawn_repl_turn(options.clone(), continuation, history));
            }
            "2" | "n" | "N" | "no" | "NO" => {
                writeln!(
                    stdout,
                    "{}× 已拒绝{}  该操作没有执行，可以输入新要求调整方案。",
                    ANSI_RED, ANSI_RESET
                )
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
                pending_approval.take();
            }
            "3" => {
                writeln!(stdout, "{}", render_approval_details(approval))
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
            }
            _ => {
                writeln!(stdout, "请输入 1、2 或 3。")
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
            }
        }
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
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    writeln!(
        stdout,
        "{}",
        render_user_message_block(&user_input, &summary.provider_id, &summary.model_name, &cwd)
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    let history = recent_repl_conversation_history(conversation_history, REPL_HISTORY_MAX_TURNS);
    *running = Some(spawn_repl_turn(options.clone(), user_input, history));
    stats.mark_turn_started();
    Ok(InputAction::Continue)
}

#[derive(Default)]
struct ProgressCursor {
    bytes_read: u64,
    visible_count: usize,
    displays: Vec<ProgressDisplay>,
    last_message: Option<String>,
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
        cursor.last_message = None;
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
    for line in new_content.lines().filter(|line| !line.trim().is_empty()) {
        if let Some(display) = format_progress_event(line) {
            if cursor.last_message.as_deref() == Some(display.message.as_str()) {
                continue;
            }
            if cursor.visible_count >= REPL_ACTIVITY_VISIBLE_LIMIT && display.suppressible {
                continue;
            }
            if cursor.visible_count == 0 {
                writeln!(
                    stdout,
                    "\n{}{}小创正在处理{}",
                    ANSI_BOLD, ANSI_CYAN, ANSI_RESET
                )
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
            }
            cursor.visible_count += 1;
            print_progress_display_line(stdout, &display)?;
            cursor.last_message = Some(display.message.clone());
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
        return repl_display_projector().project(&event);
    }
    let kind = value.get("kind").and_then(|value| value.as_str())?;
    let details = value
        .get("details")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    match kind {
        "turn_started" => Some(display_progress("正在理解你的要求")),
        "model_started" | "model_finished" | "protocol_error" | "answer_ready" => None,
        "tool_started" => Some(display_tool(
            details
                .get("activity_title")
                .and_then(|value| value.as_str())
                .map(|title| format!("正在{}", compact_preview(title, 36)))
                .unwrap_or_else(|| "正在执行当前操作".to_string()),
        )),
        "tool_finished"
            if !details
                .get("ok")
                .and_then(|value| value.as_bool())
                .unwrap_or(false) =>
        {
            Some(display_warning("当前操作失败，正在保留现场信息"))
        }
        "guidance_injected" => Some(display_progress("已接收新的补充要求")),
        _ => None,
    }
}

fn repl_display_projector() -> DisplayProjector {
    DisplayProjector::new(DisplayProjectionOptions {
        show_successful_tool_events: true,
        show_successful_step_events: true,
        show_model_progress: true,
        show_protocol_warnings: true,
        show_final_ready_event: false,
    })
}

type ProgressDisplay = DisplayEvent;

fn display_progress(message: &str) -> ProgressDisplay {
    ProgressDisplay {
        schema_version: DisplayEvent::schema_version(),
        kind: DisplayEventKind::Progress,
        state: DisplayState::Running,
        prominence: DisplayProminence::Primary,
        suppressible: false,
        message: message.to_string(),
    }
}

fn display_tool(message: String) -> ProgressDisplay {
    ProgressDisplay {
        schema_version: DisplayEvent::schema_version(),
        kind: DisplayEventKind::Tool,
        state: DisplayState::Running,
        prominence: DisplayProminence::Secondary,
        suppressible: true,
        message,
    }
}

fn display_warning(message: &str) -> ProgressDisplay {
    ProgressDisplay {
        schema_version: DisplayEvent::schema_version(),
        kind: DisplayEventKind::Warning,
        state: DisplayState::Failed,
        prominence: DisplayProminence::Alert,
        suppressible: false,
        message: message.to_string(),
    }
}

fn parse_meta_u64(meta: &std::collections::BTreeMap<String, String>, key: &str) -> Option<u64> {
    meta.get(key).and_then(|value| value.parse::<u64>().ok())
}

fn render_repl_status_line(stats: &ReplSessionStats, state: &str) -> String {
    let percent = if stats.context_max_tokens == 0 {
        0
    } else {
        stats
            .context_tokens
            .saturating_mul(100)
            .checked_div(stats.context_max_tokens)
            .unwrap_or(0)
            .min(100)
    };
    format!(
        "{}{} · {} · context {} / {} ({}%) · ↑ {} · ↓ {} · total {}{}",
        ANSI_DIM,
        state,
        stats.model_name,
        compact_token_count(stats.context_tokens),
        compact_token_count(stats.context_max_tokens),
        percent,
        compact_token_count(stats.last_input_tokens),
        compact_token_count(stats.last_output_tokens),
        compact_token_count(stats.session_total_tokens),
        ANSI_RESET
    )
}

fn compact_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}m", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn print_progress_display_line(
    stdout: &mut io::Stdout,
    display: &ProgressDisplay,
) -> Result<(), String> {
    writeln!(stdout, "{}", render_progress_display_line(display))
        .map_err(|e| format!("stdout_write_failed: {e}"))
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
    stats: &mut ReplSessionStats,
    pending_approval: &mut Option<ReplPendingApproval>,
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
                    stats,
                    pending_approval,
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
                    stats,
                    pending_approval,
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

#[derive(Debug, Clone, Default)]
struct ReplSessionStats {
    model_name: String,
    context_tokens: u64,
    context_max_tokens: u64,
    last_input_tokens: u64,
    last_output_tokens: u64,
    session_total_tokens: u64,
    turn_running: bool,
}

impl ReplSessionStats {
    fn from_summary(summary: &chuang_agent::runtime_config::ConfigSummary) -> Self {
        Self {
            model_name: summary.model_name.clone(),
            context_max_tokens: u64::from(summary.context_max_tokens),
            ..Self::default()
        }
    }

    fn mark_turn_started(&mut self) {
        self.turn_running = true;
    }

    fn update_from_result(&mut self, result: &chuang_agent::agent_runtime::RuntimeResult) {
        self.model_name = result.response.model_name.clone();
        self.context_tokens = u64::from(result.packed_token_count);
        self.last_input_tokens =
            parse_meta_u64(&result.response.meta.extra, "aggregate_prompt_tokens")
                .or_else(|| parse_meta_u64(&result.response.meta.extra, "prompt_tokens"))
                .unwrap_or(self.context_tokens);
        self.last_output_tokens =
            parse_meta_u64(&result.response.meta.extra, "aggregate_completion_tokens")
                .or_else(|| parse_meta_u64(&result.response.meta.extra, "completion_tokens"))
                .unwrap_or(0);
        self.session_total_tokens = self.session_total_tokens.saturating_add(
            parse_meta_u64(&result.response.meta.extra, "aggregate_total_tokens")
                .or_else(|| parse_meta_u64(&result.response.meta.extra, "total_tokens"))
                .unwrap_or(
                    self.last_input_tokens
                        .saturating_add(self.last_output_tokens),
                ),
        );
        self.turn_running = false;
    }

    fn mark_turn_finished(&mut self) {
        self.turn_running = false;
    }
}

#[derive(Debug, Clone)]
struct ReplPendingApproval {
    approval_id: String,
    pending_file: PathBuf,
    workspace_root: PathBuf,
    reason: String,
    action: String,
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
    stats: &mut ReplSessionStats,
    pending_approval: &mut Option<ReplPendingApproval>,
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
            stats.update_from_result(&result);
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
            if let Some(approval) = pending_approval_from_result(&result) {
                writeln!(stdout, "{}", render_approval_prompt(&approval))
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
                *pending_approval = Some(approval);
            }
            if verbose {
                print_runtime_result(&result);
            }
        }
        Err(error) => {
            stats.mark_turn_finished();
            print_repl_failure(
                stdout,
                &turn.input_preview,
                elapsed_ms,
                &error,
                progress_displays,
                show_trace,
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
    show_trace: bool,
) -> Result<(), String> {
    let mut progress_lines = Vec::new();
    for display in progress_displays
        .iter()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        progress_lines.push(display.message.clone());
    }
    writeln!(
        stdout,
        "{}",
        render_repl_failure_block(
            input_preview,
            elapsed_ms,
            error,
            &progress_lines,
            show_trace,
        )
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))
}

fn readable_runtime_error(error: &str) -> String {
    if error.contains("turn_cancelled_at_safe_point") {
        return "已按你的要求在安全点停止，未继续执行后续步骤。".to_string();
    }
    if error.contains("tool_loop_exhausted") {
        return "已经完成前面的检查，但最终答复没有生成成功。".to_string();
    }
    if error.contains("repl_turn_disconnected") {
        return "当前任务意外中断，已有结果没有被删除。".to_string();
    }
    "本轮处理没有完成，详细错误可通过 /trace 查看。".to_string()
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
    stats: &ReplSessionStats,
    awaiting_approval: bool,
) -> Result<(), String> {
    writeln!(stdout).map_err(|e| format!("stdout_write_failed: {e}"))?;
    write!(
        stdout,
        "{}",
        render_repl_prompt(running, guidance_count, stats, awaiting_approval)
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
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
    writeln!(stdout, "{}", render_repl_banner(&summary))
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
    conversation_history: &[ConversationHistoryItem],
    stdout: &mut io::Stdout,
) -> Result<(), String> {
    match input {
        "/help" | "/?" => {
            writeln!(
                stdout,
                "\n命令\n  /help      查看帮助\n  /status    查看运行状态\n  /history   查看最近对话\n  /stop      在安全点停止当前任务\n  /trace     显示技术细节\n  /notrace   隐藏技术细节\n  /verbose   显示完整运行元数据\n  /quiet     恢复简洁模式\n  /clear     清屏\n  /exit      退出\n\n任务进行中\n  !补充内容  在下一个安全点补充要求\n  直接输入文字也会加入当前任务\n\n默认展示可读的判断进展、操作目的和结果；不会展示模型隐藏的原始思维链，也不会打印原始密钥或完整命令。\n"
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
            writeln!(stdout, "完整元数据已开启")
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/trace" => {
            *show_trace = true;
            writeln!(stdout, "技术细节已开启").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/notrace" => {
            *show_trace = false;
            writeln!(stdout, "技术细节已隐藏").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/quiet" => {
            *verbose = false;
            writeln!(stdout, "已恢复简洁模式").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/clear" => {
            write!(stdout, "\x1b[2J\x1b[H").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/exit" | "/quit" => {}
        _ => {
            writeln!(stdout, "无法识别这个命令，请输入 /help 查看可用命令。")
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
    _input_preview: &str,
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
    let trace_lines = if show_trace {
        visible_trace_lines(result, elapsed_ms, &tool_meta, tool_status)
    } else {
        Vec::new()
    };
    let answer = render_repl_answer_text(result.response.body.trim(), turn_count)?;
    let metadata_line =
        render_completion_metadata_line(&elapsed_cell, tool_status, pending_guidance_count);
    writeln!(
        stdout,
        "{}",
        render_assistant_completion_block(
            result.response.model_name.as_str(),
            &answer,
            &metadata_line,
            &trace_lines,
            render_completion_audit_line(
                show_trace,
                progress_displays,
                tool_meta.tool_call_count,
                tool_meta.tool_protocol_error_count,
                report_id,
            )
            .as_deref(),
        )
    )
    .map_err(|e| format!("stdout_write_failed: {e}"))?;
    stdout
        .flush()
        .map_err(|e| format!("stdout_flush_failed: {e}"))
}

fn render_repl_answer_text(answer: &str, turn_count: usize) -> Result<String, String> {
    let answer_chars = answer.chars().count();
    if answer_chars <= REPL_ANSWER_PREVIEW_CHARS {
        return Ok(wrap_text_block(answer, REPL_TEXT_WRAP_WIDTH));
    }

    let snapshot_path = write_repl_answer_snapshot(answer, turn_count)?;
    let preview = multiline_preview(answer, REPL_ANSWER_PREVIEW_CHARS);
    let mut rendered = wrap_text_block(&preview, REPL_TEXT_WRAP_WIDTH);
    rendered.push_str(&format!(
        "\n{}[答复较长：当前显示 {} / {} 字符；完整内容已保存到 {}]{}",
        ANSI_DIM,
        REPL_ANSWER_PREVIEW_CHARS,
        answer_chars,
        snapshot_path.display(),
        ANSI_RESET
    ));
    Ok(rendered)
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

fn wrap_text_block(text: &str, width: usize) -> String {
    let mut output = String::new();
    for line in text.lines() {
        if line.is_empty() {
            output.push('\n');
            continue;
        }
        for wrapped in wrap_line_by_chars(line, width) {
            output.push_str(&wrapped);
            output.push('\n');
        }
    }
    if text.ends_with('\n') {
        output.push('\n');
    }
    output.trim_end_matches('\n').to_string()
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

fn visible_trace_lines(
    result: &chuang_agent::agent_runtime::RuntimeResult,
    elapsed_ms: u128,
    tool_meta: &ToolLoopMeta,
    tool_status: &str,
) -> Vec<String> {
    vec![
        format!(
            "trace context={} tokens={} recall_hits={} dropped={}",
            result.context_engine_kind,
            result.packed_token_count,
            result.recall_hit_count,
            result.dropped_segment_ids.len()
        ),
        format!(
            "trace model={} finish={}",
            result.response.model_name,
            result
                .response
                .meta
                .finish_reason
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "trace runtime elapsed={}ms tools={} protocol_errors={} status={}",
            elapsed_ms, tool_meta.tool_call_count, tool_meta.tool_protocol_error_count, tool_status
        ),
    ]
}

fn print_repl_section_rule(stdout: &mut io::Stdout, title: &str) -> Result<(), String> {
    writeln!(stdout, "\n{}{}{} {}", ANSI_DIM, "·", ANSI_RESET, title)
        .map_err(|e| format!("stdout_write_failed: {e}"))
}

fn render_repl_banner(summary: &chuang_agent::runtime_config::ConfigSummary) -> String {
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    [
        format!("{}{}", ANSI_BOLD, ANSI_CYAN),
        " ██████╗██╗  ██╗██╗   ██╗ █████╗ ███╗   ██╗ ██████╗ ".to_string(),
        "██╔════╝██║  ██║██║   ██║██╔══██╗████╗  ██║██╔════╝ ".to_string(),
        "██║     ███████║██║   ██║███████║██╔██╗ ██║██║  ███╗".to_string(),
        "██║     ██╔══██║██║   ██║██╔══██║██║╚██╗██║██║   ██║".to_string(),
        "╚██████╗██║  ██║╚██████╔╝██║  ██║██║ ╚████║╚██████╔╝".to_string(),
        " ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝ ".to_string(),
        format!("                  A G E N T{}", ANSI_RESET),
        format!(
            "\n{}model{} {}  {}profile{} {}",
            ANSI_DIM,
            ANSI_RESET,
            summary.model_name,
            ANSI_DIM,
            ANSI_RESET,
            summary.permission_profile
        ),
        format!("{}workspace{} {}", ANSI_DIM, ANSI_RESET, cwd),
        format!(
            "{}help{} /help   {}stop{} /stop   {}exit{} /exit",
            ANSI_DIM, ANSI_RESET, ANSI_DIM, ANSI_RESET, ANSI_DIM, ANSI_RESET
        ),
    ]
    .join("\n")
}

fn render_user_message_block(
    user_input: &str,
    provider_id: &str,
    model_name: &str,
    cwd: &str,
) -> String {
    let mut lines = vec![format!("{}{}你{}", ANSI_BOLD, ANSI_BLUE, ANSI_RESET)];
    for line in wrap_text_block(user_input, REPL_TEXT_WRAP_WIDTH).lines() {
        lines.push(format!("{}│{} {}", ANSI_BLUE, ANSI_RESET, line));
    }
    lines.push(format!(
        "{}{}{} · {}{}{} · {}{}{}",
        ANSI_DIM,
        provider_id,
        ANSI_RESET,
        ANSI_DIM,
        model_name,
        ANSI_RESET,
        ANSI_DIM,
        cwd,
        ANSI_RESET
    ));
    lines.push(String::new());
    lines.join("\n")
}

fn render_progress_display_line(display: &ProgressDisplay) -> String {
    let (icon, color) = match (display.kind, display.state) {
        (DisplayEventKind::Warning, DisplayState::Blocked) => ("!", ANSI_YELLOW),
        (DisplayEventKind::Warning, _) => ("!", ANSI_RED),
        (_, DisplayState::Succeeded) => ("✓", ANSI_GREEN),
        (_, _) => ("·", ANSI_YELLOW),
    };
    format!(
        "{}{}{} {}",
        color,
        icon,
        ANSI_RESET,
        compact_preview(&display.message, REPL_META_WRAP_WIDTH)
    )
}

fn render_repl_prompt(
    running: bool,
    guidance_count: usize,
    stats: &ReplSessionStats,
    awaiting_approval: bool,
) -> String {
    if awaiting_approval {
        return format!(
            "{}╭─ 审批选择{}\n{}╰─ 请选择 1 / 2 / 3{}\n{}\n",
            ANSI_YELLOW,
            ANSI_RESET,
            ANSI_YELLOW,
            ANSI_RESET,
            render_repl_status_line(stats, "等待确认")
        );
    }
    if running {
        let hint = if guidance_count > 0 {
            format!("已排队 {guidance_count} 条补充要求")
        } else {
            "可输入补充要求，/stop 停止".to_string()
        };
        return format!(
            "{}╭─ 当前任务{}\n{}╰─ {}{}\n{}\n",
            ANSI_YELLOW,
            ANSI_RESET,
            ANSI_YELLOW,
            hint,
            ANSI_RESET,
            render_repl_status_line(stats, "运行中")
        );
    }
    format!(
        "{}╭─ 输入{}\n{}│{} \n{}╰─{}\n{}\n\x1b[3A\x1b[3C",
        ANSI_BLUE,
        ANSI_RESET,
        ANSI_BLUE,
        ANSI_RESET,
        ANSI_BLUE,
        ANSI_RESET,
        render_repl_status_line(stats, "就绪")
    )
}

fn pending_approval_from_result(
    result: &chuang_agent::agent_runtime::RuntimeResult,
) -> Option<ReplPendingApproval> {
    let meta = &result.response.meta.extra;
    let pending_file = PathBuf::from(meta.get("pending_approval_path")?);
    let approval_id = meta.get("pending_approval_id")?.clone();
    let workspace_root = env::current_dir().ok()?;
    let pending: chuang_agent::tool_runtime::PendingApproval =
        serde_json::from_slice(&fs::read(&pending_file).ok()?).ok()?;
    let call: chuang_agent::tool_runtime::ToolCall =
        serde_json::from_str(&pending.serialized_tool_call).ok()?;
    Some(ReplPendingApproval {
        approval_id,
        pending_file,
        workspace_root,
        reason: pending.risk_decision.reason,
        action: approval_action_summary(&call),
    })
}

fn approval_action_summary(call: &chuang_agent::tool_runtime::ToolCall) -> String {
    use chuang_agent::tool_runtime::ToolCall;
    match call {
        ToolCall::ListDir { path } => format!("查看目录 {}", safe_preview(path)),
        ToolCall::ReadFile { path } => format!("读取文件 {}", safe_preview(path)),
        ToolCall::WriteFile { path, .. } => format!("写入文件 {}", safe_preview(path)),
        ToolCall::Mouse { x, y } => format!("鼠标操作 ({x}, {y})"),
        ToolCall::Keyboard { secret, .. } => {
            if *secret {
                "输入敏感信息（内容不显示）".to_string()
            } else {
                "键盘输入".to_string()
            }
        }
        ToolCall::Screenshot { .. } => "截取画面".to_string(),
        ToolCall::Locate { .. } => "定位界面元素".to_string(),
        ToolCall::OpenApp { app_name } => format!("打开应用 {}", safe_preview(app_name)),
        ToolCall::Wait { millis } => format!("等待 {millis}ms"),
        ToolCall::HumanSuspend { .. } => "等待人工处理".to_string(),
        ToolCall::ApplyPatch { .. } => "应用代码补丁".to_string(),
        ToolCall::ShellExec { command, .. } => {
            format!("执行高风险终端操作 {}", safe_preview(command))
        }
        ToolCall::MemoryRecall { query, .. } => format!("检索记忆 {}", safe_preview(query)),
    }
}

fn safe_preview(value: &str) -> String {
    compact_preview(&redact_sensitive_text("terminal-preview", value).text, 72)
}

fn render_approval_prompt(approval: &ReplPendingApproval) -> String {
    format!(
        "\n{}{}需要你的确认{}\n{}操作{} {}\n{}原因{} {}\n\n  {}[1] 允许一次{}   [2] 拒绝   [3] 查看详情",
        ANSI_BOLD,
        ANSI_YELLOW,
        ANSI_RESET,
        ANSI_DIM,
        ANSI_RESET,
        approval.action,
        ANSI_DIM,
        ANSI_RESET,
        safe_preview(&approval.reason),
        ANSI_GREEN,
        ANSI_RESET
    )
}

fn render_approval_details(approval: &ReplPendingApproval) -> String {
    format!(
        "{}审批编号{} {}\n{}一次性范围{} 当前操作的精确指纹\n{}待审批记录{} {}",
        ANSI_DIM,
        ANSI_RESET,
        approval.approval_id,
        ANSI_DIM,
        ANSI_RESET,
        ANSI_DIM,
        ANSI_RESET,
        approval.pending_file.display()
    )
}

fn humanize_approval_record(record: &chuang_agent::tool_runtime::ToolExecutionRecord) -> String {
    if record.ok {
        format!(
            "{}成功，耗时 {}ms",
            approval_action_summary(&record.call),
            record.duration_ms
        )
    } else {
        format!(
            "{}失败：{}",
            approval_action_summary(&record.call),
            safe_preview(&record.summary)
        )
    }
}

#[cfg(test)]
fn approval_fixture(approval_id: &str, action: &str, reason: &str) -> ReplPendingApproval {
    ReplPendingApproval {
        approval_id: approval_id.to_string(),
        pending_file: PathBuf::from("/tmp/pending-approval.json"),
        workspace_root: PathBuf::from("/tmp/workspace"),
        reason: reason.to_string(),
        action: action.to_string(),
    }
}

fn render_repl_failure_block(
    _input_preview: &str,
    elapsed_ms: u128,
    error: &str,
    progress_lines: &[String],
    show_trace: bool,
) -> String {
    let mut lines = vec![
        format!("{}{}小创{}", ANSI_BOLD, ANSI_RED, ANSI_RESET),
        format!(
            "{}本轮没有完成：{}{}",
            ANSI_RED,
            ANSI_RESET,
            readable_runtime_error(error)
        ),
        format!(
            "{}已用时 {:.1} 秒{}",
            ANSI_DIM,
            elapsed_ms as f64 / 1000.0,
            ANSI_RESET
        ),
    ];
    if progress_lines.is_empty() {
        lines.push(format!(
            "{}没有捕获到可展示的工作进展。{}",
            ANSI_DIM, ANSI_RESET
        ));
    } else {
        for line in progress_lines.iter().take(4) {
            lines.push(format!("{}· {}{}", ANSI_DIM, line, ANSI_RESET));
        }
    }
    if show_trace {
        lines.push(format!(
            "{}技术细节{} {}",
            ANSI_DIM,
            ANSI_RESET,
            compact_preview(error, REPL_META_WRAP_WIDTH)
        ));
    }
    lines.push(format!(
        "{}可以直接补充要求后重试；输入 /trace 可查看技术细节。{}",
        ANSI_GRAY, ANSI_RESET
    ));
    lines.join("\n")
}

fn render_assistant_completion_block(
    model_name: &str,
    answer: &str,
    metadata_line: &str,
    trace_lines: &[String],
    audit_line: Option<&str>,
) -> String {
    let mut lines = vec![format!(
        "{}{}小创{} {}{}{}",
        ANSI_BOLD, ANSI_CYAN, ANSI_RESET, ANSI_DIM, model_name, ANSI_RESET
    )];
    for line in trace_lines {
        lines.push(format!("{}{}{}", ANSI_DIM, line, ANSI_RESET));
    }
    if let Some(audit_line) = audit_line {
        lines.push(format!("{}技术细节{} {}", ANSI_DIM, ANSI_RESET, audit_line));
    }
    lines.push(answer.to_string());
    lines.push(metadata_line.to_string());
    lines.join("\n")
}

fn render_completion_audit_line(
    show_trace: bool,
    progress_displays: &[ProgressDisplay],
    tool_calls: usize,
    protocol_errors: usize,
    report_id: &str,
) -> Option<String> {
    if !show_trace {
        return None;
    }
    let last_progress = progress_displays
        .last()
        .map(|display| compact_preview(&display.message, 36))
        .unwrap_or_else(|| "无可见进展".to_string());
    Some(format!(
        "最近进展={}  tools={}  protocol={}  report={}",
        last_progress, tool_calls, protocol_errors, report_id
    ))
}

fn wrap_single_line(input: &str, width: usize) -> String {
    compact_preview(input, width)
}

fn render_completion_metadata_line(
    elapsed: &str,
    tool_status: &str,
    pending_guidance_count: usize,
) -> String {
    let mut parts = vec![format!("耗时 {elapsed}")];
    match tool_status {
        "human_input_required" => parts.push("等待你的确认".to_string()),
        "completed_after_tool_limit" => parts.push("已在执行后整理答复".to_string()),
        "tool_loop_exhausted" => parts.push("答复未完整收口".to_string()),
        _ => {}
    }
    if pending_guidance_count > 0 {
        parts.push(format!("已排队 {pending_guidance_count} 条补充要求"));
    }
    format!(
        "{}{}{}",
        ANSI_DIM,
        wrap_single_line(&parts.join("  "), REPL_META_WRAP_WIDTH),
        ANSI_RESET
    )
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
            Some(display_progress("正在理解你的要求"))
        );
        assert_eq!(format_progress_event(&tool_done), None);
        assert_eq!(format_progress_event(&protocol), None);
    }

    #[test]
    fn repl_progress_event_formats_typed_terminal_events() {
        let step = serde_json::json!({
            "schema_version": 2,
            "event": {
                "kind": "step_started",
                "title": "准备上下文",
                "detail": "整理身份和最近对话"
            }
        })
        .to_string();
        let tool = serde_json::json!({
            "schema_version": 2,
            "event": {
                "kind": "tool_started",
                "round": 1,
                "tool": "code_execute",
                "summary": null,
                "activity_title": "检查 Git 状态",
                "activity_detail": "查看版本库当前状态"
            }
        })
        .to_string();

        assert_eq!(
            format_progress_event(&step).unwrap().message,
            "正在准备上下文"
        );
        assert_eq!(
            format_progress_event(&tool).unwrap().message,
            "正在检查 Git 状态 · 查看版本库当前状态"
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
    fn repl_user_block_uses_accent_line_and_metadata() {
        let rendered = render_user_message_block(
            "请检查当前分支并总结\n不要省略第二行",
            "openai_compatible",
            "gpt-5.5",
            "/tmp/work",
        );

        assert!(rendered.contains("你"));
        assert!(rendered.contains("│"));
        assert!(rendered.contains("请检查当前分支并总结"));
        assert!(rendered.contains("不要省略第二行"));
        assert!(rendered.contains("openai_compatible"));
        assert!(rendered.contains("gpt-5.5"));
        assert!(rendered.contains("/tmp/work"));
        assert!(!rendered.contains("type !text"));
    }

    #[test]
    fn repl_progress_line_is_compact_transcript_style() {
        let rendered = render_progress_display_line(&display_tool("正在检查 Git 状态".to_string()));

        assert!(rendered.contains("检查 Git 状态"));
        assert!(!rendered.contains("tool"));
        assert!(!rendered.contains("thinking"));
        assert!(!rendered.contains(" 3"));
        assert!(!rendered.contains("TOOL STREAM"));
        assert!(!rendered.contains("│"));
    }

    #[test]
    fn repl_assistant_completion_has_answer_and_muted_metadata_without_old_wall_labels() {
        let rendered = render_assistant_completion_block(
            "gpt-5.5",
            "answer body",
            &render_completion_metadata_line("120ms", "ok", 0),
            &["trace model=gpt-5.5 finish=stop".to_string()],
            Some("最近进展=正在检查 Git 状态  tools=1  protocol=0  report=rpt_123"),
        );

        assert!(rendered.contains("小创"));
        assert!(rendered.contains("answer body"));
        assert!(rendered.contains("技术细节"));
        assert!(rendered.contains("rpt_123"));
        assert!(rendered.contains("耗时 120ms"));
        assert!(!rendered.contains("DONE"));
        assert!(!rendered.contains("TRACE"));
        assert!(!rendered.contains("THINKING"));
        assert!(!rendered.contains("TOOL STREAM"));
        assert!(!rendered.contains("ANSWER"));
    }

    #[test]
    fn repl_completion_audit_is_hidden_by_default_and_available_in_trace() {
        let displays = vec![
            display_progress("正在理解你的要求"),
            display_tool("正在检查 Git 状态".to_string()),
        ];

        assert!(render_completion_audit_line(false, &displays, 1, 0, "report-1").is_none());
        let audit = render_completion_audit_line(true, &displays, 1, 0, "report-1")
            .expect("trace should include compact technical details");

        assert!(audit.contains("正在检查 Git 状态"));
        assert!(audit.contains("tools=1"));
        assert!(audit.contains("report=report-1"));
    }

    #[test]
    fn repl_banner_is_large_and_mentions_controls() {
        let summary = chuang_agent::runtime_config::ConfigSummary {
            provider_kind: "openai_compatible".to_string(),
            provider_id: "provider-x".to_string(),
            model_name: "gpt-5.5".to_string(),
            provider_tls_ca_cert_path: None,
            provider_request_timeout_ms: Some(30_000),
            provider_reasoning_effort: None,
            provider_fallback_policy: None,
            governance_kind: "rules".to_string(),
            permission_profile: "full_local_workspace".to_string(),
            approval_policy: "auto_for_workspace".to_string(),
            permission_workspace_root: "/home/user/projects/chuang-agent".to_string(),
            actuator_kind: "none".to_string(),
            subagent_kind: "none".to_string(),
            subagent_live_worker: chuang_agent::runtime_config::SubagentLiveWorkerSummary {
                enabled: false,
                adapter_kind: "none".to_string(),
                status: "disabled".to_string(),
                starts_worker: false,
                available: false,
                reason: "disabled".to_string(),
            },
            subagent_queue_root: "queue".to_string(),
            evolution_kind: "none".to_string(),
            control_plane_kind: "none".to_string(),
            control_command_timeout_ms: None,
            external_knowledge_wiki_endpoint: None,
            external_knowledge_wiki_token_env: None,
            external_knowledge_wiki_timeout_ms: None,
            external_knowledge_gbrain_endpoint: None,
            external_knowledge_gbrain_token_env: None,
            external_knowledge_gbrain_timeout_ms: None,
            actuator_command_timeout_ms: None,
            identity_memory_kind: "fs".to_string(),
            identity_memory_root: "mem".to_string(),
            identity_experiences_path: "exp".to_string(),
            identity_user_max_chars: 0,
            identity_memory_max_chars: 0,
            identity_root: "identity".to_string(),
            soul_path: "soul".to_string(),
            story_path: "story".to_string(),
            first_wake_path: "first_wake".to_string(),
            agents_registry_path: "agents.toml".to_string(),
            rules_root: "rules".to_string(),
            rules_core_path: "rules/core.toml".to_string(),
            context_engine_kind: "deterministic".to_string(),
            tool_loop_max_rounds: 4,
            tool_shell_timeout_ms: 10_000,
            tool_shell_risk_rule_counts: "0".to_string(),
            db_path: "db.sqlite".to_string(),
            recall_limit: 5,
            context_max_tokens: 8192,
            context_reserve_system_tokens: 512,
            context_min_working_tokens: 1024,
            context_max_tool_results: 8,
            context_max_memory_segments: 8,
            api_key_state: Some("set".to_string()),
            placeholder_warnings: Vec::new(),
        };
        let rendered = render_repl_banner(&summary);

        assert!(rendered.contains("██████"));
        assert!(rendered.contains("A G E N T"));
        assert!(rendered.contains("gpt-5.5"));
        assert!(rendered.contains("full_local_workspace"));
        assert!(rendered.contains("/stop"));
    }

    #[test]
    fn repl_metadata_line_is_wrapped_without_old_table() {
        let rendered = render_completion_metadata_line("999ms", "tool_loop_exhausted", 7);

        assert!(rendered.contains("耗时 999ms"));
        assert!(rendered.contains("答复未完整收口"));
        assert!(rendered.contains("已排队 7 条补充要求"));
        assert!(!rendered.contains("tools"));
        assert!(!rendered.contains("protocol"));
        assert!(!rendered.contains("report"));
        assert!(!rendered.contains("┌"));
        assert!(
            compact_preview(&rendered, REPL_META_WRAP_WIDTH + 20)
                .chars()
                .count()
                <= REPL_META_WRAP_WIDTH + 20
        );
    }

    #[test]
    fn repl_turn_nonce_uses_wall_clock_for_temp_file_uniqueness() {
        let first = repl_turn_nonce();
        let second = repl_turn_nonce();

        assert_ne!(first, second);
        assert!(first.contains('-'));
        assert!(second.contains('-'));
    }

    #[test]
    fn repl_prompt_has_input_box_and_token_status() {
        let stats = ReplSessionStats {
            model_name: "gpt-5.6-terra".to_string(),
            context_tokens: 27_200,
            context_max_tokens: 272_000,
            last_input_tokens: 2_100,
            last_output_tokens: 684,
            session_total_tokens: 9_200,
            turn_running: false,
        };

        let rendered = render_repl_prompt(false, 0, &stats, false);

        assert!(rendered.contains("╭─ 输入"));
        assert!(rendered.contains("context 27.2k / 272.0k (10%)"));
        assert!(rendered.contains("↑ 2.1k"));
        assert!(rendered.contains("↓ 684"));
        assert!(rendered.contains("gpt-5.6-terra"));
    }

    #[test]
    fn repl_running_prompt_exposes_stop_control() {
        let stats = ReplSessionStats {
            model_name: "gpt-5.6-terra".to_string(),
            context_max_tokens: 272_000,
            turn_running: true,
            ..ReplSessionStats::default()
        };

        let rendered = render_repl_prompt(true, 0, &stats, false);

        assert!(rendered.contains("当前任务"));
        assert!(rendered.contains("/stop 停止"));
        assert!(rendered.contains("运行中"));
    }

    #[test]
    fn repl_approval_prompt_is_explicit_and_redacts_secrets() {
        let approval = approval_fixture(
            "approval-1",
            "读取文件 .env",
            "secret access requires approval; api_key=sk-sensitive-value",
        );

        let rendered = render_approval_prompt(&approval);

        assert!(rendered.contains("需要你的确认"));
        assert!(rendered.contains("[1] 允许一次"));
        assert!(rendered.contains("[2] 拒绝"));
        assert!(rendered.contains("[3] 查看详情"));
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("sk-sensitive-value"));
    }

    #[test]
    fn repl_status_tracks_context_usage_and_session_total() {
        let summary =
            chuang_agent::runtime_config::RuntimeConfig::new(PathBuf::from("memory.db")).summary();
        let mut stats = ReplSessionStats::from_summary(&summary);
        let mut meta = std::collections::BTreeMap::new();
        meta.insert("prompt_tokens".to_string(), "1200".to_string());
        meta.insert("completion_tokens".to_string(), "300".to_string());
        meta.insert("total_tokens".to_string(), "1500".to_string());
        meta.insert("aggregate_prompt_tokens".to_string(), "2400".to_string());
        meta.insert("aggregate_completion_tokens".to_string(), "600".to_string());
        meta.insert("aggregate_total_tokens".to_string(), "3000".to_string());
        let result = chuang_agent::agent_runtime::RuntimeResult {
            prompt: String::new(),
            response: chuang_agent::agent_runtime::RuntimeResponse {
                model_name: "gpt-5.6-terra".to_string(),
                body: "ok".to_string(),
                trace: String::new(),
                meta: chuang_agent::responder::ResponderMeta {
                    provider: Some("test".to_string()),
                    recall_hit_count: Some(0),
                    finish_reason: Some("stop".to_string()),
                    extra: meta,
                },
            },
            recall_summary: String::new(),
            recall_hit_count: 0,
            context_engine_kind: "deterministic".to_string(),
            packed_context_preview: String::new(),
            packed_token_count: 27_200,
            dropped_segment_ids: Vec::new(),
            context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
                drop_reasons: Vec::new(),
                budget_exceeded: false,
                budget_exceeded_reasons: Vec::new(),
                working_reservation: None,
            },
        };

        stats.update_from_result(&result);

        assert_eq!(stats.context_tokens, 27_200);
        assert_eq!(stats.last_input_tokens, 2_400);
        assert_eq!(stats.last_output_tokens, 600);
        assert_eq!(stats.session_total_tokens, 3_000);
    }
}
