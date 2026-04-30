use crate::browser_worker::{
    hash::stable_content_hash, BrowserMode, BrowserWorkerError, DispatchReceipt, ProviderKind,
    WorkerOutput, WorkerState, WorkerTask,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserWorkerSession {
    pub worker_id: String,
    pub provider: ProviderKind,
    pub mode: BrowserMode,
    pub page_url: String,
    pub logged_in: bool,
    pub last_prompt: Option<String>,
    pub last_prompt_hash: Option<String>,
    pub last_output_hash: Option<String>,
    pub last_dispatch_at: Option<String>,
    pub last_read_at: Option<String>,
    pub state: WorkerState,
}

impl BrowserWorkerSession {
    pub fn new(
        worker_id: impl Into<String>,
        provider: ProviderKind,
        page_url: impl Into<String>,
    ) -> Self {
        Self {
            worker_id: worker_id.into(),
            provider,
            mode: BrowserMode::Unknown,
            page_url: page_url.into(),
            logged_in: false,
            last_prompt: None,
            last_prompt_hash: None,
            last_output_hash: None,
            last_dispatch_at: None,
            last_read_at: None,
            state: WorkerState::Uninitialized,
        }
    }

    pub fn apply_task(&mut self, task: &WorkerTask) {
        self.last_prompt = Some(task.prompt.clone());
        self.last_prompt_hash = Some(stable_content_hash(&task.prompt));
        self.state = WorkerState::Dispatching;
    }

    pub fn apply_receipt(&mut self, receipt: &DispatchReceipt) -> Result<(), BrowserWorkerError> {
        if self.last_prompt.is_none() {
            return Err(BrowserWorkerError::MissingPromptContext);
        }
        if self.state != WorkerState::Dispatching {
            return Err(BrowserWorkerError::InvalidStateTransition {
                from: self.state.clone(),
                action: "apply_receipt",
            });
        }

        self.last_dispatch_at = Some(receipt.submitted_at.clone());
        self.last_prompt_hash = Some(receipt.prompt_hash.clone());
        self.mode = receipt.mode.clone();
        self.state = WorkerState::WaitingResponse;
        Ok(())
    }

    pub fn apply_output(&mut self, output: &WorkerOutput) -> Result<(), BrowserWorkerError> {
        if self.last_prompt.is_none() {
            return Err(BrowserWorkerError::MissingPromptContext);
        }
        if self.state != WorkerState::WaitingResponse {
            return Err(BrowserWorkerError::InvalidStateTransition {
                from: self.state.clone(),
                action: "apply_output",
            });
        }
        if self.last_dispatch_at.is_none() || self.last_prompt_hash.is_none() {
            return Err(BrowserWorkerError::MissingDispatchReceipt);
        }

        self.last_read_at = Some(output.completed_at.clone());
        self.last_output_hash = Some(stable_content_hash(&output.content));
        self.state = WorkerState::Completed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::BrowserWorkerSession;
    use crate::browser_worker::{
        stable_content_hash, BrowserMode, DispatchReceipt, DispatchStatus, ProviderKind,
        WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
    };

    #[test]
    fn new_session_starts_uninitialized_with_empty_runtime_fields() {
        let session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );

        assert_eq!(session.worker_id, "worker-1");
        assert_eq!(session.provider, ProviderKind::DeepSeekWeb);
        assert_eq!(session.mode, BrowserMode::Unknown);
        assert_eq!(session.page_url, "https://example.test/chat");
        assert!(!session.logged_in);
        assert_eq!(session.last_prompt, None);
        assert_eq!(session.last_prompt_hash, None);
        assert_eq!(session.last_output_hash, None);
        assert_eq!(session.last_dispatch_at, None);
        assert_eq!(session.last_read_at, None);
        assert_eq!(session.state, WorkerState::Uninitialized);
    }

    #[test]
    fn apply_task_updates_prompt_hash_and_dispatch_state() {
        let mut session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );
        let task = WorkerTask {
            task_id: "task-1".into(),
            title: "demo".into(),
            prompt: "hello browser worker".into(),
        };

        session.apply_task(&task);

        assert_eq!(session.last_prompt, Some("hello browser worker".into()));
        assert_eq!(session.last_prompt_hash, Some("ae2212e02f06c4bb".into()));
        assert_eq!(session.state, WorkerState::Dispatching);
    }

    #[test]
    fn apply_receipt_tracks_dispatch_metadata_and_waiting_state() {
        let mut session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );
        let receipt = DispatchReceipt {
            task_id: "task-1".into(),
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            submitted_at: "2026-04-30T15:20:00Z".into(),
            prompt_hash: "prompt-hash".into(),
            mode: BrowserMode::Expert,
            status: DispatchStatus::Submitted,
        };

        session.last_prompt = Some("hello browser worker".into());
        session.last_prompt_hash = Some(stable_content_hash("hello browser worker"));
        session.state = WorkerState::Dispatching;

        session
            .apply_receipt(&receipt)
            .expect("receipt should apply");

        assert_eq!(
            session.last_dispatch_at,
            Some("2026-04-30T15:20:00Z".into())
        );
        assert_eq!(session.last_prompt_hash, Some("prompt-hash".into()));
        assert_eq!(session.mode, BrowserMode::Expert);
        assert_eq!(session.state, WorkerState::WaitingResponse);
    }

    #[test]
    fn apply_output_tracks_readback_and_completion_state() {
        let mut session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );
        let output = WorkerOutput {
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            task_id: "task-1".into(),
            content: "done".into(),
            raw_snapshot_ref: None,
            completed_at: "2026-04-30T15:22:00Z".into(),
            finish_reason: WorkerFinishReason::Completed,
        };

        session.last_prompt = Some("hello browser worker".into());
        session.last_prompt_hash = Some("prompt-hash".into());
        session.last_dispatch_at = Some("2026-04-30T15:20:00Z".into());
        session.state = WorkerState::WaitingResponse;

        session.apply_output(&output).expect("output should apply");

        assert_eq!(session.last_read_at, Some("2026-04-30T15:22:00Z".into()));
        assert_eq!(session.last_output_hash, Some("dc51fb6761fd6e91".into()));
        assert_eq!(session.state, WorkerState::Completed);
    }

    #[test]
    fn apply_output_rejects_missing_prompt_context() {
        let mut session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );
        let output = WorkerOutput {
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            task_id: "task-1".into(),
            content: "done".into(),
            raw_snapshot_ref: None,
            completed_at: "2026-04-30T15:22:00Z".into(),
            finish_reason: WorkerFinishReason::Completed,
        };
        session.last_prompt_hash = Some("prompt-hash".into());
        session.last_dispatch_at = Some("2026-04-30T15:20:00Z".into());
        session.state = WorkerState::WaitingResponse;

        let error = session
            .apply_output(&output)
            .expect_err("missing prompt context should fail");

        assert_eq!(
            error,
            crate::browser_worker::BrowserWorkerError::MissingPromptContext
        );
    }

    #[test]
    fn apply_output_rejects_invalid_state_transition() {
        let mut session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );
        let output = WorkerOutput {
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            task_id: "task-1".into(),
            content: "done".into(),
            raw_snapshot_ref: None,
            completed_at: "2026-04-30T15:22:00Z".into(),
            finish_reason: WorkerFinishReason::Completed,
        };
        session.last_prompt = Some("hello browser worker".into());
        session.last_prompt_hash = Some("prompt-hash".into());
        session.last_dispatch_at = Some("2026-04-30T15:20:00Z".into());
        session.state = WorkerState::Dispatching;

        let error = session
            .apply_output(&output)
            .expect_err("invalid state should fail");

        assert_eq!(
            error,
            crate::browser_worker::BrowserWorkerError::InvalidStateTransition {
                from: WorkerState::Dispatching,
                action: "apply_output",
            }
        );
    }

    #[test]
    fn apply_output_rejects_missing_dispatch_receipt() {
        let mut session = BrowserWorkerSession::new(
            "worker-1",
            ProviderKind::DeepSeekWeb,
            "https://example.test/chat",
        );
        let output = WorkerOutput {
            worker_id: "worker-1".into(),
            provider: ProviderKind::DeepSeekWeb,
            task_id: "task-1".into(),
            content: "done".into(),
            raw_snapshot_ref: None,
            completed_at: "2026-04-30T15:22:00Z".into(),
            finish_reason: WorkerFinishReason::Completed,
        };
        session.last_prompt = Some("hello browser worker".into());
        session.state = WorkerState::WaitingResponse;

        let error = session
            .apply_output(&output)
            .expect_err("missing receipt should fail");

        assert_eq!(
            error,
            crate::browser_worker::BrowserWorkerError::MissingDispatchReceipt
        );
    }
}
