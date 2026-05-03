use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-agent-provider-stub-{nanos}"));
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
fn cli_run_with_provider_surfaces_stub_post_metadata() {
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
