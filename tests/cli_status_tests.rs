use std::process::Command;

use serde_json::Value;

#[test]
fn cli_status_prints_mvp_health_summary() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "status"])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("kernel_agent_id: chuang-cli"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("model: stub-responder"));
    assert!(stdout.contains("governance: static_rule"));
    assert!(stdout.contains("control_plane: fake_local"));
}

#[test]
fn cli_status_can_render_json_without_secret_leak() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-secret-key",
            "--provider-model",
            "gpt-5.5",
            "--provider-id",
            "custom-openai",
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

    assert_eq!(parsed["kernel"]["agent_id"], "chuang-cli");
    assert_eq!(parsed["config"]["provider_kind"], "openai_compatible");
    assert_eq!(parsed["config"]["provider_id"], "custom-openai");
    assert_eq!(parsed["config"]["api_key_state"], "<set>");
    assert!(!stdout.contains("test-secret-key"));
}
