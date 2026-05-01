use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn cli_run_command_boots_and_returns_structured_response() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir =
        std::env::temp_dir().join(format!("chuang-agent-cli-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let db_path = temp_dir.join("memory.db");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
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

    let db_path = temp_dir.join("memory.db");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
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

    let db_path = temp_dir.join("memory.db");
    let queue_root = temp_dir.join("queue");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
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

    let collect = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
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

    let db_path = temp_dir.join("memory.db");
    let queue_root = temp_dir.join("queue");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
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

    let db_path = temp_dir.join("memory.db");

    let mut child = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "repl",
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
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
    assert!(stdout.contains("chuang-agent repl ready"));
    assert!(stdout.contains("model_name: stub-responder"));
    assert!(stdout.contains("创项目继续推进"));
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

    let db_path = temp_dir.join("memory.db");
    let db_arg = db_path.to_str().expect("db path should be utf-8");

    let first = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
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
            "run", "--quiet", "--", "run", "--db", db_arg, "--input", "MVP",
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

    let db_path = temp_dir.join("memory.db");
    let identity_root = temp_dir.join("identity");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
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

    let db_path = temp_dir.join("memory.db");
    let oversized_input = "超限".repeat(1200);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
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
