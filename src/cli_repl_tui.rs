//! Ratatui REPL shell — calm chat on **chuang brand green** (Razer green).
//!
//! Product look (定稿):
//! - 主基调雷蛇绿，见 `brand_theme`；禁止再散落其它主色
//! - Open transcript · one green input box · quiet footer
//! - Runtime stays in existing chuang paths; this module only paints.

use std::io::{self, Stdout};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;

use crate::brand_theme::{
    ASSIST_FG, BRAND, BRAND_DIM, BRAND_MUTED, BRAND_SOFT, DANGER, INPUT_FG, PLACEHOLDER, USER_BG,
    USER_FG,
};
use crate::cli_approval::resume_local_tty_approval;
use crate::cli_types::{CliOptions, ConversationHistoryItem};
use crate::{
    append_live_guidance, compact_preview, format_ms_duration, format_progress_event,
    format_short_duration, handle_repl_command, handle_sticky_key, humanize_approval_record,
    merge_repl_guidance, note_raw_progress_line, pending_approval_from_result,
    readable_runtime_error, recent_repl_conversation_history, record_repl_conversation_turn,
    render_approval_details, render_completion_metadata_line, render_repl_answer_text,
    spawn_repl_turn, ProgressCursor, ReplPendingApproval, ReplSessionStats, RunningTurn,
    StickyKeyAction, REPL_HISTORY_MAX_TURNS,
};
use chuang_agent::display_projector::DisplayState;

/// Public entry used by `repl_interactive_loop`.
pub fn run_ratatui_repl(
    options: CliOptions,
    mut verbose: bool,
    mut show_trace: bool,
) -> Result<(), String> {
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, options, &mut verbose, &mut show_trace);
    let _ = restore_terminal(&mut terminal);
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>, String> {
    enable_raw_mode().map_err(|e| format!("raw_mode_failed: {e}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| format!("alt_screen_failed: {e}"))?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(|e| format!("terminal_new_failed: {e}"))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), String> {
    disable_raw_mode().map_err(|e| format!("raw_mode_disable_failed: {e}"))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| format!("leave_alt_screen_failed: {e}"))?;
    terminal
        .show_cursor()
        .map_err(|e| format!("show_cursor_failed: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    /// Startup ASCII wordmark (brand green).
    Banner,
    User,
    Tool,
    ToolFail,
    Assistant,
    System,
    Meta,
}

#[derive(Debug, Clone)]
struct TranscriptLine {
    kind: LineKind,
    text: String,
}

struct TuiApp {
    lines: Vec<TranscriptLine>,
    draft: String,
    scroll: u16,
    follow: bool,
    /// Right side of the input box (Grok-style model chip).
    chip: String,
    /// Dim footer under the input box (shortcuts only).
    footer: String,
    activity: String,
    running: bool,
}

impl TuiApp {
    fn new(chip: String) -> Self {
        let mut app = Self {
            lines: Vec::new(),
            draft: String::new(),
            scroll: 0,
            follow: true,
            chip,
            footer: "Enter 发送 · /help · /stop · /exit · /trace".to_string(),
            activity: String::new(),
            running: false,
        };
        app.push_startup_banner();
        app
    }

    /// 启动横幅：实心块字 CHUANG，字母间距约 10px，整行居中，雷蛇绿。
    fn push_startup_banner(&mut self) {
        self.lines.push(TranscriptLine {
            kind: LineKind::Meta,
            text: String::new(),
        });
        for row in compose_chuang_banner() {
            self.lines.push(TranscriptLine {
                kind: LineKind::Banner,
                text: row,
            });
        }
        self.lines.push(TranscriptLine {
            kind: LineKind::Meta,
            text: String::new(),
        });
    }

    fn push(&mut self, kind: LineKind, text: impl Into<String>) {
        let text = text.into();
        // Breathing room before user / assistant turns.
        if matches!(kind, LineKind::User | LineKind::Assistant)
            && self.lines.last().is_some_and(|l| !l.text.is_empty())
        {
            self.lines.push(TranscriptLine {
                kind: LineKind::Meta,
                text: String::new(),
            });
        }
        for line in text.lines() {
            self.lines.push(TranscriptLine {
                kind,
                text: line.to_string(),
            });
        }
        if self.follow {
            self.scroll = u16::MAX;
        }
    }

    fn push_unique_tool(&mut self, ok: bool, message: &str) {
        let kind = if ok {
            LineKind::Tool
        } else {
            LineKind::ToolFail
        };
        // Drop projector chrome like "正在…" noise length; keep human title.
        let message = message
            .trim()
            .trim_start_matches('·')
            .trim()
            .to_string();
        if message.is_empty() {
            return;
        }
        if self
            .lines
            .last()
            .is_some_and(|l| l.kind == kind && l.text == message)
        {
            return;
        }
        self.push(kind, message.clone());
        self.activity = message;
    }

    fn set_idle_chrome(&mut self, stats: &ReplSessionStats, effort: &str, show_trace: bool) {
        self.running = false;
        self.chip = format_chip(stats, effort, "就绪", None, show_trace);
        self.footer = "Enter 发送 · /help · /stop · /exit · /trace".to_string();
        self.activity.clear();
    }

    fn set_running_chrome(
        &mut self,
        stats: &ReplSessionStats,
        effort: &str,
        elapsed: &str,
        show_trace: bool,
    ) {
        self.running = true;
        self.chip = format_chip(stats, effort, &format!("运行中 {elapsed}"), None, show_trace);
        self.footer = "/stop 取消 · Enter 也可补充要求".to_string();
    }

    fn set_approval_chrome(&mut self, stats: &ReplSessionStats, effort: &str) {
        self.running = false;
        self.chip = format_chip(stats, effort, "待确认", None, false);
        self.footer = "1 允许 · 2 拒绝 · 3 详情".to_string();
    }
}

/// Right-side chip: `model (max) · 就绪 · 12k/272k（4%）`
fn format_chip(
    stats: &ReplSessionStats,
    effort: &str,
    state: &str,
    _extra: Option<&str>,
    show_trace: bool,
) -> String {
    let model = stats.model_name.as_str();
    let effort = effort.trim();
    let head = if effort.is_empty() {
        model.to_string()
    } else {
        format!("{model} ({effort})")
    };
    let ctx = format_context_progress(stats.context_tokens, stats.context_max_tokens);
    let mut parts = vec![head, state.to_string(), ctx];
    if show_trace {
        parts.push("trace".to_string());
    }
    parts.join(" · ")
}

fn format_context_progress(used: u64, max: u64) -> String {
    if max == 0 {
        return "0/0（0%）".to_string();
    }
    let pct = used.saturating_mul(100) / max;
    let pct = pct.min(100);
    format!(
        "{}/{}（{}%）",
        format_token_short(used),
        format_token_short(max),
        pct
    )
}

fn format_token_short(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{}k", n / 1_000)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn reasoning_effort_label(summary: &chuang_agent::runtime_config::ConfigSummary) -> String {
    summary
        .provider_reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    options: CliOptions,
    verbose: &mut bool,
    show_trace: &mut bool,
) -> Result<(), String> {
    let summary = options.runtime.summary();
    let effort = reasoning_effort_label(&summary);
    let mut turn_count = 0usize;
    let mut pending_guidance: Vec<String> = Vec::new();
    let mut conversation_history: Vec<ConversationHistoryItem> = Vec::new();
    let mut running: Option<RunningTurn> = None;
    let mut progress_cursor = ProgressCursor::default();
    let mut stats = ReplSessionStats::from_summary(&summary);
    let mut pending_approval: Option<ReplPendingApproval> = None;

    let mut app = TuiApp::new(format_chip(&stats, &effort, "就绪", None, false));

    loop {
        // --- progress / completion while turn runs ---
        if let Some(turn) = running.as_ref() {
            drain_progress(&mut app, turn, &mut progress_cursor, *show_trace);
        }
        if poll_finish_turn(
            &mut running,
            &mut turn_count,
            &mut conversation_history,
            &mut progress_cursor,
            &mut app,
            &mut stats,
            &mut pending_approval,
            *verbose,
            *show_trace,
        )? {
            if pending_approval.is_some() {
                app.set_approval_chrome(&stats, &effort);
            } else {
                app.set_idle_chrome(&stats, &effort, *show_trace);
            }
        } else if let Some(turn) = running.as_ref() {
            let elapsed = format_short_duration(turn.started_at.elapsed());
            app.set_running_chrome(&stats, &effort, &elapsed, *show_trace);
        }

        terminal
            .draw(|frame| draw_ui(frame, &app))
            .map_err(|e| format!("draw_failed: {e}"))?;

        if !event::poll(Duration::from_millis(120)).map_err(|e| format!("poll_failed: {e}"))? {
            continue;
        }

        match event::read().map_err(|e| format!("event_read_failed: {e}"))? {
            Event::Key(key) => {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }
                // Scroll transcript
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Up => {
                            app.follow = false;
                            app.scroll = app.scroll.saturating_sub(1);
                            continue;
                        }
                        KeyCode::Down => {
                            app.scroll = app.scroll.saturating_add(1);
                            continue;
                        }
                        KeyCode::Home => {
                            app.follow = false;
                            app.scroll = 0;
                            continue;
                        }
                        KeyCode::End => {
                            app.follow = true;
                            app.scroll = u16::MAX;
                            continue;
                        }
                        _ => {}
                    }
                }
                match handle_sticky_key(key, &mut app.draft)? {
                    StickyKeyAction::None | StickyKeyAction::Redraw => {}
                    StickyKeyAction::Exit => {
                        if running.is_some() {
                            app.push(LineKind::System, "任务仍在运行；先 /stop 或等结束，再 /exit。");
                        } else {
                            app.push(LineKind::System, "bye.");
                            // one last frame
                            let _ = terminal.draw(|frame| draw_ui(frame, &app));
                            return Ok(());
                        }
                    }
                    StickyKeyAction::Submit(line) => {
                        app.draft.clear();
                        match handle_submit(
                            &line,
                            &options,
                            &summary,
                            verbose,
                            show_trace,
                            &mut running,
                            &conversation_history,
                            &mut pending_guidance,
                            &mut pending_approval,
                            &mut stats,
                            &mut app,
                        )? {
                            SubmitResult::Continue => {}
                            SubmitResult::Exit => {
                                app.push(LineKind::System, "bye.");
                                let _ = terminal.draw(|frame| draw_ui(frame, &app));
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

enum SubmitResult {
    Continue,
    Exit,
}

fn handle_submit(
    raw: &str,
    options: &CliOptions,
    summary: &chuang_agent::runtime_config::ConfigSummary,
    verbose: &mut bool,
    show_trace: &mut bool,
    running: &mut Option<RunningTurn>,
    conversation_history: &[ConversationHistoryItem],
    pending_guidance: &mut Vec<String>,
    pending_approval: &mut Option<ReplPendingApproval>,
    stats: &mut ReplSessionStats,
    app: &mut TuiApp,
) -> Result<SubmitResult, String> {
    let input = raw.trim();
    if input.eq_ignore_ascii_case("exit")
        || input.eq_ignore_ascii_case("quit")
        || input.eq_ignore_ascii_case("/exit")
        || input.eq_ignore_ascii_case("/quit")
    {
        if running.is_some() {
            app.push(
                LineKind::System,
                "任务仍在运行；可用 /stop，或等结束后再退出。",
            );
            return Ok(SubmitResult::Continue);
        }
        return Ok(SubmitResult::Exit);
    }
    if input.is_empty() {
        return Ok(SubmitResult::Continue);
    }

    if input.eq_ignore_ascii_case("/stop") {
        if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, "[chuang-control] stop")?;
            app.push(LineKind::System, "■ 已请求停止，将在安全点结束。");
        } else {
            app.push(LineKind::Meta, "当前没有运行中的任务。");
        }
        return Ok(SubmitResult::Continue);
    }

    if let Some(approval) = pending_approval.as_ref() {
        match input {
            "1" | "y" | "Y" | "yes" | "YES" => {
                let outcome = resume_local_tty_approval(
                    &options.runtime,
                    &approval.workspace_root,
                    &approval.pending_file,
                )?;
                app.push(
                    LineKind::System,
                    format!("✓ 已批准  {}", humanize_approval_record(&outcome.record)),
                );
                pending_approval.take();
                let continuation = format!(
                    "继续刚才的任务。用户已在本地终端明确批准并完成了待审批操作。安全回执：{}。请基于这个结果继续，不要重复执行同一操作。",
                    humanize_approval_record(&outcome.record)
                );
                let history =
                    recent_repl_conversation_history(conversation_history, REPL_HISTORY_MAX_TURNS);
                *running = Some(spawn_repl_turn(options.clone(), continuation, history));
                stats.mark_turn_started();
                let effort = reasoning_effort_label(summary);
                app.set_running_chrome(stats, &effort, "0s", *show_trace);
            }
            "2" | "n" | "N" | "no" | "NO" => {
                app.push(LineKind::System, "× 已拒绝  该操作未执行。");
                pending_approval.take();
                let effort = reasoning_effort_label(summary);
                app.set_idle_chrome(stats, &effort, *show_trace);
            }
            "3" => {
                app.push(LineKind::Meta, render_approval_details(approval));
            }
            _ => app.push(LineKind::Meta, "请输入 1、2 或 3。"),
        }
        return Ok(SubmitResult::Continue);
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
            app.push(LineKind::System, text.trim_end());
        }
        let effort = reasoning_effort_label(summary);
        app.set_idle_chrome(stats, &effort, *show_trace);
        return Ok(SubmitResult::Continue);
    }

    if let Some(note) = input.strip_prefix('!') {
        let note = note.trim();
        if note.is_empty() {
            app.push(LineKind::Meta, "guidance ignored: empty note");
        } else if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, note)?;
            app.push(LineKind::System, "已注入补充要求到当前任务。");
        } else {
            pending_guidance.push(note.to_string());
            app.push(
                LineKind::System,
                format!("补充已排队：{}", pending_guidance.len()),
            );
        }
        return Ok(SubmitResult::Continue);
    }

    if running.is_some() {
        if let Some(turn) = running.as_ref() {
            append_live_guidance(&turn.guidance_path, input)?;
        }
        app.push(
            LineKind::System,
            "已注入当前任务（建议下次用 !补充 更明确）。",
        );
        return Ok(SubmitResult::Continue);
    }

    let user_input = merge_repl_guidance(input, pending_guidance);
    pending_guidance.clear();
    app.push(LineKind::User, format!("> {user_input}"));
    app.follow = true;
    let history = recent_repl_conversation_history(conversation_history, REPL_HISTORY_MAX_TURNS);
    *running = Some(spawn_repl_turn(options.clone(), user_input, history));
    stats.mark_turn_started();
    let effort = reasoning_effort_label(summary);
    app.set_running_chrome(stats, &effort, "0s", *show_trace);
    Ok(SubmitResult::Continue)
}

fn drain_progress(
    app: &mut TuiApp,
    turn: &RunningTurn,
    cursor: &mut ProgressCursor,
    show_trace: bool,
) {
    let Ok(content) = std::fs::read_to_string(&turn.progress_path) else {
        return;
    };
    let start = cursor.bytes_read.min(content.len() as u64) as usize;
    let new_content = &content[start..];
    for line in new_content.lines().filter(|l| !l.trim().is_empty()) {
        note_raw_progress_line(cursor, line);
        let Some(display) = format_progress_event(line, show_trace) else {
            continue;
        };
        if cursor.last_message.as_deref() == Some(display.message.as_str()) {
            continue;
        }
        let ok = !matches!(display.state, DisplayState::Failed | DisplayState::Blocked);
        // Strip leading indent noise from projector messages for clean bullets.
        let msg = display.message.trim().to_string();
        app.push_unique_tool(ok, &msg);
        cursor.note_display(&display);
        cursor.last_message = Some(display.message.clone());
        cursor.displays.push(display);
    }
    cursor.bytes_read = content.len() as u64;
}

fn poll_finish_turn(
    running: &mut Option<RunningTurn>,
    turn_count: &mut usize,
    conversation_history: &mut Vec<ConversationHistoryItem>,
    progress_cursor: &mut ProgressCursor,
    app: &mut TuiApp,
    stats: &mut ReplSessionStats,
    pending_approval: &mut Option<ReplPendingApproval>,
    verbose: bool,
    show_trace: bool,
) -> Result<bool, String> {
    let Some(turn) = running.as_mut() else {
        return Ok(false);
    };
    match turn.receiver.try_recv() {
        Ok(result) => {
            turn.result = Some(result);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(false),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            if turn.result.is_none() {
                turn.result = Some(Err("repl_turn_disconnected".to_string()));
            }
        }
    }
    let Some(turn) = running.take() else {
        return Ok(false);
    };
    let elapsed_ms = turn.started_at.elapsed().as_millis();
    let timing = progress_cursor.finish_timing();
    let _ = turn.handle.join();
    match turn.result.unwrap_or(Err("repl_turn_missing_result".into())) {
        Ok(result) => {
            stats.update_from_result(&result);
            *turn_count += 1;
            record_repl_conversation_turn(
                conversation_history,
                &turn.user_input,
                &result.response.body,
            );
            let answer = render_repl_answer_text(result.response.body.trim(), *turn_count)?;
            app.push(LineKind::Assistant, answer);
            let meta = result
                .response
                .meta
                .extra
                .get("tool_loop_status")
                .map(String::as_str)
                .unwrap_or("none");
            let metadata = render_completion_metadata_line(
                &format_ms_duration(elapsed_ms),
                meta,
                0,
                &timing,
                result.response.model_name.as_str(),
            );
            app.push(LineKind::Meta, strip_ansi(&metadata));
            if let Some(approval) = pending_approval_from_result(&result) {
                app.push(
                    LineKind::System,
                    format!(
                        "需要确认：{} · 原因：{} · 输入 1允许 / 2拒绝 / 3详情",
                        approval.action,
                        compact_preview(&approval.reason, 60)
                    ),
                );
                *pending_approval = Some(approval);
            }
            if verbose {
                app.push(LineKind::Meta, "(verbose: 见终端外日志 / 报告 id 在 meta)");
            }
            let _ = show_trace;
        }
        Err(error) => {
            stats.mark_turn_finished();
            app.push(LineKind::ToolFail, readable_runtime_error(&error));
            app.push(LineKind::Meta, format!("  {}", format_ms_duration(elapsed_ms)));
        }
    }
    progress_cursor.reset_for_idle();
    Ok(true)
}

fn draw_ui(frame: &mut ratatui::Frame, app: &TuiApp) {
    let area = frame.area();
    // Grok-like: open chat · one input box · thin shortcut footer.
    // Padding left/right so it doesn't glue to the window edge.
    let padded = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area)[1];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),    // transcript — no cage
            Constraint::Length(1), // gap
            Constraint::Length(3), // the input box
            Constraint::Length(1), // shortcuts
        ])
        .split(padded);

    draw_transcript(frame, chunks[0], app);
    draw_input(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);
}

fn draw_transcript(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    // No outer border — conversation should feel open, not caged.
    let height = area.height as usize;
    let total = app.lines.len();
    let max_scroll = total.saturating_sub(height.max(1));
    let scroll = if app.follow {
        max_scroll
    } else {
        (app.scroll as usize).min(max_scroll)
    };

    let width = area.width;
    let visible = app
        .lines
        .iter()
        .skip(scroll)
        .take(height.max(1))
        .map(|line| styled_line(line, width))
        .collect::<Vec<_>>();

    let para = Paragraph::new(visible).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn styled_line(line: &TranscriptLine, width: u16) -> Line<'static> {
    match line.kind {
        // 启动横幅：品牌绿 + 水平居中
        LineKind::Banner => {
            let centered = center_line(&line.text, width as usize);
            Line::from(Span::styled(
                centered,
                Style::default()
                    .fg(BRAND)
                    .add_modifier(Modifier::BOLD),
            ))
        }
        // 你的话：品牌绿字 + 淡绿底
        LineKind::User => {
            let raw = format!(" {} ", line.text);
            let padded = pad_line_bg(&raw, width as usize);
            Line::from(Span::styled(
                padded,
                Style::default()
                    .fg(USER_FG)
                    .bg(USER_BG)
                    .add_modifier(Modifier::BOLD),
            ))
        }
        LineKind::Tool => Line::from(Span::styled(
            format!("  · {}", line.text),
            Style::default().fg(BRAND_DIM),
        )),
        LineKind::ToolFail => Line::from(Span::styled(
            format!("  ✗ {}", line.text),
            Style::default().fg(DANGER),
        )),
        LineKind::Assistant => Line::from(Span::styled(
            line.text.clone(),
            Style::default().fg(ASSIST_FG),
        )),
        LineKind::System => Line::from(Span::styled(
            line.text.clone(),
            Style::default().fg(BRAND_SOFT),
        )),
        LineKind::Meta => {
            if line.text.is_empty() {
                Line::from("")
            } else {
                Line::from(Span::styled(
                    format!("  {}", line.text),
                    Style::default().fg(BRAND_MUTED),
                ))
            }
        }
    }
}

fn pad_line_bg(s: &str, width: usize) -> String {
    if width == 0 {
        return s.to_string();
    }
    let w = display_width(s);
    if w >= width {
        return truncate_to_width(s, width);
    }
    format!("{s}{}", " ".repeat(width - w))
}

fn center_line(s: &str, width: usize) -> String {
    if width == 0 {
        return s.to_string();
    }
    let w = display_width(s);
    if w >= width {
        return truncate_to_width(s, width);
    }
    let left = (width - w) / 2;
    format!("{}{s}", " ".repeat(left))
}

/// 字母间距：2 个半角格 ≈ 终端里约 10px（随字号略变）。
const LETTER_GAP: &str = "  ";

/// 实心 7×7 字模（OpenCode/CLI 常见 block 体），对齐不歪。
/// 每行等宽，用 █ 填实，空位用空格。
fn letter_glyphs() -> [&'static [&'static str]; 6] {
    // C H U A N G — 每字 7 列 × 7 行
    [
        // C
        &[
            " █████ ",
            "██   ██",
            "██     ",
            "██     ",
            "██     ",
            "██   ██",
            " █████ ",
        ],
        // H
        &[
            "██   ██",
            "██   ██",
            "██   ██",
            "███████",
            "██   ██",
            "██   ██",
            "██   ██",
        ],
        // U
        &[
            "██   ██",
            "██   ██",
            "██   ██",
            "██   ██",
            "██   ██",
            "██   ██",
            " █████ ",
        ],
        // A
        &[
            "  ███  ",
            " ██ ██ ",
            "██   ██",
            "███████",
            "██   ██",
            "██   ██",
            "██   ██",
        ],
        // N
        &[
            "██   ██",
            "███  ██",
            "████ ██",
            "██ ████",
            "██  ███",
            "██   ██",
            "██   ██",
        ],
        // G
        &[
            " █████ ",
            "██   ██",
            "██     ",
            "██ ████",
            "██   ██",
            "██   ██",
            " █████ ",
        ],
    ]
}

fn compose_chuang_banner() -> Vec<String> {
    let glyphs = letter_glyphs();
    let rows = glyphs[0].len();
    // 校验字模等高、等宽，避免「歪」
    let width = glyphs[0][0].chars().count();
    for (li, letter) in glyphs.iter().enumerate() {
        assert_eq!(letter.len(), rows, "letter {li} row count");
        for (ri, line) in letter.iter().enumerate() {
            assert_eq!(
                line.chars().count(),
                width,
                "letter {li} row {ri} width"
            );
        }
    }
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut line = String::new();
        for (i, letter) in glyphs.iter().enumerate() {
            if i > 0 {
                line.push_str(LETTER_GAP);
            }
            line.push_str(letter[r]);
        }
        out.push(line);
    }
    out
}

fn thinking_title() -> String {
    // Animate dots so it feels alive while the model works.
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_millis() / 400) % 4)
        .unwrap_or(0) as usize;
    let dots = match n {
        0 => "·",
        1 => "··",
        2 => "···",
        _ => "····",
    };
    format!(" thinking{dots} ")
}

fn draw_input(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    // 产品主色边框；思考时左上角 thinking···
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(BRAND)
                .add_modifier(if app.running {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        );
    if app.running {
        block = block.title(thinking_title()).title_style(
            Style::default()
                .fg(BRAND)
                .add_modifier(Modifier::BOLD),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chip = app.chip.clone();
    let chip_w = display_width(&chip) as u16;
    let gap = 2u16;
    let left_budget = inner
        .width
        .saturating_sub(chip_w)
        .saturating_sub(gap)
        .saturating_sub(2); // "> "

    let draft_vis = truncate_to_width(&app.draft, left_budget as usize);
    let mut spans = vec![Span::styled(
        "> ",
        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
    )];
    if draft_vis.is_empty() {
        spans.push(Span::styled("说点什么…", Style::default().fg(PLACEHOLDER)));
    } else {
        spans.push(Span::styled(
            draft_vis.clone(),
            Style::default()
                .fg(INPUT_FG)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Pad so chip sits on the right edge of the input row.
    let left_w = 2 + if app.draft.is_empty() {
        display_width("说点什么…")
    } else {
        display_width(&draft_vis)
    };
    let pad_w = (inner.width as usize)
        .saturating_sub(left_w)
        .saturating_sub(display_width(&chip));
    spans.push(Span::raw(" ".repeat(pad_w)));
    // Chip：暗绿，不抢主色边框
    spans.push(Span::styled(chip, Style::default().fg(BRAND_MUTED)));

    frame.render_widget(Paragraph::new(Line::from(spans)), inner);

    let caret_cols = 2 + if app.draft.is_empty() {
        0
    } else {
        display_width(&draft_vis)
    };
    let caret_x = (inner.x as usize + caret_cols) as u16;
    let caret_x = caret_x.min(inner.x + inner.width.saturating_sub(1));
    frame.set_cursor_position((caret_x, inner.y));
}

fn draw_footer(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    let mut text = app.footer.clone();
    if app.running && !app.activity.is_empty() {
        text = format!("{}  ·  {}", text, compact_preview(&app.activity, 36));
    }
    let para = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().fg(BRAND_MUTED),
    )));
    frame.render_widget(para, area);
}

fn display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

/// 半角 1、全角 CJK 2；█ 等区块字符按 1（否则横幅居中会歪）。
fn char_display_width(c: char) -> usize {
    if c <= '\u{1f}' || c == '\u{7f}' {
        return 0;
    }
    if c <= '\u{7e}' {
        return 1;
    }
    // East-Asian wide (approx.) — not box/block drawing.
    matches!(
        c,
        '\u{1100}'..='\u{115F}'
            | '\u{2E80}'..='\u{A4CF}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FE10}'..='\u{FE6F}'
            | '\u{FF01}'..='\u{FF60}'
            | '\u{FFE0}'..='\u{FFE6}'
            | '\u{1F300}'..='\u{1FAFF}'
    )
    .then_some(2)
    .unwrap_or(1)
}

fn truncate_to_width(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if display_width(s) <= max {
        return s.to_string();
    }
    let mut w = 0usize;
    let mut out = String::new();
    for c in s.chars() {
        let cw = if c <= '\u{7e}' { 1 } else { 2 };
        if w + cw > max.saturating_sub(1) {
            out.push('…');
            break;
        }
        out.push(c);
        w += cw;
    }
    out
}

fn strip_ansi(s: &str) -> String {
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
    out.trim().to_string()
}
