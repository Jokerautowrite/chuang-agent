use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::plugin_registry::{
    check_plugin_registry, load_plugin_registry, summarize_plugin_registry, PluginKind,
};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-plugin-registry-{name}-{nanos}"))
}

#[test]
fn plugin_registry_loads_checked_in_example() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/registry.example.json");
    let registry = load_plugin_registry(&path).expect("registry should load");

    assert!(registry
        .plugins
        .iter()
        .any(|plugin| plugin.id == "chuang-codex-runner"
            && plugin.kind == PluginKind::SubagentRunner));
}

#[test]
fn plugin_registry_summary_counts_enabled_plugins_and_issues() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/registry.example.json");
    let summary = summarize_plugin_registry(&path);

    assert!(summary.available);
    assert!(summary.ok);
    assert_eq!(summary.plugin_count, 5);
    assert_eq!(summary.enabled_count, 0);
    assert_eq!(summary.issue_count, 0);
}

#[test]
fn plugin_registry_summary_tolerates_missing_registry() {
    let root = temp_root("missing-summary");
    let summary = summarize_plugin_registry(&root.join("missing.json"));

    assert!(!summary.available);
    assert!(!summary.ok);
    assert_eq!(summary.plugin_count, 0);
    assert_eq!(summary.issue_count, 0);
}

#[test]
fn plugin_registry_check_reports_missing_command_without_executing() {
    let root = temp_root("missing-command");
    fs::create_dir_all(&root).expect("root should create");
    let registry_path = root.join("registry.json");
    fs::write(
        &registry_path,
        r#"{
  "plugins": [{
    "id": "missing",
    "kind": "control_adapter",
    "display_name": "Missing",
    "command": "./missing.sh",
    "config_path": null,
    "enabled": true
  }]
}"#,
    )
    .expect("registry should write");

    let check = check_plugin_registry(&registry_path).expect("check should run");

    assert!(!check.ok);
    assert_eq!(check.plugins[0].command_state, "missing");
    assert_eq!(check.plugins[0].capabilities, Vec::<String>::new());
    assert!(!check.plugins[0].dry_run_default);
    assert!(!check.plugins[0].executes_plugin);
    assert!(!check.plugins[0].reads_secret);
    assert_eq!(check.plugins[0].readiness.state, "blocked");
    assert!(check.plugins[0].readiness.blocking);
    assert_eq!(
        check.plugins[0].readiness.reason,
        "enabled_manifest_has_issues"
    );
    assert!(check.plugins[0].boundary.check_only);
    assert!(!check.plugins[0].boundary.executes_plugin);
    assert!(!check.plugins[0].boundary.reads_secret);
    assert!(!check.plugins[0].boundary.connects_external_service);
    assert!(!check.plugins[0].boundary.writes_files);
    assert!(check.plugins[0].evidence.manifest_loaded);
    assert!(check.plugins[0]
        .evidence
        .manifest_fields_checked
        .contains(&"capabilities".to_string()));
    assert!(check.plugins[0].evidence.path_checks.iter().any(|path| {
        path.field == "command"
            && path.configured
            && path.state == "missing"
            && path
                .resolved_path
                .as_deref()
                .expect("resolved command path")
                .ends_with("missing.sh")
    }));
    assert!(check.plugins[0]
        .evidence
        .path_checks
        .iter()
        .any(|path| { path.field == "config_path" && !path.configured && path.state == "none" }));
    assert!(check.plugins[0].issues[0].starts_with("command_missing:"));
}

#[test]
fn plugin_registry_check_keeps_disabled_manifest_issues_non_blocking() {
    let root = temp_root("disabled-missing");
    fs::create_dir_all(&root).expect("root should create");
    let registry_path = root.join("registry.json");
    fs::write(
        &registry_path,
        r#"{
  "plugins": [{
    "id": "disabled-missing",
    "kind": "actuator_adapter",
    "display_name": "Disabled Missing",
    "command": "./missing-actuator.sh",
    "config_path": "./missing-actuator.json",
    "enabled": false
  }]
}"#,
    )
    .expect("registry should write");

    let check = check_plugin_registry(&registry_path).expect("check should run");
    let summary = summarize_plugin_registry(&registry_path);

    assert!(check.ok);
    assert_eq!(check.plugins[0].command_state, "missing");
    assert_eq!(check.plugins[0].config_state, "missing");
    assert_eq!(check.plugins[0].readiness.state, "disabled");
    assert!(!check.plugins[0].readiness.blocking);
    assert_eq!(
        check.plugins[0].readiness.reason,
        "plugin_disabled_manifest_only"
    );
    assert!(!check.plugins[0].executes_plugin);
    assert!(!check.plugins[0].reads_secret);
    assert!(check.plugins[0].boundary.check_only);
    assert!(check.plugins[0]
        .issues
        .iter()
        .any(|issue| issue.starts_with("command_missing:")));
    assert!(check.plugins[0]
        .issues
        .iter()
        .any(|issue| issue.starts_with("config_path_missing:")));
    assert!(summary.ok);
    assert_eq!(summary.enabled_count, 0);
    assert_eq!(summary.issue_count, 0);
}

#[test]
fn plugin_registry_rejects_duplicate_ids() {
    let root = temp_root("duplicate");
    fs::create_dir_all(&root).expect("root should create");
    let registry_path = root.join("registry.json");
    fs::write(
        &registry_path,
        r#"{
  "plugins": [
    {"id":"dup","kind":"other","display_name":"A","enabled":false},
    {"id":"dup","kind":"other","display_name":"B","enabled":false}
  ]
}"#,
    )
    .expect("registry should write");

    let err = load_plugin_registry(&registry_path).expect_err("duplicate should fail");

    assert!(err.contains("plugin_registry_duplicate_id: dup"));
}
