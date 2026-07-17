//! Ratatui REPL shell (option A): clear 3-pane layout.
//!
//! ```text
//! ┌ conversation (scroll) ─────────────────────┐
//! │ > user                                     │
//! │   · tool                                   │
//! │ answer                                     │
//! ├ input ─────────────────────────────────────┤
//! │ > draft_                                   │
//! ├ status ────────────────────────────────────┤
//! │ gpt · 就绪 · Enter发送 · /help             │
//! └────────────────────────────────────────────┘
//! ```
//!
//! Runtime / turns / tools stay in existing chuang paths; this module only paints.

use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;

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
    status: String,
    activity: String,
}

impl TuiApp {
    fn new(banner: String, status: String) -> Self {
        let mut lines = Vec::new();
        for part in banner.lines() {
            if !part.trim().is_empty() {
                lines.push(TranscriptLine {
                    kind: LineKind::System,
                    text: part.to_string(),
                });
            }
        }
        lines.push(TranscriptLine {
            kind: LineKind::Meta,
            text: "Ratatui 壳 · 对话在上 · 输入在下 · Enter 发送 · /help".to_string(),
        });
        Self {
            lines,
            draft: String::new(),
            scroll: 0,
            follow: true,
            status,
            activity: String::new(),
        }
    }

    fn push(&mut self, kind: LineKind, text: impl Into<String>) {
        let text = text.into();
        for (i, line) in text.lines().enumerate() {
            // Keep multi-line blocks as consecutive same-kind lines.
            let prefix_blank = i > 0 && line.is_empty();
            if prefix_blank {
                self.lines.push(TranscriptLine {
                    kind: LineKind::Meta,
                    text: String::new(),
                });
            } else {
                self.lines.push(TranscriptLine {
                    kind,
                    text: line.to_string(),
                });
            }
        }
        if self.follow {
            self.scroll = u16::MAX; // clamp later from viewport
        }
    }

    fn push_unique_tool(&mut self, ok: bool, message: &str) {
        let kind = if ok {
            LineKind::Tool
        } else {
            LineKind::ToolFail
        };
        if self
            .lines
            .last()
            .is_some_and(|l| l.kind == kind && l.text == message)
        {
            return;
        }
        self.push(kind, message.to_string());
        self.activity = message.to_string();
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    options: CliOptions,
    verbose: &mut bool,
    show_trace: &mut bool,
) -> Result<(), String> {
    let summary = options.runtime.summary();
    let mut turn_count = 0usize;
    let mut pending_guidance: Vec<String> = Vec::new();
    let mut conversation_history: Vec<ConversationHistoryItem> = Vec::new();
    let mut running: Option<RunningTurn> = None;
    let mut progress_cursor = ProgressCursor::default();
    let mut stats = ReplSessionStats::from_summary(&summary);
    let mut pending_approval: Option<ReplPendingApproval> = None;

    let banner = format!(
        "chuang  {}  ·  {}  ·  /help /stop /exit /trace",
        summary.model_name, summary.permission_profile
    );
    let mut app = TuiApp::new(banner, status_chip(false, &stats, false, *show_trace, None));

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
            app.status = status_chip(
                false,
                &stats,
                pending_approval.is_some(),
                *show_trace,
                None,
            );
            app.activity.clear();
        } else if let Some(turn) = running.as_ref() {
            app.status = status_chip(
                true,
                &stats,
                pending_approval.is_some(),
                *show_trace,
                Some(turn.started_at),
            );
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
            }
            "2" | "n" | "N" | "no" | "NO" => {
                app.push(LineKind::System, "× 已拒绝  该操作未执行。");
                pending_approval.take();
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
        app.status = status_chip(running.is_some(), stats, false, *show_trace, None);
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
    app.status = status_chip(true, stats, false, *show_trace, Some(Instant::now()));
    let _ = summary;
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

fn status_chip(
    running: bool,
    stats: &ReplSessionStats,
    awaiting_approval: bool,
    show_trace: bool,
    started: Option<Instant>,
) -> String {
    if awaiting_approval {
        return format!("确认中 · {} · 1/2/3", stats.model_name);
    }
    if running {
        let elapsed = started
            .map(|s| format_short_duration(s.elapsed()))
            .unwrap_or_else(|| "0s".into());
        let mut s = format!("{} · 运行中 {} · /stop", stats.model_name, elapsed);
        if show_trace {
            s.push_str(" · trace");
        }
        return s;
    }
    let mut s = format!(
        "{} · 就绪 · Enter发送 · /help · /exit",
        stats.model_name
    );
    if show_trace {
        s.push_str(" · trace");
    }
    s
}

fn draw_ui(frame: &mut ratatui::Frame, app: &TuiApp) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    draw_transcript(frame, chunks[0], app);
    draw_input(frame, chunks[1], app);
    draw_status(frame, chunks[2], app);
}

fn draw_transcript(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" chuang ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let height = inner.height as usize;
    let total = app.lines.len();
    let max_scroll = total.saturating_sub(height.max(1));
    let scroll = if app.follow {
        max_scroll
    } else {
        (app.scroll as usize).min(max_scroll)
    };

    let visible = app
        .lines
        .iter()
        .skip(scroll)
        .take(height.max(1))
        .map(|line| styled_line(line))
        .collect::<Vec<_>>();

    let para = Paragraph::new(visible).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

fn styled_line(line: &TranscriptLine) -> Line<'static> {
    let style = match line.kind {
        LineKind::User => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        LineKind::Tool => Style::default().fg(Color::Gray),
        LineKind::ToolFail => Style::default().fg(Color::Red),
        LineKind::Assistant => Style::default().fg(Color::White),
        LineKind::System => Style::default().fg(Color::Yellow),
        LineKind::Meta => Style::default().fg(Color::DarkGray),
    };
    let text = match line.kind {
        LineKind::Tool => format!("  · {}", line.text),
        LineKind::ToolFail => format!("  ✗ {}", line.text),
        LineKind::Meta if !line.text.is_empty() => format!("  {}", line.text),
        _ => line.text.clone(),
    };
    Line::from(Span::styled(text, style))
}

fn draw_input(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    let title = if app.draft.is_empty() {
        " 输入 "
    } else {
        " 输入 · Enter 发送 "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let shown = if app.draft.is_empty() {
        Span::styled(
            "> ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!("> {}", app.draft),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    };
    // Placeholder when empty
    let line = if app.draft.is_empty() {
        Line::from(vec![
            Span::styled(
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("说点什么…", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(shown)
    };
    frame.render_widget(Paragraph::new(line), inner);

    // Caret: after `> ` + draft
    let caret_x = inner.x + 2 + app.draft.chars().count() as u16;
    let caret_x = caret_x.min(inner.x + inner.width.saturating_sub(1));
    frame.set_cursor_position((caret_x, inner.y));
}

fn draw_status(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    let mut text = app.status.clone();
    if !app.activity.is_empty() {
        text = format!("{text}  ·  {}", compact_preview(&app.activity, 40));
    }
    let para = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(para, area);
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
