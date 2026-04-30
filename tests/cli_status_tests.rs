use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_identity_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-{name}-{nanos}"))
}

#[test]
fn cli_status_prints_mvp_health_summary() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "status"])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("kernel_agent_id: chuang-cli"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("model: stub-responder"));
    assert!(stdout.contains("identity_memory: hermes_dual_file"));
    assert!(stdout.contains("identity_memory_limits: user=1375 memory=2200"));
    assert!(stdout.contains("identity_snapshot_chars: user=0 memory=0"));
    assert!(stdout.contains("governance: static_rule"));
    assert!(stdout.contains("subagent_queue_root: ./data/subagent-queue"));
    assert!(stdout.contains("control_plane: fake_local"));
}

#[test]
fn cli_status_can_render_json_without_secret_leak() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-secret-key",
            "--provider-model",
            "gpt-5.5",
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
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["kernel"]["agent_id"], "chuang-cli");
    assert_eq!(parsed["config"]["provider_kind"], "openai_compatible");
    assert_eq!(parsed["config"]["provider_id"], "custom-openai");
    assert_eq!(parsed["config"]["identity_memory_kind"], "hermes_dual_file");
    assert_eq!(parsed["config"]["api_key_state"], "<set>");
    assert!(!stdout.contains("test-secret-key"));
}

#[test]
fn cli_status_can_use_custom_identity_memory_root() {
    let root = temp_identity_root("identity-root");
    fs::create_dir_all(&root).expect("root should be created");
    fs::write(root.join("USER.md"), "老爸偏好简洁中文汇报").expect("user memory should be seeded");
    fs::write(root.join("MEMORY.md"), "## mem-1\n创项目聚焦核心 MVP")
        .expect("hot memory should be seeded");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
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

    assert_eq!(
        parsed["config"]["identity_memory_root"],
        root.display().to_string()
    );
    assert_eq!(
        parsed["kernel"]["identity_user_chars"].as_u64(),
        Some("老爸偏好简洁中文汇报".chars().count() as u64)
    );
    assert_eq!(
        parsed["kernel"]["identity_memory_chars"].as_u64(),
        Some("## mem-1\n创项目聚焦核心 MVP".chars().count() as u64)
    );
}

#[test]
fn cli_status_can_select_queued_external_subagent_slot() {
    let queue_root = temp_identity_root("subagent-queue");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--subagent",
            "queued_external",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
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

    assert_eq!(parsed["config"]["subagent_kind"], "queued_external");
    assert_eq!(
        parsed["config"]["subagent_queue_root"],
        queue_root.display().to_string()
    );
    assert_eq!(parsed["slots"]["subagent"], "queued_external");
}
