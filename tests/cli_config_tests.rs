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
	subagent_live_worker_enabled = "true"
	subagent_live_worker_adapter_kind = "command"
	subagent_live_worker_status = "configured_status_only"
	subagent_queue_root = "{queue}"
context_max_tokens = 333
context_reserve_system_tokens = 128
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
    assert_eq!(parsed["summary"]["subagent_live_worker"]["enabled"], true);
    assert_eq!(
        parsed["summary"]["subagent_live_worker"]["available"],
        false
    );
    assert_eq!(
        parsed["summary"]["subagent_live_worker"]["starts_worker"],
        false
    );
    assert_eq!(parsed["summary"]["context_max_tokens"], 333);
    assert_eq!(parsed["summary"]["context_reserve_system_tokens"], 128);
    assert!(parsed["summary"]["placeholder_warnings"]
        .as_array()
        .expect("placeholder warnings should be an array")
        .iter()
        .any(|warning| warning
            .as_str()
            .expect("warning should be string")
            .contains("provider=fake")));
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
provider_timeout_ms = 12345
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
    assert_eq!(parsed["summary"]["provider_request_timeout_ms"], 12_345);
    assert!(!stdout.contains("test-secret-key"));
}

#[test]
fn cli_config_check_rejects_subagent_live_worker_starting_workers() {
    let root = temp_root("live-worker-starts-worker");
    fs::create_dir_all(&root).expect("temp root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{db}"
provider = "fake"

[subagent_live_worker]
enabled = "true"
adapter_kind = "command"
status = "configured_status_only"
starts_worker = "true"
"#,
            db = root.join("chuang.db").display(),
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

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("config_invalid_value"));
    assert!(stderr.contains("subagent_live_worker.starts_worker"));
}

#[test]
fn cli_config_init_writes_default_config_without_overwriting() {
    let root = temp_root("init");
    fs::create_dir_all(&root).expect("temp root should be created");
    let config_path = root.join("config.toml");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "config",
            "init",
            "--path",
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
    assert_eq!(parsed["written"], true);
    assert_eq!(parsed["path"], config_path.display().to_string());

    let content = fs::read_to_string(&config_path).expect("config should exist");
    assert!(content.contains("provider = \"openai_compatible\""));
    // Public default is fail-closed: the subagent slot starts fake and only
    // enables queued_external dispatch after an operator explicitly opts in.
    assert!(content.contains("subagent = \"fake\""));
    assert!(content.contains("api_key_env = \"CHUANG_AGENT_API_KEY\""));

    let second = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "config",
            "init",
            "--path",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("config_init_refused"));
}
