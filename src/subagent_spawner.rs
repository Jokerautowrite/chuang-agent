use std::collections::BTreeMap;

use crate::common::{AgentId, ReportId, TaskId, Timestamp};
use crate::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use serde::{Deserialize, Serialize};

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
struct FakeRun {
    request: SpawnRequest,
    receipt: SpawnReceipt,
    state: SubagentState,
    messages: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FakeSubagentSpawner {
    runs: BTreeMap<String, FakeRun>,
    next_run: u64,
}

#[derive(Debug, Clone)]
struct QueuedRun {
    request: SpawnRequest,
    receipt: SpawnReceipt,
    state: SubagentState,
    dispatch: SubagentDispatch,
    report: Option<SubagentReport>,
}

#[derive(Debug, Clone, Default)]
pub struct QueuedSubagentSpawner {
    runs: BTreeMap<String, QueuedRun>,
    dispatch_queue: Vec<RunId>,
    next_run: u64,
}

#[derive(Debug, Clone)]
pub enum SubagentSlot {
    Fake(FakeSubagentSpawner),
    Queued(QueuedSubagentSpawner),
}

impl FakeSubagentSpawner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self, run_id: &RunId) -> Option<&SubagentState> {
        self.runs.get(&run_id.0).map(|run| &run.state)
    }

    pub fn messages(&self, run_id: &RunId) -> Option<&[String]> {
        self.runs.get(&run_id.0).map(|run| run.messages.as_slice())
    }
}

impl QueuedSubagentSpawner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn_with_ids(
        &mut self,
        request: SpawnRequest,
        run_id: RunId,
        agent_id: AgentId,
    ) -> Result<SpawnReceipt, SubagentError> {
        self.spawn_inner(request, run_id, agent_id)
    }

    pub fn pending_dispatches(&self) -> Vec<SubagentDispatch> {
        self.dispatch_queue
            .iter()
            .filter_map(|run_id| self.runs.get(&run_id.0).map(|run| run.dispatch.clone()))
            .collect()
    }

    pub fn take_next_dispatch(&mut self) -> Option<SubagentDispatch> {
        if self.dispatch_queue.is_empty() {
            return None;
        }
        let run_id = self.dispatch_queue.remove(0);
        self.runs.get(&run_id.0).map(|run| run.dispatch.clone())
    }

    pub fn attach_report(
        &mut self,
        run_id: &RunId,
        report: SubagentReport,
    ) -> Result<(), SubagentError> {
        let run = self
            .runs
            .get_mut(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        if !matches!(run.state, SubagentState::Running) {
            return Err(SubagentError::NotRunning(run_id.clone()));
        }

        if report.task_id != run.request.task_id || report.agent_id != run.receipt.agent_id {
            return Err(SubagentError::InvalidRequest(
                "report identity does not match queued run".to_string(),
            ));
        }

        run.report = Some(report);
        run.state = SubagentState::Completed;
        Ok(())
    }

    pub fn state(&self, run_id: &RunId) -> Option<&SubagentState> {
        self.runs.get(&run_id.0).map(|run| &run.state)
    }
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

impl SubagentSpawner for FakeSubagentSpawner {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnReceipt, SubagentError> {
        validate_spawn_request(&request)?;
        self.next_run += 1;
        let run_id = RunId(format!("fake-run-{}", self.next_run));
        let agent_id = AgentId(format!("{}-{}", request.agent_name, self.next_run));
        let receipt = SpawnReceipt {
            run_id: run_id.clone(),
            agent_id,
            accepted_tool_policy: request.tool_policy.clone(),
            context_isolation: request.context_isolation.clone(),
            recursive_spawn: request.recursive_spawn,
        };

        self.runs.insert(
            run_id.0.clone(),
            FakeRun {
                request,
                receipt: receipt.clone(),
                state: SubagentState::Running,
                messages: Vec::new(),
            },
        );

        Ok(receipt)
    }

    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError> {
        let run = self
            .runs
            .get_mut(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        if !matches!(run.state, SubagentState::Running) {
            return Err(SubagentError::NotRunning(run_id.clone()));
        }

        if message.trim().is_empty() {
            return Err(SubagentError::InvalidRequest(
                "steer message must not be empty".to_string(),
            ));
        }

        run.messages.push(message);
        Ok(())
    }

    fn kill(&mut self, run_id: &RunId, reason: KillReason) -> Result<(), SubagentError> {
        let run = self
            .runs
            .get_mut(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        if !matches!(run.state, SubagentState::Running) {
            return Err(SubagentError::NotRunning(run_id.clone()));
        }

        run.state = SubagentState::Killed(reason);
        Ok(())
    }

    fn collect(&mut self, run_id: &RunId) -> Result<Option<SubagentReport>, SubagentError> {
        let run = self
            .runs
            .get_mut(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        match &run.state {
            SubagentState::Running => {
                run.state = SubagentState::Completed;
                Ok(Some(build_fake_report(run, ExecutionStatus::Success, None)))
            }
            SubagentState::Completed => {
                Ok(Some(build_fake_report(run, ExecutionStatus::Success, None)))
            }
            SubagentState::Killed(reason) => Ok(Some(build_fake_report(
                run,
                ExecutionStatus::Cancelled,
                Some(format!("killed: {reason:?}")),
            ))),
        }
    }
}

impl SubagentSpawner for QueuedSubagentSpawner {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnReceipt, SubagentError> {
        self.next_run += 1;
        let run_id = RunId(format!("queued-run-{}", self.next_run));
        let agent_id = AgentId(format!("{}-{}", request.agent_name, self.next_run));
        self.spawn_inner(request, run_id, agent_id)
    }

    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError> {
        let run = self
            .runs
            .get(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        if !matches!(run.state, SubagentState::Running) {
            return Err(SubagentError::NotRunning(run_id.clone()));
        }

        if message.trim().is_empty() {
            return Err(SubagentError::InvalidRequest(
                "steer message must not be empty".to_string(),
            ));
        }

        Ok(())
    }

    fn kill(&mut self, run_id: &RunId, reason: KillReason) -> Result<(), SubagentError> {
        let run = self
            .runs
            .get_mut(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        if !matches!(run.state, SubagentState::Running) {
            return Err(SubagentError::NotRunning(run_id.clone()));
        }

        run.state = SubagentState::Killed(reason);
        self.dispatch_queue.retain(|queued| queued != run_id);
        Ok(())
    }

    fn collect(&mut self, run_id: &RunId) -> Result<Option<SubagentReport>, SubagentError> {
        let run = self
            .runs
            .get(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        match &run.state {
            SubagentState::Running => Ok(None),
            SubagentState::Completed => Ok(run.report.clone()),
            SubagentState::Killed(reason) => Ok(Some(build_fake_report(
                &FakeRun {
                    request: run.request.clone(),
                    receipt: run.receipt.clone(),
                    state: run.state.clone(),
                    messages: Vec::new(),
                },
                ExecutionStatus::Cancelled,
                Some(format!("killed: {reason:?}")),
            ))),
        }
    }
}

impl QueuedSubagentSpawner {
    fn spawn_inner(
        &mut self,
        request: SpawnRequest,
        run_id: RunId,
        agent_id: AgentId,
    ) -> Result<SpawnReceipt, SubagentError> {
        validate_spawn_request(&request)?;
        if run_id.0.trim().is_empty() {
            return Err(SubagentError::InvalidRequest(
                "run_id must not be empty".to_string(),
            ));
        }
        if agent_id.0.trim().is_empty() {
            return Err(SubagentError::InvalidRequest(
                "agent_id must not be empty".to_string(),
            ));
        }
        if self.runs.contains_key(&run_id.0) {
            return Err(SubagentError::InvalidRequest(format!(
                "run_id already exists: {}",
                run_id.0
            )));
        }
        let receipt = SpawnReceipt {
            run_id: run_id.clone(),
            agent_id: agent_id.clone(),
            accepted_tool_policy: request.tool_policy.clone(),
            context_isolation: request.context_isolation.clone(),
            recursive_spawn: request.recursive_spawn,
        };
        let dispatch = SubagentDispatch {
            run_id: run_id.clone(),
            agent_id,
            task_id: request.task_id.clone(),
            parent_agent_id: request.parent_agent_id.clone(),
            agent_name: request.agent_name.clone(),
            task: request.task.clone(),
            tool_policy: request.tool_policy.clone(),
            context_isolation: request.context_isolation.clone(),
            token_budget: request.token_budget,
            idle_timeout_ms: request.idle_timeout_ms,
            recursive_spawn: request.recursive_spawn,
            metadata: request.metadata.clone(),
        };

        self.runs.insert(
            run_id.0.clone(),
            QueuedRun {
                request,
                receipt: receipt.clone(),
                state: SubagentState::Running,
                dispatch,
                report: None,
            },
        );
        self.dispatch_queue.push(run_id);

        Ok(receipt)
    }
}

fn validate_spawn_request(request: &SpawnRequest) -> Result<(), SubagentError> {
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

fn build_fake_report(
    run: &FakeRun,
    status: ExecutionStatus,
    stderr_preview: Option<String>,
) -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", run.receipt.run_id.0)),
        task_id: run.request.task_id.clone(),
        agent_id: run.receipt.agent_id.clone(),
        parent_agent_id: Some(run.request.parent_agent_id.clone()),
        status,
        started_at: Timestamp("2026-05-01T00:00:00Z".to_string()),
        finished_at: Timestamp("2026-05-01T00:00:01Z".to_string()),
        summary: format!(
            "fake subagent {} handled task with {:?} policy",
            run.receipt.agent_id.0, run.receipt.accepted_tool_policy
        ),
        exit_code: None,
        stdout_preview: Some(format!("messages={}", run.messages.len())),
        stderr_preview,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("fake-subagent://{}", run.receipt.run_id.0)),
        context_debug: None,
        truncated: false,
    }
}
