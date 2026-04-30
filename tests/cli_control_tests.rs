use std::process::Command;

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
}
