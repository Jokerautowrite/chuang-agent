use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-agent-provider-reject-{nanos}"));
    fs::create_dir_all(&root).expect("config root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("fake config should be written");
    config_path
}

#[test]
fn cli_run_http_transport_surfaces_preview_metadata() {
    let config_path = write_fake_config();
    let fake_api_key = "test-key";
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--input",
            "创项目继续推进 transport",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            fake_api_key,
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
        stdout.contains("provider_error_class: config"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("provider_failure_reason_code: provider_config_invalid"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("provider_failure_category: config"),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("unsupported_http_scheme:https://api.example.com/v1/chat/completions"),
        "stdout={stdout}"
    );
    assert!(stdout.contains("request_method: POST"), "stdout={stdout}");
    assert!(
        stdout.contains("request_message_count: 2"),
        "stdout={stdout}"
    );
    assert!(
        !stdout.contains(fake_api_key),
        "stdout should not leak provider api key, got: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains(fake_api_key),
        "stderr should not leak provider api key, got: {stderr}"
    );
}

#[test]
fn cli_run_http_transport_surfaces_invalid_port_preview_metadata() {
    let config_path = write_fake_config();
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
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
