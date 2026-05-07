use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config(root: &std::path::Path) -> PathBuf {
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("fake config should be written");
    config_path
}

#[test]
fn second_test_smoke_wrapper_reuses_safe_mvp_smoke() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-second-test-smoke.sh"))
        .expect("second-test smoke wrapper should be readable");
    let mvp_smoke = fs::read_to_string(manifest_dir.join("scripts/chuang-mvp-smoke.sh"))
        .expect("mvp smoke should be readable");

    assert!(wrapper.contains("CHUANG_SMOKE_NAME=second_test"));
    assert!(wrapper.contains("scripts/chuang-mvp-smoke.sh"));
    assert!(mvp_smoke.contains("smoke_name=\"${CHUANG_SMOKE_NAME:-mvp}\""));
    assert!(mvp_smoke.contains("printf '%s_smoke_ok work_dir=%s\\n' \"$smoke_name\" \"$work_dir\""));
    assert!(!wrapper.contains("rm "));
    assert!(!wrapper.contains("systemctl"));
}

#[test]
fn complete_local_smoke_wrapper_reuses_safe_local_acceptance() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-complete-local-smoke.sh"))
        .expect("complete local smoke wrapper should be readable");

    assert!(wrapper.contains("scripts/chuang-second-test-smoke.sh"));
    assert!(wrapper.contains("scripts/chuang-goal-watchdog.sh"));
    assert!(wrapper.contains("--once"));
    assert!(wrapper.contains("chuang-feishu-command-smoke.js"));
    assert!(wrapper.contains("chuang-feishu-session-smoke.js"));
    assert!(wrapper.contains("chuang-feishu-rich-message-smoke.js"));
    assert!(
        wrapper.contains("app-server health --workspace-root \"$work_dir\" --diagnostic --json")
    );
    assert!(wrapper.contains("console snapshot --config \"$config_path\" --json"));
    assert!(wrapper.contains("complete_local_smoke_ok"));
    assert!(wrapper.contains("transport = \"stub\""));
    assert!(wrapper.contains("CHUANG_AGENT_COMPLETE_SMOKE_API_KEY=\"test-key\""));
    assert!(wrapper.contains("connects_real_external_services"));
    assert!(wrapper.contains("verifies_real_external_services"));
    assert!(wrapper.contains("for gate in data[\"live_adapter_gates\"][\"gates\"]"));
    assert!(!wrapper.contains("rm "));
    assert!(!wrapper.contains("systemctl"));
    assert!(!wrapper.contains("https://liusuapi.top"));
    assert!(!wrapper.contains(".codex-im/.env"));
    assert!(!wrapper.contains("hermes-gateway"));
}

#[test]
fn final_verify_wrapper_requires_clean_tree_and_complete_local_smoke() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-final-verify.sh"))
        .expect("final verify wrapper should be readable");

    let clean_tree_check = wrapper
        .find("git status --short")
        .expect("final verify should check for a clean tree first");
    let complete_local_smoke = wrapper
        .find("sh scripts/chuang-complete-local-smoke.sh")
        .expect("final verify should run complete-local smoke");
    let final_diff_check = wrapper
        .find("git diff --check")
        .expect("final verify should run a final diff check");

    assert!(clean_tree_check < complete_local_smoke);
    assert!(complete_local_smoke < final_diff_check);
    assert!(wrapper.contains("working tree must be clean before final verify"));
    assert!(wrapper.contains("exit 2"));
    assert!(wrapper.contains("chuang_final_verify_ok"));
    assert!(!wrapper.contains("rm "));
    assert!(!wrapper.contains("reset"));
    assert!(!wrapper.contains("git checkout"));
    assert!(!wrapper.contains("systemctl"));
}

#[test]
fn goal_watchdog_once_writes_readonly_status_report() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let log_dir = std::env::temp_dir().join(format!(
        "chuang-goal-watchdog-smoke-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&log_dir).expect("watchdog log dir should be created");

    let output = Command::new("bash")
        .arg(manifest_dir.join("scripts/chuang-goal-watchdog.sh"))
        .arg("--once")
        .env("ROOT", &manifest_dir)
        .env("SESSION", format!("chuang-goal-missing-{nanos}"))
        .env("LOG_DIR", &log_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("watchdog should execute once");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = log_dir.join("latest-watchdog-report.json");
    let report = fs::read_to_string(&report_path).expect("watchdog report should be readable");
    let data: serde_json::Value =
        serde_json::from_str(&report).expect("watchdog report should be valid json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["readonly"], true);
    assert_eq!(data["project_root"], manifest_dir.display().to_string());
    assert_eq!(data["tmux_session_present"], false);
    assert_eq!(
        data["takeover"]["next_action"],
        "start_or_attach_worker_after_operator_review"
    );
    assert_eq!(data["boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["boundaries"]["modifies_repo"], false);
    assert_eq!(data["boundaries"]["restarts_worker"], false);
    assert_eq!(data["boundaries"]["touches_services"], false);
    assert!(data["git"]["status_short"].is_array());
    assert!(log_dir.join("watchdog.log").exists());
}

#[test]
fn overnight_runner_writes_structured_status_without_restart_or_cleanup() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runner = fs::read_to_string(manifest_dir.join("scripts/run-chuang-goal-overnight.sh"))
        .expect("overnight runner should be readable");

    assert!(runner.contains("STATUS_FILE=\"${STATUS_FILE:-$LOG_DIR/status.json}\""));
    assert!(runner.contains("write_status \"running\" \"start_first_iteration\""));
    assert!(runner.contains("write_status \"running\" \"invoke_codex_exec\""));
    assert!(runner.contains("write_status \"finished\" \"operator_review_status_and_logs\""));
    assert!(runner.contains("\"run_id\": os.environ[\"RUN_ID\"]"));
    assert!(runner.contains("\"iteration\": int(os.environ[\"ITERATION\"])"));
    assert!(runner.contains("\"deadline\": deadline_epoch"));
    assert!(runner.contains("\"last_iteration_exit_status\": last_status"));
    assert!(runner.contains("\"last_message_file\": os.environ[\"LAST_MESSAGE\"]"));
    assert!(runner.contains("\"jsonl_log\": os.environ[\"JSONL_LOG\"]"));
    assert!(runner.contains("\"plain_log\": os.environ[\"PLAIN_LOG\"]"));
    assert!(runner.contains("\"status\": os.environ[\"RUN_STATUS\"]"));
    assert!(runner.contains("\"next_action\": os.environ[\"NEXT_ACTION\"]"));
    assert!(runner.contains("CHUANG_OVERNIGHT_DRY_RUN"));
    assert!(runner.contains("CHUANG_OVERNIGHT_MAX_ITERATIONS"));
    assert!(runner.contains("\"restarts_codex\": False"));
    assert!(runner.contains("\"cleans_logs\": False"));
    assert!(runner.contains("\"touches_services\": False"));
    assert!(!runner.contains("systemctl"));
    assert!(!runner.contains("\nrm "));
    assert!(!runner.contains(" rm -"));
    assert!(!runner.contains("git reset"));
    assert!(!runner.contains("git checkout"));
}

#[test]
fn goal_run_status_script_reads_watchdog_and_overnight_status_without_actions() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-goal-run-status.sh");
    let script = fs::read_to_string(&script_path).expect("goal run status script should be readable");

    assert!(script.contains("Readonly status view for Chuang terminal goal workers"));
    assert!(script.contains("CHUANG_GOAL_WATCHDOG_REPORT_FILE"));
    assert!(script.contains("CHUANG_GOAL_RUN_ROOT"));
    assert!(script.contains("CHUANG_GOAL_OVERNIGHT_STATUS_FILE"));
    assert!(script.contains("\"dispatches_tasks\": False"));
    assert!(script.contains("\"starts_worker\": False"));
    assert!(script.contains("\"restarts_worker\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"deletes_logs\": False"));
    assert!(script.contains("\"touches_services\": False"));
    assert!(!script.contains("tmux new"));
    assert!(!script.contains("tmux send-keys"));
    assert!(!script.contains("codex exec"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-goal-run-status-smoke-{}-{nanos}",
        std::process::id()
    ));
    let watchdog_dir = root.join("watchdog");
    let run_root = root.join("runs");
    let latest_run = run_root.join("20260507-010203");
    fs::create_dir_all(&watchdog_dir).expect("watchdog dir should be created");
    fs::create_dir_all(&latest_run).expect("latest run dir should be created");

    let watchdog_report = watchdog_dir.join("latest-watchdog-report.json");
    fs::write(
        &watchdog_report,
        serde_json::json!({
            "schema_version": 1,
            "generated_at": "2026-05-07T01:02:03+08:00",
            "readonly": true,
            "project_root": manifest_dir.display().to_string(),
            "session": "chuang-goal",
            "tmux_session_present": true,
            "pane": {"bytes": 128, "panes": ["pane=%1 active=1 pid=123 current_command=codex"]},
            "codex_processes": {"count": 1, "processes": ["123 1 00:01 codex --no-alt-screen"]},
            "git": {"dirty": false, "status_short": []},
            "takeover": {"next_action": "monitor_or_attach_if_human_review_needed"},
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
    fs::write(
        latest_run.join("summary.md"),
        "# Chuang Overnight Goal Run\n\n- run_id: 20260507-010203\n- status: running\n- iterations: 3\n",
    )
    .expect("summary should write");
    fs::write(latest_run.join("status.json"), r#"{"status":"running","iteration":3}"#)
        .expect("overnight status should write");
    fs::write(latest_run.join("run.log"), "iteration 3 still running\n")
        .expect("run log should write");
    fs::write(latest_run.join("last-message.md"), "continuing goal work\n")
        .expect("last message should write");

    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_GOAL_WATCHDOG_REPORT_FILE", &watchdog_report)
        .env("CHUANG_GOAL_RUN_ROOT", &run_root)
        .current_dir(&manifest_dir)
        .output()
        .expect("goal run status script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status output should be json");
    assert_eq!(data["ok"], true);
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["readonly_boundaries"]["readonly"], true);
    assert_eq!(data["readonly_boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["readonly_boundaries"]["starts_worker"], false);
    assert_eq!(data["readonly_boundaries"]["restarts_worker"], false);
    assert_eq!(data["readonly_boundaries"]["modifies_repo"], false);
    assert_eq!(data["readonly_boundaries"]["deletes_logs"], false);
    assert_eq!(data["readonly_boundaries"]["touches_services"], false);
    assert_eq!(data["watchdog"]["available"], true);
    assert_eq!(data["watchdog"]["readonly"], true);
    assert_eq!(data["watchdog"]["session"], "chuang-goal");
    assert_eq!(data["watchdog"]["tmux_session_present"], true);
    assert_eq!(data["watchdog"]["codex_process_count"], 1);
    assert_eq!(data["watchdog"]["git_dirty"], false);
    assert_eq!(
        data["watchdog"]["next_action"],
        "monitor_or_attach_if_human_review_needed"
    );
    assert_eq!(
        data["overnight"]["latest_run_dir"],
        latest_run.display().to_string()
    );
    assert_eq!(data["overnight"]["status_json"]["available"], true);
    assert_eq!(data["overnight"]["status_json"]["data"]["status"], "running");
    assert_eq!(data["overnight"]["summary"]["fields"]["status"], "running");
    assert_eq!(data["overnight"]["summary"]["fields"]["iterations"], "3");
    assert_eq!(data["overall_status"], "terminal_worker_observed");
}

#[test]
fn cli_run_command_boots_and_returns_structured_response() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir =
        std::env::temp_dir().join(format!("chuang-agent-cli-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::create_dir_all(temp_dir.join("identity")).expect("identity dir should be created");

    let config_path = write_fake_config(&temp_dir);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--input",
            "创项目现在启动试试",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("model_name: stub-responder"));
    assert!(stdout.contains("body:"));
    assert!(stdout.contains("trace:"));
    assert!(stdout.contains("provider: fake-responder"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
    assert!(stdout.contains("context_drop_reasons:"));
    assert!(stdout.contains("context_working_reservation:"));
    assert!(stdout.contains("context_budget_exceeded:"));
    assert!(stdout.contains("runtime_report: report-turn-1"));
    assert_eq!(stdout.matches("governance_decision: allowed:").count(), 1);
    assert!(stdout.contains("创项目现在启动试试"));
}

#[test]
fn cli_run_can_select_summary_compression_context_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-context-engine-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--context-engine",
            "summary_compression",
            "--input",
            "测试上下文引擎切换",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("context_engine: summary_compression"));
    assert!(stdout.contains("测试上下文引擎切换"));
}

#[test]
fn cli_run_can_dispatch_runtime_report_to_queued_subagent() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-run-dispatch-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let queue_root = temp_dir.join("queue");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--subagent",
            "queued_external",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--input",
            "把这一轮交给子代理复核",
            "--dispatch-subagent",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("runtime_report: report-turn-1"));
    assert!(stdout.contains("subagent_dispatch_run_id: queued-run-1"));
    assert!(stdout.contains("subagent_dispatch_agent_id: worker-1"));
    assert!(stdout.contains("subagent_dispatch_task_id: turn-1"));

    let dispatch_path = queue_root.join("dispatch").join("queued-run-1.json");
    assert!(dispatch_path.exists());
    let dispatch = fs::read_to_string(dispatch_path).expect("dispatch should be readable");
    assert!(dispatch.contains("\"source\": \"cli-run\""));
    assert!(dispatch.contains("\"report_id\": \"report-turn-1\""));
    assert!(dispatch.contains("把这一轮交给子代理复核"));

    let run_once = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--json",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo subagent run-once should execute");

    assert!(
        run_once.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run_once.stderr)
    );
    let run_once_stdout = String::from_utf8_lossy(&run_once.stdout);
    assert!(run_once_stdout.contains("\"run_id\": \"queued-run-1\""));
    assert!(run_once_stdout.contains("\"report_admission\""));
    assert!(run_once_stdout.contains("\"status\": \"Accepted\""));
    assert!(queue_root
        .join("reports")
        .join("queued-run-1.json")
        .exists());

    let report = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--run-id",
            "queued-run-1",
            "--json",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo subagent report should execute");

    assert!(
        report.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&report.stderr)
    );
    let report_stdout = String::from_utf8_lossy(&report.stdout);
    assert!(report_stdout.contains("\"available\": true"));
    assert!(report_stdout.contains("fake runner completed turn-1"));
    assert!(report_stdout.contains("\"report_admission\""));
    assert!(report_stdout.contains("\"status\": \"Accepted\""));

    let collect = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--run-id",
            "queued-run-1",
            "--json",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo subagent collect should execute");

    assert!(
        collect.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let collect_stdout = String::from_utf8_lossy(&collect.stdout);
    assert!(collect_stdout.contains("\"dispatch_available\": true"));
    assert!(collect_stdout.contains("\"report_available\": true"));
    assert!(collect_stdout.contains("fake runner completed turn-1"));
    assert!(collect_stdout.contains("\"report_admission\""));
    assert!(collect_stdout.contains("\"status\": \"Accepted\""));
}

#[test]
fn cli_run_dispatch_subagent_requires_queued_external_slot() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-run-dispatch-reject-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let queue_root = temp_dir.join("queue");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--subagent",
            "fake",
            "--input",
            "没有选择 queued external",
            "--dispatch-subagent",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("subagent_dispatch_requires_queued_external"));
    assert!(!queue_root.join("dispatch").exists());
}

#[test]
fn cli_repl_command_accepts_one_turn_and_exits() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir =
        std::env::temp_dir().join(format!("chuang-agent-cli-repl-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::create_dir_all(temp_dir.join("identity")).expect("identity dir should be created");

    let config_path = write_fake_config(&temp_dir);

    let mut child = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "repl",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .current_dir(&workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cargo run repl should start");

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin should exist");
        stdin
            .write_all("创项目继续推进\nexit\n".as_bytes())
            .expect("stdin write should succeed");
    }

    let output = child.wait_with_output().expect("process should finish");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("创项目继续推进"));
    assert!(!stdout.contains("model_name:"));
    assert!(!stdout.contains("trace:"));
}

#[test]
fn cli_run_can_remember_turn_summary_when_requested() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-remember-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let db_arg = db_path.to_str().expect("db path should be utf-8");

    let first = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_arg,
            "--input",
            "MVP记忆写入测试",
            "--remember",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("memory_recorded: turn-memory-turn-1"));

    let second = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_arg,
            "--input",
            "MVP",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("recall_hits: 1"));
    assert!(second_stdout.contains("MVP记忆写入测试"));
}

#[test]
fn cli_run_can_remember_identity_memory_when_requested() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-identity-remember-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let identity_root = temp_dir.join("identity");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--identity-memory-root",
            identity_root
                .to_str()
                .expect("identity path should be utf-8"),
            "--input",
            "身份热记忆写入测试",
            "--remember-identity",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("identity_memory_recorded: identity-turn-1-"));

    let memory_file =
        fs::read_to_string(identity_root.join("MEMORY.md")).expect("memory file should exist");
    assert!(memory_file.contains("身份热记忆写入测试"));
    assert!(memory_file.contains("## identity-turn-1-"));
}

#[test]
fn cli_run_reports_memory_write_hard_limit_clearly() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-memory-limit-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let oversized_input = "超限".repeat(1200);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--input",
            &oversized_input,
            "--remember",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("memory_write_hard_limit_exceeded"));
    assert!(stderr.contains("limit_chars=2200"));
    assert!(stderr.contains("attempted_chars="));
    assert!(stderr.contains("existing_entries="));
}
