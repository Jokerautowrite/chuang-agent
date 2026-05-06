use std::collections::BTreeMap;

use crate::common::{AgentId, AuditRecord, TaskId, Timestamp};
use crate::governance::{ActionKind, ProposedAction};

mod command;
mod fake;

pub use command::CommandControlPlane;
pub use fake::FakeControlPlane;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedUnitKind {
    Service,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedUnitStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedUnit {
    pub unit_id: String,
    pub display_name: String,
    pub kind: ManagedUnitKind,
    pub status: ManagedUnitStatus,
    pub model_name: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlAction {
    Start,
    Stop,
    Restart,
    ChangeModel { model_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlRequest {
    pub unit_id: String,
    pub action: ControlAction,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlCommandContract {
    pub unit_id: String,
    pub allowed_actions: Vec<ControlActionKind>,
    pub audit_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlReceipt {
    pub unit_id: String,
    pub action: ControlAction,
    pub previous_status: ManagedUnitStatus,
    pub next_status: ManagedUnitStatus,
    pub model_name: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    InvalidRequest(String),
    UnknownUnit(String),
    UnsupportedAction(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlActionKind {
    Start,
    Stop,
    Restart,
    ChangeModel,
}

pub trait ControlPlane {
    fn list_units(&self) -> Vec<ManagedUnit>;
    fn apply(&mut self, request: ControlRequest) -> Result<ControlReceipt, ControlError>;
}

pub fn proposed_action_for_control(
    unit: &ManagedUnit,
    request: &ControlRequest,
) -> Result<ProposedAction, ControlError> {
    validate_request(request)?;
    if unit.unit_id != request.unit_id {
        return Err(ControlError::InvalidRequest(
            "control request unit_id must match unit".to_string(),
        ));
    }

    Ok(ProposedAction {
        action_id: format!("control:{}", request.unit_id),
        kind: ActionKind::ServiceChange,
        target: format!("{}:{}", unit.kind.as_str(), unit.unit_id),
        summary: format!(
            "{} {} because {}",
            request.action.as_str(),
            unit.display_name,
            request.reason
        ),
    })
}

pub fn audit_record_for_control(
    unit: &ManagedUnit,
    request: &ControlRequest,
    approved: bool,
) -> Result<AuditRecord, ControlError> {
    validate_request(request)?;
    if unit.unit_id != request.unit_id {
        return Err(ControlError::InvalidRequest(
            "control request unit_id must match unit".to_string(),
        ));
    }

    Ok(AuditRecord {
        operation: format!("control.{}", request.action.as_str()),
        agent_id: AgentId("control-plane".to_string()),
        task_id: TaskId(format!("control:{}", unit.unit_id)),
        delta_bytes: 0,
        reason: format!(
            "{}; approved={}; target={}:{}",
            request.reason,
            approved,
            unit.kind.as_str(),
            unit.unit_id
        ),
        timestamp: Timestamp("2026-05-01T00:00:00Z".to_string()),
    })
}

pub fn contract_for_control_unit(
    unit: &ManagedUnit,
) -> Result<ControlCommandContract, ControlError> {
    validate_unit(unit)?;
    let allowlist = unit
        .metadata
        .get("allowed_actions")
        .or_else(|| unit.metadata.get("allow_actions"))
        .map(|raw| parse_allowed_actions(raw.as_str()))
        .transpose()?
        .unwrap_or_default();

    Ok(ControlCommandContract {
        unit_id: unit.unit_id.clone(),
        allowed_actions: allowlist,
        audit_label: format!("{}:{}", unit.kind.as_str(), unit.unit_id),
    })
}

pub fn validate_control_contract(
    unit: &ManagedUnit,
    request: &ControlRequest,
) -> Result<ControlCommandContract, ControlError> {
    validate_request(request)?;
    if unit.unit_id != request.unit_id {
        return Err(ControlError::InvalidRequest(
            "control request unit_id must match unit".to_string(),
        ));
    }

    let contract = contract_for_control_unit(unit)?;
    if contract.allowed_actions.is_empty() {
        return Err(ControlError::UnsupportedAction(format!(
            "control unit {} has no allowlisted actions",
            unit.unit_id
        )));
    }

    let requested = request.action.kind();
    if !contract.allowed_actions.contains(&requested) {
        return Err(ControlError::UnsupportedAction(format!(
            "control action {} is not allowlisted for {}",
            request.action.as_str(),
            unit.unit_id
        )));
    }

    Ok(contract)
}

pub fn validate_control_receipt(
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
            return Err(control_action_mismatch_error(
                &request.action,
                &receipt.action,
            ));
        }
        _ if receipt.action != request.action => {
            return Err(control_action_mismatch_error(
                &request.action,
                &receipt.action,
            ));
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

impl ManagedUnitKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Agent => "agent",
        }
    }
}

impl ControlAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::ChangeModel { .. } => "change_model",
        }
    }

    pub fn kind(&self) -> ControlActionKind {
        match self {
            Self::Start => ControlActionKind::Start,
            Self::Stop => ControlActionKind::Stop,
            Self::Restart => ControlActionKind::Restart,
            Self::ChangeModel { .. } => ControlActionKind::ChangeModel,
        }
    }
}

impl ControlActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::ChangeModel => "change_model",
        }
    }
}

pub(crate) fn validate_unit(unit: &ManagedUnit) -> Result<(), ControlError> {
    if unit.unit_id.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "unit_id must not be empty".to_string(),
        ));
    }

    if unit.display_name.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "display_name must not be empty".to_string(),
        ));
    }

    Ok(())
}

pub(crate) fn validate_request(request: &ControlRequest) -> Result<(), ControlError> {
    if request.unit_id.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "unit_id must not be empty".to_string(),
        ));
    }

    if request.reason.trim().is_empty() {
        return Err(ControlError::InvalidRequest(
            "reason must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn parse_allowed_actions(raw: &str) -> Result<Vec<ControlActionKind>, ControlError> {
    let mut actions = Vec::new();
    for item in raw.split(',') {
        let action = item.trim();
        if action.is_empty() {
            continue;
        }
        let parsed = match action {
            "start" => ControlActionKind::Start,
            "stop" => ControlActionKind::Stop,
            "restart" => ControlActionKind::Restart,
            "change_model" | "change-model" => ControlActionKind::ChangeModel,
            other => {
                return Err(ControlError::InvalidRequest(format!(
                    "invalid allowlisted control action: {other}"
                )));
            }
        };
        if !actions.contains(&parsed) {
            actions.push(parsed);
        }
    }
    Ok(actions)
}

fn control_action_mismatch_error(expected: &ControlAction, actual: &ControlAction) -> ControlError {
    ControlError::InvalidRequest(format!(
        "control apply receipt action mismatch: expected={} actual={}",
        expected.as_str(),
        actual.as_str()
    ))
}
