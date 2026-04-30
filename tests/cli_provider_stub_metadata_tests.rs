use std::process::Command;

#[test]
fn cli_run_with_provider_surfaces_stub_post_metadata() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 provider stub post",
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
    assert!(
        stdout.contains("stub_status_code: 200"),
        "stdout should expose stub status metadata, got: {stdout}"
    );
    assert!(
        stdout.contains("stub_response_kind: chat.completion"),
        "stdout should expose stub response kind metadata, got: {stdout}"
    );
}
