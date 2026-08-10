//! `subagent_spawner::queued` 模块。公开接口：struct QueuedSubagentSpawner；fn new, spawn_with_ids, restore_dispatch, pending_dispatches, dispatch_snapshot, take_next_dispatch, attach_report, state。

use std::collections::BTreeMap;

use crate::common::AgentId;
use crate::subagent_report::{ExecutionStatus, SubagentReport};
use crate::subagent_spawner::{
    build_subagent_report, validate_spawn_request, KillReason, RunId, SpawnReceipt, SpawnRequest,
    SubagentDispatch, SubagentError, SubagentSpawner, SubagentState,
    QUEUED_STEER_MESSAGES_METADATA_KEY,
};

#[derive(Debug, Clone)]
struct QueuedRun {
    request: SpawnRequest,
    receipt: SpawnReceipt,
    state: SubagentState,
    dispatch: SubagentDispatch,
    steer_inbox: Vec<String>,
    report: Option<SubagentReport>,
}

#[derive(Debug, Clone)]
struct PreparedQueuedSpawn {
    run_id: RunId,
    receipt: SpawnReceipt,
    queued_run: QueuedRun,
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
        let prepared = self.prepare_spawn(request, run_id, agent_id)?;
        Ok(self.commit_prepared_spawn(prepared, None))
    }

    pub fn restore_dispatch(
        &mut self,
        dispatch: SubagentDispatch,
    ) -> Result<SpawnReceipt, SubagentError> {
        let restored_run_number = queued_run_number(&dispatch.run_id);
        let request = SpawnRequest {
            task_id: dispatch.task_id.clone(),
            parent_agent_id: dispatch.parent_agent_id.clone(),
            agent_name: dispatch.agent_name.clone(),
            task: dispatch.task.clone(),
            tool_policy: dispatch.tool_policy.clone(),
            context_isolation: dispatch.context_isolation.clone(),
            token_budget: dispatch.token_budget,
            idle_timeout_ms: dispatch.idle_timeout_ms,
            recursive_spawn: dispatch.recursive_spawn,
            metadata: dispatch.metadata.clone(),
        };
        let prepared = self.prepare_spawn(request, dispatch.run_id, dispatch.agent_id)?;
        if let Some(restored_run_number) = restored_run_number {
            self.next_run = self.next_run.max(restored_run_number);
        }
        Ok(self.commit_prepared_spawn(prepared, None))
    }

    pub fn pending_dispatches(&self) -> Vec<SubagentDispatch> {
        self.dispatch_queue
            .iter()
            .filter_map(|run_id| self.runs.get(&run_id.0).map(|run| run.dispatch.clone()))
            .collect()
    }

    pub fn dispatch_snapshot(&self, run_id: &RunId) -> Option<SubagentDispatch> {
        self.runs.get(&run_id.0).map(|run| run.dispatch.clone())
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

        if report.task_id != run.request.task_id
            || report.agent_id != run.receipt.agent_id
            || report.parent_agent_id.as_ref() != Some(&run.request.parent_agent_id)
        {
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

    pub fn steer_messages(&self, run_id: &RunId) -> Option<&[String]> {
        self.runs
            .get(&run_id.0)
            .map(|run| run.steer_inbox.as_slice())
    }

    pub fn persist_steer<F>(
        &mut self,
        run_id: &RunId,
        message: String,
        persist_dispatch: F,
    ) -> Result<(), SubagentError>
    where
        F: FnOnce(&SubagentDispatch) -> Result<(), SubagentError>,
    {
        let (next_dispatch, next_steer_inbox) = {
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

            let mut next_steer_inbox = run.steer_inbox.clone();
            next_steer_inbox.push(message.clone());

            let mut next_dispatch = run.dispatch.clone();
            next_dispatch.metadata.insert(
                QUEUED_STEER_MESSAGES_METADATA_KEY.to_string(),
                encode_steer_messages(&next_steer_inbox),
            );

            (next_dispatch, next_steer_inbox)
        };

        persist_dispatch(&next_dispatch)?;

        let run = self
            .runs
            .get_mut(&run_id.0)
            .ok_or_else(|| SubagentError::UnknownRun(run_id.clone()))?;

        if !matches!(run.state, SubagentState::Running) {
            return Err(SubagentError::NotRunning(run_id.clone()));
        }

        run.steer_inbox = next_steer_inbox;
        run.dispatch = next_dispatch;
        Ok(())
    }

    pub fn persist_spawn<F>(
        &mut self,
        request: SpawnRequest,
        persist_dispatch: F,
    ) -> Result<SpawnReceipt, SubagentError>
    where
        F: FnOnce(&SubagentDispatch) -> Result<(), SubagentError>,
    {
        let next_run = self
            .next_run
            .checked_add(1)
            .ok_or_else(|| SubagentError::InvalidRequest("next_run overflow".to_string()))?;
        let run_id = RunId(format!("queued-run-{next_run}"));
        let agent_id = AgentId(format!("{}-{next_run}", request.agent_name));
        let prepared = self.prepare_spawn(request, run_id, agent_id)?;

        persist_dispatch(&prepared.queued_run.dispatch)?;

        Ok(self.commit_prepared_spawn(prepared, Some(next_run)))
    }

    fn prepare_spawn(
        &mut self,
        request: SpawnRequest,
        run_id: RunId,
        agent_id: AgentId,
    ) -> Result<PreparedQueuedSpawn, SubagentError> {
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
        let steer_inbox = decode_steer_messages(&request.metadata)?;
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

        Ok(PreparedQueuedSpawn {
            run_id,
            receipt: receipt.clone(),
            queued_run: QueuedRun {
                request,
                receipt,
                state: SubagentState::Running,
                dispatch,
                steer_inbox,
                report: None,
            },
        })
    }

    fn commit_prepared_spawn(
        &mut self,
        prepared: PreparedQueuedSpawn,
        next_run: Option<u64>,
    ) -> SpawnReceipt {
        let PreparedQueuedSpawn {
            run_id,
            receipt,
            queued_run,
        } = prepared;

        self.runs.insert(run_id.0.clone(), queued_run);
        self.dispatch_queue.push(run_id);
        if let Some(next_run) = next_run {
            self.next_run = next_run;
        }
        receipt
    }
}

impl SubagentSpawner for QueuedSubagentSpawner {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnReceipt, SubagentError> {
        self.persist_spawn(request, |_| Ok(()))
    }

    fn steer(&mut self, run_id: &RunId, message: String) -> Result<(), SubagentError> {
        self.persist_steer(run_id, message, |_| Ok(()))
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

fn decode_steer_messages(
    metadata: &BTreeMap<String, String>,
) -> Result<Vec<String>, SubagentError> {
    let Some(raw_messages) = metadata.get(QUEUED_STEER_MESSAGES_METADATA_KEY) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw_messages).map_err(|error| {
        SubagentError::InvalidRequest(format!(
            "invalid {}: {}",
            QUEUED_STEER_MESSAGES_METADATA_KEY, error
        ))
    })
}

fn encode_steer_messages(messages: &[String]) -> String {
    serde_json::to_string(messages).expect("serializing steer messages should not fail")
}

fn queued_run_number(run_id: &RunId) -> Option<u64> {
    let suffix = run_id.0.strip_prefix("queued-run-")?;
    suffix.parse::<u64>().ok()
}
