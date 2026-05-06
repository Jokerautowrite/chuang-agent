mod command;
mod fake;

pub use command::CommandActuator;
pub use fake::FakeActuator;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObserveTarget {
    Screen,
    Window(String),
    App(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub target: ObserveTarget,
    pub summary: String,
    pub evidence_ref: Option<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAppRequest {
    pub app_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppHandle {
    pub app_name: String,
    pub handle_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusTarget {
    App(String),
    Window(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickTarget {
    Coordinates { x: i32, y: i32 },
    UiLabel(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputTarget {
    Focused,
    UiLabel(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretOrPlainText {
    Plain(String),
    Secret { label: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenshotTarget {
    Screen,
    Window(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActuatorCommandKind {
    Observe,
    OpenApp,
    Focus,
    Click,
    InputText,
    Screenshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActuatorCommandContract {
    pub allowed_actions: Vec<ActuatorCommandKind>,
    pub audit_label: String,
    pub real_execution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActuatorError {
    pub message: String,
}

pub trait Actuator {
    fn observe(&mut self, target: ObserveTarget) -> Result<Observation, ActuatorError>;
    fn open_app(&mut self, request: OpenAppRequest) -> Result<AppHandle, ActuatorError>;
    fn focus(&mut self, target: FocusTarget) -> Result<(), ActuatorError>;
    fn click(&mut self, target: ClickTarget) -> Result<(), ActuatorError>;
    fn input_text(
        &mut self,
        target: InputTarget,
        text: SecretOrPlainText,
    ) -> Result<(), ActuatorError>;
    fn screenshot(&mut self, target: ScreenshotTarget) -> Result<EvidenceRef, ActuatorError>;
}

impl ActuatorCommandKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::OpenApp => "open_app",
            Self::Focus => "focus",
            Self::Click => "click",
            Self::InputText => "input_text",
            Self::Screenshot => "screenshot",
        }
    }
}

pub fn validate_actuator_command_contract(
    contract: &ActuatorCommandContract,
    action: ActuatorCommandKind,
) -> Result<(), ActuatorError> {
    if contract.audit_label.trim().is_empty() {
        return Err(actuator_contract_error(
            "actuator audit_label must not be empty",
        ));
    }
    if contract.allowed_actions.is_empty() {
        return Err(actuator_contract_error(
            "actuator command contract has no allowlisted actions",
        ));
    }
    if !contract.allowed_actions.contains(&action) {
        return Err(actuator_contract_error(format!(
            "actuator action {} is not allowlisted for {}",
            action.as_str(),
            contract.audit_label
        )));
    }
    Ok(())
}

fn actuator_contract_error(message: impl Into<String>) -> ActuatorError {
    ActuatorError {
        message: message.into(),
    }
}
