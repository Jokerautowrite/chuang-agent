use crate::browser_worker::{
    BrowserProviderDriver, BrowserTranscriptRecord, BrowserWorkerCoordinator, BrowserWorkerError,
    BrowserWorkerSession, DeepSeekWebAdapter, DispatchReceipt, FakeBrowserProviderDriver,
    WorkerOutput, WorkerTask,
};

pub type SimulatedResponseFn = fn(&WorkerTask) -> String;

#[derive(Debug, Clone)]
pub struct BrowserWorkerDemoService<D = FakeBrowserProviderDriver> {
    worker_id: String,
    page_url: String,
    _responder: SimulatedResponseFn,
    driver: D,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoRun {
    pub session: BrowserWorkerSession,
    pub receipt: DispatchReceipt,
    pub output: WorkerOutput,
    pub record: BrowserTranscriptRecord,
}

impl BrowserWorkerDemoService<FakeBrowserProviderDriver> {
    pub fn new(
        worker_id: impl Into<String>,
        page_url: impl Into<String>,
        responder: SimulatedResponseFn,
    ) -> Self {
        Self::with_driver(worker_id, page_url, responder, FakeBrowserProviderDriver)
    }
}

impl<D> BrowserWorkerDemoService<D> {
    pub fn with_driver(
        worker_id: impl Into<String>,
        page_url: impl Into<String>,
        responder: SimulatedResponseFn,
        driver: D,
    ) -> Self {
        Self {
            worker_id: worker_id.into(),
            page_url: page_url.into(),
            _responder: responder,
            driver,
        }
    }
}

impl<D: BrowserProviderDriver + Clone> BrowserWorkerDemoService<D> {
    pub fn run(&self, task: WorkerTask) -> Result<DemoRun, BrowserWorkerError> {
        let mut adapter = DeepSeekWebAdapter::with_driver(
            self.worker_id.clone(),
            self.page_url.clone(),
            self.driver.clone(),
        );
        adapter.ensure_expert_mode();
        adapter.mark_ready();

        let mut coordinator = BrowserWorkerCoordinator::new(adapter.session.clone());
        let planned_task = coordinator.enqueue(task.clone())?;
        let receipt = adapter.submit_task(&planned_task)?;
        coordinator.attach_receipt(receipt.clone())?;

        let output = adapter.read_output(&receipt)?;
        let record = coordinator.attach_output(&planned_task, &output)?;

        Ok(DemoRun {
            session: coordinator.session,
            receipt,
            output,
            record,
        })
    }
}
