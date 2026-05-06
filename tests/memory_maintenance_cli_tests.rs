use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-memory-maintenance-{name}-{nanos}"))
}

fn write_fake_config(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root).expect("config root should be created");
    let config_path = root.join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity-default").display()
        ),
    )
    .expect("fake config should be written");
    config_path
}

fn run_chuang(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--quiet", "--"])
        .args(args)
        .output()
        .expect("cargo run should execute")
}

fn seed_session_summary(config_path: &Path, session_id: &str, text: &str) {
    let output = run_chuang(&[
        "run",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--input",
        text,
        "--session-id",
        session_id,
        "--remember-session",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn memory_maintenance_report_batches_multiple_queries_without_writing() {
    let root = temp_root("batch-report");
    let config_path = write_fake_config(&root);
    seed_session_summary(
        &config_path,
        "maintenance",
        "批量维护候选A：网络超时先看 timeout_ms",
    );
    seed_session_summary(
        &config_path,
        "maintenance",
        "批量维护候选B：写回必须人工批准",
    );

    let output = run_chuang(&[
        "memory",
        "maintenance",
        "report",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "timeout_ms",
        "--query",
        "人工批准",
        "--session-id",
        "maintenance",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["explicit_writeback_required"], true);
    assert_eq!(parsed["queries"].as_array().expect("queries").len(), 2);
    assert_eq!(parsed["batch_count"], 2);
    assert_eq!(parsed["lim_candidate_count"], 2);
    assert_eq!(parsed["batches"][0]["lim_candidate_count"], 1);
    assert_eq!(parsed["batches"][1]["lim_candidate_count"], 1);
    assert!(
        !root.join("experiences.md").exists()
            || std::fs::read_to_string(root.join("experiences.md"))
                .expect("experiences file")
                .is_empty()
    );
}

#[test]
fn memory_maintenance_apply_dry_run_previews_selected_candidates_without_writeback() {
    let root = temp_root("apply-dry-run");
    let config_path = write_fake_config(&root);
    seed_session_summary(
        &config_path,
        "maintenance",
        "dry-run 写回候选：只预检不写 experiences",
    );

    let output = run_chuang(&[
        "memory",
        "maintenance",
        "apply",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "experiences",
        "--session-id",
        "maintenance",
        "--dry-run",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["approved_writeback"], false);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(
        parsed["selected_candidate_ids"]
            .as_array()
            .expect("selected")
            .len(),
        1
    );
    assert_eq!(
        parsed["applied_candidate_ids"]
            .as_array()
            .expect("applied")
            .len(),
        0
    );
    assert!(
        !root.join("experiences.md").exists()
            || std::fs::read_to_string(root.join("experiences.md"))
                .expect("experiences file")
                .is_empty()
    );
}

#[test]
fn memory_maintenance_decay_candidates_are_not_writeback_candidates() {
    let root = temp_root("decay-boundary");
    let config_path = write_fake_config(&root);
    let memory_body = format!("## hot\n{}\n", "热记忆".repeat(800));
    std::fs::write(root.join("MEMORY.md"), memory_body).expect("memory seed should write");

    let report = run_chuang(&[
        "memory",
        "maintenance",
        "report",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "no-match-for-lim",
        "--json",
    ]);
    assert!(
        report.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&report.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&report.stdout)).expect("stdout json");
    assert_eq!(parsed["decay_candidate_count"], 1);
    assert_eq!(
        parsed["decay_candidates"][0]["candidate_id"],
        "decay-hot-memory-review"
    );
    assert_eq!(parsed["decay_candidates"][0]["writeback_allowed"], false);

    let apply = run_chuang(&[
        "memory",
        "maintenance",
        "apply",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "no-match-for-lim",
        "--candidate-id",
        "decay-hot-memory-review",
        "--approve-writeback",
        "--json",
    ]);
    assert!(
        !apply.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&apply.stdout)
    );
    let stderr = String::from_utf8_lossy(&apply.stderr);
    assert!(stderr.contains("memory_maintenance_apply_candidate_not_writeback_candidate"));
    assert!(
        !root.join("experiences.md").exists()
            || std::fs::read_to_string(root.join("experiences.md"))
                .expect("experiences file")
                .is_empty()
    );
}
