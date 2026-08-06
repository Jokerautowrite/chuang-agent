use serde::{Deserialize, Serialize};

use crate::terminal_event::{StepStatus, TerminalEvent};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayEvent {
    pub schema_version: u32,
    pub kind: DisplayEventKind,
    pub state: DisplayState,
    pub prominence: DisplayProminence,
    pub suppressible: bool,
    pub message: String,
}

impl DisplayEvent {
    pub const fn schema_version() -> u32 {
        1
    }

    fn new(
        kind: DisplayEventKind,
        state: DisplayState,
        prominence: DisplayProminence,
        suppressible: bool,
        message: String,
    ) -> Self {
        Self {
            schema_version: Self::schema_version(),
            kind,
            state,
            prominence,
            suppressible,
            message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayEventKind {
    Progress,
    Tool,
    Warning,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayState {
    Running,
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayProminence {
    Primary,
    Secondary,
    Alert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayProjectionOptions {
    pub show_successful_tool_events: bool,
    pub show_successful_step_events: bool,
    pub show_model_progress: bool,
    pub show_protocol_warnings: bool,
    pub show_final_ready_event: bool,
    /// Lifecycle theater: TurnStarted「理解要求」、准备上下文 step 等。
    /// 对话默认应关闭——能快答就只出答复，不要 1/2/3 流水账。
    pub show_lifecycle_steps: bool,
}

impl Default for DisplayProjectionOptions {
    /// Quiet / library default: hide successful noise; failures still project.
    fn default() -> Self {
        Self {
            show_successful_tool_events: false,
            show_successful_step_events: false,
            show_model_progress: false,
            show_protocol_warnings: false,
            show_final_ready_event: false,
            show_lifecycle_steps: false,
        }
    }
}

impl DisplayProjectionOptions {
    /// Conversational REPL (default): tools visible, no step theater.
    ///
    /// Fast path = only final answer. Slow path = tools / optional thinking when enabled.
    /// Protocol self-corrections stay off-transcript (bottom status /trace only).
    pub fn repl_default() -> Self {
        Self {
            show_successful_tool_events: true,
            show_successful_step_events: false,
            show_model_progress: false,
            show_protocol_warnings: false,
            show_final_ready_event: false,
            show_lifecycle_steps: false,
        }
    }

    /// `/trace`: lifecycle + model rounds + final-ready (for operators).
    pub fn repl_trace() -> Self {
        Self {
            show_successful_tool_events: true,
            show_successful_step_events: true,
            show_model_progress: true,
            show_protocol_warnings: true,
            show_final_ready_event: true,
            show_lifecycle_steps: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DisplayProjector {
    options: DisplayProjectionOptions,
}

impl DisplayProjector {
    pub fn new(options: DisplayProjectionOptions) -> Self {
        Self { options }
    }

    pub fn project(&self, event: &TerminalEvent) -> Option<DisplayEvent> {
        match event {
            TerminalEvent::TurnStarted { .. } => {
                self.options.show_lifecycle_steps.then(|| {
                    DisplayEvent::new(
                        DisplayEventKind::Progress,
                        DisplayState::Running,
                        DisplayProminence::Secondary,
                        true,
                        "正在理解你的要求".to_string(),
                    )
                })
            }
            TerminalEvent::StepStarted { title, .. } => {
                // 机械 step（准备上下文 / 整理答复）默认不进对话流。
                if !self.options.show_lifecycle_steps && !self.options.show_successful_step_events {
                    return None;
                }
                Some(DisplayEvent::new(
                    DisplayEventKind::Progress,
                    DisplayState::Running,
                    DisplayProminence::Secondary,
                    true,
                    format!("正在{}", humanize_step_title(title)),
                ))
            }
            TerminalEvent::StepFinished {
                title,
                status,
                detail: _,
            } => project_step_finished(title, *status, &self.options),
            TerminalEvent::ModelStarted { .. } => self.options.show_model_progress.then(|| {
                DisplayEvent::new(
                    DisplayEventKind::Progress,
                    DisplayState::Running,
                    DisplayProminence::Secondary,
                    true,
                    "思考中…".to_string(),
                )
            }),
            TerminalEvent::ModelFinished { .. } => None,
            TerminalEvent::ModelRetried {
                attempt,
                reason,
                ..
            } => Some(DisplayEvent::new(
                DisplayEventKind::Progress,
                DisplayState::Running,
                DisplayProminence::Secondary,
                true,
                format!("模型服务暂时不可用（{reason}），自动重试第 {attempt} 次…"),
            )),
            TerminalEvent::ToolStarted {
                tool,
                activity_title,
                activity_detail,
                ..
            } => Some(DisplayEvent::new(
                DisplayEventKind::Tool,
                DisplayState::Running,
                DisplayProminence::Secondary,
                true,
                join_activity_detail(
                    format!(
                        "正在{}",
                        activity_title
                            .as_deref()
                            .map(humanize_activity_title)
                            .unwrap_or_else(|| tool_running_label(tool))
                    ),
                    activity_detail.as_deref().map(sanitize_activity_detail),
                ),
            )),
            TerminalEvent::ToolFinished {
                tool,
                ok,
                decision,
                activity_title,
                activity_detail,
                ..
            } => project_tool_finished(
                tool,
                activity_title.as_deref(),
                activity_detail.as_deref(),
                *ok,
                decision.as_deref(),
                &self.options,
            ),
            TerminalEvent::ProtocolError { code, .. } => {
                self.options.show_protocol_warnings.then(|| {
                    let key = normalize_key(code);
                    let recoverable = matches!(
                        key.as_str(),
                        "plain_text_response"
                            | "missing_required_action"
                            | "invalid_action_json"
                            | "action_and_final_mixed"
                            | "action_final_mixed"
                            | "trailing_final"
                    );
                    DisplayEvent::new(
                        if recoverable {
                            DisplayEventKind::Progress
                        } else {
                            DisplayEventKind::Warning
                        },
                        if recoverable {
                            DisplayState::Running
                        } else {
                            DisplayState::Failed
                        },
                        if recoverable {
                            DisplayProminence::Secondary
                        } else {
                            DisplayProminence::Alert
                        },
                        recoverable,
                        humanize_protocol_error(code),
                    )
                })
            }
            TerminalEvent::GuidanceInjected { .. } => Some(DisplayEvent::new(
                DisplayEventKind::Progress,
                DisplayState::Succeeded,
                DisplayProminence::Primary,
                false,
                "已接收新的补充要求".to_string(),
            )),
            TerminalEvent::TurnCancelled { .. } => Some(DisplayEvent::new(
                DisplayEventKind::Warning,
                DisplayState::Blocked,
                DisplayProminence::Alert,
                false,
                "已收到停止要求，当前任务已安全结束".to_string(),
            )),
            TerminalEvent::AnswerReady { .. } => self.options.show_final_ready_event.then(|| {
                DisplayEvent::new(
                    DisplayEventKind::Final,
                    DisplayState::Succeeded,
                    DisplayProminence::Primary,
                    false,
                    "答复已准备完成".to_string(),
                )
            }),
        }
    }

    pub fn project_all<'a>(
        &self,
        events: impl IntoIterator<Item = &'a TerminalEvent>,
    ) -> Vec<DisplayEvent> {
        events
            .into_iter()
            .filter_map(|event| self.project(event))
            .collect()
    }
}

fn project_step_finished(
    title: &str,
    status: StepStatus,
    options: &DisplayProjectionOptions,
) -> Option<DisplayEvent> {
    match status {
        StepStatus::Ok => {
            if !(options.show_successful_step_events || options.show_lifecycle_steps) {
                return None;
            }
            Some(DisplayEvent::new(
                DisplayEventKind::Progress,
                DisplayState::Succeeded,
                DisplayProminence::Secondary,
                true,
                format!("{}已完成", humanize_step_title(title)),
            ))
        }
        StepStatus::Failed => Some(DisplayEvent::new(
            DisplayEventKind::Warning,
            DisplayState::Failed,
            DisplayProminence::Alert,
            false,
            format!("{}失败", humanize_step_title(title)),
        )),
        StepStatus::Skipped => {
            if !(options.show_successful_step_events || options.show_lifecycle_steps) {
                return None;
            }
            Some(DisplayEvent::new(
                DisplayEventKind::Progress,
                DisplayState::Blocked,
                DisplayProminence::Secondary,
                true,
                format!("{}已跳过", humanize_step_title(title)),
            ))
        }
    }
}

fn project_tool_finished(
    tool: &str,
    activity_title: Option<&str>,
    activity_detail: Option<&str>,
    ok: bool,
    decision: Option<&str>,
    options: &DisplayProjectionOptions,
) -> Option<DisplayEvent> {
    let subject = activity_title
        .map(humanize_activity_title)
        .unwrap_or_else(|| tool_subject(tool));
    if ok {
        return options.show_successful_tool_events.then(|| {
            DisplayEvent::new(
                DisplayEventKind::Tool,
                DisplayState::Succeeded,
                DisplayProminence::Secondary,
                true,
                join_activity_detail(
                    format!("{subject}已完成"),
                    activity_detail.map(sanitize_activity_detail),
                ),
            )
        });
    }

    let blocked = decision_indicates_block(decision);
    Some(DisplayEvent::new(
        DisplayEventKind::Warning,
        if blocked {
            DisplayState::Blocked
        } else {
            DisplayState::Failed
        },
        DisplayProminence::Alert,
        false,
        if blocked {
            format!("{subject}需要你的确认")
        } else {
            format!("{subject}失败，正在保留现场信息")
        },
    ))
}

fn join_activity_detail(message: String, detail: Option<String>) -> String {
    match detail.filter(|detail| !detail.is_empty()) {
        Some(detail) if detail != message => format!("{message} · {detail}"),
        None => message,
        Some(_) => message,
    }
}

fn sanitize_activity_detail(detail: &str) -> String {
    sanitize_label(detail, 42)
}

fn humanize_step_title(title: &str) -> String {
    match normalize_key(title).as_str() {
        "prepare context" | "准备上下文" => "准备上下文".to_string(),
        "load memory" => "加载记忆".to_string(),
        "collect evidence" => "收集证据".to_string(),
        "review result" => "复核结果".to_string(),
        "finalize answer" | "整理最终答复" => "整理最终答复".to_string(),
        _ => format!("处理步骤：{}", sanitize_label(title, 18)),
    }
}

fn humanize_activity_title(title: &str) -> String {
    let title = sanitize_label(title, 24);
    if title.is_empty() {
        "执行当前操作".to_string()
    } else {
        title
    }
}

fn humanize_protocol_error(code: &str) -> String {
    match normalize_key(code).as_str() {
        "plain_text_response" => "正在调整执行格式并继续".to_string(),
        "missing_required_action" => "正在补全必要的实际检查".to_string(),
        "invalid_action_json" => "正在修正操作格式并继续".to_string(),
        "action_and_final_mixed" | "action_final_mixed" | "trailing_final" => {
            "正在拆分操作与答复并继续".to_string()
        }
        "empty_final_answer" => "答复生成失败：最终答复为空".to_string(),
        "rejected_tool_call" => "流程被拦截：出现未允许的额外操作".to_string(),
        // Never dump raw snake_case codes to the default conversation stream.
        other => format!("执行格式需调整（{}），正在继续", sanitize_label(other, 18)),
    }
}

fn tool_running_label(tool: &str) -> String {
    match normalize_key(tool).as_str() {
        "list_dir" => "检查目录".to_string(),
        "read_file" => "读取文件".to_string(),
        "write_file" => "更新文件".to_string(),
        "apply_patch" => "写入补丁".to_string(),
        "shell_exec" | "code_execute" => "执行本地检查".to_string(),
        "memory_recall" => "检索记忆".to_string(),
        "screenshot" => "查看画面".to_string(),
        "locate" => "定位界面元素".to_string(),
        "open_app" => "打开应用".to_string(),
        "mouse" => "执行鼠标操作".to_string(),
        "keyboard" => "执行键盘操作".to_string(),
        "wait" => "等待结果".to_string(),
        "human_suspend" => "等待人工处理".to_string(),
        "spawn_subagent" => "派生子代理".to_string(),
        "browser_read" => "读取网页".to_string(),
        "browser_navigate" => "打开网页".to_string(),
        _ => format!("执行{}", tool_subject(tool)),
    }
}

fn tool_subject(tool: &str) -> String {
    match normalize_key(tool).as_str() {
        "list_dir" => "目录检查".to_string(),
        "read_file" => "文件读取".to_string(),
        "write_file" => "文件更新".to_string(),
        "apply_patch" => "补丁写入".to_string(),
        "shell_exec" | "code_execute" => "本地检查".to_string(),
        "memory_recall" => "记忆检索".to_string(),
        "screenshot" => "画面查看".to_string(),
        "locate" => "界面定位".to_string(),
        "open_app" => "应用打开".to_string(),
        "mouse" => "鼠标操作".to_string(),
        "keyboard" => "键盘操作".to_string(),
        "wait" => "等待阶段".to_string(),
        "human_suspend" => "人工处理阶段".to_string(),
        "spawn_subagent" => "子代理派发".to_string(),
        "browser_read" => "网页读取".to_string(),
        "browser_navigate" => "网页导航".to_string(),
        _ => format!("操作 {}", sanitize_label(tool, 18)),
    }
}

fn decision_indicates_block(decision: Option<&str>) -> bool {
    let Some(decision) = decision else {
        return false;
    };
    let normalized = normalize_key(decision);
    normalized.contains("deny")
        || normalized.contains("denied")
        || normalized.contains("reject")
        || normalized.contains("block")
        || normalized.contains("approval")
}

fn normalize_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace('\n', " ")
}

fn sanitize_label(value: &str, max_chars: usize) -> String {
    let mut cleaned = String::new();
    for ch in value.chars() {
        if ch.is_ascii_control() {
            continue;
        }
        if matches!(ch, '/' | '\\' | '|' | ';' | '&' | '<' | '>' | '$' | '`') {
            cleaned.push(' ');
            continue;
        }
        cleaned.push(ch);
        if cleaned.chars().count() >= max_chars {
            break;
        }
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}
