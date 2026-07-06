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
    ToolStarted {
        round: usize,
        tool: String,
        summary: Option<String>,
    },
    ToolFinished {
        round: usize,
        tool: String,
        ok: bool,
        decision: Option<String>,
        summary: String,
    },
    ProtocolError {
        round: usize,
        code: String,
    },
    GuidanceInjected {
        round: usize,
        chars: usize,
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
