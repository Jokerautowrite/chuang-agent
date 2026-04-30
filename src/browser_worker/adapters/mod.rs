pub mod deepseek_web;

use crate::browser_worker::{
    BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, WorkerOutput, WorkerTask,
};

pub use deepseek_web::DeepSeekWebAdapter;

pub trait BrowserWorkerAdapter {
    fn session(&self) -> &BrowserWorkerSession;
    fn ensure_expert_mode(&mut self);
    fn mark_ready(&mut self);
    fn submit_task(&mut self, task: &WorkerTask) -> Result<DispatchReceipt, BrowserWorkerError>;
    fn read_output(
        &mut self,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError>;
}

pub fn adapter_session(adapter: &impl BrowserWorkerAdapter) -> &BrowserWorkerSession {
    adapter.session()
}

pub fn adapter_ensure_expert_mode(adapter: &mut impl BrowserWorkerAdapter) {
    adapter.ensure_expert_mode();
}

pub fn adapter_mark_ready(adapter: &mut impl BrowserWorkerAdapter) {
    adapter.mark_ready();
}

pub fn adapter_submit_task(
    adapter: &mut impl BrowserWorkerAdapter,
    task: &WorkerTask,
) -> Result<DispatchReceipt, BrowserWorkerError> {
    adapter.submit_task(task)
}

pub fn adapter_read_output(
    adapter: &mut impl BrowserWorkerAdapter,
    receipt: &DispatchReceipt,
) -> Result<WorkerOutput, BrowserWorkerError> {
    adapter.read_output(receipt)
}
