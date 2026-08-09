use std::fs;
use std::os::unix::fs::PermissionsExt;
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

fn prepend_path_env(dir: &std::path::Path) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    if current.is_empty() {
        dir.display().to_string()
    } else {
        format!("{}:{current}", dir.display())
    }
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
    assert_eq!(
        observation.audit_message.as_deref(),
        Some("fake actuator observation")
    );
    assert_eq!(screenshot.uri, "fake://screenshot");
    assert_eq!(
        screenshot.audit_message.as_deref(),
        Some("fake actuator screenshot")
    );
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
    assert_eq!(observation.audit_message.as_deref(), Some("ok"));
    assert_eq!(
        observation
            .evidence_ref
            .as_ref()
            .and_then(|evidence_ref| evidence_ref.audit_message.as_deref()),
        Some("ok")
    );
    assert_eq!(handle.handle_id, "cmd://app/Feishu");
    assert_eq!(screenshot.uri, "cmd://screenshot");
    assert_eq!(screenshot.audit_message.as_deref(), Some("ok"));
}

#[test]
fn command_actuator_normalizes_read_only_audit_messages() {
    let script = temp_script_path("adapter-audit");
    fs::write(
        &script,
        r#"#!/bin/sh
python3 -c '
import json
import sys
request = json.load(sys.stdin)
action = request.get("action")
if action == "observe":
    print(json.dumps({"observation":{"target":request["observe_target"],"summary":"screen observed","evidence_ref":{"uri":"cmd://observation","audit_message":"evidence audit"}},"app_handle":None,"evidence_ref":None,"message":None}))
elif action == "screenshot":
    print(json.dumps({"observation":None,"app_handle":None,"evidence_ref":{"uri":"cmd://screenshot"},"message":"   "}))
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
    let screenshot = actuator.screenshot(ScreenshotTarget::Screen).unwrap();

    assert_eq!(observation.audit_message.as_deref(), Some("evidence audit"));
    assert_eq!(
        observation
            .evidence_ref
            .as_ref()
            .and_then(|evidence_ref| evidence_ref.audit_message.as_deref()),
        Some("evidence audit")
    );
    assert_eq!(screenshot.uri, "cmd://screenshot");
    assert_eq!(screenshot.audit_message, None);
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
    "open_command": ["bytedance-feishu"]
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
    "open_command": ["bytedance-feishu"]
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
        .env_remove("CHUANG_REAL_ACTUATOR_ENABLE")
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
    assert!(message.contains("allowed=true"));
    assert!(message.contains("dry_run=true"));
    assert!(message.contains("real_execution=false"));
    assert!(message.contains("read_only=false"));
    assert!(message.contains("live_gate_required=false"));
    assert!(message.contains("audit_label=actuator.operation.live"));
    assert!(message.contains("required_env=CHUANG_REAL_ACTUATOR_ENABLE"));
}

#[test]
fn real_actuator_adapter_live_open_app_detaches_child_output() {
    let noisy_app = temp_script_path("noisy-open-app");
    fs::write(
        &noisy_app,
        "#!/bin/sh\nsleep 0.05\nprintf 'child stdout noise\\n'\nprintf 'child stderr noise\\n' >&2\n",
    )
    .expect("noisy app script should write");
    fs::set_permissions(&noisy_app, fs::Permissions::from_mode(0o755))
        .expect("noisy app script should be executable");
    let allowlist = temp_script_path("real-actuator-noisy-open-app").with_extension("json");
    fs::write(
        &allowlist,
        serde_json::json!({
            "apps": [{
                "app_name": "NoisyApp",
                "open_command": [noisy_app.display().to_string()]
            }],
            "input_allowed": false,
            "click_allowed": false,
            "screenshot_allowed": false
        })
        .to_string(),
    )
    .expect("allowlist should write");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-actuator-adapter.py");
    let mut child = Command::new(adapter_path)
        .args(["--json", "--allowlist"])
        .arg(&allowlist)
        .env("CHUANG_REAL_ACTUATOR_ENABLE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("adapter should spawn");
    let request = br#"{"action":"open_app","open_app":{"app_name":"NoisyApp"}}"#;
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
        serde_json::from_slice(&output.stdout).expect("response should be clean json");
    assert_eq!(response["app_handle"]["app_name"], "NoisyApp");
    let message = response["message"].as_str().expect("message should exist");
    assert!(message.contains("dry_run=false"));
    assert!(message.contains("real_execution=true"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("child stdout noise"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("child stderr noise"));
}

#[test]
fn real_actuator_adapter_observe_is_readonly_and_structured() {
    let allowlist = temp_script_path("real-actuator-observe").with_extension("json");
    fs::write(
        &allowlist,
        r#"{
  "apps": [],
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
    let request = br#"{"action":"observe","observe_target":"Screen","open_app":null,"focus_target":null,"click_target":null,"input_target":null,"text":null,"screenshot_target":null}"#;
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
    assert!(response["observation"]["summary"]
        .as_str()
        .expect("summary should exist")
        .contains("current_window_title="));
    let message = response["message"].as_str().expect("message should exist");
    assert!(message.contains("allowlisted read-only actuator observation"));
    assert!(message.contains("dry_run=false"));
    assert!(message.contains("real_execution=false"));
    assert!(message.contains("read_only=true"));
    assert!(message.contains("live_gate_required=false"));
}

#[test]
fn real_actuator_adapter_screenshot_is_readonly_and_returns_evidence_boundary() {
    let allowlist = temp_script_path("real-actuator-screenshot").with_extension("json");
    fs::write(
        &allowlist,
        r#"{
  "apps": [],
  "input_allowed": false,
  "click_allowed": false,
  "screenshot_allowed": true
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
    let request = br#"{"action":"screenshot","observe_target":null,"open_app":null,"focus_target":null,"click_target":null,"input_target":null,"text":null,"screenshot_target":"Screen"}"#;
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
    assert!(message.contains("allowlisted read-only actuator observation"));
    assert!(message.contains("action=screenshot"));
    assert!(message.contains("read_only=true"));
    assert!(message.contains("live_gate_required=false"));
    let evidence_uri = response["evidence_ref"]["uri"]
        .as_str()
        .expect("evidence ref should exist");
    assert!(
        evidence_uri.starts_with("file://")
            || evidence_uri == "chuang-actuator://screenshot/unavailable",
        "unexpected evidence uri: {evidence_uri}"
    );
}

#[test]
fn checked_in_real_actuator_allowlist_enables_ga_interaction_atoms() {
    let allowlist_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("actuator-allowlist.example.json");
    let allowlist: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(allowlist_path).expect("allowlist should read"))
            .expect("allowlist should parse");

    assert_eq!(allowlist["click_allowed"], true);
    assert_eq!(allowlist["input_allowed"], true);
    assert_eq!(allowlist["screenshot_allowed"], true);
    assert!(allowlist["apps"]
        .as_array()
        .expect("apps should be an array")
        .iter()
        .any(|app| app["app_name"] == "Chrome"
            && app["open_command"] == serde_json::json!(["google-chrome-stable"])));
}

#[test]
fn real_actuator_adapter_click_and_input_are_dry_run_without_live_gate() {
    let allowlist = temp_script_path("real-actuator-ga-dry-run").with_extension("json");
    fs::write(
        &allowlist,
        r#"{
  "apps": [],
  "input_allowed": true,
  "click_allowed": true,
  "screenshot_allowed": true
}"#,
    )
    .expect("allowlist should write");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-actuator-adapter.py");

    for (request, expected_action) in [
        (
            br#"{"action":"click","observe_target":null,"open_app":null,"focus_target":null,"click_target":{"Coordinates":{"x":10,"y":20}},"input_target":null,"text":null,"screenshot_target":null}"#.as_slice(),
            "click",
        ),
        (
            br#"{"action":"input_text","observe_target":null,"open_app":null,"focus_target":null,"click_target":null,"input_target":"Focused","text":{"Plain":"hello"},"screenshot_target":null}"#.as_slice(),
            "input_text",
        ),
    ] {
        let mut child = Command::new(&adapter_path)
            .args(["--json", "--allowlist"])
            .arg(&allowlist)
            .env_remove("CHUANG_REAL_ACTUATOR_ENABLE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("adapter should spawn");
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
        assert!(message.contains("dry_run=true"));
        assert!(message.contains("real_execution=false"));
        assert!(message.contains(&format!("action={expected_action}")));
    }
}

#[test]
fn real_actuator_adapter_live_click_and_input_call_xdotool() {
    let allowlist = temp_script_path("real-actuator-ga-live").with_extension("json");
    fs::write(
        &allowlist,
        r#"{
  "apps": [],
  "input_allowed": true,
  "click_allowed": true,
  "screenshot_allowed": true
}"#,
    )
    .expect("allowlist should write");
    let bin_dir = std::env::temp_dir().join(format!(
        "chuang-actuator-xdotool-bin-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should move forward")
            .as_nanos()
    ));
    fs::create_dir_all(&bin_dir).expect("bin dir should write");
    let log_path = bin_dir.join("xdotool.log");
    let xdotool_path = bin_dir.join("xdotool");
    fs::write(
        &xdotool_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\n",
            shell_single_quote(&log_path.display().to_string())
        ),
    )
    .expect("xdotool stub should write");
    fs::set_permissions(&xdotool_path, fs::Permissions::from_mode(0o755))
        .expect("xdotool stub should be executable");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-actuator-adapter.py");

    for request in [
        br#"{"action":"click","observe_target":null,"open_app":null,"focus_target":null,"click_target":{"Coordinates":{"x":10,"y":20}},"input_target":null,"text":null,"screenshot_target":null}"#.as_slice(),
        br#"{"action":"input_text","observe_target":null,"open_app":null,"focus_target":null,"click_target":null,"input_target":"Focused","text":{"Plain":"hello"},"screenshot_target":null}"#.as_slice(),
    ] {
        let mut child = Command::new(&adapter_path)
            .args(["--json", "--allowlist"])
            .arg(&allowlist)
            .env("CHUANG_REAL_ACTUATOR_ENABLE", "1")
            .env("PATH", prepend_path_env(&bin_dir))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("adapter should spawn");
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
        assert!(message.contains("dry_run=false"));
        assert!(message.contains("real_execution=true"));
        assert!(message.contains("live_gate_required=true"));
    }

    let log = fs::read_to_string(log_path).expect("xdotool log should exist");
    assert!(log.contains("mousemove 10 20"));
    assert!(log.contains("click 1"));
    assert!(log.contains("type --clearmodifiers --delay 0 hello"));
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
