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

#[derive(Debug, Default, Clone)]
pub struct FakeActuator {
    calls: Vec<String>,
}

impl FakeActuator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> &[String] {
        &self.calls
    }

    fn record(&mut self, call: impl Into<String>) {
        self.calls.push(call.into());
    }
}

impl Actuator for FakeActuator {
    fn observe(&mut self, target: ObserveTarget) -> Result<Observation, ActuatorError> {
        self.record(format!("observe:{target:?}"));
        Ok(Observation {
            target,
            summary: "fake observation".to_string(),
            evidence_ref: Some(EvidenceRef {
                uri: "fake://observation".to_string(),
            }),
        })
    }

    fn open_app(&mut self, request: OpenAppRequest) -> Result<AppHandle, ActuatorError> {
        self.record(format!("open_app:{}", request.app_name));
        Ok(AppHandle {
            handle_id: format!("fake-app://{}", request.app_name),
            app_name: request.app_name,
        })
    }

    fn focus(&mut self, target: FocusTarget) -> Result<(), ActuatorError> {
        self.record(format!("focus:{target:?}"));
        Ok(())
    }

    fn click(&mut self, target: ClickTarget) -> Result<(), ActuatorError> {
        self.record(format!("click:{target:?}"));
        Ok(())
    }

    fn input_text(
        &mut self,
        target: InputTarget,
        text: SecretOrPlainText,
    ) -> Result<(), ActuatorError> {
        let text_kind = match text {
            SecretOrPlainText::Plain(_) => "plain",
            SecretOrPlainText::Secret { .. } => "secret",
        };
        self.record(format!("input_text:{target:?}:{text_kind}"));
        Ok(())
    }

    fn screenshot(&mut self, target: ScreenshotTarget) -> Result<EvidenceRef, ActuatorError> {
        self.record(format!("screenshot:{target:?}"));
        Ok(EvidenceRef {
            uri: "fake://screenshot".to_string(),
        })
    }
}
