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
    std::env::temp_dir().join(format!("chuang-agent-cli-console-{name}-{nanos}"))
}

fn write_config(root: &std::path::Path) -> PathBuf {
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{db}"
identity_memory_root = "{identity}"
provider = "fake"
provider_id = "console-fake"
model = "console-stub"
control = "fake_local"
"#,
            db = root.join("memory.db").display(),
            identity = root.join("identity").display(),
        ),
    )
    .expect("config should write");
    config_path
}

#[test]
fn cli_console_snapshot_outputs_dashboard_json_without_actions() {
    let root = temp_root("json");
    let config_path = write_config(&root);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "console",
            "snapshot",
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
    assert_eq!(parsed["status"]["config"]["provider_id"], "console-fake");
    assert_eq!(parsed["status"]["slots"]["execution"], "generic_agent_mvp");
    assert_eq!(parsed["status"]["atomic_tools"]["ok"], true);
    assert_eq!(parsed["status"]["atomic_tools"]["total_count"], 9);
    assert_eq!(parsed["status"]["atomic_tools"]["mapped_count"], 3);
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_report_schema_version"],
        6
    );
    assert_eq!(parsed["status"]["plugin_registry"]["plugin_count"], 5);
    assert_eq!(
        parsed["plugins"]
            .as_array()
            .expect("plugins should be array")
            .len(),
        5
    );
    assert!(parsed["control_units"]
        .as_array()
        .expect("control units should be array")
        .iter()
        .any(|unit| unit["display_name"] == "小创"));
}

#[test]
fn cli_console_snapshot_outputs_compact_text_summary() {
    let root = temp_root("text");
    let config_path = write_config(&root);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "console",
            "snapshot",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("console_ok: true"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("execution: generic_agent_mvp"));
    assert!(stdout.contains(
        "atomic_tools: ok=true total=9 mapped=3 interface_only=6 report_schema_version=6"
    ));
    assert!(stdout.contains("control_units: "));
    assert!(stdout.contains("plugins: 5"));
    assert!(stdout.contains("plugin_registry: available=true ok=true plugin_count=5"));
}
