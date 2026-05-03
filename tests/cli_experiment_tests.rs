use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-experiment-{name}-{nanos}"))
}

#[test]
fn cli_experiment_plan_writes_safe_plan() {
    let root = temp_root("plan");
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "plan",
            "--goal",
            "验证 rules 治理层",
            "--success",
            "生成 experiment.md 且不执行破坏性命令",
            "--time-budget-minutes",
            "15",
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("experiment_planned:"));
    assert!(stdout.contains("experiment_time_budget_minutes: 15"));

    let plan_path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("experiment_plan_path: "))
        .expect("plan path should be printed");
    let content = std::fs::read_to_string(plan_path).expect("plan should exist");
    assert!(content.contains("验证 rules 治理层"));
    assert!(content.contains("Do not run `git reset --hard`"));
}

#[test]
fn cli_experiment_complete_writes_report_for_existing_plan() {
    let root = temp_root("complete");
    let plan = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "plan",
            "--goal",
            "验证 complete 命令",
            "--success",
            "生成 report.md",
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        plan.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan_stdout = String::from_utf8_lossy(&plan.stdout);
    let experiment_id = plan_stdout
        .lines()
        .find_map(|line| line.strip_prefix("experiment_planned: "))
        .expect("experiment id should be printed");

    let complete = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "complete",
            "--experiment-id",
            experiment_id,
            "--outcome",
            "success",
            "--summary",
            "complete 命令已写入报告",
            "--next",
            "后续接沙箱执行",
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        complete.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&complete.stderr)
    );
    let complete_stdout = String::from_utf8_lossy(&complete.stdout);
    assert!(complete_stdout.contains("experiment_completed:"));
    assert!(complete_stdout.contains("experiment_outcome: success"));
    let report_path = complete_stdout
        .lines()
        .find_map(|line| line.strip_prefix("experiment_report_path: "))
        .expect("report path should be printed");
    let content = std::fs::read_to_string(report_path).expect("report should exist");
    assert!(content.contains("complete 命令已写入报告"));
    assert!(content.contains("No deletion"));
}

#[test]
fn cli_experiment_list_shows_plan_and_report_state() {
    let root = temp_root("list");
    let plan = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "plan",
            "--goal",
            "验证 list 命令",
            "--success",
            "list 显示 planned",
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        plan.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan_stdout = String::from_utf8_lossy(&plan.stdout);
    let experiment_id = plan_stdout
        .lines()
        .find_map(|line| line.strip_prefix("experiment_planned: "))
        .expect("experiment id should be printed");

    let list = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "list",
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        list.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list.stderr)
    );
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("experiment_count: 1"));
    assert!(stdout.contains(experiment_id));
    assert!(stdout.contains("status=planned"));
    assert!(stdout.contains("has_report=false"));
}

#[test]
fn cli_experiment_show_reads_existing_plan() {
    let root = temp_root("show");
    let plan = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "plan",
            "--goal",
            "验证 show 命令",
            "--success",
            "show 显示 plan markdown",
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        plan.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan_stdout = String::from_utf8_lossy(&plan.stdout);
    let experiment_id = plan_stdout
        .lines()
        .find_map(|line| line.strip_prefix("experiment_planned: "))
        .expect("experiment id should be printed");

    let show = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "experiment",
            "show",
            "--experiment-id",
            experiment_id,
            "--root",
            root.to_str().expect("root should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        show.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&show.stderr)
    );
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("experiment_status: planned"));
    assert!(stdout.contains("experiment_plan_markdown:"));
    assert!(stdout.contains("验证 show 命令"));
}
