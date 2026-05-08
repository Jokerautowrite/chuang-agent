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
    write_watchdog_report_with_generated_at(
        root,
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
}

fn write_watchdog_report_with_generated_at(root: &std::path::Path, generated_at: &str) -> PathBuf {
    let report_path = root.join("latest-watchdog-report.json");
    fs::write(
        &report_path,
        serde_json::json!({
            "schema_version": 1,
            "generated_at": generated_at,
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
    assert_eq!(parsed["status"]["third_test_candidate"]["ok"], true);
    assert_eq!(
        parsed["status"]["third_test_candidate"]["overall_state"],
        "local_gate_ready_requires_manual_live_check"
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["local_gate_ready"],
        true
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["smoke_script"],
        "scripts/chuang-third-test-smoke.sh"
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["marker"],
        "third_test_candidate_smoke_ok"
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["operator_env_blocks_100_percent"],
        true
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["real_live_ready"],
        false
    );
    assert_eq!(parsed["status"]["local_contract_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["overall_state"],
        "ready"
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["contract_count"],
        6
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["writes_core_memory"],
        false
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["executes_plugins"],
        false
    );
    assert_eq!(
        parsed["status"]["memory_maintenance_receipt"]["available"],
        true
    );
    assert_eq!(
        parsed["status"]["memory_maintenance_receipt"]["readable"],
        true
    );
    assert_eq!(
        parsed["status"]["memory_maintenance_receipt"]["state"],
        "missing"
    );
    assert_eq!(
        parsed["status"]["memory_maintenance_receipt"]["receipt_count"],
        0
    );
    assert_eq!(
        parsed["status"]["memory_maintenance_receipt"]["latest_entry_id"],
        Value::Null
    );
    assert_eq!(
        parsed["status"]["memory_maintenance_receipt"]["error"],
        Value::Null
    );
    assert_eq!(parsed["status"]["goal_mode"]["ok"], true);
    assert_eq!(
        parsed["status"]["goal_mode"]["cli_entrypoint"],
        "run --goal TEXT"
    );
    assert_eq!(parsed["status"]["goal_run"]["ok"], true);
    assert_eq!(parsed["status"]["goal_run"]["goal_id"], "mainline-mvp");
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
        parsed["status"]["plugin_registry"]["evidence_available"],
        true
    );
    assert_eq!(parsed["status"]["plugin_registry"]["check_only"], true);
    assert_eq!(
        parsed["status"]["plugin_registry"]["executes_plugins"],
        false
    );
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
    assert_eq!(parsed["terminal_watchdog"]["readable"], true);
    assert_eq!(parsed["terminal_watchdog"]["fresh"], true);
    assert_eq!(parsed["terminal_watchdog"]["diagnostic_status"], "fresh");
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
    assert_eq!(parsed["app_server_health"]["diagnostic_status"], "warning");
    assert!(parsed["app_server_health"]["diagnostic_summary"]
        .as_str()
        .expect("app server diagnostic summary")
        .contains("local warning"));
    assert!(parsed["app_server_health"]["next_actions"]
        .as_array()
        .expect("app server next actions")
        .iter()
        .any(|action| action
            .as_str()
            .expect("next action")
            .contains("configure an openai_compatible provider")));
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
    assert!(stdout.contains(
        "provider_readiness: ok=true state=ready kind=fake transport=fake fallback_configured=false timeout_ms=none api_key_state=none"
    ));
    assert!(stdout.contains("atomic_tools_mapped: file_read,file_write,code_execute"));
    assert!(stdout.contains(
        "atomic_tools_interface_only: mouse,keyboard,screenshot,locate,wait,human_suspend"
    ));
    assert!(stdout.contains("project_readiness: ok=true state=mvp_ready_with_partial_modules"));
    assert!(stdout.contains("channel_readiness: ok=true state=ready"));
    assert!(stdout.contains(
        "subagent_readiness: ok=true state=queued_protocol_partial mode=fake live_worker_available=false worker_runtime_state=local_contract_only"
    ));
    assert!(stdout.contains("subagent_worker_runtime_reason: subagent slot is fake"));
    assert!(stdout.contains("external_ai_readiness: ok=true state=ready"));
    assert!(stdout.contains(
        "goal_mode: ok=true kind=lightweight_runtime_context cli_entrypoint=run --goal TEXT context_source=goal default_goal_id=mainline-mvp allowed_slots=context,governance,execution,report,memory checkpoint_policy=progress_log:true handoff:true commit:true final_report_policy=validation:true next_steps:true bypasses_governance=false adds_core_slot=false"
    ));
    assert!(stdout.contains("goal_run: ok=true"));
    assert!(stdout.contains("goal_id=mainline-mvp"));
    assert!(stdout.contains("goal_run_checkpoint_log_complete:"));
    assert!(stdout.contains("goal_run_last_checkpoint:"));
    assert!(stdout.contains("goal_run_last_checkpoint_summary:"));
    assert!(stdout.contains("goal_run_last_checkpoint_created_at:"));
    assert!(stdout.contains("goal_run_last_checkpoint_completed_worker_ids:"));
    assert!(stdout.contains("goal_run_last_checkpoint_validation_notes:"));
    assert!(stdout.contains("goal_run_incomplete_reasons:"));
    assert!(stdout.contains(
        "local_contract_readiness: ok=true state=ready contracts=6 connects_real_external_services=false writes_core_memory=false executes_plugins=false"
    ));
    assert!(stdout.contains(
        "memory_maintenance_receipt: available=true readable=true state=missing receipts=0 latest_entry_id=none latest_source_record_id=none latest_approval_source=none latest_approved_at=none latest_provenance_preserved=false"
    ));
    assert!(stdout.contains(
        "release_readiness: ok=true name=second_test_version state=second_test_version_ready"
    ));
    assert!(stdout.contains(
        "release_acceptance: count=7 connects_real_external_services=false verifies_real_external_services=false uses_stub_or_local_fixtures=true"
    ));
    assert!(stdout.contains(
        "third_test_candidate: ok=true state=local_gate_ready_requires_manual_live_check local_gate_ready=true smoke_script=scripts/chuang-third-test-smoke.sh marker=third_test_candidate_smoke_ok requires_manual_live_check=true connects_real_external_services=false operator_env_blocks_100_percent=true real_live_ready=false"
    ));
    assert!(stdout.contains("control_units: "));
    assert!(stdout.contains("plugins: 5"));
    assert!(stdout.contains("terminal_watchdog: available=false readable=false fresh=false diagnostic_status=missing readonly=true session=unknown tmux_session_present=unknown codex_process_count=unknown git_dirty=unknown next_action=run_watchdog_once_before_console_review"));
    assert!(stdout.contains("app_server_health: status=warning"));
    assert!(stdout.contains("configure an openai_compatible provider"));
    assert!(stdout.contains("plugin_registry: available=true ok=true plugin_count=5"));
}

#[test]
fn cli_console_snapshot_renders_memory_maintenance_receipt_summary() {
    let root = temp_root("receipt-text");
    let config_path = write_config(&root);
    fs::write(
        root.join("identity").join("experiences.md"),
        r#"## lim-candidate-42
writeback=memory_maintenance_apply
approved_writeback=true
approval_source=cli --approve-writeback
approved_at=2026-05-07T12:34:56Z
approval_note=老爸批准写入 LIM 候选
provenance_preserved=true
source=lim_dry_run
source_record_id=turn-42
created_at=2026-05-07T12:00:00Z
lesson=先把回执面露出来
"#,
    )
    .expect("receipt experiences should be written");

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

    assert!(stdout.contains(
        "memory_maintenance_receipt: available=true readable=true state=ready receipts=1 latest_entry_id=lim-candidate-42 latest_source_record_id=turn-42 latest_approval_source=cli --approve-writeback latest_approved_at=2026-05-07T12:34:56Z latest_provenance_preserved=true"
    ));
}

#[test]
fn cli_console_snapshot_diagnoses_invalid_watchdog_report_json() {
    let root = temp_root("invalid-watchdog");
    let config_path = write_config(&root);
    let watchdog_report = root.join("latest-watchdog-report.json");
    fs::write(&watchdog_report, "{not-json").expect("invalid watchdog report should write");

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

    assert_eq!(parsed["terminal_watchdog"]["available"], true);
    assert_eq!(parsed["terminal_watchdog"]["readable"], true);
    assert_eq!(parsed["terminal_watchdog"]["fresh"], false);
    assert_eq!(parsed["terminal_watchdog"]["diagnostic_status"], "invalid");
    assert_eq!(parsed["terminal_watchdog"]["error"], "report_parse_failed");
    assert_eq!(
        parsed["terminal_watchdog"]["next_action"],
        "inspect_or_regenerate_watchdog_report"
    );
}

#[test]
fn cli_console_snapshot_diagnoses_stale_watchdog_report() {
    let root = temp_root("stale-watchdog");
    let config_path = write_config(&root);
    let watchdog_report = write_watchdog_report_with_generated_at(&root, "2026-01-01T00:00:00Z");

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
        .env("CHUANG_GOAL_WATCHDOG_STALE_SECONDS", "60")
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["terminal_watchdog"]["available"], true);
    assert_eq!(parsed["terminal_watchdog"]["readable"], true);
    assert_eq!(parsed["terminal_watchdog"]["fresh"], false);
    assert_eq!(parsed["terminal_watchdog"]["diagnostic_status"], "stale");
    assert_eq!(parsed["terminal_watchdog"]["error"], "report_stale");
    assert_eq!(
        parsed["terminal_watchdog"]["next_action"],
        "run_watchdog_once_before_console_review"
    );
    assert_eq!(parsed["terminal_watchdog"]["stale_after_seconds"], 60);
}
