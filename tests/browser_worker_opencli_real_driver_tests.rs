use chuang_agent::browser_worker::{
    BrowserMode, BrowserWorkerDemoService, BrowserWorkerError, BrowserWorkerSession,
    DispatchStatus, ProviderBackedRealBrowserDriver, ProviderKind, RealBrowserCommand,
    RealBrowserDriver, RealBrowserObservation, WorkerFinishReason, WorkerTask,
};

fn passthrough_responder(_task: &WorkerTask) -> String {
    "responder should not overwrite driver output".to_string()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RecordingRealDriver {
    commands: Vec<RealBrowserCommand>,
}

impl RealBrowserDriver for RecordingRealDriver {
    fn execute(
        &mut self,
        _session: &BrowserWorkerSession,
        command: &RealBrowserCommand,
    ) -> Result<RealBrowserObservation, BrowserWorkerError> {
        self.commands.push(command.clone());

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
                task_id: "task-opencli-1".to_string(),
            }),
            RealBrowserCommand::WaitForAssistantTurn => {
                Ok(RealBrowserObservation::AssistantTurnReady)
            }
            RealBrowserCommand::CaptureOutput => Ok(RealBrowserObservation::OutputCaptured {
                content: "driver captured opencli deepseek output".to_string(),
                snapshot_ref: Some("opencli://deepseek/task-opencli-1".to_string()),
            }),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct WrongModeDriver;

impl RealBrowserDriver for WrongModeDriver {
    fn execute(
        &mut self,
        _session: &BrowserWorkerSession,
        command: &RealBrowserCommand,
    ) -> Result<RealBrowserObservation, BrowserWorkerError> {
        match command {
            RealBrowserCommand::EnsureMode { .. } => Ok(RealBrowserObservation::ComposerFocused),
            RealBrowserCommand::OpenPage { url } => {
                Ok(RealBrowserObservation::PageOpened { url: url.clone() })
            }
            RealBrowserCommand::FocusComposer => Ok(RealBrowserObservation::ComposerFocused),
            RealBrowserCommand::TypePrompt { text } => Ok(RealBrowserObservation::PromptTyped {
                chars: text.chars().count(),
            }),
            RealBrowserCommand::SubmitPrompt => Ok(RealBrowserObservation::PromptSubmitted {
                task_id: "task-opencli-1".to_string(),
            }),
            RealBrowserCommand::WaitForAssistantTurn => {
                Ok(RealBrowserObservation::AssistantTurnReady)
            }
            RealBrowserCommand::CaptureOutput => Ok(RealBrowserObservation::OutputCaptured {
                content: "driver captured opencli deepseek output".to_string(),
                snapshot_ref: Some("opencli://deepseek/task-opencli-1".to_string()),
            }),
        }
    }
}

#[test]
fn provider_backed_real_driver_red_opencli_workflow_shape() {
    let real = RecordingRealDriver::default();
    let service = BrowserWorkerDemoService::with_driver(
        "worker-opencli",
        "https://chat.deepseek.com/",
        passthrough_responder,
        ProviderBackedRealBrowserDriver::new(real),
    );

    let task = WorkerTask {
        task_id: "task-opencli-1".to_string(),
        title: "OpenCLI real browser run".to_string(),
        prompt:
            "请你推荐 chuang 项目下一步怎么做，重点考虑 BrowserWorker 通过 opencli 打开 Chrome。"
                .to_string(),
    };

    let run = service
        .run(task.clone())
        .expect("provider-backed real browser workflow should succeed");

    assert_eq!(run.session.provider, ProviderKind::DeepSeekWeb);
    assert_eq!(run.session.mode, BrowserMode::Expert);
    assert!(run.session.logged_in);
    assert_eq!(run.receipt.status, DispatchStatus::Submitted);
    assert_eq!(run.output.finish_reason, WorkerFinishReason::Completed);
    assert_eq!(
        run.output.content,
        "driver captured opencli deepseek output"
    );
    assert_eq!(
        run.output.raw_snapshot_ref.as_deref(),
        Some("opencli://deepseek/task-opencli-1")
    );
    assert_eq!(
        run.record.raw_snapshot_ref.as_deref(),
        Some("opencli://deepseek/task-opencli-1")
    );
}

#[test]
fn provider_backed_real_driver_rejects_unexpected_browser_observation() {
    let service = BrowserWorkerDemoService::with_driver(
        "worker-opencli",
        "https://chat.deepseek.com/",
        passthrough_responder,
        ProviderBackedRealBrowserDriver::new(WrongModeDriver),
    );

    let task = WorkerTask {
        task_id: "task-opencli-bad-mode".to_string(),
        title: "OpenCLI bad mode run".to_string(),
        prompt: "测试错误 observation 时要立刻失败。".to_string(),
    };

    let err = service
        .run(task)
        .expect_err("unexpected observation should fail fast");
    assert_eq!(
        err,
        BrowserWorkerError::UnexpectedBrowserObservation {
            command: "EnsureMode",
            observation: "non-mode-ensured",
        }
    );
}
