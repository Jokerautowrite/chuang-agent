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
