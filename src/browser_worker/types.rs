#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    DeepSeekWeb,
    ChatGPTWeb,
    ClaudeWeb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserMode {
    Unknown,
    Fast,
    Expert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerState {
    Uninitialized,
    Ready,
    SwitchingMode,
    Dispatching,
    WaitingResponse,
    ReadingResponse,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchStatus {
    Queued,
    Submitted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerFinishReason {
    Completed,
    StoppedEarly,
    Blocked,
    NetworkError,
    ManualInterruption,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserWorkerError {
    MissingPromptContext,
    MissingDispatchReceipt,
    InvalidStateTransition {
        from: WorkerState,
        action: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerTask {
    pub task_id: String,
    pub title: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchReceipt {
    pub task_id: String,
    pub worker_id: String,
    pub provider: ProviderKind,
    pub submitted_at: String,
    pub prompt_hash: String,
    pub mode: BrowserMode,
    pub status: DispatchStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerOutput {
    pub worker_id: String,
    pub provider: ProviderKind,
    pub task_id: String,
    pub content: String,
    pub raw_snapshot_ref: Option<String>,
    pub completed_at: String,
    pub finish_reason: WorkerFinishReason,
}
