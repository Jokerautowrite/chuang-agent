use chuang_agent::display_projector::{
    DisplayEvent, DisplayEventKind, DisplayProjectionOptions, DisplayProjector, DisplayProminence,
    DisplayState,
};
use chuang_agent::terminal_event::{StepStatus, TerminalEvent};

#[test]
fn repl_default_options_show_tools_but_hide_model_round_spam() {
    let projector = DisplayProjector::new(DisplayProjectionOptions::repl_default());

    let tool = projector
        .project(&TerminalEvent::ToolStarted {
            round: 1,
            tool: "read_file".to_string(),
            summary: None,
            activity_title: Some("读取文件".to_string()),
            activity_detail: Some("src/main.rs".to_string()),
        })
        .expect("repl should show tool starts");
    assert_eq!(tool.kind, DisplayEventKind::Tool);
    assert!(tool.message.contains("读取文件"));

    let model = projector.project(&TerminalEvent::ModelStarted { round: 2 });
    assert_eq!(
        model, None,
        "repl_default hides per-round model progress to reduce noise"
    );
}

#[test]
fn repl_trace_options_surface_model_and_final_ready() {
    let projector = DisplayProjector::new(DisplayProjectionOptions::repl_trace());

    let model = projector
        .project(&TerminalEvent::ModelStarted { round: 2 })
        .expect("trace mode shows model progress");
    assert_eq!(model.message, "思考中…");

    let ready = projector
        .project(&TerminalEvent::AnswerReady {
            chars: 12,
            truncated: false,
            snapshot_path: None,
        })
        .expect("trace mode shows final-ready marker");
    assert_eq!(ready.kind, DisplayEventKind::Final);
}

#[test]
fn display_event_shape_is_explicit_and_serializable() {
    // Lifecycle lines only with explicit option (not default conversation).
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_lifecycle_steps: true,
        ..DisplayProjectionOptions::default()
    });
    let event = projector
        .project(&TerminalEvent::TurnStarted {
            input_preview: "看一下当前状态".to_string(),
            max_tool_rounds: 4,
        })
        .expect("lifecycle option should project turn started");

    assert_eq!(event.schema_version, DisplayEvent::schema_version());
    assert_eq!(event.kind, DisplayEventKind::Progress);
    assert_eq!(event.state, DisplayState::Running);
    assert_eq!(event.message, "正在理解你的要求");

    let serialized = serde_json::to_value(&event).expect("display event should serialize");
    assert_eq!(serialized["schema_version"], 1);
    assert_eq!(serialized["kind"], "progress");
    assert_eq!(serialized["state"], "running");
    assert_eq!(serialized["message"], "正在理解你的要求");
}

#[test]
fn conversational_default_hides_lifecycle_theater() {
    let projector = DisplayProjector::new(DisplayProjectionOptions::repl_default());
    assert_eq!(
        projector.project(&TerminalEvent::TurnStarted {
            input_preview: "hi".into(),
            max_tool_rounds: 4,
        }),
        None
    );
    assert_eq!(
        projector.project(&TerminalEvent::StepStarted {
            title: "准备上下文".into(),
            detail: None,
        }),
        None
    );
    assert_eq!(
        projector.project(&TerminalEvent::StepFinished {
            title: "准备上下文".into(),
            status: StepStatus::Ok,
            detail: None,
        }),
        None
    );
}

#[test]
fn projector_maps_progress_steps_to_deterministic_chinese_wording() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_lifecycle_steps: true,
        show_successful_step_events: true,
        ..DisplayProjectionOptions::default()
    });

    let started = projector
        .project(&TerminalEvent::StepStarted {
            title: "prepare context".to_string(),
            detail: Some("segments=3".to_string()),
        })
        .expect("step started should project when lifecycle enabled");
    let finished = projector
        .project(&TerminalEvent::StepFinished {
            title: "prepare context".to_string(),
            status: StepStatus::Ok,
            detail: Some("segments=3".to_string()),
        })
        .expect("step finished should project");

    assert_eq!(started.message, "正在准备上下文");
    assert_eq!(finished.message, "准备上下文已完成");
    assert_eq!(finished.state, DisplayState::Succeeded);
}

#[test]
fn protocol_errors_hidden_in_default_repl_shown_human_in_trace() {
    let quiet = DisplayProjector::new(DisplayProjectionOptions::repl_default());
    assert_eq!(
        quiet.project(&TerminalEvent::ProtocolError {
            round: 2,
            code: "invalid_action_json".to_string(),
        }),
        None,
        "default chat keeps protocol self-heal off-transcript"
    );

    let trace = DisplayProjector::new(DisplayProjectionOptions::repl_trace());
    let event = trace
        .project(&TerminalEvent::ProtocolError {
            round: 2,
            code: "invalid_action_json".to_string(),
        })
        .expect("trace surfaces protocol recovery");
    assert_eq!(event.kind, DisplayEventKind::Progress);
    assert_eq!(event.message, "正在修正操作格式并继续");
    assert!(!event.message.contains("invalid_action_json"));
}

#[test]
fn successful_tools_are_low_prominence_and_suppressible() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_successful_tool_events: true,
        ..DisplayProjectionOptions::default()
    });
    let event = projector
        .project(&TerminalEvent::ToolFinished {
            round: 1,
            tool: "code_execute".to_string(),
            ok: true,
            decision: Some("allow".to_string()),
            summary: "bash -lc 'cat /tmp/secret && echo done'".to_string(),
            activity_title: Some("执行本地命令".to_string()),
            activity_detail: Some("敏感输出已隐藏".to_string()),
        })
        .expect("successful tool should still project by default");

    assert_eq!(event.kind, DisplayEventKind::Tool);
    assert_eq!(event.state, DisplayState::Succeeded);
    assert_eq!(event.prominence, DisplayProminence::Secondary);
    assert!(event.suppressible);
    assert_eq!(event.message, "执行本地命令已完成 · 敏感输出已隐藏");
    assert!(!event.message.contains("bash"));
    assert!(!event.message.contains("/tmp/secret"));
}

#[test]
fn successful_tools_can_be_suppressed_entirely() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_successful_tool_events: false,
        ..DisplayProjectionOptions::default()
    });

    let event = projector.project(&TerminalEvent::ToolFinished {
        round: 1,
        tool: "read_file".to_string(),
        ok: true,
        decision: Some("allow".to_string()),
        summary: "read /very/private/path".to_string(),
        activity_title: Some("读取文件".to_string()),
        activity_detail: None,
    });

    assert_eq!(event, None);
}

#[test]
fn tool_failures_and_warnings_stay_visible_without_raw_output() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_protocol_warnings: true,
        ..DisplayProjectionOptions::default()
    });

    let blocked = projector
        .project(&TerminalEvent::ToolFinished {
            round: 1,
            tool: "write_file".to_string(),
            ok: false,
            decision: Some("denied_by_policy".to_string()),
            summary: "permission denied for /tmp/private.txt".to_string(),
            activity_title: Some("写入文件".to_string()),
            activity_detail: None,
        })
        .expect("blocked tool should project");
    let failed = projector
        .project(&TerminalEvent::ToolFinished {
            round: 1,
            tool: "code_execute".to_string(),
            ok: false,
            decision: Some("allow".to_string()),
            summary: "command failed: cat ~/.ssh/id_rsa".to_string(),
            activity_title: Some("执行本地命令".to_string()),
            activity_detail: None,
        })
        .expect("failed tool should project");
    let protocol = projector
        .project(&TerminalEvent::ProtocolError {
            round: 2,
            code: "missing_required_action".to_string(),
        })
        .expect("protocol warning should project");

    assert_eq!(blocked.kind, DisplayEventKind::Warning);
    assert_eq!(blocked.state, DisplayState::Blocked);
    assert_eq!(blocked.prominence, DisplayProminence::Alert);
    assert_eq!(blocked.message, "写入文件需要你的确认");
    assert!(!blocked.suppressible);

    assert_eq!(failed.state, DisplayState::Failed);
    assert_eq!(failed.message, "执行本地命令失败，正在保留现场信息");
    assert!(!failed.message.contains("cat"));
    assert!(!failed.message.contains("id_rsa"));

    assert_eq!(protocol.kind, DisplayEventKind::Progress);
    assert_eq!(protocol.state, DisplayState::Running);
    assert_eq!(protocol.message, "正在补全必要的实际检查");
}

#[test]
fn projector_hides_model_and_answer_internals_by_default() {
    let projector = DisplayProjector::default();

    assert_eq!(
        projector.project(&TerminalEvent::ModelFinished {
            round: 3,
            finish: "stop".to_string(),
            chars: 928,
        }),
        None
    );
    assert_eq!(
        projector.project(&TerminalEvent::AnswerReady {
            chars: 1200,
            truncated: false,
            snapshot_path: Some("/tmp/answer.txt".to_string()),
        }),
        None
    );
}

#[test]
fn projector_can_emit_sanitized_final_ready_event_when_enabled() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_final_ready_event: true,
        ..DisplayProjectionOptions::default()
    });

    let event = projector
        .project(&TerminalEvent::AnswerReady {
            chars: 1200,
            truncated: true,
            snapshot_path: Some("/tmp/answer.txt".to_string()),
        })
        .expect("final ready event should project when enabled");

    assert_eq!(event.kind, DisplayEventKind::Final);
    assert_eq!(event.state, DisplayState::Succeeded);
    assert_eq!(event.message, "答复已准备完成");
    assert!(!event.message.contains("1200"));
    assert!(!event.message.contains("/tmp/answer.txt"));
}

#[test]
fn project_all_keeps_human_readable_sequence() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_successful_tool_events: false,
        show_successful_step_events: false,
        show_model_progress: true,
        show_protocol_warnings: false,
        show_final_ready_event: true,
        show_lifecycle_steps: true,
    });
    let events = vec![
        TerminalEvent::TurnStarted {
            input_preview: "帮我检查一下".to_string(),
            max_tool_rounds: 4,
        },
        TerminalEvent::ModelStarted { round: 1 },
        TerminalEvent::ToolFinished {
            round: 1,
            tool: "read_file".to_string(),
            ok: true,
            decision: Some("allow".to_string()),
            summary: "read file".to_string(),
            activity_title: Some("读取文件".to_string()),
            activity_detail: None,
        },
        TerminalEvent::GuidanceInjected {
            round: 1,
            chars: 12,
        },
        TerminalEvent::AnswerReady {
            chars: 88,
            truncated: false,
            snapshot_path: None,
        },
    ];

    let projected = projector.project_all(&events);
    let messages = projected
        .iter()
        .map(|event| event.message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        messages,
        vec![
            "正在理解你的要求",
            "思考中…",
            "已接收新的补充要求",
            "答复已准备完成"
        ]
    );
}

#[test]
fn projector_emits_safe_tool_detail_and_cancel_state() {
    let projector = DisplayProjector::new(DisplayProjectionOptions {
        show_successful_tool_events: true,
        show_model_progress: true,
        ..DisplayProjectionOptions::default()
    });
    let started = projector
        .project(&TerminalEvent::ToolStarted {
            round: 1,
            tool: "code_execute".to_string(),
            summary: Some("raw command must stay hidden".to_string()),
            activity_title: Some("运行测试".to_string()),
            activity_detail: Some("验证当前改动没有回归".to_string()),
        })
        .expect("tool start should project");
    let cancelled = projector
        .project(&TerminalEvent::TurnCancelled {
            stage: "工具执行前".to_string(),
        })
        .expect("cancel should project");

    assert_eq!(started.message, "正在运行测试 · 验证当前改动没有回归");
    assert!(!started.message.contains("raw command"));
    assert_eq!(cancelled.state, DisplayState::Blocked);
    assert_eq!(cancelled.prominence, DisplayProminence::Alert);
    assert!(cancelled.message.contains("安全结束"));
}
