use std::process::Command;

use crate::genesis_actuator::{
    session_expired_marker, GenesisActuator, GenesisAskRequest, GenesisAskResponse, GenesisChannel,
    GenesisConfig, GenesisError, GenesisRepairPlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisCommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub channel: GenesisChannel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisCommandOutput {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait GenesisCommandRunner {
    fn run(&mut self, spec: &GenesisCommandSpec) -> Result<GenesisCommandOutput, GenesisError>;
}

#[derive(Debug, Default, Clone)]
pub struct SystemGenesisCommandRunner;

impl GenesisCommandRunner for SystemGenesisCommandRunner {
    fn run(&mut self, spec: &GenesisCommandSpec) -> Result<GenesisCommandOutput, GenesisError> {
        let output = Command::new(&spec.program)
            .args(&spec.args)
            .output()
            .map_err(|error| GenesisError::CommandNotFound(error.to_string()))?;

        Ok(GenesisCommandOutput {
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AutoCliGenesisActuator<R> {
    config: GenesisConfig,
    runner: R,
}

impl AutoCliGenesisActuator<SystemGenesisCommandRunner> {
    pub fn new(config: GenesisConfig) -> Self {
        Self::with_runner(config, SystemGenesisCommandRunner)
    }
}

impl<R> AutoCliGenesisActuator<R> {
    pub fn with_runner(config: GenesisConfig, runner: R) -> Self {
        Self { config, runner }
    }

    pub fn primary_spec(&self, prompt: &str) -> GenesisCommandSpec {
        GenesisCommandSpec {
            program: self.config.program.clone(),
            args: vec![
                "deepseek".to_string(),
                "chat".to_string(),
                prompt.to_string(),
                "--headless".to_string(),
                "--user-data-dir".to_string(),
                self.config.profile_dir.display().to_string(),
                "--timeout".to_string(),
                self.config.timeout_ms.to_string(),
            ],
            channel: GenesisChannel::UserDataDir,
        }
    }

    pub fn fallback_spec(&self, prompt: &str) -> GenesisCommandSpec {
        GenesisCommandSpec {
            program: self.config.program.clone(),
            args: vec![
                "deepseek".to_string(),
                "chat".to_string(),
                prompt.to_string(),
                "--cdp-port".to_string(),
                self.config.cdp_port.to_string(),
                "--timeout".to_string(),
                self.config.timeout_ms.to_string(),
            ],
            channel: GenesisChannel::Cdp,
        }
    }
}

impl<R: GenesisCommandRunner> AutoCliGenesisActuator<R> {
    fn ask_channel(
        &mut self,
        spec: GenesisCommandSpec,
    ) -> Result<GenesisAskResponse, GenesisError> {
        let output = self.runner.run(&spec)?;
        if output.status_code != Some(0) {
            return Err(GenesisError::CommandFailed {
                channel: spec.channel,
                status_code: output.status_code,
                stderr_preview: preview(&output.stderr),
            });
        }

        if let Some(marker) = session_expired_marker(&output.stdout) {
            return Err(GenesisError::SessionExpired {
                channel: spec.channel,
                marker: marker.to_string(),
            });
        }

        Ok(GenesisAskResponse {
            answer: output.stdout,
            channel: spec.channel,
            primary_repair: None,
        })
    }
}

impl<R: GenesisCommandRunner> GenesisActuator for AutoCliGenesisActuator<R> {
    fn ask(&mut self, request: GenesisAskRequest) -> Result<GenesisAskResponse, GenesisError> {
        if request.prompt.trim().is_empty() {
            return Err(GenesisError::EmptyPrompt);
        }

        let primary_error = match self.ask_channel(self.primary_spec(&request.prompt)) {
            Ok(response) => return Ok(response),
            Err(error) => error,
        };

        match self.ask_channel(self.fallback_spec(&request.prompt)) {
            Ok(mut response) => {
                response.primary_repair = Some(GenesisRepairPlan {
                    reason: format!("primary channel failed: {primary_error:?}"),
                    recommended_action:
                        "inspect or refresh the userDataDir login state; do not delete profile automatically"
                            .to_string(),
                    requires_approval: true,
                });
                Ok(response)
            }
            Err(fallback_error) => Err(GenesisError::AllChannelsDown {
                primary: Box::new(primary_error),
                fallback: Box::new(fallback_error),
            }),
        }
    }
}

fn preview(value: &str) -> String {
    value.trim().chars().take(600).collect()
}
