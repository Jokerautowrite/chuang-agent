use std::collections::BTreeMap;

use crate::common::{AgentId, ReportId, TaskId, Timestamp};
use crate::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use serde::{Deserialize, Serialize};

mod fake;
mod queued;

pub use fake::FakeSubagentSpawner;
pub use queued::QueuedSubagentSpawner;

pub const QUEUED_STEER_MESSAGES_METADATA_KEY: &str = "queued_steer_messages_json";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentToolPolicy {
    Analyze,
    Execute,
    Orchestrate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextIsolation {
    Isolated,
    Forked { max_parent_tokens: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub task_id: TaskId,
    pub parent_agent_id: AgentId,
    pub agent_name: String,
    pub task: String,
    pub tool_policy: SubagentToolPolicy,
    pub context_isolation: ContextIsolation,
    pub token_budget: u16,
    pub idle_timeout_ms: u64,
    pub recursive_spawn: bool,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnReceipt {
    pub run_id: RunId,
    pub agent_id: AgentId,
    pub accepted_tool_policy: SubagentToolPolicy,
    pub context_isolation: ContextIsolation,
    pub recursive_spawn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentDispatch {
    pub run_id: RunId,
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub parent_agent_id: AgentId,
    pub agent_name: String,
    pub task: String,
    pub tool_policy: SubagentToolPolicy,
    pub context_isolation: ContextIsolation,
    pub token_budget: u16,
    pub idle_timeout_ms: u64,
    pub recursive_spawn: bool,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KillReason {
    UserRequested,
    Timeout,
    PolicyViolation,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentState {
    Running,
    Completed,
    Killed(KillReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentError {
    InvalidRequest(String),
    UnknownRun(RunId),
    NotRunning(RunId),
}

pub trait SubagentSpawner {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnReceipt, SubagentError>;
    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError>;
    fn kill(&mut self, run_id: &RunId, reason: KillReason) -> Result<(), SubagentError>;
    fn collect(&mut self, run_id: &RunId) -> Result<Option<SubagentReport>, SubagentError>;
}

#[derive(Debug, Clone)]
pub enum SubagentSlot {
    Fake(FakeSubagentSpawner),
    Queued(QueuedSubagentSpawner),
}

impl SubagentSpawner for SubagentSlot {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnReceipt, SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.spawn(request),
            Self::Queued(spawner) => spawner.spawn(request),
        }
    }

    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.steer(run_id, message),
            Self::Queued(spawner) => spawner.steer(run_id, message),
        }
    }

    fn kill(&mut self, run_id: &RunId, reason: KillReason) -> Result<(), SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.kill(run_id, reason),
            Self::Queued(spawner) => spawner.kill(run_id, reason),
        }
    }

    fn collect(&mut self, run_id: &RunId) -> Result<Option<SubagentReport>, SubagentError> {
        match self {
            Self::Fake(spawner) => spawner.collect(run_id),
            Self::Queued(spawner) => spawner.collect(run_id),
        }
    }
}

pub(crate) fn validate_spawn_request(request: &SpawnRequest) -> Result<(), SubagentError> {
    if request.task_id.0.trim().is_empty() {
        return Err(SubagentError::InvalidRequest(
            "task_id must not be empty".to_string(),
        ));
    }

    if request.parent_agent_id.0.trim().is_empty() {
        return Err(SubagentError::InvalidRequest(
            "parent_agent_id must not be empty".to_string(),
        ));
    }

    if request.agent_name.trim().is_empty() {
        return Err(SubagentError::InvalidRequest(
            "agent_name must not be empty".to_string(),
        ));
    }

    if request.task.trim().is_empty() {
        return Err(SubagentError::InvalidRequest(
            "task must not be empty".to_string(),
        ));
    }

    if request.token_budget == 0 {
        return Err(SubagentError::InvalidRequest(
            "token_budget must be greater than zero".to_string(),
        ));
    }

    if matches!(request.tool_policy, SubagentToolPolicy::Analyze) && request.recursive_spawn {
        return Err(SubagentError::InvalidRequest(
            "analyze policy cannot enable recursive spawn".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn build_subagent_report(
    request: &SpawnRequest,
    receipt: &SpawnReceipt,
    status: ExecutionStatus,
    stderr_preview: Option<String>,
    stdout_preview: Option<String>,
    replay_prefix: &str,
) -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", receipt.run_id.0)),
        task_id: request.task_id.clone(),
        agent_id: receipt.agent_id.clone(),
        parent_agent_id: Some(request.parent_agent_id.clone()),
        status,
        started_at: Timestamp("2026-05-01T00:00:00Z".to_string()),
        finished_at: Timestamp("2026-05-01T00:00:01Z".to_string()),
        summary: format!(
            "fake subagent {} handled task with {:?} policy",
            receipt.agent_id.0, receipt.accepted_tool_policy
        ),
        exit_code: None,
        stdout_preview,
        stderr_preview,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("{replay_prefix}://{}", receipt.run_id.0)),
        context_debug: None,
        governance_decision: None,
        truncated: false,
        skill_proposals: vec![],
    }
}
