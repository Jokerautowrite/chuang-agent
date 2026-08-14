use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_config() -> (std::path::PathBuf, std::path::PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-agent-external-ai-{nanos}"));
    std::fs::create_dir_all(&root).expect("temp root should create");
    let config_path = root.join("config.toml");
    std::fs::write(
        &config_path,
        r#"
provider = "openai_compatible"
provider_id = "external-ai-test-openai"
base_url = "https://api.example.com/v1"
model = "gpt-external-ai-test"
api_key_env = "CHUANG_AGENT_EXTERNAL_AI_TEST_API_KEY"
transport = "stub"
"#,
    )
    .expect("config should write");
    (root, config_path)
}

#[test]
fn cli_external_ai_dispatch_outputs_dry_run_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "external-ai",
            "dispatch",
            "--platform",
            "kimi",
            "--task",
            "collect architecture concerns",
            "--context",
            "bounded context only",
            "--session-hint",
            "session-1",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("external-ai dispatch should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");
    assert_eq!(parsed["adapter"], "unified_identity_engine");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_memory"], false);
    assert_eq!(parsed["request"]["platform"], "kimi");
    assert_eq!(parsed["request"]["session_hint"], "session-1");
    assert_eq!(parsed["result"]["quality"], "acceptable");
    assert!(parsed["result"]["audit_id"]
        .as_str()
        .expect("audit id")
        .starts_with("external-ai-kimi-"));
}

#[test]
fn cli_external_ai_dispatch_rejects_unsupported_live_platform() {
    let (_root, config_path) = temp_config();
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "external-ai",
            "dispatch",
            "--platform",
            "kimi",
            "--task",
            "collect architecture concerns",
            "--context",
            "bounded context only",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .env("CHUANG_AGENT_EXTERNAL_AI_TEST_API_KEY", "test-key")
        .output()
        .expect("external-ai dispatch should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("live platform must be opencodex or openai-compatible"));
}
