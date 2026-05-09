use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_queue_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-subagent-{name}-{nanos}"))
}

fn cargo_command() -> Command {
    let mut command = Command::new("cargo");
    command.env("CODEX_PPTOKEN_API_KEY", "test-key");
    command
}

#[test]
fn cli_subagent_dispatch_writes_queued_dispatch_json() {
    let queue_root = temp_queue_root("dispatch");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "dispatch",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--task",
            "审计 runtime 子代理队列",
            "--task-id",
            "task-cli-1",
            "--agent-name",
            "worker",
            "--policy",
            "execute",
            "--requires-capability",
            "rust",
            "--requires-capability",
            "filesystem",
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

    let run_id = parsed["run_id"].as_str().expect("run_id should be string");
    let agent_id = parsed["agent_id"]
        .as_str()
        .expect("agent_id should be string");
    assert!(run_id.starts_with("queued-cli-"));
    assert!(agent_id.starts_with("worker-"));
    assert_eq!(parsed["task_id"], "task-cli-1");
    assert_eq!(parsed["queue_root"], queue_root.display().to_string());

    let dispatch_path = queue_root.join("dispatch").join(format!("{run_id}.json"));
    assert_eq!(parsed["dispatch_path"], dispatch_path.display().to_string());
    let dispatch: Value = serde_json::from_str(
        &std::fs::read_to_string(dispatch_path).expect("dispatch file should exist"),
    )
    .expect("dispatch should be json");

    assert_eq!(dispatch["task_id"], "task-cli-1");
    assert_eq!(dispatch["task"], "审计 runtime 子代理队列");
    assert_eq!(dispatch["tool_policy"], "Execute");
    assert_eq!(dispatch["metadata"]["source"], "cli");
    assert_eq!(
        dispatch["metadata"]["required_capabilities"],
        "rust,filesystem"
    );
}

#[test]
fn cli_subagent_dispatch_can_queue_multiple_tasks_in_same_directory() {
    let queue_root = temp_queue_root("multi-dispatch");
    let first = dispatch_task(&queue_root, "task-cli-1", "审计第一段");
    let second = dispatch_task(&queue_root, "task-cli-2", "审计第二段");

    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");

    assert_ne!(first_run, second_run);
    assert!(queue_root
        .join("dispatch")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("dispatch")
        .join(format!("{second_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_list_reports_dispatches_and_report_presence() {
    let queue_root = temp_queue_root("list");
    let first = dispatch_task(&queue_root, "task-cli-1", "审计第一段");
    let second = dispatch_task(&queue_root, "task-cli-2", "审计第二段");
    let second_run = second["run_id"].as_str().expect("second run id");
    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root
            .join("reports")
            .join(format!("{second_run}.json")),
        sample_report_json("second completed"),
    )
    .expect("report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "list",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let first_run = first["run_id"].as_str().expect("first run id");

    assert_eq!(parsed["dispatch_count"], 2);
    assert_eq!(parsed["report_count"], 1);
    assert!(parsed["items"]
        .as_array()
        .expect("items should be array")
        .iter()
        .any(|item| item["run_id"] == first_run
            && item["has_report"] == false
            && item["is_claimed"] == false
            && item["required_capabilities"]
                .as_array()
                .expect("required caps")
                .is_empty()));
    assert!(parsed["items"]
        .as_array()
        .expect("items should be array")
        .iter()
        .any(|item| item["run_id"] == second_run && item["has_report"] == true));
}

#[test]
fn cli_subagent_list_text_surfaces_queue_and_readonly_evidence_fields() {
    let queue_root = temp_queue_root("list-text-evidence");
    let first = dispatch_task_with_capabilities(
        &queue_root,
        "task-cli-list-text-1",
        "文本列表证据任务一",
        &["rust", "filesystem"],
    );
    let second = dispatch_task(&queue_root, "task-cli-list-text-2", "文本列表证据任务二");
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");
    std::fs::create_dir_all(queue_root.join("claims")).expect("claims dir should exist");
    std::fs::write(
        queue_root.join("claims").join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"other-worker","claimed_at_unix_nanos":1}}"#),
    )
    .expect("claim should write");
    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root
            .join("reports")
            .join(format!("{second_run}.json")),
        sample_report_json("second completed"),
    )
    .expect("report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "list",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "subagent_queue queue_root={} dispatch_count=2 report_count=1",
        queue_root.display()
    )));
    assert!(stdout.contains(&format!("run_id={first_run} agent_id=")));
    assert!(stdout.contains("required_capabilities=rust,filesystem"));
    assert!(stdout.contains("is_claimed=true"));
    assert!(stdout.contains(&format!("run_id={second_run} agent_id=")));
    assert!(stdout.contains("has_report=true"));
}

#[test]
fn cli_subagent_run_once_respects_required_capabilities() {
    let queue_root = temp_queue_root("required-capabilities");
    let first = dispatch_task_with_capabilities(
        &queue_root,
        "task-cli-cap-rust",
        "需要 rust worker",
        &["Rust", "rust"],
    );
    let second = dispatch_task_with_capabilities(
        &queue_root,
        "task-cli-cap-python",
        "需要 python worker",
        &["python"],
    );
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--capability",
            "python",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["run_id"], second_run);
    assert!(!queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("reports")
        .join(format!("{second_run}.json"))
        .exists());

    let first_dispatch_path = queue_root
        .join("dispatch")
        .join(format!("{first_run}.json"));
    let first_dispatch: Value = serde_json::from_str(
        &std::fs::read_to_string(first_dispatch_path).expect("dispatch file should exist"),
    )
    .expect("dispatch should be json");
    assert_eq!(first_dispatch["metadata"]["required_capabilities"], "rust");
}

#[test]
fn cli_subagent_dispatch_rejects_comma_in_required_capability() {
    let queue_root = temp_queue_root("bad-required-capability");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "dispatch",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--task",
            "bad capability",
            "--requires-capability",
            "rust,python",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("--requires-capability must not contain comma"));
}

#[test]
fn cli_subagent_report_rejects_unsafe_run_id_path() {
    let queue_root = temp_queue_root("unsafe-report-run-id");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "../escape",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("InvalidRunId"));
    assert!(!queue_root.join("escape.json").exists());
}

#[test]
fn cli_subagent_run_once_rejects_comma_in_worker_capability() {
    let queue_root = temp_queue_root("bad-worker-capability");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--capability",
            "rust,python",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--capability must not contain comma"));
}

#[test]
fn cli_subagent_list_reports_stale_claims() {
    let queue_root = temp_queue_root("list-stale");
    let first = dispatch_task(&queue_root, "task-cli-stale-1", "过期领取任务");
    let first_run = first["run_id"].as_str().expect("first run id");
    std::fs::create_dir_all(queue_root.join("claims")).expect("claims dir should exist");
    std::fs::write(
        queue_root.join("claims").join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"other-worker","claimed_at_unix_nanos":0}}"#),
    )
    .expect("claim should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "list",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert!(parsed["items"]
        .as_array()
        .expect("items should be array")
        .iter()
        .any(|item| item["run_id"] == first_run
            && item["is_claimed"] == true
            && item["is_claim_stale"] == true));
}

#[test]
fn cli_subagent_run_once_skips_claimed_dispatch_and_lists_claim_state() {
    let queue_root = temp_queue_root("run-once-claimed");
    let first = dispatch_task(&queue_root, "task-cli-claimed-1", "已领取任务");
    let second = dispatch_task(&queue_root, "task-cli-claimed-2", "待领取任务");
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");
    std::fs::create_dir_all(queue_root.join("claims")).expect("claims dir should exist");
    std::fs::write(
        queue_root.join("claims").join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"other-worker"}}"#),
    )
    .expect("claim should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["run_id"], second_run);
    assert!(!queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("reports")
        .join(format!("{second_run}.json"))
        .exists());

    let list_output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "list",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        list_output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list_output.stderr)
    );
    let listed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&list_output.stdout)).expect("stdout json");
    assert!(listed["items"]
        .as_array()
        .expect("items")
        .iter()
        .any(|item| item["run_id"] == first_run && item["is_claimed"] == true));
}

#[test]
fn cli_subagent_run_once_reclaims_stale_claim() {
    let queue_root = temp_queue_root("run-once-stale-claim");
    let first = dispatch_task_with_args(
        &queue_root,
        "task-cli-stale-reclaim",
        "过期领取后重试任务",
        &["--idle-timeout-ms", "0"],
    );
    let first_run = first["run_id"].as_str().expect("first run id");
    std::fs::create_dir_all(queue_root.join("claims")).expect("claims dir should exist");
    std::fs::write(
        queue_root.join("claims").join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"stale-worker","claimed_at_unix_nanos":0}}"#),
    )
    .expect("stale claim should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let claim_payload =
        std::fs::read_to_string(queue_root.join("claims").join(format!("{first_run}.json")))
            .expect("claim payload should read");

    assert_eq!(parsed["run_id"], first_run);
    assert!(claim_payload.contains("cli-worker-"));
    assert!(queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_release_claim_allows_dispatch_to_run_again() {
    let queue_root = temp_queue_root("release-claim");
    let first = dispatch_task(&queue_root, "task-cli-release-1", "释放后运行任务");
    let first_run = first["run_id"].as_str().expect("first run id");
    std::fs::create_dir_all(queue_root.join("claims")).expect("claims dir should exist");
    std::fs::write(
        queue_root.join("claims").join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"other-worker"}}"#),
    )
    .expect("claim should write");

    let release = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "release-claim",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            first_run,
            "--reason",
            "manual retry",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        release.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&release.stderr)
    );
    let released: Value =
        serde_json::from_str(&String::from_utf8_lossy(&release.stdout)).expect("stdout json");
    assert_eq!(released["released"], true);
    assert!(queue_root
        .join("claim-releases")
        .join(format!("{first_run}.json"))
        .exists());

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["run_id"], first_run);
    assert!(queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("claims")
        .join(format!("{first_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_list_marks_released_claim_as_not_claimed() {
    let queue_root = temp_queue_root("list-released-claim");
    let first = dispatch_task(&queue_root, "task-cli-released-list", "释放后列表状态");
    let first_run = first["run_id"].as_str().expect("first run id");
    std::fs::create_dir_all(queue_root.join("claims")).expect("claims dir should exist");
    std::fs::write(
        queue_root.join("claims").join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"old-worker","claimed_at_unix_nanos":1}}"#),
    )
    .expect("claim should write");
    std::fs::create_dir_all(queue_root.join("claim-releases"))
        .expect("claim release dir should exist");
    std::fs::write(
        queue_root
            .join("claim-releases")
            .join(format!("{first_run}.json")),
        format!(r#"{{"run_id":"{first_run}","owner":"operator","reason":"retry","released_at_unix_nanos":2}}"#),
    )
    .expect("release should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "list",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert!(parsed["items"]
        .as_array()
        .expect("items")
        .iter()
        .any(|item| item["run_id"] == first_run && item["is_claimed"] == false));
}

#[test]
fn cli_subagent_run_once_fake_runner_writes_report_for_first_pending_dispatch() {
    let queue_root = temp_queue_root("run-once");
    let first = dispatch_task(&queue_root, "task-cli-1", "审计第一段");
    let second = dispatch_task(&queue_root, "task-cli-2", "审计第二段");
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "fake");
    assert_eq!(parsed["ran"], true);
    assert_eq!(parsed["run_id"], first_run);
    assert!(queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(!queue_root
        .join("reports")
        .join(format!("{second_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_run_once_reports_idle_when_no_pending_dispatch_exists() {
    let queue_root = temp_queue_root("run-once-idle");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "fake");
    assert_eq!(parsed["ran"], false);
    assert_eq!(parsed["summary"], "no pending dispatch");
}

#[test]
fn cli_subagent_run_loop_processes_multiple_pending_dispatches_with_limit() {
    let queue_root = temp_queue_root("run-loop-limit");
    let first = dispatch_task(&queue_root, "task-cli-loop-1", "循环任务一");
    let second = dispatch_task(&queue_root, "task-cli-loop-2", "循环任务二");
    let third = dispatch_task(&queue_root, "task-cli-loop-3", "循环任务三");
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");
    let third_run = third["run_id"].as_str().expect("third run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--max-runs",
            "2",
            "--max-concurrency",
            "1",
            "--capability",
            "rust",
            "--capability",
            "filesystem",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "fake");
    assert_eq!(parsed["max_runs"], 2);
    assert_eq!(parsed["max_concurrency"], 1);
    assert_eq!(parsed["ran_count"], 2);
    assert_eq!(parsed["idle"], false);
    assert_eq!(parsed["worker_capabilities"][0], "rust");
    assert_eq!(parsed["worker_capabilities"][1], "filesystem");
    assert_eq!(
        parsed["report_admissions"]
            .as_array()
            .expect("report admissions array")
            .len(),
        2
    );
    assert_eq!(
        parsed["report_admissions"][0]["status"],
        Value::String("Accepted".to_string())
    );
    assert_eq!(
        parsed["report_admissions"][0]["reason_code"],
        "report_validated"
    );
    assert_eq!(
        parsed["report_admissions"][1]["status"],
        Value::String("Accepted".to_string())
    );
    assert_eq!(
        parsed["report_admissions"][1]["reason_code"],
        "report_validated"
    );
    assert!(queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("reports")
        .join(format!("{second_run}.json"))
        .exists());
    assert!(!queue_root
        .join("reports")
        .join(format!("{third_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_run_loop_runs_bounded_parallel_workers() {
    let queue_root = temp_queue_root("run-loop-concurrency");
    let first = dispatch_task(&queue_root, "task-cli-concurrency-1", "并发任务一");
    let second = dispatch_task(&queue_root, "task-cli-concurrency-2", "并发任务二");
    let first_run = first["run_id"].as_str().expect("first run id");
    let second_run = second["run_id"].as_str().expect("second run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--max-concurrency",
            "2",
            "--max-runs",
            "2",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["max_concurrency"], 2);
    assert_eq!(parsed["ran_count"], 2);
    let run_ids = parsed["run_ids"]
        .as_array()
        .expect("run_ids should be array")
        .iter()
        .map(|value| value.as_str().expect("run id string"))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(run_ids.contains(first_run));
    assert!(run_ids.contains(second_run));
    assert!(queue_root
        .join("reports")
        .join(format!("{first_run}.json"))
        .exists());
    assert!(queue_root
        .join("reports")
        .join(format!("{second_run}.json"))
        .exists());
}

#[test]
fn cli_subagent_run_loop_rejects_unbounded_parallel_concurrency() {
    let queue_root = temp_queue_root("run-loop-concurrency-limit");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--max-concurrency",
            "9",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("--max-concurrency above 8 is not supported"));
}

#[test]
fn cli_subagent_run_loop_reports_idle_after_queue_drains() {
    let queue_root = temp_queue_root("run-loop-idle");
    let first = dispatch_task(&queue_root, "task-cli-loop-idle-1", "循环任务一");
    let first_run = first["run_id"].as_str().expect("first run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--max-runs",
            "3",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["ran_count"], 1);
    assert_eq!(parsed["idle"], true);
    assert_eq!(parsed["run_ids"][0], first_run);
}

#[test]
fn cli_subagent_run_loop_rejects_zero_max_runs() {
    let queue_root = temp_queue_root("run-loop-zero");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--max-runs",
            "0",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--max-runs must be greater than zero")
    );
}

#[test]
fn cli_subagent_run_loop_live_gate_fails_before_worker_without_env_gate() {
    let queue_root = temp_queue_root("run-loop-live-gate-disabled");
    let output = cargo_command()
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "sh",
            "--allow-runner-command",
            "sh",
            "--approve-exec",
            "--require-live-gate",
            "--max-runs",
            "1",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("live_runner_gate_disabled"));
    assert!(stderr.contains("CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(stderr.contains("subagent.runner.live"));
    assert!(!queue_root.join("claims").exists());
    assert!(!queue_root.join("reports").exists());
}

#[test]
fn cli_subagent_run_loop_live_gate_rejects_command_outside_allowlist_first() {
    let queue_root = temp_queue_root("run-loop-live-gate-allowlist");
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-loop",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "sh",
            "--allow-runner-command",
            "bash",
            "--approve-exec",
            "--require-live-gate",
            "--max-runs",
            "1",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("live_runner_command_not_allowlisted"));
    assert!(stderr.contains("runner_command=sh"));
    assert!(!queue_root.join("claims").exists());
    assert!(!queue_root.join("reports").exists());
}

#[test]
fn cli_subagent_run_once_command_runner_requires_explicit_approval() {
    let queue_root = temp_queue_root("command-runner-approval");
    let _dispatch = dispatch_task(&queue_root, "task-cli-command-approval", "命令 runner 审批");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "printf",
            "--runner-arg",
            "command runner ok",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("command_runner_requires_approve_exec"),
        "stderr={stderr}"
    );
}

#[test]
fn cli_subagent_run_once_command_runner_writes_report_from_process_output() {
    let queue_root = temp_queue_root("command-runner");
    let dispatch = dispatch_task(&queue_root, "task-cli-command", "命令 runner 任务");
    let run_id = dispatch["run_id"].as_str().expect("run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "printf",
            "--runner-arg",
            "command runner ok",
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "command");
    assert_eq!(parsed["ran"], true);
    assert_eq!(parsed["run_id"], run_id);

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Success");
    assert_eq!(report["exit_code"], 0);
    assert_eq!(report["stdout_preview"], "command runner ok");
    assert_eq!(
        report["replay_ref"],
        format!("queued-subagent-command://{run_id}")
    );
    assert_eq!(
        report["governance_decision"]["action_id"],
        format!("subagent-command-runner:{run_id}")
    );
    assert_eq!(report["governance_decision"]["decision"], "needs_approval");
    assert_eq!(
        report["governance_decision"]["reason"],
        "approved_by_cli_flag: --approve-exec"
    );
}

#[test]
fn cli_subagent_run_once_command_runner_accepts_protocol_report_json() {
    let queue_root = temp_queue_root("command-runner-protocol-report");
    let dispatch = dispatch_task(&queue_root, "task-cli-protocol", "命令 runner 协议报告");
    let run_id = dispatch["run_id"].as_str().expect("run id");
    let agent_id = dispatch_agent_id(&queue_root, run_id);
    let protocol_report = report_json(
        "task-cli-protocol",
        &agent_id,
        "real runner protocol report accepted",
    );

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "printf",
            "--runner-arg",
            &protocol_report,
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(
        parsed["report_admission"]["status"],
        Value::String("Accepted".to_string())
    );
    assert_eq!(
        parsed["report_admission"]["reason_code"],
        "report_validated"
    );

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Success");
    assert_eq!(report["summary"], "real runner protocol report accepted");
    assert_eq!(report["agent_id"], agent_id);
    assert_eq!(report["stdout_preview"], "ok");
    assert_eq!(
        report["governance_decision"]["action_id"],
        format!("subagent-command-runner:{run_id}")
    );
    assert_eq!(report["governance_decision"]["decision"], "needs_approval");
    assert_eq!(
        report["governance_decision"]["reason"],
        "approved_by_cli_flag: --approve-exec"
    );
}

#[test]
fn cli_subagent_run_once_command_runner_rejects_protocol_report_identity_mismatch() {
    let queue_root = temp_queue_root("command-runner-protocol-mismatch");
    let dispatch = dispatch_task(
        &queue_root,
        "task-cli-protocol-mismatch",
        "命令 runner 坏报告",
    );
    let run_id = dispatch["run_id"].as_str().expect("run id");
    let protocol_report = report_json(
        "task-cli-protocol-mismatch",
        "wrong-agent",
        "bad protocol report",
    );

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "printf",
            "--runner-arg",
            &protocol_report,
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(
        parsed["report_admission"]["status"],
        Value::String("Rejected".to_string())
    );
    assert_eq!(
        parsed["report_admission"]["reason_code"],
        "command_protocol_report_rejected"
    );
    assert_eq!(
        parsed["report_admission"]["upstream_reason_code"],
        "agent_id_mismatch"
    );

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Failed");
    assert!(report["summary"]
        .as_str()
        .expect("summary string")
        .contains("protocol rejected"));
    assert!(report["stderr_preview"]
        .as_str()
        .expect("stderr preview")
        .contains("agent_id_mismatch"));
    assert_eq!(
        report["governance_decision"]["action_id"],
        format!("subagent-command-runner:{run_id}")
    );
    assert_eq!(report["governance_decision"]["decision"], "needs_approval");
}

#[test]
fn cli_subagent_run_once_command_runner_rejects_incomplete_protocol_report() {
    let queue_root = temp_queue_root("command-runner-protocol-incomplete");
    let dispatch = dispatch_task(
        &queue_root,
        "task-cli-protocol-incomplete",
        "命令 runner 缺字段报告",
    );
    let run_id = dispatch["run_id"].as_str().expect("run id");
    let agent_id = dispatch_agent_id(&queue_root, run_id);
    let protocol_report = report_json_without_truncated(
        "task-cli-protocol-incomplete",
        &agent_id,
        "missing required truncated field",
    );

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "printf",
            "--runner-arg",
            &protocol_report,
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Failed");
    assert!(report["summary"]
        .as_str()
        .expect("summary string")
        .contains("protocol rejected"));
    assert!(report["stderr_preview"]
        .as_str()
        .expect("stderr preview")
        .contains("MissingRequiredField"));
    assert!(report["stderr_preview"]
        .as_str()
        .expect("stderr preview")
        .contains("truncated"));

    let report_output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            run_id,
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        report_output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&report_output.stderr)
    );
    let report_parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&report_output.stdout))
            .expect("report stdout json");
    assert_eq!(
        report_parsed["report_admission"]["status"],
        Value::String("Rejected".to_string())
    );
    assert_eq!(
        report_parsed["report_admission"]["reason_code"],
        "command_protocol_report_rejected"
    );
    assert_eq!(
        report_parsed["report_admission"]["upstream_reason_code"],
        "missing_required_field"
    );
}

#[test]
fn cli_subagent_run_once_command_runner_rejects_protocol_report_missing_status() {
    let queue_root = temp_queue_root("command-runner-protocol-missing-status");
    let dispatch = dispatch_task(
        &queue_root,
        "task-cli-protocol-missing-status",
        "命令 runner 缺 status 报告",
    );
    let run_id = dispatch["run_id"].as_str().expect("run id");
    let agent_id = dispatch_agent_id(&queue_root, run_id);
    let protocol_report = report_json_without_status(
        "task-cli-protocol-missing-status",
        &agent_id,
        "missing status",
    );

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "printf",
            "--runner-arg",
            &protocol_report,
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(
        parsed["report_admission"]["status"],
        Value::String("Rejected".to_string())
    );
    assert_eq!(
        parsed["report_admission"]["reason_code"],
        "command_protocol_report_rejected"
    );
    assert_eq!(
        parsed["report_admission"]["upstream_reason_code"],
        "missing_required_field"
    );

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Failed");
    assert!(report["stderr_preview"]
        .as_str()
        .expect("stderr preview")
        .contains("MissingRequiredField"));
    assert!(report["stderr_preview"]
        .as_str()
        .expect("stderr preview")
        .contains("status"));
}

#[test]
fn cli_subagent_run_once_command_runner_bounds_large_output_preview() {
    let queue_root = temp_queue_root("command-runner-large-output");
    let dispatch = dispatch_task(
        &queue_root,
        "task-cli-large-output",
        "命令 runner 大输出任务",
    );
    let run_id = dispatch["run_id"].as_str().expect("run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "sh",
            "--runner-arg",
            "-c",
            "--runner-arg",
            "i=0; while [ $i -lt 8000 ]; do printf 1234567890; i=$((i+1)); done",
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    let stdout_preview = report["stdout_preview"]
        .as_str()
        .expect("stdout preview string");

    assert_eq!(report["status"], "Success");
    assert_eq!(report["truncated"], true);
    assert_eq!(stdout_preview.chars().count(), 1200);
    assert!(stdout_preview.starts_with("1234567890"));
}

#[test]
fn cli_subagent_run_once_command_runner_times_out_and_writes_failed_report() {
    let queue_root = temp_queue_root("command-runner-timeout");
    let dispatch = dispatch_task_with_args(
        &queue_root,
        "task-cli-timeout",
        "命令 runner 超时任务",
        &["--idle-timeout-ms", "20"],
    );
    let run_id = dispatch["run_id"].as_str().expect("run id");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            "sh",
            "--runner-arg",
            "-c",
            "--runner-arg",
            "sleep 1",
            "--approve-exec",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["runner"], "command");
    assert_eq!(parsed["ran"], true);
    assert_eq!(parsed["run_id"], run_id);

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Failed");
    assert!(report["summary"]
        .as_str()
        .expect("summary string")
        .contains("timed_out=true"));
    assert!(report["stderr_preview"]
        .as_str()
        .expect("stderr preview")
        .contains("timed out after 20ms"));
}

#[test]
fn cli_subagent_run_once_accepts_checked_in_codex_runner_disabled_report() {
    let queue_root = temp_queue_root("codex-runner-disabled");
    let dispatch = dispatch_task_with_capabilities(
        &queue_root,
        "task-cli-codex-runner",
        "Codex runner 协议检查",
        &["codex"],
    );
    let run_id = dispatch["run_id"].as_str().expect("run id");
    let runner_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("chuang-codex-runner.py");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--runner",
            "command",
            "--runner-command",
            runner_path.to_str().expect("runner path should be utf8"),
            "--approve-exec",
            "--capability",
            "codex",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = queue_root.join("reports").join(format!("{run_id}.json"));
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path).expect("report file should exist"),
    )
    .expect("report should be json");
    assert_eq!(report["status"], "Failed");
    assert!(report["summary"]
        .as_str()
        .expect("summary string")
        .contains("codex runner disabled"));
    assert_eq!(
        report["replay_ref"],
        format!("queued-subagent-codex://{run_id}")
    );
}

#[test]
fn cli_subagent_dispatch_requires_task() {
    let queue_root = temp_queue_root("missing-task");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "dispatch",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("subagent dispatch requires --task"));
}

#[test]
fn cli_subagent_report_reads_available_report_json() {
    let queue_root = temp_queue_root("report-available");
    let reports = queue_root.join("reports");
    std::fs::create_dir_all(&reports).expect("reports dir should be created");
    std::fs::write(
        reports.join("queued-run-1.json"),
        sample_report_json("queued worker completed"),
    )
    .expect("report should be written");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "queued-run-1",
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

    assert_eq!(parsed["run_id"], "queued-run-1");
    assert_eq!(parsed["available"], true);
    assert_eq!(parsed["report"]["summary"], "queued worker completed");
    assert_eq!(parsed["report"]["status"], "Success");
}

#[test]
fn cli_subagent_report_marks_missing_report_without_error() {
    let queue_root = temp_queue_root("report-missing");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "queued-run-404",
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

    assert_eq!(parsed["run_id"], "queued-run-404");
    assert_eq!(parsed["available"], false);
    assert_eq!(parsed["report"], Value::Null);
}

#[test]
fn cli_subagent_report_returns_rejected_admission_for_malformed_report_file() {
    let queue_root = temp_queue_root("report-malformed");
    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root.join("reports").join("queued-run-bad.json"),
        "{bad json",
    )
    .expect("bad report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "queued-run-bad",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["available"], true);
    assert_eq!(parsed["report"], Value::Null);
    assert_eq!(
        parsed["report_admission"]["status"],
        Value::String("Rejected".to_string())
    );
    assert_eq!(parsed["report_admission"]["reason_code"], "invalid_json");
}

#[test]
fn cli_subagent_collect_uses_dispatch_identity_before_returning_report() {
    let queue_root = temp_queue_root("collect");
    let dispatch_output = dispatch_task(&queue_root, "task-cli-collect", "收集子代理报告");
    let run_id = dispatch_output["run_id"].as_str().expect("run id");
    let dispatch_path = queue_root.join("dispatch").join(format!("{run_id}.json"));
    let dispatch: Value = serde_json::from_str(
        &std::fs::read_to_string(dispatch_path).expect("dispatch should exist"),
    )
    .expect("dispatch should be json");
    let agent_id = dispatch["agent_id"].as_str().expect("agent id");

    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root.join("reports").join(format!("{run_id}.json")),
        report_json("task-cli-collect", agent_id, "identity checked report"),
    )
    .expect("report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            run_id,
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["run_id"], run_id);
    assert_eq!(parsed["dispatch_available"], true);
    assert_eq!(parsed["report_available"], true);
    assert_eq!(parsed["report"]["summary"], "identity checked report");
}

#[test]
fn cli_subagent_collect_returns_rejected_admission_for_partial_report_file() {
    let queue_root = temp_queue_root("collect-partial-report");
    let dispatch_output = dispatch_task(&queue_root, "task-cli-partial-report", "收集缺字段报告");
    let run_id = dispatch_output["run_id"].as_str().expect("run id");
    let agent_id = dispatch_agent_id(&queue_root, run_id);

    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root.join("reports").join(format!("{run_id}.json")),
        report_json_without_status("task-cli-partial-report", &agent_id, "missing status"),
    )
    .expect("partial report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            run_id,
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["dispatch_available"], true);
    assert_eq!(parsed["report_available"], true);
    assert_eq!(parsed["report"], Value::Null);
    assert_eq!(
        parsed["report_admission"]["status"],
        Value::String("Rejected".to_string())
    );
    assert_eq!(
        parsed["report_admission"]["reason_code"],
        "missing_required_field"
    );
}

#[test]
fn cli_subagent_collect_rejects_mismatched_report_identity() {
    let queue_root = temp_queue_root("collect-mismatch");
    let dispatch_output = dispatch_task(&queue_root, "task-cli-mismatch", "收集坏报告");
    let run_id = dispatch_output["run_id"].as_str().expect("run id");

    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root.join("reports").join(format!("{run_id}.json")),
        report_json("task-cli-mismatch", "wrong-agent", "wrong identity report"),
    )
    .expect("report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            run_id,
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("subagent_collect_failed"));
}

#[test]
fn cli_subagent_collect_rejects_mismatched_parent_identity() {
    let queue_root = temp_queue_root("collect-parent-mismatch");
    let dispatch_output = dispatch_task(
        &queue_root,
        "task-cli-parent-mismatch",
        "收集 parent 错误报告",
    );
    let run_id = dispatch_output["run_id"].as_str().expect("run id");
    let agent_id = dispatch_agent_id(&queue_root, run_id);

    std::fs::create_dir_all(queue_root.join("reports")).expect("reports dir should exist");
    std::fs::write(
        queue_root.join("reports").join(format!("{run_id}.json")),
        report_json_with_parent(
            "task-cli-parent-mismatch",
            &agent_id,
            "wrong parent identity report",
            "wrong-parent",
        ),
    )
    .expect("report should write");

    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            run_id,
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("subagent_collect_failed"));
}

#[test]
fn cli_subagent_collect_marks_missing_dispatch_without_error() {
    let queue_root = temp_queue_root("collect-missing-dispatch");
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
            "--run-id",
            "missing-run",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["run_id"], "missing-run");
    assert_eq!(parsed["dispatch_available"], false);
    assert_eq!(parsed["report_available"], false);
    assert_eq!(parsed["report"], Value::Null);
}

fn sample_report_json(summary: &str) -> String {
    report_json("task-cli-1", "worker-1", summary)
}

fn report_json(task_id: &str, agent_id: &str, summary: &str) -> String {
    report_json_with_parent(task_id, agent_id, summary, "chuang-cli")
}

fn report_json_with_parent(
    task_id: &str,
    agent_id: &str,
    summary: &str,
    parent_id: &str,
) -> String {
    format!(
        r#"{{
  "schema_version": "1.0",
  "report_id": "report-queued-run-1",
  "task_id": "{task_id}",
  "agent_id": "{agent_id}",
  "parent_agent_id": "{parent_id}",
  "status": "Success",
  "started_at": "2026-05-01T00:00:00Z",
  "finished_at": "2026-05-01T00:00:01Z",
  "summary": "{summary}",
  "exit_code": 0,
  "stdout_preview": "ok",
  "stderr_preview": null,
  "resource_usage": {{
    "wall_time_ms": 1000,
    "prompt_tokens": 10,
    "completion_tokens": 5,
    "cpu_time_ms": 0,
    "peak_memory_bytes": 0
  }},
  "artifacts": [],
  "replay_ref": "queued-subagent://queued-run-1",
  "context_debug": null,
  "truncated": false
}}"#
    )
}

fn report_json_without_truncated(task_id: &str, agent_id: &str, summary: &str) -> String {
    format!(
        r#"{{
  "schema_version": "1.0",
  "report_id": "report-queued-run-1",
  "task_id": "{task_id}",
  "agent_id": "{agent_id}",
  "parent_agent_id": "chuang-cli",
  "status": "Success",
  "started_at": "2026-05-01T00:00:00Z",
  "finished_at": "2026-05-01T00:00:01Z",
  "summary": "{summary}",
  "exit_code": 0,
  "stdout_preview": "ok",
  "stderr_preview": null,
  "resource_usage": {{
    "wall_time_ms": 1000,
    "prompt_tokens": 10,
    "completion_tokens": 5,
    "cpu_time_ms": 0,
    "peak_memory_bytes": 0
  }},
  "artifacts": [],
  "replay_ref": "queued-subagent://queued-run-1",
  "context_debug": null
}}"#
    )
}

fn report_json_without_status(task_id: &str, agent_id: &str, summary: &str) -> String {
    format!(
        r#"{{
  "schema_version": "1.0",
  "report_id": "report-queued-run-1",
  "task_id": "{task_id}",
  "agent_id": "{agent_id}",
  "parent_agent_id": "chuang-cli",
  "started_at": "2026-05-01T00:00:00Z",
  "finished_at": "2026-05-01T00:00:01Z",
  "summary": "{summary}",
  "exit_code": 0,
  "stdout_preview": "ok",
  "stderr_preview": null,
  "resource_usage": {{
    "wall_time_ms": 1000,
    "prompt_tokens": 10,
    "completion_tokens": 5,
    "cpu_time_ms": 0,
    "peak_memory_bytes": 0
  }},
  "artifacts": [],
  "replay_ref": "queued-subagent://queued-run-1",
  "context_debug": null,
  "truncated": false
}}"#
    )
}

fn dispatch_agent_id(queue_root: &std::path::Path, run_id: &str) -> String {
    let dispatch_path = queue_root.join("dispatch").join(format!("{run_id}.json"));
    let dispatch: Value = serde_json::from_str(
        &std::fs::read_to_string(dispatch_path).expect("dispatch should exist"),
    )
    .expect("dispatch should be json");
    dispatch["agent_id"]
        .as_str()
        .expect("agent id should be string")
        .to_string()
}

fn dispatch_task(queue_root: &std::path::Path, task_id: &str, task: &str) -> Value {
    dispatch_task_with_args(queue_root, task_id, task, &[])
}

fn dispatch_task_with_capabilities(
    queue_root: &std::path::Path,
    task_id: &str,
    task: &str,
    capabilities: &[&str],
) -> Value {
    let mut extra_args = Vec::new();
    for capability in capabilities {
        extra_args.push("--requires-capability");
        extra_args.push(*capability);
    }
    dispatch_task_with_args(queue_root, task_id, task, &extra_args)
}

fn dispatch_task_with_args(
    queue_root: &std::path::Path,
    task_id: &str,
    task: &str,
    extra_args: &[&str],
) -> Value {
    let mut args = vec![
        "run",
        "--quiet",
        "--",
        "subagent",
        "dispatch",
        "--subagent-queue-root",
        queue_root.to_str().expect("temp path should be utf8"),
        "--task",
        task,
        "--task-id",
        task_id,
        "--agent-name",
        "worker",
    ];
    args.extend(extra_args);
    args.push("--json");

    let output = cargo_command()
        .args(args)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout should be json")
}
