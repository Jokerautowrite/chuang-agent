use chuang_agent::browser_worker::{
    stable_content_hash, BrowserMode, BrowserTranscriptEntry, BrowserWorkerCoordinator,
    BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, DispatchStatus, ProviderKind,
    WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
};

fn sample_session() -> BrowserWorkerSession {
    BrowserWorkerSession {
        worker_id: "worker-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        mode: BrowserMode::Expert,
        page_url: "https://chat.deepseek.com".to_string(),
        logged_in: true,
        last_prompt: None,
        last_prompt_hash: None,
        last_output_hash: None,
        last_dispatch_at: None,
        last_read_at: None,
        state: WorkerState::Ready,
    }
}

fn sample_task() -> WorkerTask {
    WorkerTask {
        task_id: "task-1".to_string(),
        title: "Summarize page".to_string(),
        prompt: "Summarize the current page".to_string(),
    }
}

fn sample_receipt() -> DispatchReceipt {
    DispatchReceipt {
        task_id: "task-1".to_string(),
        worker_id: "worker-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        submitted_at: "2026-04-30T15:20:00Z".to_string(),
        prompt_hash: "ed8a50922cd4daab".to_string(),
        mode: BrowserMode::Expert,
        status: DispatchStatus::Submitted,
    }
}

fn sample_output() -> WorkerOutput {
    WorkerOutput {
        worker_id: "worker-1".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        task_id: "task-1".to_string(),
        content: "Page summary".to_string(),
        raw_snapshot_ref: Some("snapshot://task-1".to_string()),
        completed_at: "2026-04-30T15:21:00Z".to_string(),
        finish_reason: WorkerFinishReason::Completed,
    }
}

#[test]
fn enqueue_attach_receipt_attach_output_happy_path_preserves_boundary_fields() {
    let mut coordinator = BrowserWorkerCoordinator::new(sample_session());
    let task = sample_task();
    let receipt = sample_receipt();
    let output = sample_output();

    let planned = coordinator.enqueue(task.clone()).unwrap();
    coordinator.attach_receipt(receipt.clone()).unwrap();
    let record = coordinator.attach_output(&task, &output).unwrap();

    assert_eq!(planned, task);
    assert_eq!(coordinator.session.worker_id, "worker-1");
    assert_eq!(coordinator.session.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(coordinator.session.page_url, "https://chat.deepseek.com");
    assert!(coordinator.session.logged_in);
    assert_eq!(coordinator.session.mode, BrowserMode::Expert);
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
    assert_eq!(coordinator.session.state, WorkerState::Completed);
    assert_eq!(record.task_id, task.task_id);
    assert_eq!(record.worker_id, receipt.worker_id);
    assert_eq!(record.provider, receipt.provider);
    assert_eq!(record.prompt, task.prompt);
    assert_eq!(record.output.as_deref(), Some(output.content.as_str()));
    assert_eq!(record.entries.len(), 2);
    assert_eq!(record.entries[0].timestamp, receipt.submitted_at);
    assert_eq!(record.entries[1].timestamp, output.completed_at);
}

#[test]
fn enqueue_moves_ready_session_to_dispatching_and_records_prompt() {
    let mut coordinator = BrowserWorkerCoordinator::new(sample_session());

    let planned = coordinator.enqueue(sample_task()).unwrap();

    assert_eq!(planned.task_id, "task-1");
    assert_eq!(
        coordinator.session.last_prompt.as_deref(),
        Some("Summarize the current page")
    );
    assert_eq!(coordinator.session.state, WorkerState::Dispatching);
}

#[test]
fn attach_receipt_moves_session_to_waiting_and_keeps_prompt_hash() {
    let mut coordinator = BrowserWorkerCoordinator::new(sample_session());
    coordinator.enqueue(sample_task()).unwrap();

    coordinator.attach_receipt(sample_receipt()).unwrap();

    assert_eq!(
        coordinator.session.last_prompt_hash.as_deref(),
        Some("ed8a50922cd4daab")
    );
    assert_eq!(
        coordinator.session.last_dispatch_at.as_deref(),
        Some("2026-04-30T15:20:00Z")
    );
    assert_eq!(coordinator.session.state, WorkerState::WaitingResponse);
}

#[test]
fn attach_output_completes_session_and_builds_transcript_record() {
    let mut coordinator = BrowserWorkerCoordinator::new(sample_session());
    let task = sample_task();
    coordinator.enqueue(task.clone()).unwrap();
    coordinator.attach_receipt(sample_receipt()).unwrap();

    let record = coordinator.attach_output(&task, &sample_output()).unwrap();

    assert_eq!(
        coordinator.session.last_read_at.as_deref(),
        Some("2026-04-30T15:21:00Z")
    );
    assert_eq!(
        coordinator.session.last_output_hash.as_deref(),
        Some("f0ef3b6c59b198ae")
    );
    assert_eq!(coordinator.session.state, WorkerState::Completed);
    assert_eq!(record.task_id, "task-1");
    assert_eq!(record.prompt, "Summarize the current page");
    assert_eq!(record.output.as_deref(), Some("Page summary"));
    assert_eq!(
        record.entries,
        vec![
            BrowserTranscriptEntry {
                role: "user".to_string(),
                content: "Summarize the current page".to_string(),
                timestamp: "2026-04-30T15:20:00Z".to_string(),
            },
            BrowserTranscriptEntry {
                role: "assistant".to_string(),
                content: "Page summary".to_string(),
                timestamp: "2026-04-30T15:21:00Z".to_string(),
            }
        ]
    );
}

#[test]
fn attach_receipt_rejects_missing_prompt_context() {
    let mut coordinator = BrowserWorkerCoordinator::new(sample_session());

    let error = coordinator
        .attach_receipt(sample_receipt())
        .expect_err("receipt without enqueue should fail");

    assert_eq!(error, BrowserWorkerError::MissingPromptContext);
}

#[test]
fn attach_receipt_rejects_invalid_session_state() {
    let mut session = sample_session();
    session.last_prompt = Some("Summarize the current page".to_string());
    session.last_prompt_hash = Some(stable_content_hash("Summarize the current page"));
    session.state = WorkerState::Completed;
    let mut coordinator = BrowserWorkerCoordinator::new(session);

    let error = coordinator
        .attach_receipt(sample_receipt())
        .expect_err("receipt in completed state should fail");

    assert_eq!(
        error,
        BrowserWorkerError::InvalidStateTransition {
            from: WorkerState::Completed,
            action: "attach_receipt",
        }
    );
}

#[test]
fn attach_output_rejects_invalid_waiting_state() {
    let mut session = sample_session();
    session.last_prompt = Some("Summarize the current page".to_string());
    session.last_prompt_hash = Some("ed8a50922cd4daab".to_string());
    session.last_dispatch_at = Some("2026-04-30T15:20:00Z".to_string());
    session.state = WorkerState::Dispatching;
    let mut coordinator = BrowserWorkerCoordinator::new(session);
    let task = sample_task();

    let error = coordinator
        .attach_output(&task, &sample_output())
        .expect_err("output in dispatching state should fail");

    assert_eq!(
        error,
        BrowserWorkerError::InvalidStateTransition {
            from: WorkerState::Dispatching,
            action: "attach_output",
        }
    );
}

#[test]
fn attach_output_rejects_missing_prompt_context_even_if_receipt_fields_exist() {
    let mut session = sample_session();
    session.last_dispatch_at = Some("2026-04-30T15:20:00Z".to_string());
    session.last_prompt_hash = Some("hash-123".to_string());
    session.state = WorkerState::WaitingResponse;
    let mut coordinator = BrowserWorkerCoordinator::new(session);
    let task = sample_task();

    let error = coordinator
        .attach_output(&task, &sample_output())
        .expect_err("output without prompt context should fail");

    assert_eq!(error, BrowserWorkerError::MissingPromptContext);
}
