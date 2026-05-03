use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-identity-memory-{name}-{nanos}"))
}

fn write_fake_config(root: &std::path::Path) -> PathBuf {
    std::fs::create_dir_all(root).expect("config root should be created");
    let config_path = root.join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity-default").display()
        ),
    )
    .expect("fake config should be written");
    config_path
}

#[test]
fn cli_identity_memory_show_append_and_compact_memory() {
    let root = temp_root("flow");
    let config_path = write_fake_config(&root);

    let append = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "append",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--id",
            "mem-1",
            "--content",
            "老爸偏好简洁中文进度",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        append.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&append.stderr)
    );

    let shown = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "show",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        shown.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&shown.stdout)).expect("stdout json");
    assert_eq!(parsed["user_chars"], 0);
    assert_eq!(parsed["user_max_chars"], 1375);
    assert_eq!(parsed["memory_max_chars"], 2200);
    assert!(parsed["memory"]
        .as_str()
        .expect("memory string")
        .contains("## mem-1"));

    let compact = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "write-memory",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--content",
            "## compact-1\n老爸偏好简洁中文进度\n",
            "--approve-overwrite",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        compact.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&compact.stderr)
    );
    let compacted: Value =
        serde_json::from_str(&String::from_utf8_lossy(&compact.stdout)).expect("stdout json");
    assert_eq!(compacted["scope"], "memory");
    assert_eq!(compacted["replaced"], true);

    let memory = std::fs::read_to_string(root.join("MEMORY.md")).expect("memory file");
    assert_eq!(memory, "## compact-1\n老爸偏好简洁中文进度\n");
}

#[test]
fn cli_identity_memory_write_memory_rejects_over_limit_without_mutation() {
    let root = temp_root("write-memory-limit");
    let config_path = write_fake_config(&root);
    std::fs::write(root.join("MEMORY.md"), "## seed-1\nabc\n").expect("memory seed should write");
    let before = std::fs::read_to_string(root.join("MEMORY.md")).expect("memory file");
    let oversized_content = format!("## over-limit\n{}\n", "0".repeat(2300));

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "write-memory",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--content",
            oversized_content.as_str(),
            "--approve-overwrite",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("identity_memory_hard_limit_exceeded"));
    assert_eq!(
        std::fs::read_to_string(root.join("MEMORY.md")).expect("memory file"),
        before
    );
}

#[test]
fn cli_identity_memory_write_requires_explicit_overwrite_approval() {
    let root = temp_root("approval");
    let config_path = write_fake_config(&root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "write-user",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--content",
            "老爸",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("identity_memory_write_requires_approve_overwrite"));
}
