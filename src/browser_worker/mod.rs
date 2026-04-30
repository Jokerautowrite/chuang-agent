pub mod adapters;
mod coordinator;
mod hash;
mod session;
mod transcript;
mod types;

pub use adapters::deepseek_web::DeepSeekWebAdapter;
pub use adapters::{
    adapter_ensure_expert_mode, adapter_mark_ready, adapter_read_output, adapter_session,
    adapter_submit_task, BrowserWorkerAdapter,
};
pub use coordinator::BrowserWorkerCoordinator;
pub use hash::stable_content_hash;
pub use session::BrowserWorkerSession;
pub use transcript::{BrowserTranscript, BrowserTranscriptEntry, BrowserTranscriptRecord};
pub use types::{
    BrowserMode, BrowserWorkerError, DispatchReceipt, DispatchStatus, ProviderKind,
    WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
};
