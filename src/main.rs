use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

/// Display width helpers (ASCII=1, most CJK=2).
fn ansi_plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for x in chars.by_ref() {
                    if x.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn display_width(s: &str) -> usize {
    ansi_plain(s)
        .chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c <= '\u{1f}' {
                0
            } else if c <= '\u{7e}' {
                1
            } else {
                2
            }
        })
        .sum()
}

fn fit_display(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if display_width(s) <= max_cols {
        return s.to_string();
    }
    let plain = ansi_plain(s);
    let mut w = 0usize;
    let mut out = String::new();
    for c in plain.chars() {
        let cw = if c <= '\u{7e}' { 1 } else { 2 };
        if w + cw > max_cols.saturating_sub(1) {
            out.push('…');
            break;
        }
        out.push(c);
        w += cw;
    }
    out
}

fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

/// Grok-style chrome: app owns input buffer + fixed bottom strip.
///
/// Layout (bottom 3 rows, matches right-side Grok TUI):
/// ```text
/// ────────────────────────────────────────  hairline
/// > 输入消息…                               input box (primary)
/// 就绪 · gpt-5.6-terra · /help              footer status
/// ```
/// Transcript scrolls only above the hairline (DECSTBM).
struct ReplChrome {
    cols: u16,
    rows: u16,
    /// Bottom rows reserved: rule + input + status.
    reserve: u16,
    raw_active: bool,
}

impl ReplChrome {
    /// Hairline + input + status footer.
    const RESERVE_ROWS: u16 = 3;

    fn detect(_interactive: bool) -> Self {
        let (cols, rows) = terminal_size();
        Self {
            cols,
            rows,
            reserve: Self::RESERVE_ROWS,
            raw_active: false,
        }
    }

    fn refresh_size(&mut self) {
        let (cols, rows) = terminal_size();
        self.cols = cols.max(40);
        self.rows = rows.max(10);
    }

    fn body_bottom_row(&self) -> u16 {
        self.rows.saturating_sub(self.reserve).max(1)
    }

    /// Row indices (1-based): rule, input, status.
    fn strip_rows(&self) -> (u16, u16, u16) {
        let rule = self.rows.saturating_sub(self.reserve) + 1;
        let input = rule + 1;
        let status = rule + 2;
        (rule, input, status)
    }

    fn enable(&mut self, stdout: &mut io::Stdout) -> Result<(), String> {
        self.refresh_size();
        enable_raw_mode().map_err(|e| format!("raw_mode_failed: {e}"))?;
        self.raw_active = true;
        // Keep main scrollback; pin chrome with DECSTBM + app-drawn input.
        let bottom = self.body_bottom_row();
        write!(stdout, "\x1b[1;{bottom}r").map_err(|e| format!("stdout_write_failed: {e}"))?;
        execute!(stdout, Hide).map_err(|e| format!("stdout_write_failed: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("stdout_flush_failed: {e}"))
    }

    fn disable(&mut self, stdout: &mut io::Stdout) -> Result<(), String> {
        // Reset scroll region, clear strip, show cursor, leave raw mode.
        let _ = write!(stdout, "\x1b[r");
        let _ = self.clear_prompt_strip(stdout);
        let _ = execute!(stdout, Show);
        let _ = stdout.flush();
        if self.raw_active {
            let _ = disable_raw_mode();
            self.raw_active = false;
        }
        Ok(())
    }

    fn write_body(&mut self, stdout: &mut io::Stdout, text: &str) -> Result<(), String> {
        self.refresh_size();
        let bottom = self.body_bottom_row();
        // Ensure scroll region still correct after resize.
        write!(stdout, "\x1b[1;{bottom}r").map_err(|e| format!("stdout_write_failed: {e}"))?;
        write!(stdout, "\x1b[{bottom};1H").map_err(|e| format!("stdout_write_failed: {e}"))?;
        // In raw mode, need explicit \r\n for newline.
        let normalized = text.replace('\n', "\r\n");
        write!(stdout, "{normalized}").map_err(|e| format!("stdout_write_failed: {e}"))?;
        if !normalized.ends_with("\r\n") {
            write!(stdout, "\r\n").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        stdout
            .flush()
            .map_err(|e| format!("stdout_flush_failed: {e}"))
    }

    /// Paint fixed bottom strip (Grok layout): hairline + input box + status footer.
    /// `input` is the live draft (app-owned, not terminal echo).
    fn pin_prompt(
        &mut self,
        stdout: &mut io::Stdout,
        status_line: &str,
        input: &str,
    ) -> Result<(), String> {
        self.refresh_size();
        let cols = self.cols as usize;
        let (rule_row, input_row, status_row) = self.strip_rows();

        // 1) Full-width hairline — the visual top of the "input box".
        let rule = format!(
            "{ANSI_DIM}{}{ANSI_RESET}",
            "─".repeat(cols.max(8))
        );

        // 2) Input row: Grok `> ` glyph + draft (or dim placeholder).
        let prefix = "> ";
        let prefix_w = display_width(prefix);
        let avail = cols.saturating_sub(prefix_w + 1).max(8);
        let empty = input.is_empty();
        let mut visible = if empty {
            String::new()
        } else {
            input.to_string()
        };
        while display_width(&visible) > avail && !visible.is_empty() {
            let mut chars = visible.chars();
            chars.next();
            visible = chars.as_str().to_string();
        }
        // Soft "box": dark bar behind the whole input row (Konsole / xterm-256).
        // Re-apply BG after every SGR reset so the bar stays solid.
        const BG: &str = "\x1b[48;5;236m";
        const FG: &str = "\x1b[38;5;252m";
        let placeholder = "输入消息…  Enter 发送  /help";
        let used = prefix_w
            + if empty {
                display_width(placeholder)
            } else {
                display_width(&visible)
            };
        let pad = " ".repeat(cols.saturating_sub(used).min(cols));
        let input_line = if empty {
            format!(
                "{BG}{ANSI_BOLD}{ANSI_CYAN}{prefix}{ANSI_RESET}{BG}{ANSI_DIM}{placeholder}{ANSI_RESET}{BG}{pad}{ANSI_RESET}"
            )
        } else {
            format!(
                "{BG}{ANSI_BOLD}{ANSI_CYAN}{prefix}{ANSI_RESET}{BG}{FG}{visible}{ANSI_RESET}{BG}{pad}{ANSI_RESET}"
            )
        };

        // 3) Footer status (Grok puts model/mode under the input).
        let status_plain = fit_display(&ansi_plain(status_line), cols.saturating_sub(1));
        // Re-color: keep running/confirm bright; idle dim.
        let status_painted = if status_line.contains("运行中") || status_line.contains("确认") {
            // status_line already carries ANSI for 运行中/确认.
            fit_display_ansi(status_line, cols.saturating_sub(1))
        } else {
            format!("{ANSI_DIM}{status_plain}{ANSI_RESET}")
        };

        // Paint three reserved rows (clear each, never wrap).
        write!(
            stdout,
            "\x1b[{rule_row};1H\x1b[2K{rule}\
             \x1b[{input_row};1H\x1b[2K{input_line}\
             \x1b[{status_row};1H\x1b[2K{status_painted}"
        )
        .map_err(|e| format!("stdout_write_failed: {e}"))?;

        // Caret sits in the input box after `> ` (+ typed text).
        let caret_col = if empty {
            (prefix_w as u16).saturating_add(1)
        } else {
            (display_width(&format!("{prefix}{visible}")) as u16).saturating_add(1)
        }
        .min(self.cols);
        write!(stdout, "\x1b[{input_row};{caret_col}H")
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        execute!(stdout, Show).map_err(|e| format!("stdout_write_failed: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("stdout_flush_failed: {e}"))
    }

    fn clear_prompt_strip(&mut self, stdout: &mut io::Stdout) -> Result<(), String> {
        self.refresh_size();
        let (rule_row, _, _) = self.strip_rows();
        // Clear from hairline through end of screen.
        write!(stdout, "\x1b[{rule_row};1H\x1b[0J")
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        Ok(())
    }
}

/// Fit text that may already contain ANSI; width counted on plain text.
fn fit_display_ansi(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if display_width(s) <= max_cols {
        return s.to_string();
    }
    // Fall back to plain fit + reset so we never leave half-open SGR.
    format!("{}{ANSI_RESET}", fit_display(s, max_cols))
}

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
mod cli_browser;
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
use cli_output::{
    print_json, print_runtime_result, print_runtime_result_verbose, print_status, usage,
    ControlOutputFormat,
};
use cli_plugin::plugin_command;
use cli_runtime::{kernel_config_from_runtime, run_with_options};
use cli_browser::browser_command;
use cli_skill::skill_command;
use cli_subagent::subagent_command;
use cli_types::*;

const REPL_ANSWER_PREVIEW_CHARS: usize = 2400;
const REPL_TEXT_WRAP_WIDTH: usize = 78;
const REPL_HISTORY_MAX_TURNS: usize = 8;
const REPL_META_WRAP_WIDTH: usize = 92;
/// Default conversation: keep the live stream short (Grok-like secondary noise).
const REPL_ACTIVITY_VISIBLE_LIMIT: usize = 8;
/// With `/trace`, allow a longer live activity stream before folding successes.
const REPL_ACTIVITY_TRACE_LIMIT: usize = 40;
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
        Some("browser") => browser_command(&args[2..]),
        Some("field-accept") | Some("field") => field_accept_command(&args[2..]),
        Some("experiment") => experiment_command(&args[2..]),
        Some("external-ai") => external_ai_command(&args[2..]),
        Some("app-server") => app_server::app_server_command(&args[2..]),
        Some("approval") => approval_command(&args[2..]),
        _ => Err(usage()),
    }
}

fn field_accept_command(args: &[String]) -> Result<(), String> {
    use std::process::Command;
    let root = std::env::var("CHUANG_AGENT_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let script = root.join("scripts/chuang-field-accept-10.sh");
    if !script.is_file() {
        return Err(format!(
            "field-accept script missing: {}",
            script.display()
        ));
    }
    let status = Command::new("bash")
        .arg(&script)
        .args(args)
        .env(
            "CHUANG_BIN",
            std::env::current_exe().unwrap_or_else(|_| root.join("target/debug/chuang-agent")),
        )
        .env(
            "CHUANG_FIELD_CONFIG",
            std::env::var("CHUANG_FIELD_CONFIG").unwrap_or_else(|_| {
                root.join("config.toml").display().to_string()
            }),
        )
        .status()
        .map_err(|e| format!("field-accept spawn failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "field-accept failed exit={}",
            status.code().unwrap_or(-1)
        ))
    }
}

fn run_command(args: &[String]) -> Result<(), String> {
    let (runtime_args, verbose) = split_run_verbosity(args);
    let request = parse_run_request(&runtime_args)?;
    let (result, memory_records) = run_with_options(&request)?;
    if verbose {
        print_runtime_result_verbose(&result);
    } else {
        print_runtime_result(&result);
    }
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

fn split_run_verbosity(args: &[String]) -> (Vec<String>, bool) {
    let mut verbose = false;
    let mut runtime_args = Vec::with_capacity(args.len());
    for arg in args {
        if arg == "--verbose" {
            verbose = true;
        } else {
            runtime_args.push(arg.clone());
        }
    }
    (runtime_args, verbose)
}

fn repl_command(args: &[String]) -> Result<(), String> {
    let (options, verbose) = parse_repl_options(args)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let interactive = stdin.is_terminal() && stdout.is_terminal();
    let show_trace = false;

    if interactive {
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
            print_runtime_result_verbose(&result);
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
    let mut chrome = ReplChrome::detect(true);
    // App-owned draft buffer (Grok-style). Terminal does NOT echo.
    let mut draft = String::new();

    chrome.enable(stdout)?;
    chrome.write_body(
        stdout,
        &format!("{}\n", render_repl_banner(&options.runtime.summary())),
    )?;
    print_repl_prompt(
        stdout,
        &mut chrome,
        false,
        pending_guidance.len(),
        &stats,
        pending_approval.is_some(),
        show_trace,
        None,
        &draft,
    )?;

    let result = (|| -> Result<(), String> {
        loop {
            // Background turn progress while idle for keys.
            let had_progress = poll_progress_events(
                stdout,
                &mut chrome,
                running.as_ref(),
                &mut progress_cursor,
                &stats,
                show_trace,
            )?;
            let turn_finished = poll_running_turn(
                stdout,
                &mut chrome,
                &mut running,
                &mut turn_count,
                &mut conversation_history,
                &mut progress_cursor,
                show_trace,
                verbose,
                &mut pending_guidance,
                &mut stats,
                &mut pending_approval,
            )?;
            if had_progress || turn_finished {
                print_repl_prompt(
                    stdout,
                    &mut chrome,
                    running.is_some(),
                    pending_guidance.len(),
                    &stats,
                    pending_approval.is_some(),
                    show_trace,
                    running.as_ref().map(|turn| turn.started_at),
                    &draft,
                )?;
            }

            if !event::poll(Duration::from_millis(200))
                .map_err(|e| format!("event_poll_failed: {e}"))?
            {
                // Keep timer on bottom strip while a turn runs (safe: we own buffer).
                if running.is_some() {
                    print_repl_prompt(
                        stdout,
                        &mut chrome,
                        true,
                        pending_guidance.len(),
                        &stats,
                        pending_approval.is_some(),
                        show_trace,
                        running.as_ref().map(|turn| turn.started_at),
                        &draft,
                    )?;
                }
                continue;
            }

            match event::read().map_err(|e| format!("event_read_failed: {e}"))? {
                Event::Resize(_, _) => {
                    chrome.refresh_size();
                    let bottom = chrome.body_bottom_row();
                    write!(stdout, "\x1b[1;{bottom}r")
                        .map_err(|e| format!("stdout_write_failed: {e}"))?;
                    print_repl_prompt(
                        stdout,
                        &mut chrome,
                        running.is_some(),
                        pending_guidance.len(),
                        &stats,
                        pending_approval.is_some(),
                        show_trace,
                        running.as_ref().map(|turn| turn.started_at),
                        &draft,
                    )?;
                }
                Event::Key(key) => {
                    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                        continue;
                    }
                    match handle_sticky_key(key, &mut draft)? {
                        StickyKeyAction::None => {
                            print_repl_prompt(
                                stdout,
                                &mut chrome,
                                running.is_some(),
                                pending_guidance.len(),
                                &stats,
                                pending_approval.is_some(),
                                show_trace,
                                running.as_ref().map(|turn| turn.started_at),
                                &draft,
                            )?;
                        }
                        StickyKeyAction::Redraw => {
                            print_repl_prompt(
                                stdout,
                                &mut chrome,
                                running.is_some(),
                                pending_guidance.len(),
                                &stats,
                                pending_approval.is_some(),
                                show_trace,
                                running.as_ref().map(|turn| turn.started_at),
                                &draft,
                            )?;
                        }
                        StickyKeyAction::Submit(line) => {
                            draft.clear();
                            match process_repl_input(
                                &line,
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
                                &mut chrome,
                            )? {
                                InputAction::Continue => {
                                    print_repl_prompt(
                                        stdout,
                                        &mut chrome,
                                        running.is_some(),
                                        pending_guidance.len(),
                                        &stats,
                                        pending_approval.is_some(),
                                        show_trace,
                                        running.as_ref().map(|turn| turn.started_at),
                                        &draft,
                                    )?;
                                }
                                InputAction::Exit => {
                                    chrome.write_body(stdout, "bye.\r\n")?;
                                    return Ok(());
                                }
                            }
                        }
                        StickyKeyAction::Exit => {
                            chrome.write_body(stdout, "bye.\r\n")?;
                            return Ok(());
                        }
                    }
                }
                _ => {}
            }
        }
    })();

    let _ = chrome.disable(stdout);
    result
}

enum StickyKeyAction {
    None,
    Redraw,
    Submit(String),
    Exit,
}

fn handle_sticky_key(key: KeyEvent, draft: &mut String) -> Result<StickyKeyAction, String> {
    // Ctrl+C / Ctrl+D
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => {
                if draft.is_empty() {
                    return Ok(StickyKeyAction::Exit);
                }
                draft.clear();
                return Ok(StickyKeyAction::Redraw);
            }
            KeyCode::Char('d') if draft.is_empty() => return Ok(StickyKeyAction::Exit),
            KeyCode::Char('u') => {
                draft.clear();
                return Ok(StickyKeyAction::Redraw);
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Enter => {
            let line = draft.trim().to_string();
            if line.is_empty() {
                return Ok(StickyKeyAction::Redraw);
            }
            Ok(StickyKeyAction::Submit(line))
        }
        KeyCode::Backspace => {
            draft.pop(); // pops one Unicode scalar (one Chinese char)
            Ok(StickyKeyAction::Redraw)
        }
        KeyCode::Delete => {
            // No cursor motion yet — treat like backspace for simplicity.
            draft.pop();
            Ok(StickyKeyAction::Redraw)
        }
        KeyCode::Esc => {
            draft.clear();
            Ok(StickyKeyAction::Redraw)
        }
        KeyCode::Char(c) => {
            // Crossterm delivers composed IME characters as Char events.
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                draft.push(c);
            }
            Ok(StickyKeyAction::Redraw)
        }
        _ => Ok(StickyKeyAction::None),
    }
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
    chrome: &mut ReplChrome,
) -> Result<InputAction, String> {
    let input = raw_input.trim();
    if input.eq_ignore_ascii_case("exit")
        || input.eq_ignore_ascii_case("quit")
        || input.eq_ignore_ascii_case("/exit")
        || input.eq_ignore_ascii_case("/quit")
    {
        if running.is_some() {
            chrome.write_body(
                stdout,
                "task still running; close the terminal to force quit, or wait for completion.\n",
            )?;
            return Ok(InputAction::Continue);
        }
        return Ok(InputAction::Exit);
    }
    if input.is_empty() {
        return Ok(InputAction::Continue);
    }

    if input.eq_ignore_ascii_case("/stop") {
        if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, "[chuang-control] stop")?;
            chrome.write_body(
                stdout,
                &format!("{ANSI_YELLOW}■{ANSI_RESET} 已请求停止，将在当前安全点结束任务。\n"),
            )?;
        } else {
            chrome.write_body(
                stdout,
                &format!("{ANSI_DIM}当前没有运行中的任务。{ANSI_RESET}\n"),
            )?;
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
                chrome.write_body(
                    stdout,
                    &format!(
                        "{ANSI_GREEN}✓ 已批准一次{ANSI_RESET}  {}\n",
                        humanize_approval_record(&outcome.record)
                    ),
                )?;
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
                chrome.write_body(
                    stdout,
                    &format!(
                        "{ANSI_RED}× 已拒绝{ANSI_RESET}  该操作没有执行，可以输入新要求调整方案。\n"
                    ),
                )?;
                pending_approval.take();
            }
            "3" => {
                chrome.write_body(
                    stdout,
                    &format!("{}\n", render_approval_details(approval)),
                )?;
            }
            _ => {
                chrome.write_body(stdout, "请输入 1、2 或 3。\n")?;
            }
        }
        return Ok(InputAction::Continue);
    }

    if input.starts_with('/') {
        let mut buf: Vec<u8> = Vec::new();
        handle_repl_command(
            input,
            verbose,
            show_trace,
            options,
            conversation_history,
            &mut buf,
        )?;
        let text = String::from_utf8_lossy(&buf);
        if !text.trim().is_empty() {
            chrome.write_body(stdout, &format!("{text}\n"))?;
        }
        return Ok(InputAction::Continue);
    }

    if let Some(note) = input.strip_prefix('!') {
        let note = note.trim();
        if note.is_empty() {
            chrome.write_body(stdout, "guidance ignored: empty note\n")?;
        } else if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, note)?;
            chrome.write_body(stdout, "guidance injected into current turn\n")?;
        } else {
            pending_guidance.push(note.to_string());
            chrome.write_body(
                stdout,
                &format!("guidance queued: {}\n", pending_guidance.len()),
            )?;
        }
        return Ok(InputAction::Continue);
    }

    if running.is_some() {
        if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, input)?;
        }
        chrome.write_body(
            stdout,
            "guidance injected into current turn. Prefix with ! next time to make this explicit.\n",
        )?;
        return Ok(InputAction::Continue);
    }

    let user_input = merge_repl_guidance(input, pending_guidance);
    pending_guidance.clear();
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    // App-owned input: no terminal echo. Clear bottom strip, print「你」once in body.
    chrome.clear_prompt_strip(stdout)?;
    chrome.write_body(
        stdout,
        &render_user_message_block(&user_input, &summary.provider_id, &summary.model_name, &cwd),
    )?;
    let history = recent_repl_conversation_history(conversation_history, REPL_HISTORY_MAX_TURNS);
    *running = Some(spawn_repl_turn(options.clone(), user_input, history));
    stats.mark_turn_started();
    Ok(InputAction::Continue)
}

/// What the agent is doing right now (for Grok-like status HUD).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LivePhase {
    #[default]
    Idle,
    Understanding,
    Thinking,
    Acting,
    Finalizing,
}

impl LivePhase {
    fn label_zh(self) -> &'static str {
        match self {
            Self::Idle => "就绪",
            Self::Understanding => "理解中",
            Self::Thinking => "思考中",
            Self::Acting => "执行中",
            Self::Finalizing => "整理答复",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TurnTimingSummary {
    thinking_ms: u128,
    acting_ms: u128,
}

#[derive(Default)]
struct ProgressCursor {
    bytes_read: u64,
    visible_count: usize,
    displays: Vec<ProgressDisplay>,
    last_message: Option<String>,
    collapsed_notice_shown: bool,
    section_opened: bool,
    phase: LivePhase,
    phase_started: Option<Instant>,
    current_activity: String,
    thinking_ms: u128,
    acting_ms: u128,
}

impl ProgressCursor {
    fn reset_for_idle(&mut self) {
        self.bytes_read = 0;
        self.visible_count = 0;
        self.displays.clear();
        self.last_message = None;
        self.collapsed_notice_shown = false;
        self.section_opened = false;
        self.phase = LivePhase::Idle;
        self.phase_started = None;
        self.current_activity.clear();
        self.thinking_ms = 0;
        self.acting_ms = 0;
    }

    fn set_phase(&mut self, phase: LivePhase, activity: impl Into<String>) {
        if self.phase == phase && self.phase_started.is_some() {
            let activity = activity.into();
            if !activity.is_empty() {
                self.current_activity = activity;
            }
            return;
        }
        self.accumulate_phase_time();
        self.phase = phase;
        self.phase_started = Some(Instant::now());
        let activity = activity.into();
        if !activity.is_empty() {
            self.current_activity = activity;
        } else if self.current_activity.is_empty() {
            self.current_activity = phase.label_zh().to_string();
        }
    }

    fn accumulate_phase_time(&mut self) {
        let Some(started) = self.phase_started.take() else {
            return;
        };
        let ms = started.elapsed().as_millis();
        match self.phase {
            LivePhase::Thinking => self.thinking_ms = self.thinking_ms.saturating_add(ms),
            LivePhase::Acting => self.acting_ms = self.acting_ms.saturating_add(ms),
            _ => {}
        }
    }

    fn finish_timing(&mut self) -> TurnTimingSummary {
        self.accumulate_phase_time();
        TurnTimingSummary {
            thinking_ms: self.thinking_ms,
            acting_ms: self.acting_ms,
        }
    }

    fn note_display(&mut self, display: &ProgressDisplay) {
        // Infer phase from projected human message / kind.
        match (display.kind, display.state) {
            (DisplayEventKind::Tool, DisplayState::Running) => {
                self.set_phase(LivePhase::Acting, display.message.clone());
            }
            (DisplayEventKind::Tool, DisplayState::Succeeded | DisplayState::Failed) => {
                self.set_phase(LivePhase::Thinking, "判断下一步".to_string());
            }
            (DisplayEventKind::Progress, DisplayState::Running)
                if display.message.contains("判断下一步")
                    || display.message.contains("理解你的要求") =>
            {
                if display.message.contains("理解") {
                    self.set_phase(LivePhase::Understanding, display.message.clone());
                } else {
                    self.set_phase(LivePhase::Thinking, display.message.clone());
                }
            }
            (DisplayEventKind::Progress, DisplayState::Running)
                if display.message.contains("整理最终答复") =>
            {
                self.set_phase(LivePhase::Finalizing, display.message.clone());
            }
            (DisplayEventKind::Progress, DisplayState::Running) => {
                if self.phase == LivePhase::Idle {
                    self.set_phase(LivePhase::Understanding, display.message.clone());
                } else {
                    self.current_activity = display.message.clone();
                }
            }
            (DisplayEventKind::Final, _) => {
                self.set_phase(LivePhase::Finalizing, "答复就绪".to_string());
            }
            _ => {
                if !display.message.is_empty() {
                    self.current_activity = display.message.clone();
                }
            }
        }
    }
}

fn format_short_duration(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 10.0 {
        format!("{secs:.1}s")
    } else if secs < 60.0 {
        format!("{secs:.0}s")
    } else {
        let mins = (secs / 60.0).floor() as u64;
        let rem = secs - (mins as f64 * 60.0);
        format!("{mins}m{rem:02.0}s")
    }
}

fn format_ms_duration(ms: u128) -> String {
    format_short_duration(Duration::from_millis(ms as u64))
}

fn activity_visible_limit(show_trace: bool) -> usize {
    if show_trace {
        REPL_ACTIVITY_TRACE_LIMIT
    } else {
        REPL_ACTIVITY_VISIBLE_LIMIT
    }
}

fn poll_progress_events(
    stdout: &mut io::Stdout,
    chrome: &mut ReplChrome,
    running: Option<&RunningTurn>,
    cursor: &mut ProgressCursor,
    stats: &ReplSessionStats,
    show_trace: bool,
) -> Result<bool, String> {
    let Some(turn) = running else {
        cursor.reset_for_idle();
        return Ok(false);
    };
    let content = match fs::read_to_string(&turn.progress_path) {
        Ok(content) => content,
        Err(_) => {
            // Timer lives in pinned bottom strip now; skip body HUD spam.
            let _ = (chrome, stats, show_trace);
            return Ok(false);
        }
    };
    let start = cursor.bytes_read.min(content.len() as u64) as usize;
    let new_content = &content[start..];
    let limit = activity_visible_limit(show_trace);
    let mut wrote_progress = false;
    if !new_content.trim().is_empty() {
        for line in new_content.lines().filter(|line| !line.trim().is_empty()) {
            // Always update phase/HUD from raw events; only some become printed lines.
            note_raw_progress_line(cursor, line);
            let Some(display) = format_progress_event(line, show_trace) else {
                continue;
            };
            if cursor.last_message.as_deref() == Some(display.message.as_str()) {
                continue;
            }
            if cursor.visible_count >= limit && display.suppressible {
                if !cursor.collapsed_notice_shown {
                    let hint = if show_trace {
                        format!("{ANSI_DIM}  … 后续成功步骤已折叠（失败仍会显示）{ANSI_RESET}\n")
                    } else {
                        format!(
                            "{ANSI_DIM}  … 后续成功步骤已折叠（失败仍会显示；/trace 可看更多过程）{ANSI_RESET}\n"
                        )
                    };
                    chrome.write_body(stdout, &hint)?;
                    cursor.collapsed_notice_shown = true;
                    wrote_progress = true;
                }
                continue;
            }
            if !cursor.section_opened {
                // No "过程" billboard in default chat — progress is indented secondary lines.
                if show_trace {
                    chrome.write_body(
                        stdout,
                        &format!(
                            "{ANSI_DIM}  过程 · {}{ANSI_RESET}\n",
                            stats.model_name
                        ),
                    )?;
                }
                cursor.section_opened = true;
            }
            cursor.visible_count += 1;
            let line = format!(
                "{}\n",
                render_progress_display_line(&display, cursor.visible_count)
            );
            chrome.write_body(stdout, &line)?;
            cursor.note_display(&display);
            cursor.last_message = Some(display.message.clone());
            cursor.displays.push(display);
            wrote_progress = true;
        }
    }
    cursor.bytes_read = content.len() as u64;
    // Bottom strip is refreshed by the interactive loop (running prompt + timer).
    let _ = (turn, show_trace, wrote_progress);
    Ok(wrote_progress)
}

fn note_raw_progress_line(cursor: &mut ProgressCursor, line: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    if let Some(event_value) = value.get("event") {
        if let Ok(event) = serde_json::from_value::<TerminalEvent>(event_value.clone()) {
            note_raw_terminal_event(cursor, &event);
            return;
        }
    }
    let Some(kind) = value.get("kind").and_then(|value| value.as_str()) else {
        return;
    };
    match kind {
        "turn_started" => cursor.set_phase(LivePhase::Understanding, "处理中"),
        "model_started" => cursor.set_phase(LivePhase::Thinking, "思考中…"),
        "tool_started" => {
            let title = value
                .get("details")
                .and_then(|details| details.get("activity_title"))
                .and_then(|title| title.as_str())
                .unwrap_or("执行中");
            cursor.set_phase(LivePhase::Acting, format!("正在{title}"));
        }
        "answer_ready" => cursor.set_phase(LivePhase::Finalizing, "整理答复"),
        _ => {}
    }
}

fn note_raw_terminal_event(cursor: &mut ProgressCursor, event: &TerminalEvent) {
    match event {
        TerminalEvent::TurnStarted { .. } => {
            cursor.set_phase(LivePhase::Understanding, "处理中");
        }
        TerminalEvent::StepStarted { title, .. } => {
            if title.contains("最终") || title.contains("finalize") {
                cursor.set_phase(LivePhase::Finalizing, "整理答复");
            } else {
                cursor.set_phase(LivePhase::Understanding, "处理中");
            }
        }
        TerminalEvent::ModelStarted { .. } => {
            cursor.set_phase(LivePhase::Thinking, "思考中…");
        }
        TerminalEvent::ToolStarted {
            activity_title,
            activity_detail,
            tool,
            ..
        } => {
            let title = activity_title
                .clone()
                .unwrap_or_else(|| format!("执行{tool}"));
            let detail = activity_detail.clone().unwrap_or_default();
            let activity = if detail.is_empty() {
                format!("正在{title}")
            } else {
                format!("正在{title} · {detail}")
            };
            cursor.set_phase(LivePhase::Acting, activity);
        }
        TerminalEvent::ToolFinished { ok, .. } => {
            if *ok {
                cursor.set_phase(LivePhase::Thinking, "思考中…");
            }
        }
        TerminalEvent::AnswerReady { .. } => {
            cursor.set_phase(LivePhase::Finalizing, "整理答复");
        }
        TerminalEvent::TurnCancelled { .. } => {
            cursor.set_phase(LivePhase::Idle, "已停止");
        }
        _ => {}
    }
}

fn format_progress_event(line: &str, show_trace: bool) -> Option<ProgressDisplay> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if let Some(event) = value.get("event") {
        let event: TerminalEvent = serde_json::from_value(event.clone()).ok()?;
        return repl_display_projector(show_trace).project(&event);
    }
    let kind = value.get("kind").and_then(|value| value.as_str())?;
    let details = value
        .get("details")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    match kind {
        // Default conversation: no lifecycle theater.
        "turn_started" | "step_started" | "step_finished" if !show_trace => None,
        "turn_started" if show_trace => Some(display_progress("正在理解你的要求")),
        "model_started" if show_trace => Some(display_progress("思考中…")),
        "model_started" | "model_finished" | "answer_ready" => None,
        "protocol_error" if show_trace => {
            let code = details
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            Some(display_progress(&format!("协议提示：{}", compact_preview(code, 24))))
        }
        "protocol_error" => None,
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

fn repl_display_projector(show_trace: bool) -> DisplayProjector {
    if show_trace {
        DisplayProjector::new(DisplayProjectionOptions::repl_trace())
    } else {
        DisplayProjector::new(DisplayProjectionOptions::repl_default())
    }
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

fn render_repl_status_line(
    stats: &ReplSessionStats,
    state: &str,
    show_trace: bool,
    turn_started: Option<Instant>,
) -> String {
    // Keep SHORT so the line never wraps under the input caret (wrap → stack/half-cell).
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
    let mut parts = vec![
        state.to_string(),
        stats.model_name.clone(),
        format!("ctx {percent}%"),
    ];
    if show_trace {
        parts.push("详细".to_string());
    }
    if let Some(started) = turn_started {
        parts.push(format!("⏱{}", format_short_duration(started.elapsed())));
    }
    // Quiet footer hints (like OpenCode ctrl+p line, but local slash commands).
    parts.push("/help".to_string());
    format!("{}", parts.join(" · "))
}

fn poll_running_turn(
    stdout: &mut io::Stdout,
    chrome: &mut ReplChrome,
    running: &mut Option<RunningTurn>,
    turn_count: &mut usize,
    conversation_history: &mut Vec<ConversationHistoryItem>,
    progress_cursor: &mut ProgressCursor,
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
                    chrome,
                    turn,
                    turn_count,
                    conversation_history,
                    progress_cursor,
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
                    chrome,
                    turn,
                    turn_count,
                    conversation_history,
                    progress_cursor,
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
    /// Provider HTTP timeout (for remaining-time countdown while running).
    request_timeout_ms: Option<u64>,
}

impl ReplSessionStats {
    fn from_summary(summary: &chuang_agent::runtime_config::ConfigSummary) -> Self {
        Self {
            model_name: summary.model_name.clone(),
            context_max_tokens: u64::from(summary.context_max_tokens),
            request_timeout_ms: summary.provider_request_timeout_ms,
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
    chrome: &mut ReplChrome,
    turn: RunningTurn,
    turn_count: &mut usize,
    conversation_history: &mut Vec<ConversationHistoryItem>,
    progress_cursor: &mut ProgressCursor,
    show_trace: bool,
    verbose: bool,
    pending_guidance: &mut [String],
    stats: &mut ReplSessionStats,
    pending_approval: &mut Option<ReplPendingApproval>,
) -> Result<(), String> {
    let elapsed_ms = turn.started_at.elapsed().as_millis();
    let timing = progress_cursor.finish_timing();
    let progress_displays = progress_cursor.displays.clone();
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
                chrome,
                &result,
                elapsed_ms,
                *turn_count,
                show_trace,
                &turn.input_preview,
                pending_guidance.len(),
                &progress_displays,
                &timing,
            )?;
            if let Some(approval) = pending_approval_from_result(&result) {
                chrome.write_body(stdout, &format!("{}\n", render_approval_prompt(&approval)))?;
                *pending_approval = Some(approval);
            }
            if verbose {
                print_runtime_result_verbose(&result);
            }
        }
        Err(error) => {
            stats.mark_turn_finished();
            print_repl_failure(
                stdout,
                chrome,
                &turn.input_preview,
                elapsed_ms,
                &error,
                &progress_displays,
                show_trace,
                &timing,
            )?;
        }
    }
    Ok(())
}

fn print_repl_failure(
    stdout: &mut io::Stdout,
    chrome: &mut ReplChrome,
    input_preview: &str,
    elapsed_ms: u128,
    error: &str,
    progress_displays: &[ProgressDisplay],
    show_trace: bool,
    timing: &TurnTimingSummary,
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
    chrome.write_body(
        stdout,
        &format!(
            "{}\n",
            render_repl_failure_block(
                input_preview,
                elapsed_ms,
                error,
                &progress_lines,
                show_trace,
                timing,
            )
        ),
    )
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
    chrome: &mut ReplChrome,
    running: bool,
    guidance_count: usize,
    stats: &ReplSessionStats,
    awaiting_approval: bool,
    show_trace: bool,
    turn_started: Option<Instant>,
    draft: &str,
) -> Result<(), String> {
    let status = render_sticky_status_line(
        running,
        guidance_count,
        stats,
        awaiting_approval,
        show_trace,
        turn_started,
    );
    chrome.pin_prompt(stdout, &status, draft)
}

fn render_sticky_status_line(
    running: bool,
    guidance_count: usize,
    stats: &ReplSessionStats,
    awaiting_approval: bool,
    show_trace: bool,
    turn_started: Option<Instant>,
) -> String {
    // Grok footer under the input box: mode · model · light metrics · hints.
    if awaiting_approval {
        return format!(
            "{ANSI_YELLOW}确认{ANSI_RESET} · {} · 1允许  2拒绝  3详情",
            stats.model_name
        );
    }
    if running {
        let elapsed = turn_started
            .map(|started| format_short_duration(started.elapsed()))
            .unwrap_or_else(|| "0s".to_string());
        let mut parts = vec![
            format!("{ANSI_YELLOW}运行中{ANSI_RESET}"),
            format!("⏱{elapsed}"),
            stats.model_name.clone(),
        ];
        if guidance_count > 0 {
            parts.push(format!("排队{guidance_count}"));
        }
        if let (Some(timeout_ms), Some(started)) = (stats.request_timeout_ms, turn_started) {
            if timeout_ms > 0 {
                let left = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
                parts.push(format!("剩{}", format_ms_duration(u128::from(left))));
            }
        }
        parts.push("/stop 取消".to_string());
        return parts.join(" · ");
    }
    // Idle: mirror Grok "model · mode · shortcuts".
    let mut line = render_repl_status_line(stats, "就绪", show_trace, None);
    if !line.contains("/help") {
        line = format!("{line} · Enter发送 · /help");
    }
    line
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

fn handle_repl_command(
    input: &str,
    verbose: &mut bool,
    show_trace: &mut bool,
    options: &CliOptions,
    conversation_history: &[ConversationHistoryItem],
    out: &mut dyn Write,
) -> Result<(), String> {
    match input {
        "/help" | "/?" => {
            writeln!(
                out,
                "\n命令\n  /help      查看帮助\n  /status    查看运行状态\n  /history   查看最近对话\n  /stop      在安全点停止当前任务\n  /trace     详细模式：显示准备步骤/思考轮次/技术汇总（排障用）\n  /notrace   对话默认：能快答就只出答复；有工具才显示在干嘛\n  /verbose   显示完整运行元数据\n  /quiet     关闭 verbose（不影响 /trace）\n  /clear     清屏\n  /exit      退出\n\n任务进行中\n  !补充内容  在下一个安全点补充要求\n  直接输入文字也会加入当前任务\n\n底部三行固定（对齐 Grok）：分隔线 · 输入框（> 打字）· 状态栏。\n应用自绘输入，支持中文；Enter 发送。不打印隐藏思维链和密钥。\n"
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/history" => {
            writeln!(out, "· HISTORY").map_err(|e| format!("stdout_write_failed: {e}"))?;
            if conversation_history.is_empty() {
                writeln!(out, "no completed REPL turns yet")
                    .map_err(|e| format!("stdout_write_failed: {e}"))?;
            } else {
                for item in conversation_history {
                    writeln!(
                        out,
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
                out,
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
            writeln!(out, "完整元数据已开启").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/trace" => {
            *show_trace = true;
            writeln!(
                out,
                "详细模式已开启：后续工作进展会显示模型轮次等过程；回合结束也会附技术汇总。"
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/notrace" => {
            *show_trace = false;
            writeln!(
                out,
                "已恢复默认显示：过程保持人话；结束不再附技术汇总。"
            )
            .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/quiet" => {
            *verbose = false;
            writeln!(out, "已恢复简洁模式").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/clear" => {
            // Caller paints body; send a form-feed style note.
            writeln!(out, "(clear)").map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
        "/exit" | "/quit" => {}
        _ => {
            writeln!(out, "无法识别这个命令，请输入 /help 查看可用命令。")
                .map_err(|e| format!("stdout_write_failed: {e}"))?;
        }
    }
    Ok(())
}

fn print_repl_result(
    stdout: &mut io::Stdout,
    chrome: &mut ReplChrome,
    result: &chuang_agent::agent_runtime::RuntimeResult,
    elapsed_ms: u128,
    turn_count: usize,
    show_trace: bool,
    _input_preview: &str,
    pending_guidance_count: usize,
    progress_displays: &[ProgressDisplay],
    timing: &TurnTimingSummary,
) -> Result<(), String> {
    let meta = &result.response.meta.extra;
    let tool_meta = ToolLoopMeta::from_extra(meta)?;
    let tool_status = meta
        .get("tool_loop_status")
        .map(String::as_str)
        .unwrap_or("none");
    let elapsed_cell = format_ms_duration(elapsed_ms);
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
    let metadata_line = render_completion_metadata_line(
        &elapsed_cell,
        tool_status,
        pending_guidance_count,
        timing,
        result.response.model_name.as_str(),
    );
    let block = render_assistant_completion_block(
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
    );
    chrome.write_body(stdout, &format!("{block}\n"))
}

fn render_repl_answer_text(answer: &str, turn_count: usize) -> Result<String, String> {
    // Defense in depth: never paint raw tool JSON / governance dumps as the answer.
    let answer = cli_runtime::sanitize_operator_facing_answer_for_display(answer);
    let answer = answer.as_str();
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

fn render_repl_banner(summary: &chuang_agent::runtime_config::ConfigSummary) -> String {
    let cwd = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let cwd_short = short_path_for_display(&cwd, 48);
    // Default quiet (精装修). ASCII billboard only with CHUANG_FANCY_BANNER=1.
    let fancy = env::var("CHUANG_FANCY_BANNER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // Legacy: CHUANG_QUIET_BANNER=0 also forces fancy for old muscle memory.
    let force_quiet = env::var("CHUANG_QUIET_BANNER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if force_quiet || !fancy {
        return format!(
            "{ANSI_BOLD}{ANSI_CYAN}chuang{ANSI_RESET}  {ANSI_DIM}{} · {} · {}{ANSI_RESET}\n{ANSI_DIM}/help · /stop · /exit · /trace{ANSI_RESET}\n",
            summary.model_name, summary.permission_profile, cwd_short
        );
    }
    [
        format!("{ANSI_BOLD}{ANSI_CYAN}"),
        " ██████╗██╗  ██╗██╗   ██╗ █████╗ ███╗   ██╗ ██████╗ ".to_string(),
        "██╔════╝██║  ██║██║   ██║██╔══██╗████╗  ██║██╔════╝ ".to_string(),
        "██║     ███████║██║   ██║███████║██╔██╗ ██║██║  ███╗".to_string(),
        "██║     ██╔══██║██║   ██║██╔══██║██║╚██╗██║██║   ██║".to_string(),
        "╚██████╗██║  ██║╚██████╔╝██║  ██║██║ ╚████║╚██████╔╝".to_string(),
        " ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝ ".to_string(),
        format!("                  A G E N T{ANSI_RESET}"),
        format!(
            "{ANSI_DIM}{} · {} · {}{ANSI_RESET}",
            summary.model_name, summary.permission_profile, cwd_short
        ),
        format!("{ANSI_DIM}/help · /stop · /exit · /trace{ANSI_RESET}"),
        String::new(),
    ]
    .join("\n")
}

fn short_path_for_display(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_string();
    }
    let home = env::var("HOME").unwrap_or_default();
    let condensed = if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };
    if condensed.chars().count() <= max_chars {
        return condensed;
    }
    // keep tail (usually project name)
    let tail: String = condensed
        .chars()
        .rev()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{tail}")
}

fn render_user_message_block(
    user_input: &str,
    provider_id: &str,
    model_name: &str,
    cwd: &str,
) -> String {
    // Grok-like turn open: hairline + primary user line. Model lives on answer footer.
    let _ = (provider_id, model_name, cwd);
    let text = user_input.trim();
    let rule = format!("{ANSI_DIM}{}{ANSI_RESET}", "─".repeat(36));
    if text.chars().count() <= 96 && !text.contains('\n') {
        return format!("\n{rule}\n{ANSI_BOLD}{ANSI_BLUE}你{ANSI_RESET}  {text}\n");
    }
    let mut lines = vec![
        String::new(),
        rule,
        format!("{ANSI_BOLD}{ANSI_BLUE}你{ANSI_RESET}"),
    ];
    for line in wrap_text_block(text, REPL_TEXT_WRAP_WIDTH).lines() {
        // Indent body under the label (no pipe theater).
        lines.push(format!("  {line}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_progress_display_line(display: &ProgressDisplay, step_index: usize) -> String {
    // Secondary stream: always indented under the turn so 小创 final owns the column.
    let (icon, color) = match (display.kind, display.state) {
        (DisplayEventKind::Warning, DisplayState::Blocked) => ("!", ANSI_YELLOW),
        (DisplayEventKind::Warning, _) | (_, DisplayState::Failed) => ("✗", ANSI_RED),
        (_, DisplayState::Succeeded) => ("✓", ANSI_GREEN),
        (DisplayEventKind::Tool, DisplayState::Running) => ("▸", ANSI_CYAN),
        (_, DisplayState::Running) if display.prominence == DisplayProminence::Primary => {
            ("●", ANSI_CYAN)
        }
        (_, DisplayState::Blocked) => ("…", ANSI_YELLOW),
        (_, _) => ("·", ANSI_GRAY),
    };
    let message = compact_preview(&display.message, REPL_META_WRAP_WIDTH.saturating_sub(4));
    let _ = step_index;
    let dim_body = display.state == DisplayState::Succeeded
        || (display.kind == DisplayEventKind::Progress
            && display.prominence == DisplayProminence::Secondary);
    if dim_body {
        format!("  {color}{icon}{ANSI_RESET} {ANSI_DIM}{message}{ANSI_RESET}")
    } else if display.state == DisplayState::Failed || display.state == DisplayState::Blocked {
        format!("  {color}{icon}{ANSI_RESET} {message}")
    } else {
        format!("  {color}{icon}{ANSI_RESET} {ANSI_DIM}{message}{ANSI_RESET}")
    }
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
        ToolCall::SpawnSubagent {
            task,
            tasks,
            agent_name,
            max_concurrency,
            ..
        } => {
            let n = tasks
                .as_ref()
                .map(|t| t.iter().filter(|s| !s.trim().is_empty()).count())
                .filter(|n| *n > 0)
                .unwrap_or(if task.trim().is_empty() { 0 } else { 1 });
            if n > 1 {
                format!(
                    "并行派发 {} 个子代理（concurrency={}）",
                    n,
                    max_concurrency
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "auto".to_string())
                )
            } else {
                format!(
                    "派发子代理 {}：{}",
                    agent_name.as_deref().unwrap_or("worker"),
                    safe_preview(task)
                )
            }
        }
        ToolCall::BrowserRead { .. } => "读取无头浏览器当前页".to_string(),
        ToolCall::BrowserNavigate { url } => format!("打开网页 {}", safe_preview(url)),
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
    timing: &TurnTimingSummary,
) -> String {
    let mut parts = vec![format_ms_duration(elapsed_ms)];
    if timing.thinking_ms > 0 {
        parts.push(format!("思考 {}", format_ms_duration(timing.thinking_ms)));
    }
    if timing.acting_ms > 0 {
        parts.push(format!("执行 {}", format_ms_duration(timing.acting_ms)));
    }
    let mut lines = vec![
        String::new(),
        format!("{ANSI_BOLD}{ANSI_RED}小创{ANSI_RESET}"),
        String::new(),
        format!("{ANSI_RED}{}{ANSI_RESET}", readable_runtime_error(error)),
        String::new(),
        format!("{ANSI_DIM}  {}{ANSI_RESET}", parts.join(" · ")),
    ];
    if !progress_lines.is_empty() && show_trace {
        lines.push(format!("{ANSI_DIM}  ── 最近进展 ──{ANSI_RESET}"));
        for line in progress_lines.iter().take(6) {
            lines.push(format!("{ANSI_DIM}  · {line}{ANSI_RESET}"));
        }
    }
    if show_trace {
        lines.push(format!(
            "{ANSI_DIM}  {}{ANSI_RESET}",
            compact_preview(error, REPL_META_WRAP_WIDTH)
        ));
    } else {
        lines.push(format!(
            "{ANSI_GRAY}  可补充后重试；/trace 看技术细节{ANSI_RESET}"
        ));
    }
    lines.join("\n")
}

fn render_assistant_completion_block(
    model_name: &str,
    answer: &str,
    metadata_line: &str,
    trace_lines: &[String],
    audit_line: Option<&str>,
) -> String {
    // Final owns attention: label → body → dim footer. Model only in footer (not dual).
    let _ = model_name;
    let mut lines = vec![
        String::new(),
        format!("{ANSI_BOLD}{ANSI_CYAN}小创{ANSI_RESET}"),
        String::new(),
        answer.to_string(),
        String::new(),
        metadata_line.to_string(),
    ];
    if !trace_lines.is_empty() || audit_line.is_some() {
        lines.push(format!("{ANSI_DIM}  ── /trace ──{ANSI_RESET}"));
    }
    for line in trace_lines {
        lines.push(format!("{ANSI_DIM}  {line}{ANSI_RESET}"));
    }
    if let Some(audit_line) = audit_line {
        lines.push(format!("{ANSI_DIM}  {audit_line}{ANSI_RESET}"));
    }
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
    timing: &TurnTimingSummary,
    model_name: &str,
) -> String {
    // Compact footer under the answer (Grok-style status crumbs).
    let mut parts = vec![elapsed.to_string(), model_name.to_string()];
    if timing.thinking_ms > 0 {
        parts.push(format!("思考 {}", format_ms_duration(timing.thinking_ms)));
    }
    if timing.acting_ms > 0 {
        parts.push(format!("执行 {}", format_ms_duration(timing.acting_ms)));
    }
    match tool_status {
        "human_input_required" => parts.push("等待确认".to_string()),
        "completed_after_tool_limit" => parts.push("工具后收口".to_string()),
        "tool_loop_exhausted" => parts.push("未完整收口".to_string()),
        "terminal_tool_failure" => parts.push("动作未完成".to_string()),
        _ => {}
    }
    if pending_guidance_count > 0 {
        parts.push(format!("补充×{pending_guidance_count}"));
    }
    format!(
        "{ANSI_DIM}  {}{ANSI_RESET}",
        wrap_single_line(&parts.join(" · "), REPL_META_WRAP_WIDTH)
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

        // Conversation default: no lifecycle theater.
        assert_eq!(format_progress_event(&started, false), None);
        assert_eq!(format_progress_event(&tool_done, false), None);
        assert_eq!(format_progress_event(&protocol, false), None);
        assert_eq!(
            format_progress_event(
                &serde_json::json!({"kind":"model_started","details":{}}).to_string(),
                true
            ),
            Some(display_progress("思考中…"))
        );
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
        let model = serde_json::json!({
            "schema_version": 2,
            "event": {
                "kind": "model_started",
                "round": 2
            }
        })
        .to_string();

        assert_eq!(
            format_progress_event(&step, false),
            None,
            "default conversation hides prepare-context theater"
        );
        assert_eq!(
            format_progress_event(&tool, false).unwrap().message,
            "正在检查 Git 状态 · 查看版本库当前状态"
        );
        assert_eq!(
            format_progress_event(&model, false),
            None,
            "default hides model-round spam"
        );
        assert_eq!(
            format_progress_event(&model, true).unwrap().message,
            "思考中…"
        );
        assert!(
            format_progress_event(&step, true)
                .is_some_and(|d| d.message.contains("准备上下文")),
            "trace still shows lifecycle steps"
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
        assert!(rendered.contains("请检查当前分支并总结"));
        assert!(rendered.contains("不要省略第二行"));
        // Model stays on answer footer, not under the user bubble.
        assert!(!rendered.contains("gpt-5.5"));
        assert!(!rendered.contains("type !text"));
        assert!(!rendered.contains('│'));

        let short = render_user_message_block("哈喽小创", "p", "m", "/tmp");
        assert!(short.contains("你"));
        assert!(short.contains("哈喽小创"));
        assert!(short.contains('─'));
        // Compact short messages: one line bubble, no pipe reprint theater.
        assert!(!short.contains('│'));
    }

    #[test]
    fn repl_progress_line_is_compact_transcript_style() {
        let rendered =
            render_progress_display_line(&display_tool("正在检查 Git 状态".to_string()), 1);

        assert!(rendered.contains("检查 Git 状态"));
        assert!(rendered.contains("▸") || rendered.contains("·"));
        // Secondary stream is indented under the turn.
        assert!(rendered.starts_with("  "));
        assert!(!rendered.contains("tool"));
        assert!(!rendered.contains("thinking"));
        assert!(!rendered.contains("TOOL STREAM"));
        assert!(!rendered.contains("│"));
    }

    #[test]
    fn repl_assistant_completion_puts_answer_before_trace() {
        let rendered = render_assistant_completion_block(
            "gpt-5.5",
            "answer body",
            &render_completion_metadata_line(
                "120ms",
                "ok",
                0,
                &TurnTimingSummary {
                    thinking_ms: 5000,
                    acting_ms: 2000,
                },
                "gpt-5.5",
            ),
            &["trace model=gpt-5.5 finish=stop".to_string()],
            Some("最近进展=正在检查 Git 状态  tools=1  protocol=0  report=rpt_123"),
        );

        assert!(rendered.contains("小创"));
        assert!(rendered.contains("answer body"));
        assert!(rendered.contains("/trace"));
        assert!(rendered.contains("rpt_123"));
        assert!(rendered.contains("120ms"));
        assert!(rendered.contains("思考"));
        assert!(rendered.contains("gpt-5.5"));
        let answer_pos = rendered.find("answer body").expect("answer");
        let footer_pos = rendered.find("120ms").expect("footer");
        let trace_pos = rendered.find("/trace").expect("trace section");
        assert!(
            answer_pos < footer_pos && answer_pos < trace_pos,
            "final answer must appear before footer/trace"
        );
        assert!(!rendered.contains("DONE"));
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
            tool_shell_rtk_rewrite: true,
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

        // Default banner is quiet (精装修); no ASCII billboard.
        assert!(rendered.contains("chuang"));
        assert!(!rendered.contains("██████"));
        assert!(rendered.contains("gpt-5.5"));
        assert!(rendered.contains("full_local_workspace"));
        assert!(rendered.contains("/stop"));
        assert!(rendered.contains("/trace"));
    }

    #[test]
    fn repl_metadata_footer_is_compact_crumbs() {
        let line = render_completion_metadata_line(
            "1.2s",
            "terminal_tool_failure",
            1,
            &TurnTimingSummary {
                thinking_ms: 800,
                acting_ms: 400,
            },
            "gpt-5.6-terra",
        );
        assert!(line.contains("1.2s"));
        assert!(line.contains("gpt-5.6-terra"));
        assert!(line.contains("思考"));
        assert!(line.contains("动作未完成"));
        assert!(line.contains("补充×1"));
        assert!(line.contains('·'));
        assert!(!line.contains("耗时 "));
        assert!(!line.contains("模型 "));
    }

    #[test]
    fn repl_metadata_line_is_wrapped_without_old_table() {
        let rendered = render_completion_metadata_line(
            "999ms",
            "tool_loop_exhausted",
            7,
            &TurnTimingSummary::default(),
            "gpt-5.5",
        );

        assert!(rendered.contains("999ms"));
        assert!(rendered.contains("gpt-5.5"));
        assert!(rendered.contains("未完整收口"));
        assert!(rendered.contains("补充×7"));
        assert!(rendered.contains('·'));
        assert!(!rendered.contains("tools"));
        assert!(!rendered.contains("protocol"));
        assert!(!rendered.contains("report"));
        assert!(!rendered.contains("┌"));
    }

    #[test]
    fn repl_chrome_reserves_three_rows_like_grok() {
        let mut chrome = ReplChrome {
            cols: 80,
            rows: 24,
            reserve: ReplChrome::RESERVE_ROWS,
            raw_active: false,
        };
        assert_eq!(chrome.reserve, 3);
        assert_eq!(chrome.body_bottom_row(), 21);
        let (rule, input, status) = chrome.strip_rows();
        assert_eq!((rule, input, status), (22, 23, 24));
        chrome.rows = 12;
        assert_eq!(chrome.body_bottom_row(), 9);
        let (rule, input, status) = chrome.strip_rows();
        assert_eq!((rule, input, status), (10, 11, 12));
    }

    #[test]
    fn format_short_duration_is_human_readable() {
        assert_eq!(format_short_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_short_duration(Duration::from_secs(12)), "12s");
        assert!(format_short_duration(Duration::from_secs(75)).contains('m'));
    }

    #[test]
    fn sticky_status_shows_model_timer_and_remaining() {
        let started = Instant::now() - Duration::from_secs(3);
        let stats = ReplSessionStats {
            model_name: "gpt-5.5".into(),
            context_tokens: 1000,
            context_max_tokens: 10000,
            request_timeout_ms: Some(30_000),
            ..ReplSessionStats::default()
        };
        let line = render_sticky_status_line(true, 0, &stats, false, false, Some(started));
        assert!(line.contains("gpt-5.5"));
        assert!(line.contains("运行中"));
        assert!(line.contains("⏱"));
        assert!(line.contains("剩"));
        assert!(line.contains("/stop"));
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
            request_timeout_ms: None,
        };

        let rendered = render_sticky_status_line(false, 0, &stats, false, false, None);

        assert!(rendered.contains("就绪"));
        assert!(rendered.contains("ctx 10%"));
        assert!(rendered.contains("gpt-5.6-terra"));
        assert!(!rendered.contains("╭"));

        let detailed = render_sticky_status_line(false, 0, &stats, false, true, None);
        assert!(detailed.contains("详细"));
    }

    #[test]
    fn display_width_counts_cjk_as_double() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("你好"), 4);
        assert_eq!(display_width("a中"), 3);
        assert!(fit_display("你好世界测试超长", 5).chars().count() <= 4);
    }

    #[test]
    fn repl_running_prompt_exposes_stop_control() {
        let stats = ReplSessionStats {
            model_name: "gpt-5.6-terra".to_string(),
            context_max_tokens: 272_000,
            turn_running: true,
            ..ReplSessionStats::default()
        };

        let started = Instant::now() - Duration::from_secs(2);
        let rendered = render_sticky_status_line(true, 0, &stats, false, false, Some(started));

        assert!(rendered.contains("运行中"));
        assert!(rendered.contains("/stop"));
        assert!(rendered.contains("⏱"));
        assert!(!rendered.contains("╭"));
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
