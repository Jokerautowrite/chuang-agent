use std::process::Command;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-plugin-{name}-{nanos}"))
}

#[test]
fn cli_plugin_list_reads_checked_in_registry() {
    let registry = format!(
        "{}/plugins/registry.example.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args(["plugin", "list", "--registry", &registry, "--json"])
        .output()
        .expect("plugin list should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout json");

    assert_eq!(parsed["plugin_count"], 5);
    assert!(parsed["plugins"]
        .as_array()
        .expect("plugins array")
        .iter()
        .any(|plugin| plugin["id"] == "chuang-feishu-bridge"));
}

#[test]
fn cli_plugin_check_reports_registry_health() {
    let registry = format!(
        "{}/plugins/registry.example.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args(["plugin", "check", "--registry", &registry, "--json"])
        .output()
        .expect("plugin check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout json");

    assert_eq!(parsed["ok"], true);
    assert!(parsed["plugins"]
        .as_array()
        .expect("plugins array")
        .iter()
        .any(|plugin| plugin["id"] == "chuang-real-control"
            && plugin["command_state"] == "exists"
            && plugin["config_state"] == "exists"));
}

#[test]
fn cli_plugin_check_keeps_disabled_manifest_issues_non_blocking() {
    let root = temp_root("disabled-manifest");
    fs::create_dir_all(&root).expect("root should create");
    let registry_path = root.join("registry.json");
    fs::write(
        &registry_path,
        r#"{
  "plugins": [{
    "id": "disabled-manifest",
    "kind": "genesis_adapter",
    "display_name": "Disabled Manifest",
    "command": "./missing-genesis.sh",
    "config_path": "./missing-genesis.json",
    "enabled": false
  }]
}"#,
    )
    .expect("registry should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "plugin",
            "check",
            "--registry",
            registry_path.to_str().expect("registry path"),
            "--json",
        ])
        .output()
        .expect("plugin check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout json");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["plugins"][0]["enabled"], false);
    assert!(parsed["plugins"][0]["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .any(|issue| issue
            .as_str()
            .expect("issue string")
            .starts_with("command_missing:")));
    assert!(parsed["plugins"][0]["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .any(|issue| issue
            .as_str()
            .expect("issue string")
            .starts_with("config_path_missing:")));
}
