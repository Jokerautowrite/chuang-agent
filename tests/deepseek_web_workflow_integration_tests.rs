use chuang_agent::browser_worker::{
    stable_content_hash, BrowserMode, BrowserProviderDriver, BrowserTranscriptEntry,
    BrowserTranscriptRecord, BrowserWorkerError, BrowserWorkerSession, DeepSeekWebAdapter,
    DispatchReceipt, DispatchStatus, ProviderKind, WorkerFinishReason, WorkerOutput, WorkerState,
    WorkerTask,
};

#[derive(Clone)]
struct SimulatedDeepSeekDriver {
    submitted_at: String,
    completed_at: String,
    snapshot_ref: String,
}

impl SimulatedDeepSeekDriver {
    fn new() -> Self {
        Self {
            submitted_at: "2026-04-30T15:30:00Z".to_string(),
            completed_at: "2026-04-30T15:31:30Z".to_string(),
            snapshot_ref: "snapshot://deepseek-web/task-demo-1".to_string(),
        }
    }
}

impl BrowserProviderDriver for SimulatedDeepSeekDriver {
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
            content: format!(
                "simulated output for worker {} task {}",
                session.worker_id, receipt.task_id
            ),
            raw_snapshot_ref: Some(self.snapshot_ref.clone()),
            completed_at: self.completed_at.clone(),
            finish_reason: WorkerFinishReason::Completed,
        })
    }
}

fn sample_task() -> WorkerTask {
    WorkerTask {
        task_id: "task-deepseek-web-1".to_string(),
        title: "Summarize current page".to_string(),
        prompt: "请总结当前网页的关键要点，并标出 3 个后续行动。".to_string(),
    }
}

#[test]
fn deepseek_web_workflow_happy_path_preserves_runtime_state_and_transcript_shape() {
    let mut adapter = DeepSeekWebAdapter::with_driver(
        "worker-deepseek-1",
        "https://chat.deepseek.com/",
        SimulatedDeepSeekDriver::new(),
    );

    adapter.ensure_expert_mode();
    assert_eq!(adapter.session.mode, BrowserMode::Expert);
    assert_eq!(adapter.session.state, WorkerState::SwitchingMode);

    adapter.mark_ready();
    assert!(adapter.session.logged_in);
    assert_eq!(adapter.session.state, WorkerState::Ready);

    let task = sample_task();
    let receipt = adapter
        .submit_task(&task)
        .expect("ready deepseek workflow should accept submit");

    assert_eq!(receipt.task_id, task.task_id);
    assert_eq!(receipt.worker_id, "worker-deepseek-1");
    assert_eq!(receipt.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(receipt.mode, BrowserMode::Expert);
    assert_eq!(receipt.status, DispatchStatus::Submitted);
    assert_eq!(receipt.prompt_hash, stable_content_hash(&task.prompt));
    assert_eq!(
        adapter.session.last_prompt.as_deref(),
        Some(task.prompt.as_str())
    );
    assert_eq!(
        adapter.session.last_prompt_hash.as_deref(),
        Some(receipt.prompt_hash.as_str())
    );
    assert_eq!(
        adapter.session.last_dispatch_at.as_deref(),
        Some(receipt.submitted_at.as_str())
    );
    assert_eq!(adapter.session.state, WorkerState::WaitingResponse);

    let output = adapter
        .read_output(&receipt)
        .expect("submitted deepseek workflow should read output");

    assert_eq!(output.worker_id, "worker-deepseek-1");
    assert_eq!(output.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(output.task_id, task.task_id);
    assert_eq!(output.finish_reason, WorkerFinishReason::Completed);
    assert!(output.content.contains("worker worker-deepseek-1"));
    assert!(output.content.contains("task task-deepseek-web-1"));
    assert_eq!(
        output.raw_snapshot_ref.as_deref(),
        Some("snapshot://deepseek-web/task-demo-1")
    );
    assert_eq!(
        adapter.session.last_read_at.as_deref(),
        Some(output.completed_at.as_str())
    );
    assert_eq!(
        adapter.session.last_output_hash,
        Some(stable_content_hash(&output.content))
    );
    assert_eq!(adapter.session.state, WorkerState::Completed);

    let record = BrowserTranscriptRecord {
        task_id: task.task_id.clone(),
        worker_id: receipt.worker_id.clone(),
        provider: receipt.provider.clone(),
        prompt: task.prompt.clone(),
        output: Some(output.content.clone()),
        raw_snapshot_ref: output.raw_snapshot_ref.clone(),
        entries: vec![
            BrowserTranscriptEntry {
                role: "user".to_string(),
                content: task.prompt.clone(),
                timestamp: receipt.submitted_at.clone(),
            },
            BrowserTranscriptEntry {
                role: "assistant".to_string(),
                content: output.content.clone(),
                timestamp: output.completed_at.clone(),
            },
        ],
    };

    assert_eq!(record.raw_snapshot_ref, output.raw_snapshot_ref);

    assert_eq!(record.entries.len(), 2);
    assert_eq!(record.entries[0].role, "user");
    assert_eq!(record.entries[0].content, task.prompt);
    assert_eq!(record.entries[0].timestamp, receipt.submitted_at);
    assert_eq!(record.entries[1].role, "assistant");
    assert_eq!(record.entries[1].content, output.content);
    assert_eq!(record.entries[1].timestamp, output.completed_at);
}

#[test]
fn deepseek_web_read_output_requires_dispatch_receipt_state() {
    let mut adapter = DeepSeekWebAdapter::new("worker-deepseek-2", "https://chat.deepseek.com/");
    adapter.session = BrowserWorkerSession {
        worker_id: "worker-deepseek-2".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        mode: BrowserMode::Expert,
        page_url: "https://chat.deepseek.com/".to_string(),
        logged_in: true,
        last_prompt: Some("已有 prompt 上下文".to_string()),
        last_prompt_hash: Some(stable_content_hash("已有 prompt 上下文")),
        last_output_hash: None,
        last_dispatch_at: None,
        last_read_at: None,
        state: WorkerState::WaitingResponse,
    };
    let receipt = DispatchReceipt {
        task_id: "task-deepseek-web-2".to_string(),
        worker_id: "worker-deepseek-2".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        submitted_at: "fake-submitted-at".to_string(),
        prompt_hash: stable_content_hash("已有 prompt 上下文"),
        mode: BrowserMode::Expert,
        status: DispatchStatus::Submitted,
    };

    let error = adapter
        .read_output(&receipt)
        .expect_err("missing dispatch receipt metadata should fail");

    assert_eq!(error, BrowserWorkerError::MissingDispatchReceipt);
}
