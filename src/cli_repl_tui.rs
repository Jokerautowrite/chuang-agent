//! Ratatui REPL shell — calm chat on **chuang brand green** (Razer green).
//!
//! Product look (定稿):
//! - 主基调雷蛇绿，见 `brand_theme`；禁止再散落其它主色
//! - 启动字模仅空会话展示，首条用户消息后清掉
//! - **用户发送后：本轮用户话钉在对话区顶部**（Grok 式）；本轮内容超出视口才跟到底
//! - thinking / 用量在输入框上方（外），模型名在右下角；框内只有 `>` + 光标
//! - 助手正文左缩进 2 格；用户消息右侧显示时间
//! - 输入 `/` 弹出 slash 命令菜单（筛选 / ↑↓ / Tab 补全 / Enter 执行）
//! - 输入光标可左右移动
//! - Runtime stays in existing chuang paths; this module only paints.
//!
//! 改 TUI 时勿破坏上述行为；优先加字段/分支，禁止 silently 改回 scroll=MAX 跟底。

use std::io::{self, Stdout};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, Timelike};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Terminal;

use crate::brand_theme::{
    ASSIST_FG, BG, BRAND, BRAND_DIM, BRAND_MUTED, BRAND_SOFT, DANGER, INPUT_FG, USER_BG, USER_FG,
};
use crate::cli_approval::resume_local_tty_approval;
use crate::cli_types::{CliOptions, ConversationHistoryItem};
use crate::{
    append_live_guidance, compact_preview, format_ms_duration, format_progress_event,
    format_short_duration, handle_repl_command, handle_sticky_key, humanize_approval_record,
    insert_str_at, merge_repl_guidance, note_raw_progress_line, pending_approval_from_result,
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
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)
        .map_err(|e| format!("alt_screen_failed: {e}"))?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(|e| format!("terminal_new_failed: {e}"))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), String> {
    // Best-effort: leave paste mode even if later steps fail.
    let _ = execute!(terminal.backend_mut(), DisableBracketedPaste);
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
    /// 用户消息最右侧时钟，如 `5:14 am`（仅首行）。
    time: Option<String>,
}

fn transcript_line(kind: LineKind, text: impl Into<String>) -> TranscriptLine {
    TranscriptLine {
        kind,
        text: text.into(),
        time: None,
    }
}

/// 本地 12 小时制，如 `5:14 am` / `12:03 pm`。
fn format_clock_now() -> String {
    let now = Local::now();
    let (is_pm, hour) = now.hour12();
    let ampm = if is_pm { "pm" } else { "am" };
    format!("{hour}:{:02} {ampm}", now.minute())
}

struct TuiApp {
    lines: Vec<TranscriptLine>,
    draft: String,
    /// 输入光标：字符下标（0..=draft.chars().count()），支持左右移动。
    draft_cursor: usize,
    scroll: u16,
    /// 手动滚动时 false；true 表示自动管理视口（配合 turn_top）。
    follow: bool,
    /// 本轮用户消息行下标：优先钉在视口顶；本轮内容超出高度后才跟底。
    /// 这是「发的话显示在最上面」的核心，勿删、勿被 follow=scroll_max 覆盖。
    turn_top: Option<usize>,
    /// 最近一次 transcript 区高度（行），供 PageUp/PageDown 分页步长。
    last_transcript_h: u16,
    /// 输入框右上角外：仅用量（不进输入框内）。
    usage: String,
    /// 界面右下角：模型名（不进输入框内）。
    model: String,
    /// 运行用时，跟在 thinking··· 后面（如 `3s`）。
    elapsed: String,
    /// 底栏左侧快捷键。
    footer: String,
    activity: String,
    running: bool,
    /// 启动字模只在空会话展示；首条用户消息后清掉，避免常驻顶栏。
    banner_cleared: bool,
    /// slash 菜单当前高亮（相对过滤后列表）。
    slash_sel: usize,
}

impl TuiApp {
    fn new(model: String, usage: String) -> Self {
        let mut app = Self {
            lines: Vec::new(),
            draft: String::new(),
            draft_cursor: 0,
            scroll: 0,
            follow: true,
            turn_top: None,
            last_transcript_h: 0,
            usage,
            model,
            elapsed: String::new(),
            footer: "Enter 发送 · /help · /stop · /exit · /trace · 输入 / 看命令".to_string(),
            activity: String::new(),
            running: false,
            banner_cleared: false,
            slash_sel: 0,
        };
        app.push_startup_banner();
        app
    }

    /// 启动横幅：只在尚未对话时出现，随首条用户消息清掉。
    fn push_startup_banner(&mut self) {
        self.lines.push(transcript_line(LineKind::Meta, ""));
        for row in compose_chuang_banner() {
            self.lines.push(transcript_line(LineKind::Banner, row));
        }
        self.lines.push(transcript_line(LineKind::Meta, ""));
    }

    /// 首条用户话后去掉字模，对话区像 Grok 一样只剩消息流。
    fn clear_startup_banner(&mut self) {
        if self.banner_cleared {
            return;
        }
        self.lines.retain(|l| l.kind != LineKind::Banner);
        while self
            .lines
            .first()
            .is_some_and(|l| l.kind == LineKind::Meta && l.text.is_empty())
        {
            self.lines.remove(0);
        }
        self.banner_cleared = true;
    }

    fn push(&mut self, kind: LineKind, text: impl Into<String>) {
        if matches!(kind, LineKind::User) {
            self.clear_startup_banner();
        }
        let text = text.into();
        // Breathing room before user / assistant turns.
        if matches!(kind, LineKind::User | LineKind::Assistant)
            && self.lines.last().is_some_and(|l| !l.text.is_empty())
        {
            self.lines.push(transcript_line(LineKind::Meta, ""));
        }
        let clock = matches!(kind, LineKind::User).then(format_clock_now);
        // 用户行真正写入前的下标 = 钉顶锚点（空行呼吸在其前，不占锚点）
        let user_anchor = matches!(kind, LineKind::User).then_some(self.lines.len());
        for (i, line) in text.lines().enumerate() {
            self.lines.push(TranscriptLine {
                kind,
                text: line.to_string(),
                // 时间只挂在用户消息首行最右侧
                time: if i == 0 { clock.clone() } else { None },
            });
        }
        if let Some(anchor) = user_anchor {
            // 新用户话：钉在视口顶（Grok）；不要 scroll=MAX 把话沉到底
            self.turn_top = Some(anchor);
            self.follow = true;
            self.scroll = anchor.min(u16::MAX as usize) as u16;
        } else if self.turn_top.is_none() && self.follow {
            // 无本轮锚点时才跟底（例如启动期 system 行）
            self.scroll = u16::MAX;
        }
        // 有 turn_top 时视口由 draw_transcript 按高度计算，此处不强制跟底
    }

    fn push_unique_tool(&mut self, ok: bool, message: &str) {
        let kind = if ok {
            LineKind::Tool
        } else {
            LineKind::ToolFail
        };
        // Drop projector chrome like "正在…" noise length; keep human title.
        let message = message.trim().trim_start_matches('·').trim().to_string();
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
        self.model = format_model_label(stats, effort, show_trace);
        self.usage = format_context_progress(stats.context_tokens, stats.context_max_tokens);
        self.elapsed.clear();
        self.footer = "Enter 发送 · /help · /stop · /exit · /trace · 输入 / 看命令".to_string();
        self.activity.clear();
    }

    fn clamp_slash_sel(&mut self) {
        let n = filtered_slash_commands(&self.draft).len();
        if n == 0 {
            self.slash_sel = 0;
        } else if self.slash_sel >= n {
            self.slash_sel = n - 1;
        }
    }

    fn set_running_chrome(
        &mut self,
        stats: &ReplSessionStats,
        effort: &str,
        elapsed: &str,
        show_trace: bool,
    ) {
        self.running = true;
        self.model = format_model_label(stats, effort, show_trace);
        self.elapsed = elapsed.to_string();
        // 右上角只留用量；用时跟在左侧 thinking 后面
        self.usage = format_context_progress(stats.context_tokens, stats.context_max_tokens);
        self.footer = "/stop 取消 · Enter 也可补充要求".to_string();
    }

    fn set_approval_chrome(&mut self, stats: &ReplSessionStats, effort: &str) {
        self.running = false;
        self.model = format_model_label(stats, effort, false);
        self.elapsed.clear();
        self.usage = format!(
            "待确认 · {}",
            format_context_progress(stats.context_tokens, stats.context_max_tokens)
        );
        self.footer = "1 允许 · 2 拒绝 · 3 详情".to_string();
    }
}

/// 右下角模型名：`gpt-5.6-terra (max)`
fn format_model_label(stats: &ReplSessionStats, effort: &str, show_trace: bool) -> String {
    let model = stats.model_name.as_str();
    let effort = effort.trim();
    let mut head = if effort.is_empty() {
        model.to_string()
    } else {
        format!("{model} ({effort})")
    };
    if show_trace {
        head.push_str(" · trace");
    }
    head
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

    let mut app = TuiApp::new(
        format_model_label(&stats, &effort, false),
        format_context_progress(stats.context_tokens, stats.context_max_tokens),
    );

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
            .draw(|frame| draw_ui(frame, &mut app))
            .map_err(|e| format!("draw_failed: {e}"))?;

        if !event::poll(Duration::from_millis(120)).map_err(|e| format!("poll_failed: {e}"))? {
            continue;
        }

        match event::read().map_err(|e| format!("event_read_failed: {e}"))? {
            Event::Key(key) => {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }
                // slash 菜单：↑↓ 选择 · Tab 补全 · Enter 执行选中项
                if slash_menu_active(&app.draft) && !key.modifiers.contains(KeyModifiers::CONTROL) {
                    let matches = filtered_slash_commands(&app.draft);
                    if !matches.is_empty() {
                        match key.code {
                            KeyCode::Up => {
                                app.slash_sel = app.slash_sel.saturating_sub(1);
                                continue;
                            }
                            KeyCode::Down => {
                                let max = matches.len().saturating_sub(1);
                                if app.slash_sel < max {
                                    app.slash_sel += 1;
                                }
                                continue;
                            }
                            KeyCode::Tab => {
                                if let Some((cmd, _)) = matches.get(app.slash_sel) {
                                    app.draft = (*cmd).to_string();
                                    app.draft_cursor = app.draft.chars().count();
                                    app.slash_sel = 0;
                                    app.clamp_slash_sel();
                                }
                                continue;
                            }
                            KeyCode::Enter => {
                                let line = matches
                                    .get(app.slash_sel)
                                    .map(|(cmd, _)| (*cmd).to_string())
                                    .unwrap_or_else(|| app.draft.trim().to_string());
                                app.draft.clear();
                                app.draft_cursor = 0;
                                app.slash_sel = 0;
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
                                        let _ = terminal.draw(|frame| draw_ui(frame, &mut app));
                                        return Ok(());
                                    }
                                }
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                // Scroll transcript（手动滚会清掉 turn_top 钉顶）
                // PageUp/PageDown（含 Ctrl）在 slash 菜单打开时仍滚对话区，放在 sticky 之前。
                match key.code {
                    KeyCode::PageUp => {
                        app.follow = false;
                        app.turn_top = None;
                        app.scroll = app
                            .scroll
                            .saturating_sub(page_scroll_step(app.last_transcript_h));
                        continue;
                    }
                    KeyCode::PageDown => {
                        app.follow = false;
                        app.turn_top = None;
                        app.scroll = app
                            .scroll
                            .saturating_add(page_scroll_step(app.last_transcript_h));
                        continue;
                    }
                    _ => {}
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Up => {
                            app.follow = false;
                            app.turn_top = None;
                            app.scroll = app.scroll.saturating_sub(1);
                            continue;
                        }
                        KeyCode::Down => {
                            app.follow = false;
                            app.turn_top = None;
                            app.scroll = app.scroll.saturating_add(1);
                            continue;
                        }
                        KeyCode::Home => {
                            app.follow = false;
                            app.turn_top = None;
                            app.scroll = 0;
                            continue;
                        }
                        KeyCode::End => {
                            app.follow = true;
                            app.turn_top = None;
                            app.scroll = u16::MAX;
                            continue;
                        }
                        _ => {}
                    }
                }
                match handle_sticky_key(key, &mut app.draft, &mut app.draft_cursor)? {
                    StickyKeyAction::None => {}
                    StickyKeyAction::Redraw => {
                        app.clamp_slash_sel();
                    }
                    StickyKeyAction::Exit => {
                        if running.is_some() {
                            app.push(
                                LineKind::System,
                                "任务仍在运行；先 /stop 或等结束，再 /exit。",
                            );
                        } else {
                            app.push(LineKind::System, "bye.");
                            // one last frame
                            let _ = terminal.draw(|frame| draw_ui(frame, &mut app));
                            return Ok(());
                        }
                    }
                    StickyKeyAction::Submit(line) => {
                        app.draft.clear();
                        app.draft_cursor = 0;
                        app.slash_sel = 0;
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
                                let _ = terminal.draw(|frame| draw_ui(frame, &mut app));
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Event::Paste(text) => {
                // Multi-line / bulk paste into draft at cursor; works with slash menu open.
                app.draft_cursor = app.draft_cursor.min(app.draft.chars().count());
                let n = text.chars().count();
                insert_str_at(&mut app.draft, app.draft_cursor, &text);
                app.draft_cursor = app.draft_cursor.saturating_add(n);
                app.clamp_slash_sel();
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
        // TUI 清屏：直接清空对话区（legacy 的 form-feed 在 alt screen 里没用）
        if input.eq_ignore_ascii_case("/clear") {
            app.lines.clear();
            app.banner_cleared = true;
            app.follow = true;
            app.turn_top = None;
            app.scroll = 0;
            let effort = reasoning_effort_label(summary);
            app.set_idle_chrome(stats, &effort, *show_trace);
            return Ok(SubmitResult::Continue);
        }
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
            // 多行帮助按行推入，避免挤成一块
            for line in text.trim_end().lines() {
                if line.is_empty() {
                    app.push(LineKind::Meta, "");
                } else {
                    app.push(LineKind::System, line);
                }
            }
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
    // turn_top 已在 push(User) 里设好；不要在这里 follow 跟底盖掉钉顶
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
    match turn
        .result
        .unwrap_or(Err("repl_turn_missing_result".into()))
    {
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
            app.push(
                LineKind::Meta,
                format!("  {}", format_ms_duration(elapsed_ms)),
            );
        }
    }
    progress_cursor.reset_for_idle();
    Ok(true)
}

/// 与 `handle_repl_command` 对齐的 slash 命令表（菜单用）。
const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "查看帮助"),
    ("/status", "查看运行状态"),
    ("/history", "查看最近对话"),
    ("/stop", "在安全点停止当前任务"),
    ("/trace", "详细过程模式（排障）"),
    ("/notrace", "恢复默认简洁过程"),
    ("/verbose", "显示完整运行元数据"),
    ("/quiet", "关闭 verbose"),
    ("/clear", "清屏"),
    ("/exit", "退出"),
];

fn slash_menu_active(draft: &str) -> bool {
    let d = draft.trim_end();
    d.starts_with('/') && !d.contains(' ')
}

fn filtered_slash_commands(draft: &str) -> Vec<(&'static str, &'static str)> {
    if !slash_menu_active(draft) {
        return Vec::new();
    }
    let q = draft.trim_end().to_ascii_lowercase();
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|(cmd, _)| {
            if q == "/" {
                true
            } else {
                cmd.to_ascii_lowercase().starts_with(&q)
            }
        })
        .collect()
}

fn draw_ui(frame: &mut ratatui::Frame, app: &mut TuiApp) {
    let area = frame.area();
    // 整屏纯黑底板，盖住终端主题灰/默认色
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    // Grok-like: open chat · chrome above input · clean box · footer.
    // Padding left/right so it doesn't glue to the window edge.
    let padded = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let slash_items = filtered_slash_commands(&app.draft);
    let menu_h = if slash_items.is_empty() {
        0u16
    } else {
        // 标题 1 行 + 命令行（最多 10）
        (1 + slash_items.len().min(10)) as u16
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),         // transcript
            Constraint::Length(menu_h), // slash 菜单（无则 0）
            Constraint::Length(1),      // thinking + 用量
            Constraint::Length(3),      // 输入框：上边 + 内容 + 下边（高度保持 3 行）
            Constraint::Length(1),      // 快捷键 + 模型
        ])
        .split(padded);

    app.last_transcript_h = chunks[0].height;
    draw_transcript(frame, chunks[0], app);
    if menu_h > 0 {
        draw_slash_menu(frame, chunks[1], app, &slash_items);
    }
    draw_input_chrome(frame, chunks[2], app);
    draw_input(frame, chunks[3], app);
    draw_footer(frame, chunks[4], app);
}

fn draw_slash_menu(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &TuiApp,
    items: &[(&'static str, &'static str)],
) {
    if area.width == 0 || area.height == 0 || items.is_empty() {
        return;
    }
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        pad_line_bg("  命令  ↑↓选择  Tab补全  Enter执行", width),
        Style::default().fg(BRAND_MUTED).bg(BG),
    )));

    let max_rows = area.height.saturating_sub(1) as usize;
    let sel = app.slash_sel.min(items.len().saturating_sub(1));
    // 保证选中项在可见窗口内
    let start = if sel + 1 > max_rows {
        sel + 1 - max_rows
    } else {
        0
    };
    for (i, (cmd, desc)) in items.iter().enumerate().skip(start).take(max_rows) {
        let marker = if i == sel { "› " } else { "  " };
        let raw = format!("{marker}{cmd:<10} {desc}");
        let text = pad_line_bg(&raw, width);
        let style = if i == sel {
            Style::default()
                .fg(BG)
                .bg(BRAND)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BRAND_SOFT).bg(BG)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(BG)), area);
}

fn draw_transcript(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    // No outer border — conversation should feel open, not caged.
    let height = area.height.max(1) as usize;
    let total = app.lines.len();
    // 视口策略（Grok）：见 resolve_scroll
    let scroll = resolve_scroll(total, height, app.turn_top, app.follow, app.scroll);

    let width = area.width;
    let visible = app
        .lines
        .iter()
        .skip(scroll)
        .take(height)
        .map(|line| styled_line(line, width))
        .collect::<Vec<_>>();

    // 不足一屏时用空行垫满，黑底，避免露底；用户话仍在顶部
    let mut lines = visible;
    while lines.len() < height {
        lines.push(Line::from(Span::styled(
            " ".repeat(width as usize),
            Style::default().bg(BG),
        )));
    }

    let para = Paragraph::new(lines)
        .style(Style::default().bg(BG))
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn styled_line(line: &TranscriptLine, width: u16) -> Line<'static> {
    match line.kind {
        // 启动横幅：只给实心块上色，空格保持纯黑底
        LineKind::Banner => {
            let centered = center_line(&line.text, width as usize);
            let brand = Style::default()
                .fg(BRAND)
                .bg(BG)
                .add_modifier(Modifier::BOLD);
            let empty = Style::default().bg(BG);
            let spans: Vec<Span<'static>> = centered
                .chars()
                .map(|c| {
                    if c == ' ' {
                        Span::styled(" ", empty)
                    } else {
                        Span::styled(c.to_string(), brand)
                    }
                })
                .collect();
            Line::from(spans)
        }
        // 你的话：品牌绿字 + 淡绿底；最右侧时钟 `5:14 am`
        LineKind::User => {
            let w = width as usize;
            let time = line.time.as_deref().unwrap_or("");
            let time_w = display_width(time);
            let gap = if time_w > 0 { 2 } else { 0 }; // 正文与时间之间至少 2 空格
            let left_budget = w.saturating_sub(time_w).saturating_sub(gap);
            let left_raw = format!(" {} ", line.text);
            let left = if display_width(&left_raw) > left_budget {
                // 给时间留位，正文过长则截断
                let mut t = truncate_to_width(&left_raw, left_budget.saturating_sub(1));
                if !t.ends_with(' ') {
                    t.push(' ');
                }
                t
            } else {
                left_raw
            };
            let left_w = display_width(&left);
            let pad_w = w.saturating_sub(left_w).saturating_sub(time_w);
            let body_style = Style::default()
                .fg(USER_FG)
                .bg(USER_BG)
                .add_modifier(Modifier::BOLD);
            let time_style = Style::default().fg(BRAND_MUTED).bg(USER_BG);
            let mut spans = vec![Span::styled(left, body_style)];
            if pad_w > 0 {
                spans.push(Span::styled(
                    " ".repeat(pad_w),
                    Style::default().bg(USER_BG),
                ));
            }
            if time_w > 0 {
                spans.push(Span::styled(time.to_string(), time_style));
            }
            Line::from(spans)
        }
        LineKind::Tool => Line::from(Span::styled(
            format!("  · {}", line.text),
            Style::default().fg(BRAND_DIM).bg(BG),
        )),
        LineKind::ToolFail => Line::from(Span::styled(
            format!("  ✗ {}", line.text),
            Style::default().fg(DANGER).bg(BG),
        )),
        // 助手正文：左缩进 2 格，不顶界面最左
        LineKind::Assistant => Line::from(Span::styled(
            format!("  {}", line.text),
            Style::default().fg(ASSIST_FG).bg(BG),
        )),
        LineKind::System => Line::from(Span::styled(
            format!("  {}", line.text),
            Style::default().fg(BRAND_SOFT).bg(BG),
        )),
        LineKind::Meta => {
            if line.text.is_empty() {
                Line::from(Span::styled("", Style::default().bg(BG)))
            } else {
                Line::from(Span::styled(
                    format!("  {}", line.text),
                    Style::default().fg(BRAND_MUTED).bg(BG),
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

/// 字母间距：1 半角格（实心字模比半块更宽，间距收一点更紧凑清晰）。
const LETTER_GAP: &str = " ";

/// 实心点阵字：5 行 × 5 列，只用 `█` 与空格。
///
/// 不用 ▀/▄/_ ：半块锯齿 + 下划线当「空」时整行刷绿，看起来糊、丑。
/// 纯 █ 点阵更接近终端原生字的清晰度（Grok 级「能读清字母」）。
fn letter_glyphs() -> [&'static [&'static str]; 6] {
    // C H U A N G
    [
        // C
        &[" ███ ", "█   █", "█    ", "█   █", " ███ "],
        // H
        &["█   █", "█   █", "█████", "█   █", "█   █"],
        // U
        &["█   █", "█   █", "█   █", "█   █", " ███ "],
        // A
        &[" ███ ", "█   █", "█████", "█   █", "█   █"],
        // N
        &["█   █", "██  █", "█ █ █", "█  ██", "█   █"],
        // G
        &[" ███ ", "█    ", "█  ██", "█   █", " ███ "],
    ]
}

fn compose_chuang_banner() -> Vec<String> {
    let glyphs = letter_glyphs();
    let rows = glyphs[0].len();
    let width = glyphs[0][0].chars().count();
    for (li, letter) in glyphs.iter().enumerate() {
        debug_assert_eq!(letter.len(), rows, "letter {li} row count");
        for (ri, line) in letter.iter().enumerate() {
            debug_assert_eq!(line.chars().count(), width, "letter {li} row {ri} width");
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

/// `thinking` + 三位宽动态点（· / ·· / ··· 循环），宽度固定避免布局抖动。
fn thinking_dots() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_millis() / 350) % 3)
        .unwrap_or(0) as usize;
    // 右侧用空格补齐到 3 列，elapsed 位置不跟着跳
    match n {
        0 => "·  ".to_string(),
        1 => "·· ".to_string(),
        _ => "···".to_string(),
    }
}

/// 输入框上方一行：左 `thinking··· 3s`，右仅用量。
fn draw_input_chrome(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let left = if app.running {
        let mut s = format!("thinking{}", thinking_dots());
        if !app.elapsed.is_empty() {
            s.push(' ');
            s.push_str(&app.elapsed);
        }
        s
    } else {
        String::new()
    };
    let right = app.usage.clone();
    let left_w = display_width(&left);
    let right_w = display_width(&right);
    let width = area.width as usize;
    let mut spans = Vec::new();
    if !left.is_empty() {
        spans.push(Span::styled(
            left,
            Style::default()
                .fg(BRAND)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let pad = width.saturating_sub(left_w).saturating_sub(right_w);
    spans.push(Span::styled(" ".repeat(pad), Style::default().bg(BG)));
    if !right.is_empty() {
        spans.push(Span::styled(right, Style::default().fg(BRAND_MUTED).bg(BG)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
        area,
    );
}

fn draw_input(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    // 完整四边细框（上/下/左/右都要有）。高度 3 行：顶线 + 内容 + 底线。
    // 绝不能再用 LEFT|RIGHT|BOTTOM 漏掉顶边。线细：Plain + 不加粗。
    let _ = app.running;
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
        .border_type(BorderType::Plain)
        .style(Style::default().bg(BG))
        .border_style(Style::default().fg(BRAND).bg(BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let left_budget = inner.width.saturating_sub(2); // "> "
                                                     // 光标在字符下标；显示宽度按光标前缀计算（支持中文）
    let cursor = app.draft_cursor.min(app.draft.chars().count());
    let before: String = app.draft.chars().take(cursor).collect();
    let full = app.draft.clone();
    let draft_vis = if display_width(&full) <= left_budget as usize {
        full.clone()
    } else {
        // 超长时从左侧截断，尽量保住光标右侧可见
        let mut chars: Vec<char> = full.chars().collect();
        while !chars.is_empty()
            && display_width(&chars.iter().collect::<String>()) > left_budget as usize
        {
            chars.remove(0);
        }
        chars.iter().collect()
    };
    let mut spans = vec![Span::styled(
        "> ",
        Style::default()
            .fg(BRAND)
            .bg(BG)
            .add_modifier(Modifier::BOLD),
    )];
    if !draft_vis.is_empty() {
        spans.push(Span::styled(
            draft_vis.clone(),
            Style::default()
                .fg(INPUT_FG)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ));
    }
    // 填满输入行剩余空白，避免露主题底色
    let used = 2 + display_width(&draft_vis);
    let rest = (inner.width as usize).saturating_sub(used);
    if rest > 0 {
        spans.push(Span::styled(" ".repeat(rest), Style::default().bg(BG)));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
        inner,
    );

    // 光标列："> " + 光标前缀显示宽（若左侧被截断则贴在可见区）
    let prefix_w = display_width(&before);
    let full_w = display_width(&full);
    let caret_in_vis = if full_w <= left_budget as usize {
        prefix_w
    } else {
        // 左侧截断了 (full_w - vis_w) 个显示列
        let cut = full_w.saturating_sub(display_width(&draft_vis));
        prefix_w.saturating_sub(cut)
    };
    let caret_cols = 2 + caret_in_vis;
    let caret_x = (inner.x as usize + caret_cols) as u16;
    let caret_x = caret_x.min(inner.x + inner.width.saturating_sub(1));
    frame.set_cursor_position((caret_x, inner.y));
}

fn draw_footer(frame: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut left = app.footer.clone();
    if app.running && !app.activity.is_empty() {
        left = format!("{}  ·  {}", left, compact_preview(&app.activity, 28));
    }
    let right = app.model.clone();
    let left_w = display_width(&left);
    let right_w = display_width(&right);
    let width = area.width as usize;
    // 左快捷键过长时截断，给模型名留位
    let left_max = width.saturating_sub(right_w.saturating_add(1));
    let left = if left_w > left_max {
        truncate_to_width(&left, left_max)
    } else {
        left
    };
    let left_w = display_width(&left);
    let pad = width.saturating_sub(left_w).saturating_sub(right_w);
    let spans = vec![
        Span::styled(left, Style::default().fg(BRAND_MUTED).bg(BG)),
        Span::styled(" ".repeat(pad), Style::default().bg(BG)),
        Span::styled(right, Style::default().fg(BRAND_MUTED).bg(BG)),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
        area,
    );
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
        let cw = char_display_width(c);
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

/// 视口滚动位置：钉顶 / 超屏跟底 / 手动 / 无锚跟底。
///
/// 策略（Grok）：
/// 1) 有 turn_top 且本轮内容不超过一屏 → 用户话钉在最上面
/// 2) 有 turn_top 但本轮已超一屏且 follow → 跟底看最新输出
/// 3) 无 turn_top：follow 跟底 / 否则用手动 scroll
fn resolve_scroll(
    total: usize,
    height: usize,
    turn_top: Option<usize>,
    follow: bool,
    manual_scroll: u16,
) -> usize {
    let height = height.max(1);
    let max_scroll = total.saturating_sub(height);
    if let Some(top) = turn_top {
        let top = top.min(total.saturating_sub(1));
        let from_top = total.saturating_sub(top);
        if follow && from_top > height {
            max_scroll
        } else if follow {
            top
        } else {
            (manual_scroll as usize).min(max_scroll)
        }
    } else if follow {
        max_scroll
    } else {
        (manual_scroll as usize).min(max_scroll)
    }
}

/// PageUp/PageDown 步长：一屏减一行；尚未 layout 过则 10。
fn page_scroll_step(transcript_h: u16) -> u16 {
    if transcript_h == 0 {
        10
    } else {
        transcript_h.saturating_sub(1).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_scroll_pin_top_when_turn_fits() {
        // total 20, height 10, turn at 12 → from_top=8 <= 10 → pin at 12
        assert_eq!(resolve_scroll(20, 10, Some(12), true, 0), 12);
    }

    #[test]
    fn resolve_scroll_overflow_follow_goes_bottom() {
        // total 30, height 10, turn at 5 → from_top=25 > 10 → max_scroll=20
        assert_eq!(resolve_scroll(30, 10, Some(5), true, 0), 20);
    }

    #[test]
    fn resolve_scroll_manual_respects_scroll() {
        assert_eq!(resolve_scroll(30, 10, Some(5), false, 7), 7);
        assert_eq!(resolve_scroll(30, 10, Some(5), false, 99), 20);
        assert_eq!(resolve_scroll(30, 10, None, false, 3), 3);
    }

    #[test]
    fn resolve_scroll_no_anchor_follow_bottom() {
        assert_eq!(resolve_scroll(30, 10, None, true, 0), 20);
        assert_eq!(resolve_scroll(5, 10, None, true, 0), 0);
    }

    #[test]
    fn display_width_mixed_cjk() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("你好"), 4);
        assert_eq!(display_width("a中b"), 4);
        assert_eq!(display_width("█"), 1); // block glyph stays half-width for banner
    }

    #[test]
    fn truncate_to_width_matches_display_width() {
        let s = "你好世界";
        let t = truncate_to_width(s, 5);
        // max 5 with ellipsis room: fits chars until remaining for …
        assert!(display_width(&t) <= 5);
        assert!(t.ends_with('…') || display_width(s) <= 5);
        // short string unchanged
        assert_eq!(truncate_to_width("ab", 5), "ab");
        // pure ASCII path consistent
        let ascii = truncate_to_width("abcdefgh", 5);
        assert!(display_width(&ascii) <= 5);
        // CJK: one full char = 2, so max 3 → one CJK + …
        let cjk = truncate_to_width("你好", 3);
        assert_eq!(display_width(&cjk), 3); // '你' (2) + '…' (1) if … is width 1
        assert!(cjk.contains('…'));
    }

    #[test]
    fn page_scroll_step_uses_height_or_default() {
        assert_eq!(page_scroll_step(0), 10);
        assert_eq!(page_scroll_step(1), 1);
        assert_eq!(page_scroll_step(20), 19);
    }
}
