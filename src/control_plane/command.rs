use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::control_plane::{
    validate_request, validate_unit, ControlAction, ControlError, ControlPlane, ControlReceipt,
    ControlRequest, ManagedUnit, ManagedUnitKind, ManagedUnitStatus,
};
use crate::runtime_config::ControlPlaneCommandConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPlaneCommandResult {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlPlaneUnitRecord {
    unit_id: String,
    display_name: String,
    kind: String,
    status: String,
    model_name: Option<String>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlPlaneReceiptRecord {
    unit_id: String,
    action: String,
    previous_status: String,
    next_status: String,
    model_name: Option<String>,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ControlRequestRecord {
    unit_id: String,
    action: String,
    reason: String,
    model_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandControlPlane {
    config: ControlPlaneCommandConfig,
}

impl CommandControlPlane {
    pub fn new(config: ControlPlaneCommandConfig) -> Self {
        Self { config }
    }

    fn list_args(&self) -> Result<Vec<String>, ControlError> {
        split_args(&self.config.list_args).map_err(|error| {
            ControlError::InvalidRequest(format!("control list args parse failed: {error}"))
        })
    }

    fn apply_args(&self) -> Result<Vec<String>, ControlError> {
        split_args(&self.config.apply_args).map_err(|error| {
            ControlError::InvalidRequest(format!("control apply args parse failed: {error}"))
        })
    }

    fn run_command(
        &self,
        args: &[String],
        stdin_json: Option<&str>,
    ) -> Result<ControlPlaneCommandResult, ControlError> {
        let mut command = Command::new(&self.config.program);
        command.args(args);
        if stdin_json.is_some() {
            command.stdin(Stdio::piped());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| {
            ControlError::InvalidRequest(format!(
                "control command spawn failed: program={} error={error}",
                self.config.program
            ))
        })?;

        if let Some(stdin_json) = stdin_json {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                ControlError::InvalidRequest("control command stdin unavailable".to_string())
            })?;
            stdin.write_all(stdin_json.as_bytes()).map_err(|error| {
                ControlError::InvalidRequest(format!("control command stdin write failed: {error}"))
            })?;
            stdin.flush().map_err(|error| {
                ControlError::InvalidRequest(format!("control command stdin flush failed: {error}"))
            })?;
        }

        let output = wait_with_timeout(child, self.config.timeout_ms).map_err(|error| {
            ControlError::InvalidRequest(format!("control command wait failed: {error}"))
        })?;

        Ok(ControlPlaneCommandResult {
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    pub fn try_list_units(&self) -> Result<Vec<ManagedUnit>, ControlError> {
        let result = self.run_command(&self.list_args()?, None)?;
        if result.status_code != Some(0) {
            return Err(ControlError::InvalidRequest(format!(
                "control command failed: status={:?} stderr={}",
                result.status_code,
                result.stderr.trim()
            )));
        }
        Self::parse_units(&result.stdout)
    }

    fn parse_units(stdout: &str) -> Result<Vec<ManagedUnit>, ControlError> {
        let records: Vec<ControlPlaneUnitRecord> =
            serde_json::from_str(stdout).map_err(|error| {
                ControlError::InvalidRequest(format!("control list output parse failed: {error}"))
            })?;
        records
            .into_iter()
            .map(Self::record_to_unit)
            .collect::<Result<Vec<_>, _>>()
    }

    fn parse_receipt(stdout: &str) -> Result<ControlReceipt, ControlError> {
        let record: ControlPlaneReceiptRecord = serde_json::from_str(stdout).map_err(|error| {
            ControlError::InvalidRequest(format!("control apply output parse failed: {error}"))
        })?;
        let action = parse_action(&record.action, record.model_name.clone())?;
        Ok(ControlReceipt {
            unit_id: record.unit_id,
            action,
            previous_status: parse_status(&record.previous_status)?,
            next_status: parse_status(&record.next_status)?,
            model_name: record.model_name,
            message: record.message,
        })
    }

    fn record_to_unit(record: ControlPlaneUnitRecord) -> Result<ManagedUnit, ControlError> {
        let unit = ManagedUnit {
            unit_id: record.unit_id,
            display_name: record.display_name,
            kind: parse_kind(&record.kind)?,
            status: parse_status(&record.status)?,
            model_name: record.model_name,
            metadata: record.metadata,
        };
        validate_unit(&unit)?;
        Ok(unit)
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
                    "control command timed out after {timeout_ms}ms status={:?}",
                    output.status.code()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl ControlPlane for CommandControlPlane {
    fn list_units(&self) -> Vec<ManagedUnit> {
        self.try_list_units().unwrap_or_default()
    }

    fn apply(&mut self, request: ControlRequest) -> Result<ControlReceipt, ControlError> {
        validate_request(&request)?;
        let stdin_json = serde_json::to_string(&request_record(&request)).map_err(|error| {
            ControlError::InvalidRequest(format!("control request serialization failed: {error}"))
        })?;
        let result = self.run_command(&self.apply_args()?, Some(&stdin_json))?;
        if result.status_code != Some(0) {
            return Err(ControlError::InvalidRequest(format!(
                "control command failed: status={:?} stderr={}",
                result.status_code,
                result.stderr.trim()
            )));
        }
        let receipt = Self::parse_receipt(&result.stdout)?;
        validate_receipt_matches_request(&request, &receipt)?;
        Ok(receipt)
    }
}

fn request_record(request: &ControlRequest) -> ControlRequestRecord {
    ControlRequestRecord {
        unit_id: request.unit_id.clone(),
        action: request.action.as_str().to_string(),
        reason: request.reason.clone(),
        model_name: match &request.action {
            ControlAction::ChangeModel { model_name } => Some(model_name.clone()),
            _ => None,
        },
    }
}

fn split_args(raw: &str) -> Result<Vec<String>, String> {
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
        return Err("trailing escape in command args".to_string());
    }
    if let Some(active) = quote {
        return Err(format!("unterminated {active} quote in command args"));
    }
    if !current.is_empty() {
        args.push(current);
    }

    Ok(args)
}

fn parse_kind(raw: &str) -> Result<ManagedUnitKind, ControlError> {
    match raw {
        "service" => Ok(ManagedUnitKind::Service),
        "agent" => Ok(ManagedUnitKind::Agent),
        other => Err(ControlError::InvalidRequest(format!(
            "invalid control unit kind: {other}"
        ))),
    }
}

fn parse_status(raw: &str) -> Result<ManagedUnitStatus, ControlError> {
    match raw {
        "Running" => Ok(ManagedUnitStatus::Running),
        "Stopped" => Ok(ManagedUnitStatus::Stopped),
        "Failed" => Ok(ManagedUnitStatus::Failed),
        "Unknown" => Ok(ManagedUnitStatus::Unknown),
        other => Err(ControlError::InvalidRequest(format!(
            "invalid control unit status: {other}"
        ))),
    }
}

fn parse_action(raw: &str, model_name: Option<String>) -> Result<ControlAction, ControlError> {
    match raw {
        "start" => Ok(ControlAction::Start),
        "stop" => Ok(ControlAction::Stop),
        "restart" => Ok(ControlAction::Restart),
        "change_model" => {
            let model_name = model_name.ok_or_else(|| {
                ControlError::InvalidRequest(
                    "control apply output missing model_name for change_model".to_string(),
                )
            })?;
            Ok(ControlAction::ChangeModel { model_name })
        }
        other => Err(ControlError::InvalidRequest(format!(
            "invalid control action: {other}"
        ))),
    }
}

fn validate_receipt_matches_request(
    request: &ControlRequest,
    receipt: &ControlReceipt,
) -> Result<(), ControlError> {
    if receipt.unit_id != request.unit_id {
        return Err(ControlError::InvalidRequest(format!(
            "control apply receipt unit_id mismatch: expected={} actual={}",
            request.unit_id, receipt.unit_id
        )));
    }
    match (&request.action, &receipt.action) {
        (
            ControlAction::ChangeModel {
                model_name: expected,
            },
            ControlAction::ChangeModel { model_name: actual },
        ) if expected == actual => {}
        (
            ControlAction::ChangeModel {
                model_name: expected,
            },
            ControlAction::ChangeModel { model_name: actual },
        ) if expected != actual => {
            return Err(ControlError::InvalidRequest(format!(
                "control apply receipt model_name mismatch: expected={expected} actual={actual}"
            )));
        }
        (ControlAction::ChangeModel { .. }, _) | (_, ControlAction::ChangeModel { .. }) => {
            return Err(action_mismatch_error(&request.action, &receipt.action));
        }
        _ if receipt.action != request.action => {
            return Err(action_mismatch_error(&request.action, &receipt.action));
        }
        _ => {}
    }

    if !matches!(request.action, ControlAction::ChangeModel { .. }) && receipt.model_name.is_some()
    {
        return Err(ControlError::InvalidRequest(
            "control apply receipt model_name must be null unless action is change_model"
                .to_string(),
        ));
    }
    Ok(())
}

fn action_mismatch_error(expected: &ControlAction, actual: &ControlAction) -> ControlError {
    ControlError::InvalidRequest(format!(
        "control apply receipt action mismatch: expected={} actual={}",
        expected.as_str(),
        actual.as_str()
    ))
}
