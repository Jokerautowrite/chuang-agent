use std::collections::BTreeMap;

use crate::common::AgentId;
use crate::subagent_report::{ExecutionStatus, SubagentReport};
use crate::subagent_spawner::{
    build_subagent_report, validate_spawn_request, KillReason, RunId, SpawnReceipt, SpawnRequest,
    SubagentError, SubagentSpawner, SubagentState,
};

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
                Ok(Some(build_report(run, ExecutionStatus::Success, None)))
            }
            SubagentState::Completed => Ok(Some(build_report(run, ExecutionStatus::Success, None))),
            SubagentState::Killed(reason) => Ok(Some(build_report(
                run,
                ExecutionStatus::Cancelled,
                Some(format!("killed: {reason:?}")),
            ))),
        }
    }
}

fn build_report(
    run: &FakeRun,
    status: ExecutionStatus,
    stderr_preview: Option<String>,
) -> SubagentReport {
    build_subagent_report(
        &run.request,
        &run.receipt,
        status,
        stderr_preview,
        Some(format!("messages={}", run.messages.len())),
        "fake-subagent",
    )
}
