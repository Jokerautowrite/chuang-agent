use std::process::Command;

#[test]
fn cli_run_accepts_provider_config_and_surfaces_provider_identity() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 provider",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
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
    assert!(stdout.contains("model_name: gpt-4.1-mini"));
    assert!(stdout.contains("provider: custom-openai"));
    assert!(stdout.contains("transport=openai-compatible"));
}

#[test]
fn cli_run_rejects_partial_provider_config() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 provider",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-model",
            "gpt-4.1-mini",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("provider config requires base_url + api_key + model"));
}
