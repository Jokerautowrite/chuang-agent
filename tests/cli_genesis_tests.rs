use std::process::Command;

#[test]
fn cli_genesis_ask_requires_explicit_execution_approval() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "genesis",
            "ask",
            "--prompt",
            "测试 Genesis",
            "--program",
            "printf",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("genesis_ask_requires_approve_exec"),
        "stderr={stderr}"
    );
}

#[test]
fn cli_genesis_ask_can_run_approved_program_and_render_json() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "genesis",
            "ask",
            "--prompt",
            "测试 Genesis",
            "--program",
            "printf",
            "--profile-dir",
            "/tmp/chuang-genesis-cli-profile",
            "--approve-exec",
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
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be json");
    assert_eq!(parsed["response"]["channel"], "UserDataDir");
    assert_eq!(parsed["response"]["answer"], "deepseek");
    assert_eq!(
        parsed["response"]["primary_repair"],
        serde_json::Value::Null
    );
    assert_eq!(parsed["audit_recorded"], true);
    assert!(parsed["governance_decision"]
        .as_str()
        .expect("decision should be string")
        .starts_with("needs_approval:"));
}
