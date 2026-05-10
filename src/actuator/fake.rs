use crate::actuator::{
    Actuator, ActuatorError, AppHandle, ClickTarget, EvidenceRef, FocusTarget, InputTarget,
    Observation, ObserveTarget, OpenAppRequest, ScreenshotTarget, SecretOrPlainText,
};

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
                audit_message: Some("fake actuator observation".to_string()),
            }),
            audit_message: Some("fake actuator observation".to_string()),
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
            audit_message: Some("fake actuator screenshot".to_string()),
        })
    }
}
