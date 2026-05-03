use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("chuang-agent-provider-default-transport-{nanos}"));
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
fn cli_run_defaults_provider_transport_to_stub() {
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
            "创项目继续推进 default transport",
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
    assert!(stdout.contains("transport_mode: stub"), "stdout={stdout}");
}
