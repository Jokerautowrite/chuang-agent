//! Browser-backed external worker adapter line.
//!
//! `browser_worker` belongs to the adapter/plugin side of Chuang Agent. It can drive browser
//! surfaces such as DeepSeek Web through fake, injected, or opencli-backed drivers, but the core
//! runtime must continue to depend on generic provider/subagent/actuator ports instead of this
//! module directly.

pub mod adapters;
mod coordinator;
mod hash;
pub mod opencli_driver;
mod service;
mod session;
mod transcript;
mod types;

pub use adapters::deepseek_web::DeepSeekWebAdapter;
pub use adapters::{
    adapter_ensure_expert_mode, adapter_mark_ready, adapter_read_output, adapter_session,
    adapter_submit_task, BrowserProviderDriver, BrowserWorkerAdapter, FakeBrowserProviderDriver,
    ProviderBackedRealBrowserDriver, RealBrowserCommand, RealBrowserDriver, RealBrowserObservation,
};
pub use coordinator::BrowserWorkerCoordinator;
pub use hash::stable_content_hash;
pub use opencli_driver::{
    OpenCliCommandResult, OpenCliCommandSpec, OpenCliRealBrowserDriver, OpenCliRunner,
    SystemOpenCliRunner,
};
pub use service::{BrowserWorkerDemoService, DemoRun, SimulatedResponseFn};
pub use session::BrowserWorkerSession;
pub use transcript::{BrowserTranscript, BrowserTranscriptEntry, BrowserTranscriptRecord};
pub use types::{
    BrowserMode, BrowserWorkerError, DispatchReceipt, DispatchStatus, ProviderKind,
    WorkerFinishReason, WorkerOutput, WorkerState, WorkerTask,
};
