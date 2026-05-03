use std::process::Command;

use serde_json::Value;

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
