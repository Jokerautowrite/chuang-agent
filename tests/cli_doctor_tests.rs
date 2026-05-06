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
    assert!(stdout.contains("doctor_check name=memory_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=channel_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=subagent_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=external_ai_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=slots ok=true"));
    assert!(stdout.contains("doctor_check name=atomic_tools ok=true"));
    assert!(stdout.contains("doctor_check name=governance_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=goal_mode ok=true"));
    assert!(stdout.contains("doctor_check name=goal_run_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=project_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=actuator_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=control_plane_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=runtime_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=subagent_queue_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=plugin_registry ok=true"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("execution: generic_agent_mvp"));
    assert!(stdout.contains(
        "governance_readiness: ok=true kind=static_rule rules_loaded=true tool_surface_governed=true goal_run_executes=false"
    ));
    assert!(stdout.contains(
        "governance_decisions: read_only=allowed dangerous_write=needs_approval dangerous_shell=needs_approval secret_shell=draft_only"
    ));
    assert!(stdout.contains(
        "atomic_tools_ok: true manifest_schema_version=1 action_schema_version=1 report_schema_version=6"
    ));
    assert!(stdout.contains("atomic_tools_mapped: file_read,file_write,code_execute"));
    assert!(stdout.contains(
        "atomic_tools_interface_only: mouse,keyboard,screenshot,locate,wait,human_suspend"
    ));
    assert!(stdout.contains("goal_mode_ok: true entrypoint=run --goal TEXT"));
    assert!(stdout.contains("goal_run_ok: true"));
    assert!(stdout.contains("goal_id=mainline-mvp"));
    assert!(stdout.contains("goal_run_checkpoint_log_complete:"));
    assert!(stdout.contains("goal_run_last_checkpoint:"));
    assert!(stdout.contains("goal_run_last_checkpoint_summary:"));
    assert!(stdout.contains("goal_run_incomplete_reasons:"));
    assert!(stdout.contains("project_readiness: ok=true state=mvp_ready_with_partial_modules"));
    assert!(stdout.contains(
        "release_readiness: ok=true name=second_test_version state=second_test_version_ready"
    ));
    assert!(stdout.contains("release_acceptance: count=7"));
    assert!(stdout.contains(
        "connects_real_external_services=false verifies_real_external_services=false uses_stub_or_local_fixtures=true writes_repo_files=false"
    ));
    assert!(stdout.contains("memory_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("channel_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains(
        "subagent_readiness: ok=true state=queued_protocol_partial mode=fake local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=partial"
    ));
    assert!(stdout.contains("subagent_readiness_local_contract_reason:"));
    assert!(stdout.contains("subagent_readiness_live_adapter_reason:"));
    assert!(stdout.contains("external_ai_readiness: ok=true state=ready"));
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
    assert_eq!(parsed["checks"].as_array().expect("checks array").len(), 19);
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
        .any(|check| check["name"] == "governance_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "goal_run_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "project_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "release_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "identity_experiences"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "memory_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "channel_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "subagent_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "external_ai_readiness"));
    assert_eq!(
        parsed["status"]["config"]["identity_experiences_path"],
        root.join("identity")
            .join("experiences.md")
            .display()
            .to_string()
    );
    assert_eq!(parsed["status"]["atomic_tools"]["ok"], true);
    assert_eq!(parsed["status"]["governance"]["ok"], true);
    assert_eq!(parsed["status"]["governance"]["rules_loaded"], true);
    assert_eq!(
        parsed["status"]["governance"]["tool_surface_governed"],
        true
    );
    assert_eq!(
        parsed["status"]["governance"]["dangerous_shell_decision"],
        "needs_approval"
    );
    assert_eq!(
        parsed["status"]["governance"]["dangerous_write_decision"],
        "needs_approval"
    );
    assert_eq!(parsed["status"]["governance"]["goal_run_executes"], false);
    assert_eq!(parsed["status"]["goal_mode"]["ok"], true);
    assert_eq!(
        parsed["status"]["goal_mode"]["cli_entrypoint"],
        "run --goal TEXT"
    );
    assert_eq!(parsed["status"]["goal_run"]["ok"], true);
    assert_eq!(parsed["status"]["goal_run"]["goal_id"], "mainline-mvp");
    assert!(parsed["status"]["goal_run"]["plan_exists"].is_boolean());
    assert!(parsed["status"]["goal_run"]["checkpoint_count"].is_number());
    assert!(parsed["status"]["goal_run"]["checkpoint_log_complete"].is_boolean());
    assert!(parsed["status"]["goal_run"]["last_checkpoint_summary"].is_string());
    assert!(parsed["status"]["goal_run"]["incomplete_reasons"]
        .as_array()
        .expect("goal run incomplete reasons should be an array")
        .iter()
        .all(|reason| reason.is_string()));
    assert_eq!(parsed["status"]["project_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["project_readiness"]["overall_state"],
        "mvp_ready_with_partial_modules"
    );
    assert_eq!(parsed["status"]["release_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["release_readiness"]["release_name"],
        "second_test_version"
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["overall_state"],
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["readiness_scope"],
        "readiness_and_smoke_acceptance_only_no_live_external_service_connection"
    );
    assert_eq!(parsed["status"]["release_readiness"]["acceptance_count"], 7);
    assert_eq!(
        parsed["status"]["release_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["verifies_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["uses_stub_or_local_fixtures"],
        true
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["writes_repo_files"],
        false
    );
    assert!(parsed["status"]["release_readiness"]["acceptance"]
        .as_array()
        .expect("release acceptance array")
        .iter()
        .any(|item| item["name"] == "real_external_services"
            && item["state"] == "deferred"
            && item["connects_real_service"] == false));
    assert_eq!(parsed["status"]["memory_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["memory_readiness"]["overall_state"],
        "ready"
    );
    assert_eq!(parsed["status"]["memory_readiness"]["layer_count"], 5);
    assert!(parsed["status"]["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers array")
        .iter()
        .any(|layer| layer["name"] == "external_knowledge"
            && layer["state"] == "ready"
            && layer["storage"] == "docs/external-knowledge-adapter.md"));
    assert!(parsed["status"]["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers array")
        .iter()
        .any(|layer| layer["name"] == "maintenance_loop"
            && layer["state"] == "ready"
            && layer["storage"] == "docs/memory-maintenance-loop.md"));
    assert_eq!(parsed["status"]["channel_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["channel_readiness"]["overall_state"],
        "ready"
    );
    assert_eq!(parsed["status"]["subagent_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["subagent_readiness"]["overall_state"],
        "queued_protocol_partial"
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["local_contract_ready"],
        true
    );
    assert!(
        parsed["status"]["subagent_readiness"]["local_contract_reason"]
            .as_str()
            .expect("subagent local contract reason")
            .contains("protocol-ready")
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["live_adapter_ready"],
        false
    );
    assert!(
        parsed["status"]["subagent_readiness"]["live_adapter_reason"]
            .as_str()
            .expect("subagent live adapter reason")
            .contains("not yet connected")
    );
    assert_eq!(parsed["status"]["subagent_readiness"]["layer_count"], 5);
    assert!(parsed["status"]["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "multi_worker"
            && layer["state"] == "ready"
            && layer["live_adapter_reason"]
                .as_str()
                .expect("multi-worker live adapter reason")
                .contains("external worker pools remain deferred")));
    assert!(parsed["status"]["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "external_ai_downstream" && layer["state"] == "ready"));
    assert_eq!(parsed["status"]["external_ai_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["external_ai_readiness"]["overall_state"],
        "ready"
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
