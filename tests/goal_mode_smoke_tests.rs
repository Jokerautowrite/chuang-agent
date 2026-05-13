use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn goal_mode_smoke_script_is_closed_loop_only_and_readonly() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-goal-mode-smoke.sh");
    let script =
        fs::read_to_string(&script_path).expect("goal mode smoke script should be readable");

    assert!(script.contains("[goal-mode] plan"));
    assert!(script.contains("[goal-mode] dispatch"));
    assert!(script.contains("[goal-mode] step"));
    assert!(script.contains("[goal-mode] collect"));
    assert!(script.contains("[goal-mode] checkpoint-from-collect"));
    assert!(script.contains("handoff_query_summary"));
    assert!(script.contains("step_summary = data[\"collection\"][\"handoff_query_summary\"]"));
    assert!(script.contains("step_summary[\"report_admission_ref_count\"] == 2"));
    assert!(script.contains("step_summary[\"report_admission_reason_codes\"] == {\"report_validated\": 2}"));
    assert!(script.contains("len(step_summary[\"report_admission_refs\"]) == 2"));
    assert!(script.contains("goal-report-admission://"));
    assert!(script.contains("goal_mode_smoke_ok"));
    assert!(script.contains("CHUANG_GOAL_MODE_SMOKE_BIN"));
    assert!(script.contains("--runner fake"));
    assert!(script.contains("--from-collect"));
    assert!(script.contains("--subagent-queue-root \"$queue_root\""));
    assert!(script.contains("goal_operability\"][\"goal_collect\"][\"handoff_query_summary\"]"));
    assert!(!script.contains("--validation-note"));
    assert!(!script.contains("--completed-worker-id"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains(".codex-im/.env"));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
}

#[test]
fn goal_mode_smoke_script_runs_full_goal_closed_loop() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-goal-mode-smoke.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let smoke_name = format!("goal-mode-smoke-test-{}-{nanos}", std::process::id());
    let output = Command::new("sh")
        .arg(&script_path)
        .env(
            "CHUANG_GOAL_MODE_SMOKE_BIN",
            env!("CARGO_BIN_EXE_chuang-agent"),
        )
        .env("CHUANG_GOAL_MODE_SMOKE_NAME", &smoke_name)
        .current_dir(&manifest_dir)
        .output()
        .expect("goal mode smoke script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let final_line = stdout
        .lines()
        .filter(|line| line.starts_with("goal_mode_smoke_ok "))
        .last()
        .expect("script should print final marker");
    let fields = parse_key_value_line(final_line);

    let work_dir = PathBuf::from(
        fields
            .get("work_dir")
            .expect("work_dir should be present in final marker"),
    );
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
    let checkpoint_id = fields
        .get("checkpoint_id")
        .expect("checkpoint_id should be present in final marker");

    assert!(
        work_dir.exists(),
        "work dir should be preserved for inspection"
    );
    assert!(goal_root.exists(), "goal root should exist");
    assert!(queue_root.exists(), "queue root should exist");

    let goal_run_path = goal_root.join("goal-mode-smoke.json");
    let goal_dispatch_path = goal_root.join("goal-mode-smoke.dispatch.json");
    let goal_run: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&goal_run_path).expect("goal run json should be readable"),
    )
    .expect("goal run json should parse");

    assert_eq!(goal_run["goal_spec"]["goal_id"], "goal-mode-smoke");
    assert_eq!(
        goal_run["checkpoint_log"]
            .as_array()
            .expect("checkpoint log")
            .len(),
        1
    );
    assert_eq!(
        goal_run["checkpoint_log"][0]["checkpoint_id"],
        checkpoint_id.as_str()
    );
    assert_eq!(
        goal_run["checkpoint_log"][0]["summary"],
        "checkpoint ready for goal_id=goal-mode-smoke workers=goal-worker-1 | goal-worker-2"
    );
    assert!(
        goal_dispatch_path.exists(),
        "dispatch manifest should exist"
    );

    let dispatch_dir = queue_root.join("dispatch");
    let report_dir = queue_root.join("reports");
    let dispatch_count = count_json_files(&dispatch_dir);
    let report_count = count_json_files(&report_dir);
    assert_eq!(dispatch_count, 2);
    assert_eq!(report_count, 2);
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
