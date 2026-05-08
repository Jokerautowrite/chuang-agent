use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn goal_mode_negative_smoke_script_rejects_not_ready_from_collect() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-goal-mode-negative-smoke.sh");
    let script =
        fs::read_to_string(&script_path).expect("goal mode negative smoke should be readable");

    assert!(script.contains("[goal-mode-negative] partial-step"));
    assert!(script.contains("[goal-mode-negative] checkpoint-from-collect-rejects"));
    assert!(script.contains("goal_mode_negative_smoke_ok"));
    assert!(script.contains("CHUANG_GOAL_MODE_NEGATIVE_SMOKE_BIN"));
    assert!(script.contains("--max-runs 1"));
    assert!(script.contains("--from-collect"));
    assert!(script.contains("goal_checkpoint_invalid: collect.ready_to_checkpoint"));
    assert!(script.contains("missing_run_ids="));
    assert!(script.contains("blocked_report_run_ids=none"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains(".codex-im/.env"));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
}

#[test]
fn goal_mode_negative_smoke_script_runs_not_ready_rejection_flow() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-goal-mode-negative-smoke.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let smoke_name = format!(
        "goal-mode-negative-smoke-test-{}-{nanos}",
        std::process::id()
    );
    let output = Command::new("sh")
        .arg(&script_path)
        .env(
            "CHUANG_GOAL_MODE_NEGATIVE_SMOKE_BIN",
            env!("CARGO_BIN_EXE_chuang-agent"),
        )
        .env("CHUANG_GOAL_MODE_NEGATIVE_SMOKE_NAME", &smoke_name)
        .current_dir(&manifest_dir)
        .output()
        .expect("goal mode negative smoke should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("goal_mode_negative_collect_ready_to_checkpoint=false"));
    assert!(stdout.contains("missing_run_ids="));
    assert!(stdout.contains("goal_checkpoint_invalid: collect.ready_to_checkpoint"));
    assert!(stdout.contains("blocked_report_run_ids=none"));
    assert!(stdout.contains("blocked_report_reasons=none"));
    let final_line = stdout
        .lines()
        .filter(|line| line.starts_with("goal_mode_negative_smoke_ok "))
        .last()
        .expect("script should print final marker");
    let fields = parse_key_value_line(final_line);

    let goal_root = PathBuf::from(
        fields
            .get("goal_root")
            .expect("goal_root should be present in final marker"),
    );
    let queue_root = PathBuf::from(
        fields
            .get("queue_root")
            .expect("queue_root should be present in final marker"),
    );

    let goal_run_path = goal_root.join("goal-mode-negative-smoke.json");
    let goal_run: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&goal_run_path).expect("goal run json should be readable"),
    )
    .expect("goal run json should parse");

    assert_eq!(goal_run["goal_spec"]["goal_id"], "goal-mode-negative-smoke");
    assert_eq!(
        goal_run["checkpoint_log"]
            .as_array()
            .expect("checkpoint log")
            .len(),
        0
    );
    assert_eq!(count_json_files(&queue_root.join("dispatch")), 2);
    assert_eq!(count_json_files(&queue_root.join("reports")), 1);
}

fn parse_key_value_line(line: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for token in line.split_whitespace().skip(1) {
        if let Some((key, value)) = token.split_once('=') {
            fields.insert(key.to_string(), value.to_string());
        }
    }
    fields
}

fn count_json_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .expect("directory should be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .count()
}
