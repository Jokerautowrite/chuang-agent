use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::actuator::{
    Actuator, ActuatorError, AppHandle, ClickTarget, EvidenceRef, FocusTarget, InputTarget,
    Observation, ObserveTarget, OpenAppRequest, ScreenshotTarget, SecretOrPlainText,
};
use crate::runtime_config::ActuatorCommandConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandActuator {
    config: ActuatorCommandConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ActuatorCommandRequest {
    action: String,
    observe_target: Option<ObserveTarget>,
    open_app: Option<OpenAppRequest>,
    focus_target: Option<FocusTarget>,
    click_target: Option<ClickTarget>,
    input_target: Option<InputTarget>,
    text: Option<SecretOrPlainText>,
    screenshot_target: Option<ScreenshotTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActuatorCommandResponse {
    observation: Option<Observation>,
    app_handle: Option<AppHandle>,
    evidence_ref: Option<EvidenceRef>,
    message: Option<String>,
}

impl CommandActuator {
    pub fn new(config: ActuatorCommandConfig) -> Self {
        Self { config }
    }

    fn args(&self) -> Vec<String> {
        split_args(&self.config.args)
    }

    fn run(
        &self,
        request: ActuatorCommandRequest,
    ) -> Result<ActuatorCommandResponse, ActuatorError> {
        let stdin_json = serde_json::to_string(&request).map_err(|error| {
            actuator_error(format!("actuator request serialization failed: {error}"))
        })?;
        let mut command = Command::new(&self.config.program);
        command
            .args(self.args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| {
            actuator_error(format!(
                "actuator command spawn failed: program={} error={error}",
                self.config.program
            ))
        })?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| actuator_error("actuator command stdin unavailable"))?;
        stdin.write_all(stdin_json.as_bytes()).map_err(|error| {
            actuator_error(format!("actuator command stdin write failed: {error}"))
        })?;
        stdin.flush().map_err(|error| {
            actuator_error(format!("actuator command stdin flush failed: {error}"))
        })?;
        drop(stdin);

        let output = wait_with_timeout(child, self.config.timeout_ms)
            .map_err(|error| actuator_error(format!("actuator command wait failed: {error}")))?;
        if output.status.code() != Some(0) {
            return Err(actuator_error(format!(
                "actuator command failed: status={:?} stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| {
            actuator_error(format!("actuator command output parse failed: {error}"))
        })
    }
}

impl Actuator for CommandActuator {
    fn observe(&mut self, target: ObserveTarget) -> Result<Observation, ActuatorError> {
        self.run(ActuatorCommandRequest {
            action: "observe".to_string(),
            observe_target: Some(target),
            open_app: None,
            focus_target: None,
            click_target: None,
            input_target: None,
            text: None,
            screenshot_target: None,
        })?
        .observation
        .ok_or_else(|| actuator_error("actuator observe response missing observation"))
    }

    fn open_app(&mut self, request: OpenAppRequest) -> Result<AppHandle, ActuatorError> {
        self.run(ActuatorCommandRequest {
            action: "open_app".to_string(),
            observe_target: None,
            open_app: Some(request),
            focus_target: None,
            click_target: None,
            input_target: None,
            text: None,
            screenshot_target: None,
        })?
        .app_handle
        .ok_or_else(|| actuator_error("actuator open_app response missing app_handle"))
    }

    fn focus(&mut self, target: FocusTarget) -> Result<(), ActuatorError> {
        self.run(ActuatorCommandRequest {
            action: "focus".to_string(),
            observe_target: None,
            open_app: None,
            focus_target: Some(target),
            click_target: None,
            input_target: None,
            text: None,
            screenshot_target: None,
        })?;
        Ok(())
    }

    fn click(&mut self, target: ClickTarget) -> Result<(), ActuatorError> {
        self.run(ActuatorCommandRequest {
            action: "click".to_string(),
            observe_target: None,
            open_app: None,
            focus_target: None,
            click_target: Some(target),
            input_target: None,
            text: None,
            screenshot_target: None,
        })?;
        Ok(())
    }

    fn input_text(
        &mut self,
        target: InputTarget,
        text: SecretOrPlainText,
    ) -> Result<(), ActuatorError> {
        self.run(ActuatorCommandRequest {
            action: "input_text".to_string(),
            observe_target: None,
            open_app: None,
            focus_target: None,
            click_target: None,
            input_target: Some(target),
            text: Some(text),
            screenshot_target: None,
        })?;
        Ok(())
    }

    fn screenshot(&mut self, target: ScreenshotTarget) -> Result<EvidenceRef, ActuatorError> {
        self.run(ActuatorCommandRequest {
            action: "screenshot".to_string(),
            observe_target: None,
            open_app: None,
            focus_target: None,
            click_target: None,
            input_target: None,
            text: None,
            screenshot_target: Some(target),
        })?
        .evidence_ref
        .ok_or_else(|| actuator_error("actuator screenshot response missing evidence_ref"))
    }
}

fn actuator_error(message: impl Into<String>) -> ActuatorError {
    ActuatorError {
        message: message.into(),
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout_ms: u64,
) -> std::io::Result<std::process::Output> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "actuator command timed out after {timeout_ms}ms status={:?}",
                    output.status.code()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn split_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if (ch == '"' || ch == '\'') && current.is_empty() => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        args.push(current);
    }

    args
}
