use std::collections::BTreeMap;

use crate::common::AgentId;
use crate::subagent_report::{ExecutionStatus, SubagentReport};
use crate::subagent_spawner::{
    build_subagent_report, validate_spawn_request, KillReason, RunId, SpawnReceipt, SpawnRequest,
    SubagentDispatch, SubagentError, SubagentSpawner, SubagentState,
};

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
            SubagentState::Killed(reason) => Ok(Some(build_subagent_report(
                &run.request,
                &run.receipt,
                ExecutionStatus::Cancelled,
                Some(format!("killed: {reason:?}")),
                Some("messages=0".to_string()),
                "fake-subagent",
            ))),
        }
    }
}
