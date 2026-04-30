use crate::browser_worker::{
    BrowserMode, BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, DispatchStatus,
    ProviderKind, WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
};

use super::BrowserWorkerAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepSeekWebAdapter {
    pub session: BrowserWorkerSession,
}

impl DeepSeekWebAdapter {
    pub fn new(worker_id: impl Into<String>, page_url: impl Into<String>) -> Self {
        Self {
            session: BrowserWorkerSession {
                worker_id: worker_id.into(),
                provider: ProviderKind::DeepSeekWeb,
                mode: BrowserMode::Unknown,
                page_url: page_url.into(),
                logged_in: false,
                last_prompt: None,
                last_prompt_hash: None,
                last_output_hash: None,
                last_dispatch_at: None,
                last_read_at: None,
                state: WorkerState::Uninitialized,
            },
        }
    }

    pub fn ensure_expert_mode(&mut self) {
        self.session.mode = BrowserMode::Expert;
        self.session.state = WorkerState::SwitchingMode;
    }

    pub fn mark_ready(&mut self) {
        self.session.logged_in = true;
        self.session.state = WorkerState::Ready;
    }

    pub fn submit_task(
        &mut self,
        task: &WorkerTask,
    ) -> Result<DispatchReceipt, BrowserWorkerError> {
        self.session.apply_task(task);

        let receipt = DispatchReceipt {
            task_id: task.task_id.clone(),
            worker_id: self.session.worker_id.clone(),
            provider: self.session.provider.clone(),
            submitted_at: "placeholder-submitted-at".to_string(),
            prompt_hash: self.session.last_prompt_hash.clone().unwrap_or_default(),
            mode: self.session.mode.clone(),
            status: DispatchStatus::Submitted,
        };

        self.session.apply_receipt(&receipt)?;
        Ok(receipt)
    }

    pub fn read_output(
        &mut self,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError> {
        let output = WorkerOutput {
            worker_id: self.session.worker_id.clone(),
            provider: self.session.provider.clone(),
            task_id: receipt.task_id.clone(),
            content: format!(
                "placeholder output for worker {} task {}",
                self.session.worker_id, receipt.task_id
            ),
            raw_snapshot_ref: Some("placeholder-snapshot".to_string()),
            completed_at: "placeholder-completed-at".to_string(),
            finish_reason: WorkerFinishReason::Completed,
        };

        self.session.apply_output(&output)?;
        Ok(output)
    }
}

impl BrowserWorkerAdapter for DeepSeekWebAdapter {
    fn session(&self) -> &BrowserWorkerSession {
        &self.session
    }

    fn ensure_expert_mode(&mut self) {
        DeepSeekWebAdapter::ensure_expert_mode(self);
    }

    fn mark_ready(&mut self) {
        DeepSeekWebAdapter::mark_ready(self);
    }

    fn submit_task(&mut self, task: &WorkerTask) -> Result<DispatchReceipt, BrowserWorkerError> {
        DeepSeekWebAdapter::submit_task(self, task)
    }

    fn read_output(
        &mut self,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError> {
        DeepSeekWebAdapter::read_output(self, receipt)
    }
}
