use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_config_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-control-{name}-{nanos}.toml"))
}

fn write_fake_runtime_config(name: &str) -> PathBuf {
    let config_path = temp_config_path(name);
    let root = config_path.with_extension("");
    fs::create_dir_all(&root).expect("fake config root should be created");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("fake runtime config should be writable");
    config_path
}

#[test]
fn cli_control_list_shows_default_local_agents() {
    let config_path = write_fake_runtime_config("list-default");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("name=小创"));
    assert!(stdout.contains("name=小承"));
    assert!(stdout.contains("name=小云"));
    assert!(stdout.contains("name=小策"));
    assert!(stdout.contains("unit_id=codex-feishu-bot.service"));
}

#[test]
fn cli_control_apply_requires_approval_for_service_change() {
    let config_path = write_fake_runtime_config("apply-requires-approval");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--unit",
            "codex-xiaoce",
            "--action",
            "restart",
            "--reason",
            "test restart",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.contains("decision=needs_approval"));
    assert!(stderr.contains("control action requires --approve"));
}

#[test]
fn cli_control_apply_runs_after_explicit_approval() {
    let config_path = write_fake_runtime_config("apply-approved");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--unit",
            "codex-xiaoce",
            "--action",
            "change-model",
            "--model",
            "gpt-5.5",
            "--reason",
            "test model switch",
            "--approve",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("decision=needs_approval"));
    assert!(stdout.contains("control_applied unit_id=codex-xiaoce"));
    assert!(stdout.contains("action=change_model"));
    assert!(stdout.contains("model=gpt-5.5"));
    assert!(stdout.contains("control_audit: recorded"));
}

#[test]
fn cli_control_apply_can_use_command_control_plane_from_config() {
    let config_path = temp_config_path("command");
    fs::write(
        &config_path,
        r#"
[control]
kind = "command"
program = "printf"
list_args = "[{"unit_id":"command-agent","display_name":"CommandAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{"channel":"command"}}]"
apply_args = "{"unit_id":"command-agent","action":"change_model","previous_status":"Running","next_status":"Running","model_name":"gpt-5.4","message":"command_control_applied"}"
"#,
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
            "--unit",
            "command-agent",
            "--action",
            "change-model",
            "--model",
            "gpt-5.4",
            "--reason",
            "test command control",
            "--approve",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("unit_id=command-agent"), "stdout={stdout}");
    assert!(stdout.contains("model=gpt-5.4"), "stdout={stdout}");
    assert!(stdout.contains("control_applied"), "stdout={stdout}");
}

#[test]
fn cli_control_can_use_checked_in_example_adapter() {
    let config_path = temp_config_path("example-adapter");
    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-control-adapter-example.sh");
    fs::write(
        &config_path,
        format!(
            r#"
control = "command"
program = "sh"
list_args = "{} list --json"
apply_args = "{} apply --json"
control_timeout_ms = 30000
"#,
            script_path.display(),
            script_path.display()
        ),
    )
    .expect("config file should be writable");

    let list = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        list.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list.stderr)
    );
    let units: Value =
        serde_json::from_slice(&list.stdout).expect("control list should output json");
    assert!(units
        .as_array()
        .expect("units should be array")
        .iter()
        .any(|unit| unit["unit_id"] == "chuang-demo-agent"));

    let apply = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
            "--unit",
            "chuang-demo-agent",
            "--action",
            "change-model",
            "--model",
            "gpt-5.4",
            "--reason",
            "test checked in example adapter",
            "--approve",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        apply.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let stdout = String::from_utf8_lossy(&apply.stdout);
    assert!(stdout.contains("control_applied unit_id=chuang-demo-agent"));
    assert!(stdout.contains("model=gpt-5.4"));
    assert!(stdout.contains("control_audit: recorded"));
}

#[test]
fn cli_control_can_use_allowlisted_real_adapter_in_dry_run() {
    let root = temp_config_path("real-adapter-dry-run-root");
    let config_path = root.with_extension("toml");
    let allowlist_path = root.with_extension("json");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-control-adapter.py");
    fs::write(
        &allowlist_path,
        r#"{
  "units": [{
    "unit_id": "chuang-test.service",
    "display_name": "Chuang Test Service",
    "kind": "service",
    "default_status": "Stopped",
    "start_command": ["systemctl", "--user", "start", "chuang-test.service"],
    "restart_command": ["systemctl", "--user", "restart", "chuang-test.service"],
    "stop_command": ["systemctl", "--user", "stop", "chuang-test.service"],
    "metadata": {"owner": "chuang-test"}
  }]
}"#,
    )
    .expect("allowlist should write");
    fs::write(
        &config_path,
        format!(
            r#"
control = "command"
program = "{}"
list_args = "list --json --allowlist {}"
apply_args = "apply --json --allowlist {}"
control_timeout_ms = 30000
"#,
            adapter_path.display(),
            allowlist_path.display(),
            allowlist_path.display()
        ),
    )
    .expect("config should write");

    let list = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--config",
            config_path.to_str().expect("config path utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        list.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list.stderr)
    );
    let units: Value = serde_json::from_slice(&list.stdout).expect("units json");
    assert_eq!(units[0]["unit_id"], "chuang-test.service");
    assert_eq!(units[0]["channel"], "unknown");

    let apply = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--config",
            config_path.to_str().expect("config path utf8"),
            "--unit",
            "chuang-test.service",
            "--action",
            "start",
            "--reason",
            "dry run allowlist check",
            "--approve",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        apply.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let stdout = String::from_utf8_lossy(&apply.stdout);
    assert!(stdout.contains("control_applied unit_id=chuang-test.service"));
    assert!(stdout.contains("action=start"));
    assert!(stdout.contains("next=Running"));
}

#[test]
fn cli_control_real_adapter_rejects_unallowlisted_unit() {
    let root = temp_config_path("real-adapter-reject-root");
    let config_path = root.with_extension("toml");
    let allowlist_path = root.with_extension("json");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-control-adapter.py");
    fs::write(&allowlist_path, r#"{"units":[]}"#).expect("allowlist should write");
    fs::write(
        &config_path,
        format!(
            r#"
control = "command"
program = "{}"
list_args = "list --json --allowlist {}"
apply_args = "apply --json --allowlist {}"
"#,
            adapter_path.display(),
            allowlist_path.display(),
            allowlist_path.display()
        ),
    )
    .expect("config should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--config",
            config_path.to_str().expect("config path utf8"),
            "--unit",
            "not-allowlisted.service",
            "--action",
            "start",
            "--reason",
            "should fail",
            "--approve",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown control unit"));
}

#[test]
fn cli_control_real_adapter_direct_receipt_keeps_live_gate_closed_by_default() {
    let root = temp_config_path("real-adapter-direct-dry-run-root");
    let allowlist_path = root.with_extension("json");
    let marker_path = root.with_extension("marker");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-control-adapter.py");
    fs::write(
        &allowlist_path,
        serde_json::json!({
            "units": [{
                "unit_id": "chuang-direct-test.service",
                "display_name": "Chuang Direct Test",
                "kind": "service",
                "default_status": "Stopped",
                "start_command": [
                    "sh",
                    "-c",
                    format!("printf executed > {}", marker_path.display())
                ],
                "metadata": {"owner": "chuang-test"}
            }]
        })
        .to_string(),
    )
    .expect("allowlist should write");

    let list = Command::new(&adapter_path)
        .args([
            "list",
            "--json",
            "--allowlist",
            allowlist_path.to_str().expect("allowlist path utf8"),
        ])
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_STATUS_ENABLE")
        .output()
        .expect("real adapter list should execute");
    assert!(
        list.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list.stderr)
    );
    let units: Value = serde_json::from_slice(&list.stdout).expect("list output should be json");
    assert_eq!(units[0]["metadata"]["adapter"], "chuang-real-control");
    assert_eq!(units[0]["metadata"]["dry_run"], "true");
    assert_eq!(units[0]["metadata"]["live_enabled"], "false");
    assert_eq!(units[0]["metadata"]["audit_label"], "control.apply.live");
    assert_eq!(
        units[0]["metadata"]["required_env"],
        "CHUANG_REAL_CONTROL_ENABLE"
    );
    assert_eq!(units[0]["metadata"]["allowed_actions"], "start");

    let mut child = Command::new(&adapter_path)
        .args([
            "apply",
            "--json",
            "--allowlist",
            allowlist_path.to_str().expect("allowlist path utf8"),
        ])
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_STATUS_ENABLE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("real adapter apply should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(
            br#"{"unit_id":"chuang-direct-test.service","action":"start","reason":"dry-run regression"}"#,
        )
        .expect("request should write");
    let apply = child
        .wait_with_output()
        .expect("real adapter apply should finish");
    assert!(
        apply.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&apply.stderr)
    );
    assert!(
        !marker_path.exists(),
        "adapter must not execute allowlisted command without CHUANG_REAL_CONTROL_ENABLE=1"
    );
    let receipt: Value =
        serde_json::from_slice(&apply.stdout).expect("apply output should be json receipt");
    assert_eq!(receipt["unit_id"], "chuang-direct-test.service");
    assert_eq!(receipt["action"], "start");
    assert_eq!(receipt["next_status"], "Running");
    let message = receipt["message"].as_str().expect("receipt message");
    assert!(message.contains("allowed=true"), "message={message}");
    assert!(message.contains("dry_run=true"), "message={message}");
    assert!(message.contains("live_enabled=false"), "message={message}");
    assert!(
        message.contains("audit_label=control.apply.live"),
        "message={message}"
    );
    assert!(
        message.contains("required_env=CHUANG_REAL_CONTROL_ENABLE"),
        "message={message}"
    );
}

#[test]
fn cli_control_real_adapter_directly_rejects_unallowlisted_action() {
    let root = temp_config_path("real-adapter-direct-reject-root");
    let allowlist_path = root.with_extension("json");
    let adapter_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-real-control-adapter.py");
    fs::write(
        &allowlist_path,
        r#"{
  "units": [{
    "unit_id": "chuang-direct-test.service",
    "display_name": "Chuang Direct Test",
    "kind": "service",
    "default_status": "Stopped",
    "start_command": ["sh", "-c", "exit 42"],
    "metadata": {"owner": "chuang-test"}
  }]
}"#,
    )
    .expect("allowlist should write");

    let mut child = Command::new(&adapter_path)
        .args([
            "apply",
            "--json",
            "--allowlist",
            allowlist_path.to_str().expect("allowlist path utf8"),
        ])
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_STATUS_ENABLE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("real adapter apply should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(
            br#"{"unit_id":"chuang-direct-test.service","action":"restart","reason":"reject regression"}"#,
        )
        .expect("request should write");
    let output = child
        .wait_with_output()
        .expect("real adapter apply should finish");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("action not allowlisted: chuang-direct-test.service:restart"),
        "stderr={stderr}"
    );
}

#[test]
fn cli_control_apply_uses_fallible_list_once_before_workflow() {
    let root = temp_config_path("command-apply-single-list-root");
    let script_path = root.with_extension("sh");
    let state_path = root.with_extension("state");
    let config_path = root.with_extension("toml");
    fs::write(
        &script_path,
        format!(
            r#"if [ "$1" = "list" ]; then
  if [ -f "{state}" ]; then
    exit 9
  fi
  printf seen > "{state}"
  printf '[{{"unit_id":"single-list-agent","display_name":"SingleListAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{{"channel":"command"}}}}]'
  exit 0
fi
if [ "$1" = "apply" ]; then
  printf '{{"unit_id":"single-list-agent","action":"change_model","previous_status":"Running","next_status":"Running","model_name":"gpt-5.4","message":"applied_after_single_list"}}'
  exit 0
fi
exit 2
"#,
            state = state_path.display()
        ),
    )
    .expect("script file should be writable");
    fs::write(
        &config_path,
        format!(
            r#"
[control]
kind = "command"
program = "sh"
list_args = "{} list"
apply_args = "{} apply"
"#,
            script_path.display(),
            script_path.display()
        ),
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
            "--unit",
            "single-list-agent",
            "--action",
            "change-model",
            "--model",
            "gpt-5.4",
            "--reason",
            "test single fallible list",
            "--approve",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("control_applied"), "stdout={stdout}");
    assert!(stdout.contains("model=gpt-5.4"), "stdout={stdout}");
}

#[test]
fn cli_control_list_can_use_command_control_plane_from_config() {
    let config_path = temp_config_path("command-list");
    fs::write(
        &config_path,
        r#"
[control]
kind = "command"
program = "printf"
list_args = "[{"unit_id":"command-list-agent","display_name":"CommandListAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{"channel":"command"}}]"
apply_args = "{}"
"#,
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");
    let units = parsed.as_array().expect("control list should return array");

    let unit = units
        .iter()
        .find(|unit| unit["unit_id"] == "command-list-agent")
        .expect("command unit should exist");
    assert_eq!(unit["display_name"], "CommandListAgent");
    assert_eq!(unit["channel"], "command");
}

#[test]
fn cli_control_list_reports_command_control_failure() {
    let config_path = temp_config_path("command-list-fails");
    fs::write(
        &config_path,
        r#"
[control]
kind = "command"
program = "false"
list_args = "--version"
apply_args = "{}"
"#,
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("control_failed"), "stderr={stderr}");
    assert!(stderr.contains("status=Some(1)"), "stderr={stderr}");
}

#[test]
fn cli_control_list_reports_malformed_command_control_json() {
    let config_path = temp_config_path("command-list-malformed");
    fs::write(
        &config_path,
        r#"
[control]
kind = "command"
program = "printf"
list_args = "not-json"
apply_args = "{}"
"#,
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("control_failed"), "stderr={stderr}");
    assert!(
        stderr.contains("control list output parse failed"),
        "stderr={stderr}"
    );
}

#[test]
fn cli_control_apply_reports_missing_action_concisely() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--unit",
            "codex-xiaoce",
            "--reason",
            "test missing action",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("control apply requires --action"));
}

#[test]
fn cli_control_apply_reports_unsupported_action_concisely() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--unit",
            "codex-xiaoce",
            "--action",
            "reload",
            "--reason",
            "test unsupported action",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("unsupported control action: reload"));
}

#[test]
fn cli_control_list_can_render_json_for_control_surfaces() {
    let config_path = write_fake_runtime_config("list-json");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "list",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");
    let units = parsed.as_array().expect("control list should return array");

    let xiaoce = units
        .iter()
        .find(|unit| unit["unit_id"] == "codex-xiaoce")
        .expect("xiaoce should exist");
    assert_eq!(xiaoce["display_name"], "小策");
    assert_eq!(xiaoce["kind"], "agent");
    assert_eq!(xiaoce["channel"], "feishu");
}

#[test]
fn cli_control_apply_can_render_json_view() {
    let config_path = write_fake_runtime_config("apply-json");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--unit",
            "codex-xiaoce",
            "--action",
            "change-model",
            "--model",
            "gpt-5.5",
            "--reason",
            "test json model switch",
            "--approve",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["unit_id"], "codex-xiaoce");
    assert_eq!(parsed["display_name"], "小策");
    assert_eq!(parsed["action"], "change_model");
    assert_eq!(parsed["model_name"], "gpt-5.5");
    assert_eq!(parsed["audit_recorded"], true);
}

#[test]
fn cli_control_apply_accepts_display_name_for_mvp_surfaces() {
    let config_path = write_fake_runtime_config("display-name");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "control",
            "apply",
            "--unit",
            "小策",
            "--action",
            "重启",
            "--reason",
            "test display name restart",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.contains("unit_id=codex-xiaoce"));
    assert!(stdout.contains("name=小策"));
    assert!(stdout.contains("decision=needs_approval"));
    assert!(stderr.contains("control action requires --approve"));
}
