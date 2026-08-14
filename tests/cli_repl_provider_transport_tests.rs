use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-agent-repl-provider-{nanos}"));
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
fn cli_repl_accepts_provider_transport_flag() {
    let config_path = write_fake_config();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "repl",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-key",
            "--provider-model",
            "gpt-4.1-mini",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "stub",
            "--verbose",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("repl should execute");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all("continue transport\nexit\n".as_bytes())
        .expect("repl input should write");
    let output = child.wait_with_output().expect("repl should finish");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("transport_mode: stub"), "stdout={stdout}");
}
