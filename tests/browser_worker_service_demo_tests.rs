use chuang_agent::browser_worker::{
    stable_content_hash, BrowserMode, BrowserProviderDriver, BrowserWorkerDemoService,
    BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, DispatchStatus, ProviderKind,
    WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
};

fn simulated_deepseek_web_response(task: &WorkerTask) -> String {
    format!(
        "simulated deepseek web response for {}: 最小 browser_worker demo/service 闭环已串起 session/coordinator/adapter",
        task.task_id
    )
}

#[derive(Clone)]
struct RecordingDriver {
    submitted_at: String,
    completed_at: String,
    snapshot_ref: String,
    output_content: String,
}

impl RecordingDriver {
    fn new() -> Self {
        Self {
            submitted_at: "2026-04-30T15:30:00Z".to_string(),
            completed_at: "2026-04-30T15:31:30Z".to_string(),
            snapshot_ref: "snapshot://deepseek-web/service-driver".to_string(),
            output_content: "driver supplied output".to_string(),
        }
    }
}

impl BrowserProviderDriver for RecordingDriver {
    fn submit_task(
        &mut self,
        session: &BrowserWorkerSession,
        task: &WorkerTask,
    ) -> Result<DispatchReceipt, BrowserWorkerError> {
        Ok(DispatchReceipt {
            task_id: task.task_id.clone(),
            worker_id: session.worker_id.clone(),
            provider: session.provider.clone(),
            submitted_at: self.submitted_at.clone(),
            prompt_hash: session.last_prompt_hash.clone().unwrap_or_default(),
            mode: session.mode.clone(),
            status: DispatchStatus::Submitted,
        })
    }

    fn read_output(
        &mut self,
        session: &BrowserWorkerSession,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError> {
        Ok(WorkerOutput {
            worker_id: session.worker_id.clone(),
            provider: session.provider.clone(),
            task_id: receipt.task_id.clone(),
            content: self.output_content.clone(),
            raw_snapshot_ref: Some(self.snapshot_ref.clone()),
            completed_at: self.completed_at.clone(),
            finish_reason: WorkerFinishReason::Completed,
        })
    }
}

#[test]
fn browser_worker_service_runs_minimal_deepseek_demo_workflow() {
    let service = BrowserWorkerDemoService::new(
        "worker-demo",
        "https://chat.deepseek.com",
        simulated_deepseek_web_response,
    );
    let task = WorkerTask {
        task_id: "task-demo-1".to_string(),
        title: "Summarize changelog".to_string(),
        prompt: "请模拟 DeepSeek Web 总结 browser_worker 最小闭环状态".to_string(),
    };

    let run = service
        .run(task.clone())
        .expect("demo workflow should succeed");

    assert_eq!(run.session.worker_id, "worker-demo");
    assert_eq!(run.session.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(run.session.mode, BrowserMode::Expert);
    assert_eq!(run.session.page_url, "https://chat.deepseek.com");
    assert!(run.session.logged_in);
    assert_eq!(run.session.state, WorkerState::Completed);
    assert_eq!(run.receipt.task_id, task.task_id);
    assert_eq!(run.output.task_id, task.task_id);
    assert_eq!(run.record.task_id, task.task_id);
    assert_eq!(run.record.prompt, task.prompt);
    assert!(run.output.content.contains("fake provider output"));
    assert_eq!(
        run.record.output.as_deref(),
        Some(run.output.content.as_str())
    );
    assert_eq!(run.record.entries.len(), 2);
    assert_eq!(run.record.entries[0].timestamp, run.receipt.submitted_at);
    assert_eq!(run.record.entries[1].timestamp, run.output.completed_at);
}

#[test]
fn browser_worker_service_supports_injected_driver() {
    let service = BrowserWorkerDemoService::with_driver(
        "worker-demo-driver",
        "https://chat.deepseek.com",
        simulated_deepseek_web_response,
        RecordingDriver::new(),
    );
    let task = WorkerTask {
        task_id: "task-demo-driver-1".to_string(),
        title: "Summarize changelog".to_string(),
        prompt: "请通过注入 driver 跑通 service 层闭环".to_string(),
    };

    let run = service
        .run(task.clone())
        .expect("injected driver workflow should succeed");

    assert_eq!(run.session.worker_id, "worker-demo-driver");
    assert_eq!(run.receipt.submitted_at, "2026-04-30T15:30:00Z");
    assert_eq!(run.output.completed_at, "2026-04-30T15:31:30Z");
    assert_eq!(
        run.output.raw_snapshot_ref.as_deref(),
        Some("snapshot://deepseek-web/service-driver")
    );
    assert_eq!(run.output.finish_reason, WorkerFinishReason::Completed);
    assert_eq!(run.session.state, WorkerState::Completed);
    assert_eq!(
        run.session.last_prompt_hash.as_deref(),
        Some(stable_content_hash(&task.prompt).as_str())
    );
    assert_eq!(run.record.entries[0].timestamp, run.receipt.submitted_at);
    assert_eq!(run.record.entries[1].timestamp, run.output.completed_at);
    assert_eq!(run.output.content, "driver supplied output");
}
