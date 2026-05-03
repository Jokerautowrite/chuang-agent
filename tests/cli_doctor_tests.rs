use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-doctor-test-{name}-{nanos}"))
}

#[test]
fn cli_doctor_reports_mvp_health_in_text() {
    let root = temp_root("text");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("config should be written");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("doctor_ok: true"));
    assert!(stdout.contains("doctor_check name=config ok=true"));
    assert!(stdout.contains("doctor_check name=identity_memory ok=true"));
    assert!(stdout.contains("doctor_check name=identity_experiences ok=true"));
    assert!(stdout.contains("doctor_check name=slots ok=true"));
    assert!(stdout.contains("doctor_check name=atomic_tools ok=true"));
    assert!(stdout.contains("doctor_check name=goal_mode ok=true"));
    assert!(stdout.contains("doctor_check name=actuator_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=control_plane_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=runtime_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=subagent_queue_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=plugin_registry ok=true"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("execution: generic_agent_mvp"));
    assert!(stdout.contains(
        "atomic_tools_ok: true manifest_schema_version=1 action_schema_version=1 report_schema_version=6"
    ));
    assert!(stdout.contains("atomic_tools_mapped: file_read,file_write,code_execute"));
    assert!(stdout.contains(
        "atomic_tools_interface_only: mouse,keyboard,screenshot,locate,wait,human_suspend"
    ));
    assert!(stdout.contains("goal_mode_ok: true entrypoint=run --goal TEXT"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
    assert!(stdout.contains("placeholder_warning: provider=fake"));
    assert!(stdout.contains("placeholder_warning: control_plane=fake_local"));
}

#[test]
fn cli_doctor_can_render_json_without_secret_leak() {
    let root = temp_root("json");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("config should be written");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "doctor-secret-key",
            "--provider-model",
            "gpt-5.5",
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

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["checks"].as_array().expect("checks array").len(), 11);
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "atomic_tools"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "goal_mode"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "identity_experiences"));
    assert_eq!(
        parsed["status"]["config"]["identity_experiences_path"],
        root.join("identity")
            .join("experiences.md")
            .display()
            .to_string()
    );
    assert_eq!(parsed["status"]["atomic_tools"]["ok"], true);
    assert_eq!(parsed["status"]["goal_mode"]["ok"], true);
    assert_eq!(
        parsed["status"]["goal_mode"]["cli_entrypoint"],
        "run --goal TEXT"
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["manifest_schema_version"],
        1
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_report_schema_version"],
        6
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_action_schema_version"],
        1
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["mapped_atomic_tool_names"],
        serde_json::json!(["file_read", "file_write", "code_execute"])
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["interface_only_atomic_tool_names"],
        serde_json::json!([
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "wait",
            "human_suspend"
        ])
    );
    assert_eq!(
        parsed["status"]["config"]["provider_kind"],
        "openai_compatible"
    );
    assert_eq!(parsed["status"]["config"]["api_key_state"], "<set>");
    assert!(parsed["status"]["config"]["placeholder_warnings"]
        .as_array()
        .expect("placeholder warnings should be an array")
        .iter()
        .any(|warning| warning
            .as_str()
            .expect("warning should be string")
            .contains("actuator=fake")));
    assert!(!stdout.contains("doctor-secret-key"));
}

#[test]
fn cli_doctor_reports_command_control_list_failure() {
    let root = temp_root("command-control-fail");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{}"
identity_memory_root = "{}"
control = "command"
program = "false"
list_args = "--version"
apply_args = "apply --json"
"#,
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("doctor_control_plane_list_failed"),
        "stderr={stderr}"
    );
    assert!(stderr.contains("status=Some(1)"), "stderr={stderr}");
}

#[test]
fn cli_doctor_command_control_smoke_does_not_call_apply() {
    let root = temp_root("command-control-readonly");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let marker_path = root.join("apply-called");
    let script_path = root.join("control-adapter.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
if [ "$1" = "list" ]; then
  printf '[{{"unit_id":"chuang-readonly","display_name":"Chuang Readonly","kind":"service","status":"Running","model_name":null,"metadata":{{"adapter":"doctor-test"}}}}]'
  exit 0
fi
if [ "$1" = "apply" ]; then
  printf applied > "{}"
  printf '{{"unit_id":"chuang-readonly","action":"restart","previous_status":"Running","next_status":"Running","model_name":null,"message":"apply should not run"}}'
  exit 0
fi
exit 2
"#,
            marker_path.display()
        ),
    )
    .expect("script should be writable");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{}"
identity_memory_root = "{}"
control = "command"
program = "sh"
list_args = "{} list"
apply_args = "{} apply"
"#,
            root.join("memory.db").display(),
            root.join("identity").display(),
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
            "doctor",
            "--json",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !marker_path.exists(),
        "doctor control smoke must remain list-only and must not call apply"
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("doctor stdout should be json");
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "control_plane_smoke" && check["ok"] == true));
}
