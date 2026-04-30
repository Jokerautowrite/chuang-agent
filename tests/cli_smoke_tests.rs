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
    assert!(stdout.contains("context_drop_reasons:"));
    assert!(stdout.contains("context_working_reservation:"));
    assert!(stdout.contains("context_budget_exceeded:"));
    assert!(stdout.contains("创项目现在启动试试"));
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
