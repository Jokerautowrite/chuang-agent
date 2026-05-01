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
    std::env::temp_dir().join(format!("chuang-agent-doctor-test-{name}-{nanos}"))
}

#[test]
fn cli_doctor_reports_mvp_health_in_text() {
    let root = temp_root("text");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--db",
            root.join("memory.db")
                .to_str()
                .expect("db path should be utf8"),
            "--identity-memory-root",
            root.join("identity")
                .to_str()
                .expect("identity path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("doctor_ok: true"));
    assert!(stdout.contains("doctor_check name=config ok=true"));
    assert!(stdout.contains("doctor_check name=identity_memory ok=true"));
    assert!(stdout.contains("doctor_check name=slots ok=true"));
    assert!(stdout.contains("doctor_check name=runtime_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=subagent_queue_smoke ok=true"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
}

#[test]
fn cli_doctor_can_render_json_without_secret_leak() {
    let root = temp_root("json");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--json",
            "--db",
            root.join("memory.db")
                .to_str()
                .expect("db path should be utf8"),
            "--identity-memory-root",
            root.join("identity")
                .to_str()
                .expect("identity path should be utf8"),
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "doctor-secret-key",
            "--provider-model",
            "gpt-5.5",
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
    assert_eq!(parsed["checks"].as_array().expect("checks array").len(), 5);
    assert_eq!(
        parsed["status"]["config"]["provider_kind"],
        "openai_compatible"
    );
    assert_eq!(parsed["status"]["config"]["api_key_state"], "<set>");
    assert!(!stdout.contains("doctor-secret-key"));
}
