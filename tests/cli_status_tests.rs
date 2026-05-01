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
    assert!(stdout.contains("provider_slot: fake"));
    assert!(stdout.contains("model: stub-responder"));
    assert!(stdout.contains("identity_memory: hermes_dual_file"));
    assert!(stdout.contains("identity_memory_limits: user=1375 memory=2200"));
    assert!(stdout.contains("identity_snapshot_chars: user=0 memory=0"));
    assert!(stdout.contains("governance: static_rule"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
    assert!(stdout.contains("subagent_queue_root: ./data/subagent-queue"));
    assert!(stdout.contains(
        "context_budget: max=512 reserve_system=32 min_working=1 max_tool_results=5 max_memory_segments=5"
    ));
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

#[test]
fn cli_status_can_override_context_budget_fields() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--context-max-tokens",
            "256",
            "--context-reserve-system-tokens",
            "64",
            "--context-min-working-tokens",
            "8",
            "--context-max-tool-results",
            "2",
            "--context-max-memory-segments",
            "3",
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

    assert_eq!(parsed["config"]["context_max_tokens"], 256);
    assert_eq!(
        parsed["config"]["context_engine_kind"],
        "deterministic_budget"
    );
    assert_eq!(parsed["config"]["context_reserve_system_tokens"], 64);
    assert_eq!(parsed["config"]["context_min_working_tokens"], 8);
    assert_eq!(parsed["config"]["context_max_tool_results"], 2);
    assert_eq!(parsed["config"]["context_max_memory_segments"], 3);
    assert_eq!(parsed["kernel"]["context_budget_max_tokens"], 256);
}

#[test]
fn cli_status_can_select_summary_compression_context_engine() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--context-engine",
            "summary_compression",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(
        parsed["config"]["context_engine_kind"],
        "summary_compression"
    );
}

#[test]
fn cli_status_can_load_simple_config_file_and_accept_cli_overrides() {
    let root = temp_identity_root("config");
    let identity_root = root.join("identity");
    let queue_root = root.join("queue");
    let config_path = root.join("config.toml");
    fs::create_dir_all(&identity_root).expect("identity root should be created");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{db}"
recall_limit = 9
identity_memory_root = "{identity}"
subagent = "queued_external"
subagent_queue_root = "{queue}"

[provider]
kind = "fake"
id = "config-fake"
model = "config-stub"

[context]
max_tokens = 384
reserve_system_tokens = 48
min_working_tokens = 3
max_tool_results = 4
max_memory_segments = 6
"#,
            db = root.join("chuang.db").display(),
            identity = identity_root.display(),
            queue = queue_root.display()
        ),
    )
    .expect("config should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--context-max-tokens",
            "256",
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

    assert_eq!(parsed["config"]["provider_id"], "config-fake");
    assert_eq!(parsed["config"]["model_name"], "config-stub");
    assert_eq!(parsed["config"]["recall_limit"], 9);
    assert_eq!(
        parsed["config"]["subagent_queue_root"],
        queue_root.display().to_string()
    );
    assert_eq!(parsed["config"]["subagent_kind"], "queued_external");
    assert_eq!(parsed["config"]["context_max_tokens"], 256);
    assert_eq!(parsed["config"]["context_reserve_system_tokens"], 48);
}
