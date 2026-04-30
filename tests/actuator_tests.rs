use chuang_agent::actuator::{
    Actuator, ClickTarget, FakeActuator, FocusTarget, InputTarget, ObserveTarget, OpenAppRequest,
    ScreenshotTarget, SecretOrPlainText,
};

#[test]
fn fake_actuator_records_human_level_operation_sequence() {
    let mut actuator = FakeActuator::new();

    let handle = actuator
        .open_app(OpenAppRequest {
            app_name: "Feishu".to_string(),
        })
        .unwrap();
    actuator
        .focus(FocusTarget::App(handle.app_name.clone()))
        .unwrap();
    actuator
        .click(ClickTarget::UiLabel("composer".to_string()))
        .unwrap();
    actuator
        .input_text(
            InputTarget::Focused,
            SecretOrPlainText::Plain("hello".to_string()),
        )
        .unwrap();

    assert_eq!(
        actuator.calls(),
        &[
            "open_app:Feishu".to_string(),
            "focus:App(\"Feishu\")".to_string(),
            "click:UiLabel(\"composer\")".to_string(),
            "input_text:Focused:plain".to_string(),
        ]
    );
}

#[test]
fn fake_actuator_observe_and_screenshot_return_evidence_refs() {
    let mut actuator = FakeActuator::new();

    let observation = actuator.observe(ObserveTarget::Screen).unwrap();
    let screenshot = actuator.screenshot(ScreenshotTarget::Screen).unwrap();

    assert_eq!(observation.evidence_ref.unwrap().uri, "fake://observation");
    assert_eq!(screenshot.uri, "fake://screenshot");
}

#[test]
fn fake_actuator_does_not_record_secret_text_content() {
    let mut actuator = FakeActuator::new();

    actuator
        .input_text(
            InputTarget::Focused,
            SecretOrPlainText::Secret {
                label: "verification-code".to_string(),
            },
        )
        .unwrap();

    assert_eq!(actuator.calls(), &["input_text:Focused:secret".to_string()]);
    assert!(!actuator.calls()[0].contains("verification-code"));
}
