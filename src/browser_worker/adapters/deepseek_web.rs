//! `browser_worker::adapters::deepseek_web` 模块。公开接口：struct DeepSeekWebAdapter；fn new, with_driver, ensure_expert_mode, mark_ready, submit_task, read_output。

use crate::browser_worker::{
    BrowserMode, BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, ProviderKind,
    WorkerOutput, WorkerState, WorkerTask,
};

use super::{BrowserProviderDriver, BrowserWorkerAdapter, FakeBrowserProviderDriver};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepSeekWebAdapter<D = FakeBrowserProviderDriver> {
    pub session: BrowserWorkerSession,
    driver: D,
}

impl DeepSeekWebAdapter<FakeBrowserProviderDriver> {
    pub fn new(worker_id: impl Into<String>, page_url: impl Into<String>) -> Self {
        Self::with_driver(worker_id, page_url, FakeBrowserProviderDriver)
    }
}

impl<D> DeepSeekWebAdapter<D> {
    pub fn with_driver(
        worker_id: impl Into<String>,
        page_url: impl Into<String>,
        driver: D,
    ) -> Self {
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
            driver,
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
}

impl<D: BrowserProviderDriver> DeepSeekWebAdapter<D> {
    pub fn submit_task(
        &mut self,
        task: &WorkerTask,
    ) -> Result<DispatchReceipt, BrowserWorkerError> {
        self.session.apply_task(task);

        let receipt = self.driver.submit_task(&self.session, task)?;
        self.session.apply_receipt(&receipt)?;
        Ok(receipt)
    }

    pub fn read_output(
        &mut self,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError> {
        let output = self.driver.read_output(&self.session, receipt)?;
        self.session.apply_output(&output)?;
        Ok(output)
    }
}

impl<D: BrowserProviderDriver> BrowserWorkerAdapter for DeepSeekWebAdapter<D> {
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
