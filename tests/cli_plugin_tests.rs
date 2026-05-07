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
            && plugin["config_state"] == "exists"
            && plugin["capabilities"]
                .as_array()
                .expect("capabilities array")
                .iter()
                .any(|capability| capability == "service-control")
            && plugin["dry_run_default"] == true
            && plugin["executes_plugin"] == false
            && plugin["reads_secret"] == false
            && plugin["boundary"]["check_only"] == true
            && plugin["boundary"]["executes_plugin"] == false
            && plugin["boundary"]["reads_secret"] == false
            && plugin["boundary"]["connects_external_service"] == false
            && plugin["boundary"]["writes_files"] == false
            && plugin["readiness"]["state"] == "disabled"
            && plugin["readiness"]["blocking"] == false
            && plugin["readiness"]["reason"] == "plugin_disabled_manifest_only"
            && plugin["evidence"]["manifest_loaded"] == true
            && plugin["evidence"]["path_checks"]
                .as_array()
                .expect("path evidence array")
                .iter()
                .any(|path| path["field"] == "command"
                    && path["configured"] == true
                    && path["state"] == "exists")));
}

#[test]
fn cli_plugin_check_text_reports_readiness_and_boundary_evidence() {
    let registry = format!(
        "{}/plugins/registry.example.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args(["plugin", "check", "--registry", &registry])
        .output()
        .expect("plugin check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("plugin_registry_check ok=true"));
    assert!(stdout.contains("id=chuang-real-control"));
    assert!(stdout.contains("readiness=disabled"));
    assert!(stdout.contains("reason=plugin_disabled_manifest_only"));
    assert!(stdout.contains("capabilities=service-control,allowlist"));
    assert!(stdout.contains("dry_run_default=true"));
    assert!(stdout.contains("executes_plugin=false"));
    assert!(stdout.contains("reads_secret=false"));
    assert!(stdout.contains("boundary_check_only=true"));
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
    assert_eq!(parsed["plugins"][0]["readiness"]["state"], "disabled");
    assert_eq!(parsed["plugins"][0]["readiness"]["blocking"], false);
    assert_eq!(
        parsed["plugins"][0]["readiness"]["reason"],
        "plugin_disabled_manifest_only"
    );
    assert_eq!(parsed["plugins"][0]["executes_plugin"], false);
    assert_eq!(parsed["plugins"][0]["reads_secret"], false);
    assert_eq!(parsed["plugins"][0]["boundary"]["check_only"], true);
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
