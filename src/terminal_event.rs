use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalEvent {
    TurnStarted {
        input_preview: String,
        max_tool_rounds: usize,
    },
    StepStarted {
        title: String,
        detail: Option<String>,
    },
    StepFinished {
        title: String,
        status: StepStatus,
        detail: Option<String>,
    },
    ModelStarted {
        round: usize,
    },
    ModelFinished {
        round: usize,
        finish: String,
        chars: usize,
    },
    ModelRetried {
        round: usize,
        attempt: usize,
        reason: String,
    },
    ToolStarted {
        round: usize,
        tool: String,
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity_title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity_detail: Option<String>,
    },
    ToolFinished {
        round: usize,
        tool: String,
        ok: bool,
        decision: Option<String>,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity_title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity_detail: Option<String>,
    },
    ProtocolError {
        round: usize,
        code: String,
    },
    GuidanceInjected {
        round: usize,
        chars: usize,
    },
    TurnCancelled {
        stage: String,
    },
    AnswerReady {
        chars: usize,
        truncated: bool,
        snapshot_path: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Ok,
    Failed,
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::{StepStatus, TerminalEvent};

    #[test]
    fn terminal_event_tool_started_accepts_legacy_payload_without_activity_fields() {
        let event: TerminalEvent = serde_json::from_str(
            r#"{
                "kind":"tool_started",
                "round":1,
                "tool":"shell_exec",
                "summary":"legacy"
            }"#,
        )
        .expect("legacy payload should remain compatible");

        assert_eq!(
            event,
            TerminalEvent::ToolStarted {
                round: 1,
                tool: "shell_exec".to_string(),
                summary: Some("legacy".to_string()),
                activity_title: None,
                activity_detail: None,
            }
        );
    }

    #[test]
    fn terminal_event_tool_finished_omits_empty_activity_fields_when_serialized() {
        let event = TerminalEvent::ToolFinished {
            round: 2,
            tool: "read_file".to_string(),
            ok: true,
            decision: Some("allow".to_string()),
            summary: "done".to_string(),
            activity_title: None,
            activity_detail: None,
        };

        let json = serde_json::to_string(&event).expect("event should serialize");
        assert!(json.contains(r#""kind":"tool_finished""#));
        assert!(!json.contains("activity_title"));
        assert!(!json.contains("activity_detail"));
    }

    #[test]
    fn terminal_event_step_variants_remain_unchanged() {
        let event = TerminalEvent::StepFinished {
            title: "整理最终答复".to_string(),
            status: StepStatus::Ok,
            detail: Some("已生成最终答复".to_string()),
        };

        let json = serde_json::to_string(&event).expect("step event should serialize");
        assert!(json.contains(r#""kind":"step_finished""#));
        assert!(json.contains("整理最终答复"));
    }
}
