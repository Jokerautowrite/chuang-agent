use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::actuator::{
    Actuator, ClickTarget, CommandActuator, FakeActuator, FocusTarget, InputTarget, ObserveTarget,
    OpenAppRequest, ScreenshotTarget, SecretOrPlainText,
};
use chuang_agent::runtime_config::ActuatorCommandConfig;

fn temp_script_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-actuator-{name}-{nanos}.sh"))
}

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

#[test]
fn command_actuator_invokes_external_json_adapter() {
    let script = temp_script_path("adapter");
    fs::write(
        &script,
        r#"#!/bin/sh
python3 -c '
import json
import sys
request = json.load(sys.stdin)
action = request.get("action")
if action == "observe":
    print(json.dumps({"observation":{"target":request["observe_target"],"summary":"screen observed","evidence_ref":{"uri":"cmd://observation"}},"app_handle":None,"evidence_ref":None,"message":"ok"}))
elif action == "open_app":
    app = request["open_app"]["app_name"]
    print(json.dumps({"observation":None,"app_handle":{"app_name":app,"handle_id":"cmd://app/" + app},"evidence_ref":None,"message":"ok"}))
elif action == "screenshot":
    print(json.dumps({"observation":None,"app_handle":None,"evidence_ref":{"uri":"cmd://screenshot"},"message":"ok"}))
else:
    print(json.dumps({"observation":None,"app_handle":None,"evidence_ref":None,"message":"ok"}))
'
"#,
    )
    .expect("script should write");

    let mut actuator = CommandActuator::new(ActuatorCommandConfig {
        program: "sh".to_string(),
        args: script.display().to_string(),
        timeout_ms: 30_000,
    });

    let observation = actuator.observe(ObserveTarget::Screen).unwrap();
    let handle = actuator
        .open_app(OpenAppRequest {
            app_name: "Feishu".to_string(),
        })
        .unwrap();
    actuator
        .focus(FocusTarget::App("Feishu".to_string()))
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
    let screenshot = actuator.screenshot(ScreenshotTarget::Screen).unwrap();

    assert_eq!(observation.summary, "screen observed");
    assert_eq!(handle.handle_id, "cmd://app/Feishu");
    assert_eq!(screenshot.uri, "cmd://screenshot");
}

#[test]
fn command_actuator_reports_malformed_adapter_output() {
    let mut actuator = CommandActuator::new(ActuatorCommandConfig {
        program: "printf".to_string(),
        args: "not-json".to_string(),
        timeout_ms: 30_000,
    });

    let err = actuator
        .observe(ObserveTarget::Screen)
        .expect_err("malformed output should fail");

    assert!(err.message.contains("actuator command output parse failed"));
}

#[test]
fn command_actuator_rejects_malformed_args_before_spawn() {
    let mut actuator = CommandActuator::new(ActuatorCommandConfig {
        program: "printf".to_string(),
        args: r#"unterminated ""#.to_string(),
        timeout_ms: 30_000,
    });

    let err = actuator
        .observe(ObserveTarget::Screen)
        .expect_err("malformed args should fail before spawn");

    assert!(err.message.contains("actuator args parse failed"));
}

#[test]
fn command_actuator_rejects_unknown_response_fields() {
    let mut actuator = CommandActuator::new(ActuatorCommandConfig {
        program: "printf".to_string(),
        args: r#"{"observation":{"target":"Screen","summary":"screen_observed","evidence_ref":null},"app_handle":null,"evidence_ref":null,"message":"ok","unexpected":"ignored-before"}"#.to_string(),
        timeout_ms: 30_000,
    });

    let err = actuator
        .observe(ObserveTarget::Screen)
        .expect_err("unknown top-level response field should fail");

    assert!(err.message.contains("unknown field"));
}

#[test]
fn command_actuator_times_out_stuck_adapter() {
    let mut actuator = CommandActuator::new(ActuatorCommandConfig {
        program: "sleep".to_string(),
        args: "1".to_string(),
        timeout_ms: 20,
    });
    let started = Instant::now();

    let err = actuator
        .observe(ObserveTarget::Screen)
        .expect_err("stuck adapter should time out");

    assert!(err
        .message
        .contains("actuator command timed out after 20ms"));
    assert!(started.elapsed().as_millis() < 500);
}

#[test]
fn real_actuator_adapter_allows_only_allowlisted_app_in_dry_run() {
    let allowlist = temp_script_path("real-actuator-allowlist").with_extension("json");
    fs::write(
        &allowlist,
        r#"{
  "apps": [{
    "app_name": "Feishu",
    "open_command": ["feishu"]
  }],
  "input_allowed": false,
  "click_allowed": false,
  "screenshot_allowed": false
}"#,
    )
    .expect("allowlist should write");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-actuator-adapter.py");
    let mut actuator = CommandActuator::new(ActuatorCommandConfig {
        program: adapter_path.display().to_string(),
        args: format!("--json --allowlist {}", allowlist.display()),
        timeout_ms: 30_000,
    });

    let handle = actuator
        .open_app(OpenAppRequest {
            app_name: "Feishu".to_string(),
        })
        .expect("allowlisted app should be accepted");
    assert_eq!(handle.handle_id, "chuang-actuator://app/Feishu");

    let err = actuator
        .open_app(OpenAppRequest {
            app_name: "Konsole".to_string(),
        })
        .expect_err("unallowlisted app should fail");
    assert!(err.message.contains("app not allowlisted"));

    let err = actuator
        .click(ClickTarget::UiLabel("send".to_string()))
        .expect_err("click should not be allowlisted");
    assert!(err.message.contains("click not allowlisted"));
}

#[test]
fn real_actuator_adapter_dry_run_message_carries_audit_boundary() {
    let allowlist = temp_script_path("real-actuator-boundary").with_extension("json");
    fs::write(
        &allowlist,
        r#"{
  "apps": [{
    "app_name": "Feishu",
    "open_command": ["feishu"]
  }],
  "input_allowed": false,
  "click_allowed": false,
  "screenshot_allowed": false
}"#,
    )
    .expect("allowlist should write");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-actuator-adapter.py");
    let mut child = Command::new(adapter_path)
        .args(["--json", "--allowlist"])
        .arg(&allowlist)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("adapter should spawn");
    let request = br#"{"action":"open_app","open_app":{"app_name":"Feishu"}}"#;
    std::io::Write::write_all(
        child.stdin.as_mut().expect("stdin should be available"),
        request,
    )
    .expect("request should write");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("adapter should finish");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("response should be json");
    let message = response["message"].as_str().expect("message should exist");
    assert!(message.contains("real_execution=false"));
    assert!(message.contains("audit_label=actuator.operation.live"));
    assert!(message.contains("required_env=CHUANG_REAL_ACTUATOR_ENABLE"));
}
