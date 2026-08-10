//! `browser_worker::coordinator` 模块。公开接口：struct BrowserWorkerCoordinator；fn new, enqueue, attach_receipt, attach_output。

use crate::browser_worker::{
    BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, DispatchStatus, WorkerOutput,
    WorkerState, WorkerTask,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserWorkerCoordinator {
    pub session: BrowserWorkerSession,
}

impl BrowserWorkerCoordinator {
    pub fn new(session: BrowserWorkerSession) -> Self {
        Self { session }
    }

    pub fn enqueue(&mut self, task: WorkerTask) -> Result<WorkerTask, BrowserWorkerError> {
        self.session.apply_task(&task);
        Ok(task)
    }

    pub fn attach_receipt(&mut self, receipt: DispatchReceipt) -> Result<(), BrowserWorkerError> {
        if self.session.last_prompt.is_none() {
            return Err(BrowserWorkerError::MissingPromptContext);
        }
        if self.session.state != WorkerState::Dispatching {
            return Err(BrowserWorkerError::InvalidStateTransition {
                from: self.session.state.clone(),
                action: "attach_receipt",
            });
        }

        self.session.apply_receipt(&receipt)?;
        Ok(())
    }

    pub fn attach_output(
        &mut self,
        task: &WorkerTask,
        output: &WorkerOutput,
    ) -> Result<crate::browser_worker::BrowserTranscriptRecord, BrowserWorkerError> {
        if self.session.last_prompt.is_none() {
            return Err(BrowserWorkerError::MissingPromptContext);
        }
        if self.session.state != WorkerState::WaitingResponse {
            return Err(BrowserWorkerError::InvalidStateTransition {
                from: self.session.state.clone(),
                action: "attach_output",
            });
        }
        if self.session.last_dispatch_at.is_none() || self.session.last_prompt_hash.is_none() {
            return Err(BrowserWorkerError::MissingDispatchReceipt);
        }

        self.session.apply_output(output)?;

        let receipt = DispatchReceipt {
            task_id: task.task_id.clone(),
            worker_id: self.session.worker_id.clone(),
            provider: self.session.provider.clone(),
            submitted_at: self
                .session
                .last_dispatch_at
                .clone()
                .ok_or(BrowserWorkerError::MissingDispatchReceipt)?,
            prompt_hash: self
                .session
                .last_prompt_hash
                .clone()
                .ok_or(BrowserWorkerError::MissingDispatchReceipt)?,
            mode: self.session.mode.clone(),
            status: DispatchStatus::Submitted,
        };

        let transcript = crate::browser_worker::BrowserTranscript::new();
        let record = transcript.start_record(task, &receipt);
        Ok(transcript.complete_record(record, output))
    }
}
