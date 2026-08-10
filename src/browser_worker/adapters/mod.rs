//! `browser_worker::adapters::mod` 模块。公开接口：trait BrowserWorkerAdapter, BrowserProviderDriver, RealBrowserDriver；struct ProviderBackedRealBrowserDriver, FakeBrowserProviderDriver；enum RealBrowserCommand, RealBrowserObservation；fn new, into_inner, adapter_session, adapter_ensure_expert_mode, adapter_mark_ready, adapter_submit_task, adapter_read_output；mod deepseek_web；use deepseek_web。

pub mod deepseek_web;

use crate::browser_worker::{
    BrowserMode, BrowserWorkerError, BrowserWorkerSession, DispatchReceipt, DispatchStatus,
    WorkerFinishReason, WorkerOutput, WorkerTask,
};

pub use deepseek_web::DeepSeekWebAdapter;

pub trait BrowserWorkerAdapter {
    fn session(&self) -> &BrowserWorkerSession;
    fn ensure_expert_mode(&mut self);
    fn mark_ready(&mut self);
    fn submit_task(&mut self, task: &WorkerTask) -> Result<DispatchReceipt, BrowserWorkerError>;
    fn read_output(
        &mut self,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError>;
}

pub trait BrowserProviderDriver {
    fn submit_task(
        &mut self,
        session: &BrowserWorkerSession,
        task: &WorkerTask,
    ) -> Result<DispatchReceipt, BrowserWorkerError>;

    fn read_output(
        &mut self,
        session: &BrowserWorkerSession,
        receipt: &DispatchReceipt,
    ) -> Result<WorkerOutput, BrowserWorkerError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealBrowserCommand {
    EnsureMode { mode: BrowserMode },
    OpenPage { url: String },
    FocusComposer,
    TypePrompt { text: String },
    SubmitPrompt,
    WaitForAssistantTurn,
    CaptureOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealBrowserObservation {
    ModeEnsured {
        mode: BrowserMode,
    },
    PageOpened {
        url: String,
    },
    ComposerFocused,
    PromptTyped {
        chars: usize,
    },
    PromptSubmitted {
        task_id: String,
    },
    AssistantTurnReady,
    OutputCaptured {
        content: String,
        snapshot_ref: Option<String>,
    },
}

pub trait RealBrowserDriver {
    fn execute(
        &mut self,
        session: &BrowserWorkerSession,
        command: &RealBrowserCommand,
    ) -> Result<RealBrowserObservation, BrowserWorkerError>;
}

impl<D: RealBrowserDriver> BrowserProviderDriver for ProviderBackedRealBrowserDriver<D> {
    fn submit_task(
        &mut self,
        session: &BrowserWorkerSession,
        task: &WorkerTask,
    ) -> Result<DispatchReceipt, BrowserWorkerError> {
        match self.provider.execute(
            session,
            &RealBrowserCommand::EnsureMode {
                mode: session.mode.clone(),
            },
        )? {
            RealBrowserObservation::ModeEnsured { .. } => {}
            _ => {
                return Err(BrowserWorkerError::UnexpectedBrowserObservation {
                    command: "EnsureMode",
                    observation: "non-mode-ensured",
                })
            }
        }

        match self.provider.execute(
            session,
            &RealBrowserCommand::OpenPage {
                url: session.page_url.clone(),
            },
        )? {
            RealBrowserObservation::PageOpened { .. } => {}
            _ => {
                return Err(BrowserWorkerError::UnexpectedBrowserObservation {
                    command: "OpenPage",
                    observation: "non-page-opened",
                })
            }
        }

        match self
            .provider
            .execute(session, &RealBrowserCommand::FocusComposer)?
        {
            RealBrowserObservation::ComposerFocused => {}
            _ => {
                return Err(BrowserWorkerError::UnexpectedBrowserObservation {
                    command: "FocusComposer",
                    observation: "non-composer-focused",
                })
            }
        }

        match self.provider.execute(
            session,
            &RealBrowserCommand::TypePrompt {
                text: task.prompt.clone(),
            },
        )? {
            RealBrowserObservation::PromptTyped { .. } => {}
            _ => {
                return Err(BrowserWorkerError::UnexpectedBrowserObservation {
                    command: "TypePrompt",
                    observation: "non-prompt-typed",
                })
            }
        }

        let prompt_submission = self
            .provider
            .execute(session, &RealBrowserCommand::SubmitPrompt)?;

        let submitted_task_id = match prompt_submission {
            RealBrowserObservation::PromptSubmitted { task_id } => task_id,
            _ => {
                return Err(BrowserWorkerError::UnexpectedBrowserObservation {
                    command: "SubmitPrompt",
                    observation: "non-prompt-submitted",
                })
            }
        };

        Ok(DispatchReceipt {
            task_id: submitted_task_id,
            worker_id: session.worker_id.clone(),
            provider: session.provider.clone(),
            submitted_at: "real-browser-submitted-at".to_string(),
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
        match self
            .provider
            .execute(session, &RealBrowserCommand::WaitForAssistantTurn)?
        {
            RealBrowserObservation::AssistantTurnReady => {}
            _ => {
                return Err(BrowserWorkerError::UnexpectedBrowserObservation {
                    command: "WaitForAssistantTurn",
                    observation: "non-assistant-turn-ready",
                })
            }
        }

        let captured = self
            .provider
            .execute(session, &RealBrowserCommand::CaptureOutput)?;

        match captured {
            RealBrowserObservation::OutputCaptured {
                content,
                snapshot_ref,
            } => Ok(WorkerOutput {
                worker_id: session.worker_id.clone(),
                provider: session.provider.clone(),
                task_id: receipt.task_id.clone(),
                content,
                raw_snapshot_ref: snapshot_ref,
                completed_at: "browser-driver-output-captured-at".to_string(),
                finish_reason: WorkerFinishReason::Completed,
            }),
            _ => Err(BrowserWorkerError::UnexpectedBrowserObservation {
                command: "CaptureOutput",
                observation: "non-output-captured",
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderBackedRealBrowserDriver<D> {
    provider: D,
}

impl<D> ProviderBackedRealBrowserDriver<D> {
    pub fn new(provider: D) -> Self {
        Self { provider }
    }

    pub fn into_inner(self) -> D {
        self.provider
    }
}

impl<D: BrowserProviderDriver> RealBrowserDriver for ProviderBackedRealBrowserDriver<D> {
    fn execute(
        &mut self,
        session: &BrowserWorkerSession,
        command: &RealBrowserCommand,
    ) -> Result<RealBrowserObservation, BrowserWorkerError> {
        match command {
            RealBrowserCommand::EnsureMode { mode } => {
                Ok(RealBrowserObservation::ModeEnsured { mode: mode.clone() })
            }
            RealBrowserCommand::OpenPage { url } => {
                Ok(RealBrowserObservation::PageOpened { url: url.clone() })
            }
            RealBrowserCommand::FocusComposer => Ok(RealBrowserObservation::ComposerFocused),
            RealBrowserCommand::TypePrompt { text } => Ok(RealBrowserObservation::PromptTyped {
                chars: text.chars().count(),
            }),
            RealBrowserCommand::SubmitPrompt => Ok(RealBrowserObservation::PromptSubmitted {
                task_id: session
                    .last_prompt_hash
                    .clone()
                    .unwrap_or_else(|| "pending-task".to_string()),
            }),
            RealBrowserCommand::WaitForAssistantTurn => {
                Ok(RealBrowserObservation::AssistantTurnReady)
            }
            RealBrowserCommand::CaptureOutput => Ok(RealBrowserObservation::OutputCaptured {
                content: format!("fake provider output for worker {}", session.worker_id),
                snapshot_ref: Some("fake-provider-snapshot".to_string()),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FakeBrowserProviderDriver;

impl BrowserProviderDriver for FakeBrowserProviderDriver {
    fn submit_task(
        &mut self,
        session: &BrowserWorkerSession,
        task: &WorkerTask,
    ) -> Result<DispatchReceipt, BrowserWorkerError> {
        Ok(DispatchReceipt {
            task_id: task.task_id.clone(),
            worker_id: session.worker_id.clone(),
            provider: session.provider.clone(),
            submitted_at: "fake-submitted-at".to_string(),
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
                "fake provider output for worker {} task {}",
                session.worker_id, receipt.task_id
            ),
            raw_snapshot_ref: Some("fake-provider-snapshot".to_string()),
            completed_at: "fake-completed-at".to_string(),
            finish_reason: WorkerFinishReason::Completed,
        })
    }
}

pub fn adapter_session(adapter: &impl BrowserWorkerAdapter) -> &BrowserWorkerSession {
    adapter.session()
}

pub fn adapter_ensure_expert_mode(adapter: &mut impl BrowserWorkerAdapter) {
    adapter.ensure_expert_mode();
}

pub fn adapter_mark_ready(adapter: &mut impl BrowserWorkerAdapter) {
    adapter.mark_ready();
}

pub fn adapter_submit_task(
    adapter: &mut impl BrowserWorkerAdapter,
    task: &WorkerTask,
) -> Result<DispatchReceipt, BrowserWorkerError> {
    adapter.submit_task(task)
}

pub fn adapter_read_output(
    adapter: &mut impl BrowserWorkerAdapter,
    receipt: &DispatchReceipt,
) -> Result<WorkerOutput, BrowserWorkerError> {
    adapter.read_output(receipt)
}
