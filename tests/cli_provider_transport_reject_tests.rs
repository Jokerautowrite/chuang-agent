use std::process::Command;

#[test]
fn cli_run_http_transport_surfaces_preview_metadata() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 transport",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "http",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("transport_mode: http"), "stdout={stdout}");
    assert!(
        stdout.contains("config_error_field: base_url"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("unsupported_http_scheme:https://api.example.com/v1/chat/completions"),
        "stdout={stdout}"
    );
}

#[test]
fn cli_run_http_transport_surfaces_invalid_port_preview_metadata() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 invalid port",
            "--provider-base-url",
            "http://127.0.0.1:notaport/v1",
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "http",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("transport_mode: http"), "stdout={stdout}");
    assert!(
        stdout.contains("config_error_field: base_url"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("invalid_port:http://127.0.0.1:notaport/v1/chat/completions"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("request_url: http://127.0.0.1:notaport/v1/chat/completions"),
        "stdout={stdout}"
    );
}
