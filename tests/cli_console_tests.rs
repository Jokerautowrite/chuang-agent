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

fn write_watchdog_report(root: &std::path::Path) -> PathBuf {
    let report_path = root.join("latest-watchdog-report.json");
    fs::write(
        &report_path,
        serde_json::json!({
            "schema_version": 1,
            "generated_at": "2026-05-07T10:00:00+08:00",
            "readonly": true,
            "project_root": "/home/user/projects/chuang-agent",
            "session": "chuang-goal",
            "tmux_session_present": true,
            "pane": {
                "bytes": 2048,
                "panes": ["pane=%1 active=1 pid=123 current_command=codex"]
            },
            "codex_processes": {
                "count": 2,
                "processes": ["123 1 00:01 codex --no-alt-screen"]
            },
            "git": {
                "dirty": true,
                "status_short": [" M src/cli_console.rs", " M tests/cli_console_tests.rs"]
            },
            "takeover": {
                "next_action": "review_git_status_and_diff",
                "attach_command": "tmux attach -t chuang-goal",
                "review_command": "git -C /home/user/projects/chuang-agent status --short"
            },
            "boundaries": {
                "dispatches_tasks": false,
                "modifies_repo": false,
                "restarts_worker": false,
                "touches_services": false
            }
        })
        .to_string(),
    )
    .expect("watchdog report should write");
    report_path
}

#[test]
fn cli_console_snapshot_outputs_dashboard_json_without_actions() {
    let root = temp_root("json");
    let config_path = write_config(&root);
    let watchdog_report = write_watchdog_report(&root);

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
        .env("CHUANG_GOAL_WATCHDOG_REPORT_FILE", &watchdog_report)
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
    assert_eq!(parsed["status"]["release_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["release_readiness"]["release_name"],
        "second_test_version"
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["overall_state"],
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["verifies_real_external_services"],
        false
    );
    assert!(parsed["status"]["release_readiness"]["acceptance"]
        .as_array()
        .expect("release acceptance should be array")
        .iter()
        .any(|item| item["name"] == "real_external_services" && item["state"] == "deferred"));
    assert_eq!(
        parsed["status"]["atomic_tools"]["mapped_atomic_tool_names"],
        serde_json::json!(["file_read", "file_write", "code_execute"])
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["interface_only_atomic_tool_names"],
        serde_json::json!([
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "wait",
            "human_suspend"
        ])
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["manifest_schema_version"],
        1
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_report_schema_version"],
        6
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_action_schema_version"],
        1
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
    assert_eq!(parsed["terminal_watchdog"]["available"], true);
    assert_eq!(
        parsed["terminal_watchdog"]["report_file"],
        watchdog_report.display().to_string()
    );
    assert_eq!(parsed["terminal_watchdog"]["readonly"], true);
    assert_eq!(parsed["terminal_watchdog"]["session"], "chuang-goal");
    assert_eq!(parsed["terminal_watchdog"]["tmux_session_present"], true);
    assert_eq!(parsed["terminal_watchdog"]["codex_process_count"], 2);
    assert_eq!(parsed["terminal_watchdog"]["git_dirty"], true);
    assert_eq!(parsed["terminal_watchdog"]["git_status_count"], 2);
    assert_eq!(
        parsed["terminal_watchdog"]["next_action"],
        "review_git_status_and_diff"
    );
    assert_eq!(
        parsed["terminal_watchdog"]["attach_command"],
        "tmux attach -t chuang-goal"
    );
    assert_eq!(parsed["terminal_watchdog"]["dispatches_tasks"], false);
    assert_eq!(parsed["terminal_watchdog"]["modifies_repo"], false);
    assert_eq!(parsed["terminal_watchdog"]["restarts_worker"], false);
    assert_eq!(parsed["terminal_watchdog"]["touches_services"], false);
}

#[test]
fn cli_console_snapshot_outputs_compact_text_summary() {
    let root = temp_root("text");
    let config_path = write_config(&root);
    let missing_watchdog_report = root.join("missing-watchdog-report.json");

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
        .env("CHUANG_GOAL_WATCHDOG_REPORT_FILE", &missing_watchdog_report)
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
        "atomic_tools: ok=true total=9 mapped=3 interface_only=6 action_schema_version=1 report_schema_version=6"
    ));
    assert!(stdout.contains("atomic_tools_mapped: file_read,file_write,code_execute"));
    assert!(stdout.contains(
        "atomic_tools_interface_only: mouse,keyboard,screenshot,locate,wait,human_suspend"
    ));
    assert!(stdout.contains("project_readiness: ok=true state=mvp_ready_with_partial_modules"));
    assert!(stdout.contains("channel_readiness: ok=true state=ready"));
    assert!(stdout.contains("subagent_readiness: ok=true state=queued_protocol_partial"));
    assert!(stdout.contains("external_ai_readiness: ok=true state=ready"));
    assert!(stdout.contains(
        "release_readiness: ok=true name=second_test_version state=second_test_version_ready"
    ));
    assert!(stdout.contains(
        "release_acceptance: count=7 connects_real_external_services=false verifies_real_external_services=false uses_stub_or_local_fixtures=true"
    ));
    assert!(stdout.contains("control_units: "));
    assert!(stdout.contains("plugins: 5"));
    assert!(stdout.contains(
        "terminal_watchdog: available=false readonly=true session=unknown tmux_session_present=unknown codex_process_count=unknown git_dirty=unknown next_action=run_watchdog_once_before_console_review"
    ));
    assert!(stdout.contains("plugin_registry: available=true ok=true plugin_count=5"));
}
