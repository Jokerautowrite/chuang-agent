use std::process::Command;

use crate::browser_worker::{
    BrowserWorkerError, BrowserWorkerSession, RealBrowserCommand, RealBrowserDriver,
    RealBrowserObservation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCliCommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCliCommandResult {
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait OpenCliRunner {
    fn run(&mut self, spec: OpenCliCommandSpec)
        -> Result<OpenCliCommandResult, BrowserWorkerError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemOpenCliRunner;

impl OpenCliRunner for SystemOpenCliRunner {
    fn run(
        &mut self,
        spec: OpenCliCommandSpec,
    ) -> Result<OpenCliCommandResult, BrowserWorkerError> {
        let output = Command::new(&spec.program)
            .args(&spec.args)
            .output()
            .map_err(|_| BrowserWorkerError::OpenCliCommandFailed {
                command: spec.program.clone(),
                detail: "spawn-failed".to_string(),
            })?;

        Ok(OpenCliCommandResult {
            status_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCliRealBrowserDriver<R = SystemOpenCliRunner> {
    runner: R,
}

impl OpenCliRealBrowserDriver<SystemOpenCliRunner> {
    pub fn new() -> Self {
        Self::with_runner(SystemOpenCliRunner)
    }
}

impl<R> OpenCliRealBrowserDriver<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }

    fn spec_for(
        command: &RealBrowserCommand,
        _session: &BrowserWorkerSession,
    ) -> OpenCliCommandSpec {
        match command {
            RealBrowserCommand::EnsureMode { .. } => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec!["browser".to_string(), "state".to_string()],
            },
            RealBrowserCommand::OpenPage { url } => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec!["browser".to_string(), "open".to_string(), url.clone()],
            },
            RealBrowserCommand::FocusComposer => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec!["browser".to_string(), "state".to_string()],
            },
            RealBrowserCommand::TypePrompt { text } => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec![
                    "browser".to_string(),
                    "type".to_string(),
                    "0".to_string(),
                    text.clone(),
                ],
            },
            RealBrowserCommand::SubmitPrompt => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec!["browser".to_string(), "state".to_string()],
            },
            RealBrowserCommand::WaitForAssistantTurn => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec!["browser".to_string(), "state".to_string()],
            },
            RealBrowserCommand::CaptureOutput => OpenCliCommandSpec {
                program: "opencli".to_string(),
                args: vec!["browser".to_string(), "state".to_string()],
            },
        }
    }
}

impl<R: OpenCliRunner> RealBrowserDriver for OpenCliRealBrowserDriver<R> {
    fn execute(
        &mut self,
        session: &BrowserWorkerSession,
        command: &RealBrowserCommand,
    ) -> Result<RealBrowserObservation, BrowserWorkerError> {
        let spec = Self::spec_for(command, session);
        let result = self.runner.run(spec.clone())?;

        if result.status_code != 0 {
            return Err(BrowserWorkerError::OpenCliCommandFailed {
                command: format!("{} {}", spec.program, spec.args.join(" ")),
                detail: result.stderr,
            });
        }

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
                    .unwrap_or_else(|| "opencli-pending-task".to_string()),
            }),
            RealBrowserCommand::WaitForAssistantTurn => {
                Ok(RealBrowserObservation::AssistantTurnReady)
            }
            RealBrowserCommand::CaptureOutput => Ok(RealBrowserObservation::OutputCaptured {
                content: result.stdout.clone(),
                snapshot_ref: Some(format!(
                    "opencli://state/{}",
                    session
                        .last_prompt_hash
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string())
                )),
            }),
        }
    }
}
