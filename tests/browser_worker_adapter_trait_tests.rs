use chuang_agent::browser_worker::{
    BrowserMode, BrowserWorkerAdapter, DeepSeekWebAdapter, DispatchStatus, ProviderKind,
    WorkerFinishReason, WorkerState, WorkerTask,
};

#[test]
fn deepseek_web_adapter_exposes_session_via_trait() {
    let adapter = DeepSeekWebAdapter::new("worker-1", "https://chat.deepseek.com");

    let session = chuang_agent::browser_worker::adapter_session(&adapter);

    assert_eq!(session.worker_id, "worker-1");
    assert_eq!(session.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(session.page_url, "https://chat.deepseek.com");
    assert_eq!(session.state, WorkerState::Uninitialized);
}

#[test]
fn deepseek_web_adapter_trait_mutations_update_underlying_session() {
    let mut adapter = DeepSeekWebAdapter::new("worker-1", "https://chat.deepseek.com");

    chuang_agent::browser_worker::adapter_ensure_expert_mode(&mut adapter);
    {
        let session = chuang_agent::browser_worker::adapter_session(&adapter);
        assert_eq!(session.mode, BrowserMode::Expert);
        assert_eq!(session.state, WorkerState::SwitchingMode);
    }

    chuang_agent::browser_worker::adapter_mark_ready(&mut adapter);
    let session = chuang_agent::browser_worker::adapter_session(&adapter);
    assert!(session.logged_in);
    assert_eq!(session.state, WorkerState::Ready);
}

#[test]
fn deepseek_web_adapter_submit_task_uses_default_fake_driver() {
    let mut adapter = DeepSeekWebAdapter::new("worker-1", "https://chat.deepseek.com");
    adapter.ensure_expert_mode();
    adapter.mark_ready();

    let task = WorkerTask {
        task_id: "task-1".to_string(),
        title: "Summarize page".to_string(),
        prompt: "Summarize the current page".to_string(),
    };

    let receipt = chuang_agent::browser_worker::adapter_submit_task(&mut adapter, &task)
        .expect("default fake driver submit should succeed");

    let session = chuang_agent::browser_worker::adapter_session(&adapter);
    assert_eq!(receipt.task_id, task.task_id);
    assert_eq!(receipt.worker_id, "worker-1");
    assert_eq!(receipt.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(receipt.status, DispatchStatus::Submitted);
    assert_eq!(receipt.mode, BrowserMode::Expert);
    assert_eq!(receipt.submitted_at, "fake-submitted-at");
    assert_eq!(
        receipt.prompt_hash,
        session.last_prompt_hash.clone().unwrap()
    );
    assert_eq!(session.last_prompt.as_deref(), Some(task.prompt.as_str()));
    assert_eq!(session.state, WorkerState::WaitingResponse);
}

#[test]
fn deepseek_web_adapter_read_output_uses_default_fake_driver() {
    let mut adapter = DeepSeekWebAdapter::new("worker-1", "https://chat.deepseek.com");
    adapter.ensure_expert_mode();
    adapter.mark_ready();

    let task = WorkerTask {
        task_id: "task-1".to_string(),
        title: "Summarize page".to_string(),
        prompt: "Summarize the current page".to_string(),
    };

    let receipt = chuang_agent::browser_worker::adapter_submit_task(&mut adapter, &task)
        .expect("default fake driver submit should succeed");
    let output = chuang_agent::browser_worker::adapter_read_output(&mut adapter, &receipt)
        .expect("default fake driver read should succeed");

    let session = chuang_agent::browser_worker::adapter_session(&adapter);
    assert_eq!(output.worker_id, "worker-1");
    assert_eq!(output.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(output.task_id, task.task_id);
    assert_eq!(output.finish_reason, WorkerFinishReason::Completed);
    assert_eq!(
        output.content,
        "fake provider output for worker worker-1 task task-1"
    );
    assert_eq!(
        output.raw_snapshot_ref.as_deref(),
        Some("fake-provider-snapshot")
    );
    assert_eq!(output.completed_at, "fake-completed-at");
    assert_eq!(session.state, WorkerState::Completed);
    assert_eq!(
        session.last_output_hash,
        Some(chuang_agent::browser_worker::stable_content_hash(
            &output.content
        ))
    );
}

#[test]
fn deepseek_web_adapter_implements_browser_worker_adapter_trait() {
    fn assert_adapter<T: BrowserWorkerAdapter>(_adapter: &T) {}

    let adapter = DeepSeekWebAdapter::new("worker-2", "https://chat.deepseek.com");
    assert_adapter(&adapter);
}
