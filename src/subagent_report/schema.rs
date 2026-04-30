use crate::common::{AgentId, ReportId, TaskId, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Success,
    Failed,
    Cancelled,
    TimedOut,
}

impl ExecutionStatus {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "Success" => Some(Self::Success),
            "Failed" => Some(Self::Failed),
            "Cancelled" => Some(Self::Cancelled),
            "TimedOut" => Some(Self::TimedOut),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactKind {
    File,
    Directory,
    Url,
    Log,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    pub locator: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResourceUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub wall_time_ms: u64,
    pub cpu_time_ms: u64,
    pub peak_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentReport {
    pub schema_version: String,
    pub report_id: ReportId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub parent_agent_id: Option<AgentId>,
    pub status: ExecutionStatus,
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
    pub summary: String,
    pub exit_code: Option<i32>,
    pub stdout_preview: Option<String>,
    pub stderr_preview: Option<String>,
    pub resource_usage: ResourceUsage,
    pub artifacts: Vec<ArtifactRef>,
    pub replay_ref: Option<String>,
    pub truncated: bool,
}
