use std::process::Command;

#[test]
fn cli_run_http_transport_surfaces_https_scheme_error_with_request_preview() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--input",
            "创项目继续推进 http transport",
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
        stdout.contains("body: CONFIG_ERROR: openai-compatible provider invalid field=base_url reason=unsupported_http_scheme:https://api.example.com/v1/chat/completions"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("request_url: https://api.example.com/v1/chat/completions"),
        "stdout={stdout}"
    );
    assert!(stdout.contains("request_method: POST"), "stdout={stdout}");
    assert!(
        stdout.contains("request_message_count: 2"),
        "stdout={stdout}"
    );
}

#[test]
fn cli_run_http_transport_reports_invalid_port_shape() {
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
        stdout.contains("body: CONFIG_ERROR: openai-compatible provider invalid field=base_url reason=invalid_port:http://127.0.0.1:notaport/v1/chat/completions"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("request_url: http://127.0.0.1:notaport/v1/chat/completions"),
        "stdout={stdout}"
    );
    assert!(stdout.contains("request_method: POST"), "stdout={stdout}");
}
