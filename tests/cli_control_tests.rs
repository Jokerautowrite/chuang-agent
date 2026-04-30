use std::process::Command;

use serde_json::Value;

#[test]
fn cli_control_list_shows_default_local_agents() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "control", "list"])
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
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "control", "list", "--json"])
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
