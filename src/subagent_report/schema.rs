use crate::common::{AgentId, ReportId, TaskId, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    File,
    Directory,
    Url,
    Log,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    pub locator: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDropReasonSummary {
    pub segment_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDebugSummary {
    pub dropped_segment_ids: Vec<String>,
    pub drop_reasons: Vec<ContextDropReasonSummary>,
    pub budget_exceeded: bool,
    pub budget_exceeded_reasons: Vec<String>,
    pub working_reservation: Option<WorkingReservationDebug>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingReservationDebug {
    pub reserved_segment_id: String,
    pub reserved_tokens: u32,
    pub dropped_segment_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceDecisionSummary {
    pub action_id: String,
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub wall_time_ms: u64,
    pub cpu_time_ms: u64,
    pub peak_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub context_debug: Option<ContextDebugSummary>,
    pub governance_decision: Option<GovernanceDecisionSummary>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportAdmissionStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportAdmission {
    pub schema_version: String,
    pub report_id: Option<ReportId>,
    pub task_id: Option<TaskId>,
    pub agent_id: Option<AgentId>,
    pub controller_agent_id: AgentId,
    pub status: ReportAdmissionStatus,
    pub reason_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_reason_code: Option<String>,
    pub reason: String,
    pub decided_at: Timestamp,
}
