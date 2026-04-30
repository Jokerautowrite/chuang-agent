use chuang_agent::browser_worker::{
    stable_content_hash, BrowserMode, BrowserTranscriptRecord, BrowserWorkerCoordinator,
    BrowserWorkerError, BrowserWorkerSession, DeepSeekWebAdapter, DispatchReceipt, DispatchStatus,
    ProviderKind, WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
};

#[derive(Clone)]
struct RuntimeLikeDriver {
    submitted_at: String,
    completed_at: String,
    snapshot_ref: String,
}

impl RuntimeLikeDriver {
    fn new() -> Self {
        Self {
            submitted_at: "2026-04-30T16:00:00Z".to_string(),
            completed_at: "2026-04-30T16:01:30Z".to_string(),
            snapshot_ref: "snapshot://runtime-like/task-browser-1".to_string(),
        }
    }
}

impl chuang_agent::browser_worker::BrowserProviderDriver for RuntimeLikeDriver {
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
                "runtime-like browser output for {} via {}",
                receipt.task_id, session.page_url
            ),
            raw_snapshot_ref: Some(self.snapshot_ref.clone()),
            completed_at: self.completed_at.clone(),
            finish_reason: WorkerFinishReason::Completed,
        })
    }
}

fn runtime_session() -> BrowserWorkerSession {
    BrowserWorkerSession {
        worker_id: "worker-runtime-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        mode: BrowserMode::Expert,
        page_url: "https://chat.deepseek.com/real-browser-anchor".to_string(),
        logged_in: true,
        last_prompt: None,
        last_prompt_hash: None,
        last_output_hash: None,
        last_dispatch_at: None,
        last_read_at: None,
        state: WorkerState::Ready,
    }
}

fn runtime_task() -> WorkerTask {
    WorkerTask {
        task_id: "task-browser-1".to_string(),
        title: "Browser runtime integration".to_string(),
        prompt: "请模拟真实浏览器链路，提交任务并读取 transcript。".to_string(),
    }
}

#[test]
fn browser_worker_runtime_like_submit_read_transcript_completed_flow() {
    let mut adapter = DeepSeekWebAdapter::with_driver(
        "worker-runtime-1",
        "https://chat.deepseek.com/real-browser-anchor",
        RuntimeLikeDriver::new(),
    );
    adapter.ensure_expert_mode();
    adapter.mark_ready();

    let mut coordinator = BrowserWorkerCoordinator::new(adapter.session.clone());
    let task = runtime_task();

    let planned = coordinator
        .enqueue(task.clone())
        .expect("enqueue should succeed");
    let receipt = adapter
        .submit_task(&planned)
        .expect("submit in runtime-like flow should succeed");
    coordinator
        .attach_receipt(receipt.clone())
        .expect("coordinator should accept submit receipt");

    let output = adapter
        .read_output(&receipt)
        .expect("read in runtime-like flow should succeed");
    let record = coordinator
        .attach_output(&planned, &output)
        .expect("coordinator should build transcript after read");

    assert_eq!(receipt.task_id, task.task_id);
    assert_eq!(receipt.worker_id, "worker-runtime-1");
    assert_eq!(receipt.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(receipt.status, DispatchStatus::Submitted);
    assert_eq!(receipt.mode, BrowserMode::Expert);
    assert_eq!(receipt.prompt_hash, stable_content_hash(&task.prompt));

    assert_eq!(output.task_id, task.task_id);
    assert_eq!(output.finish_reason, WorkerFinishReason::Completed);
    assert_eq!(
        output.raw_snapshot_ref.as_deref(),
        Some("snapshot://runtime-like/task-browser-1")
    );
    assert!(output.content.contains("runtime-like browser output"));
    assert!(output.content.contains("real-browser-anchor"));

    assert_eq!(coordinator.session.state, WorkerState::Completed);
    assert_eq!(
        coordinator.session.last_prompt.as_deref(),
        Some(task.prompt.as_str())
    );
    assert_eq!(
        coordinator.session.last_prompt_hash.as_deref(),
        Some(receipt.prompt_hash.as_str())
    );
    assert_eq!(
        coordinator.session.last_dispatch_at.as_deref(),
        Some(receipt.submitted_at.as_str())
    );
    assert_eq!(
        coordinator.session.last_read_at.as_deref(),
        Some(output.completed_at.as_str())
    );
    assert_eq!(
        coordinator.session.last_output_hash,
        Some(stable_content_hash(&output.content))
    );

    assert_eq!(record.task_id, task.task_id);
    assert_eq!(record.worker_id, "worker-runtime-1");
    assert_eq!(record.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(record.prompt, task.prompt);
    assert_eq!(record.output.as_deref(), Some(output.content.as_str()));
    assert_eq!(record.entries.len(), 2);
    assert_eq!(record.entries[0].role, "user");
    assert_eq!(record.entries[0].timestamp, receipt.submitted_at);
    assert_eq!(record.entries[1].role, "assistant");
    assert_eq!(record.entries[1].timestamp, output.completed_at);
}

#[test]
fn browser_worker_runtime_like_attach_output_requires_submit_before_transcript_completion() {
    let mut coordinator = BrowserWorkerCoordinator::new(runtime_session());
    let task = runtime_task();
    let output = WorkerOutput {
        worker_id: "worker-runtime-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        task_id: task.task_id.clone(),
        content: "runtime-like browser output without submit".to_string(),
        raw_snapshot_ref: Some("snapshot://runtime-like/task-browser-1".to_string()),
        completed_at: "2026-04-30T16:01:30Z".to_string(),
        finish_reason: WorkerFinishReason::Completed,
    };

    coordinator
        .enqueue(task.clone())
        .expect("enqueue should succeed");

    let error = coordinator
        .attach_output(&task, &output)
        .expect_err("output before receipt should fail");

    assert_eq!(
        error,
        BrowserWorkerError::InvalidStateTransition {
            from: WorkerState::Dispatching,
            action: "attach_output",
        }
    );
}

#[test]
fn browser_worker_runtime_like_transcript_anchor_accepts_real_browser_snapshot_reference() {
    let task = runtime_task();
    let receipt = DispatchReceipt {
        task_id: task.task_id.clone(),
        worker_id: "worker-runtime-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        submitted_at: "2026-04-30T16:00:00Z".to_string(),
        prompt_hash: stable_content_hash(&task.prompt),
        mode: BrowserMode::Expert,
        status: DispatchStatus::Submitted,
    };
    let output = WorkerOutput {
        worker_id: "worker-runtime-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        task_id: task.task_id.clone(),
        content: "future real browser output anchor".to_string(),
        raw_snapshot_ref: Some("playwright-trace://browser-runtime/task-browser-1".to_string()),
        completed_at: "2026-04-30T16:01:30Z".to_string(),
        finish_reason: WorkerFinishReason::Completed,
    };

    let transcript = chuang_agent::browser_worker::BrowserTranscript::new();
    let record: BrowserTranscriptRecord =
        transcript.complete_record(transcript.start_record(&task, &receipt), &output);

    assert_eq!(
        record.output.as_deref(),
        Some("future real browser output anchor")
    );
    assert_eq!(
        output.raw_snapshot_ref.as_deref(),
        Some("playwright-trace://browser-runtime/task-browser-1")
    );
    assert_eq!(record.entries.len(), 2);
}
