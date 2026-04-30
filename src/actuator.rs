mod fake;

pub use fake::FakeActuator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserveTarget {
    Screen,
    Window(String),
    App(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub target: ObserveTarget,
    pub summary: String,
    pub evidence_ref: Option<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAppRequest {
    pub app_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppHandle {
    pub app_name: String,
    pub handle_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusTarget {
    App(String),
    Window(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    Coordinates { x: i32, y: i32 },
    UiLabel(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputTarget {
    Focused,
    UiLabel(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretOrPlainText {
    Plain(String),
    Secret { label: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenshotTarget {
    Screen,
    Window(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub uri: String,
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
