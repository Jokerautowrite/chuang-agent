use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-config-{name}-{nanos}"))
}

#[test]
fn cli_config_check_accepts_flat_config_file() {
    let root = temp_root("check");
    fs::create_dir_all(&root).expect("temp root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{db}"
identity_memory_root = "{identity}"
provider = "fake"
provider_id = "config-check-fake"
model = "config-check-stub"
subagent = "queued_external"
subagent_queue_root = "{queue}"
context_max_tokens = 333
"#,
            db = root.join("chuang.db").display(),
            identity = root.join("identity").display(),
            queue = root.join("queue").display()
        ),
    )
    .expect("config should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "config",
            "check",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
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
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["source"], config_path.display().to_string());
    assert_eq!(parsed["summary"]["provider_id"], "config-check-fake");
    assert_eq!(parsed["summary"]["model_name"], "config-check-stub");
    assert_eq!(parsed["summary"]["subagent_kind"], "queued_external");
    assert_eq!(parsed["summary"]["context_max_tokens"], 333);
}

#[test]
fn cli_config_show_masks_provider_key_state() {
    let root = temp_root("show");
    fs::create_dir_all(&root).expect("temp root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{db}"
identity_memory_root = "{identity}"
provider = "openai_compatible"
provider_id = "masked-provider"
base_url = "http://127.0.0.1:8000/v1"
model = "gpt-test"
api_key_env = "CHUANG_AGENT_CLI_CONFIG_TEST_KEY"
transport = "stub"
"#,
            db = root.join("chuang.db").display(),
            identity = root.join("identity").display()
        ),
    )
    .expect("config should write");
    std::env::set_var("CHUANG_AGENT_CLI_CONFIG_TEST_KEY", "test-secret-key");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "config",
            "show",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    std::env::remove_var("CHUANG_AGENT_CLI_CONFIG_TEST_KEY");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["summary"]["provider_kind"], "openai_compatible");
    assert_eq!(parsed["summary"]["api_key_state"], "<set>");
    assert!(!stdout.contains("test-secret-key"));
}
