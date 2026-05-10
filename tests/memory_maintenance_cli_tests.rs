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
fn memory_knowledge_search_json_hits_include_auditable_provenance_and_evidence() {
    let root = temp_root("knowledge-provenance");
    let knowledge_root = root.join("knowledge");
    std::fs::create_dir_all(&knowledge_root).expect("knowledge root should be created");
    std::fs::write(
        knowledge_root.join("adapter.md"),
        "外脑 provenance evidence 边界\n无关内容\n",
    )
    .expect("knowledge fixture should write");

    let output = run_chuang(&[
        "memory",
        "knowledge",
        "search",
        "--root",
        knowledge_root
            .to_str()
            .expect("knowledge path should be utf8"),
        "--query",
        "provenance",
        "--limit",
        "1",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["read_only"], true);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["hit_count"], 1);

    let hit = &parsed["hits"][0];
    assert_eq!(hit["source"], "local_file");
    assert_eq!(hit["path"], "adapter.md");
    assert_eq!(hit["line"], 1);
    assert_eq!(hit["score"], 15);
    assert_eq!(hit["provenance"]["source"], "local_file");
    assert_eq!(hit["provenance"]["adapter"], "local_external_knowledge");
    assert_eq!(hit["provenance"]["local_file"], "adapter.md");
    assert_eq!(hit["provenance"]["line"], 1);
    assert_eq!(hit["provenance"]["score"], 15);
    assert_eq!(hit["provenance"]["query"], "provenance");
    assert_eq!(hit["provenance"]["read_only"], true);
    assert_eq!(hit["provenance"]["connects_real_service"], false);
    assert_eq!(hit["provenance"]["writes_automatically"], false);
    assert_eq!(hit["evidence"]["kind"], "line_match");
    assert_eq!(hit["evidence"]["local_file"], "adapter.md");
    assert_eq!(hit["evidence"]["line"], 1);
    assert_eq!(hit["evidence"]["score"], 15);
    assert_eq!(hit["evidence"]["query"], "provenance");
    assert_eq!(hit["evidence"]["read_only"], true);
    assert_eq!(hit["evidence"]["connects_real_service"], false);
    assert!(hit["evidence"]["preview"]
        .as_str()
        .expect("preview")
        .contains("provenance"));
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
    assert_eq!(
        parsed["boundary"]["archive_layer"],
        "history_session_archive"
    );
    assert_eq!(parsed["boundary"]["archive_read_only"], true);
    assert_eq!(parsed["boundary"]["archive_mutation_allowed"], false);
    assert_eq!(
        parsed["boundary"]["decay_boundary"],
        "review_only_not_writeback_candidate"
    );
    assert_eq!(parsed["boundary"]["decay_writeback_allowed"], false);
    assert_eq!(parsed["boundary"]["core_memory_rewrite_allowed"], false);
    assert_eq!(parsed["boundary"]["automatic_writeback"], false);
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
        parsed["boundary"]["maintenance_mode"],
        "dry_run_report_then_explicit_apply"
    );
    assert_eq!(parsed["boundary"]["writeback_target"], "experiences.md");
    assert_eq!(parsed["boundary"]["lim_writeback_requires_approval"], true);
    assert_eq!(parsed["approval"]["required"], true);
    assert_eq!(parsed["approval"]["approved"], false);
    assert_eq!(parsed["approval"]["writes_automatically"], false);
    assert_eq!(parsed["requested_candidate_count"], 1);
    assert_eq!(parsed["duplicate_candidate_count"], 0);
    assert_eq!(parsed["selection_state"], "selected");
    assert_eq!(parsed["selection_reason"], "plan_candidates");
    assert_eq!(
        parsed["selected_candidate_ids"]
            .as_array()
            .expect("selected")
            .len(),
        1
    );
    assert_eq!(
        parsed["selected_candidates"]
            .as_array()
            .expect("selected candidates")
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
fn memory_maintenance_apply_approved_writeback_records_approval_and_provenance() {
    let root = temp_root("approved-writeback");
    let config_path = write_fake_config(&root);
    seed_session_summary(
        &config_path,
        "maintenance",
        "批准写回候选：保留 provenance 和人工批准记录",
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
        "provenance",
        "--session-id",
        "maintenance",
        "--approve-writeback",
        "--approval-note",
        "老爸批准写入 LIM 候选",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["dry_run"], false);
    assert_eq!(parsed["approved_writeback"], true);
    assert_eq!(parsed["approval"]["required"], true);
    assert_eq!(parsed["approval"]["approved"], true);
    assert_eq!(
        parsed["approval"]["approval_source"],
        "cli --approve-writeback"
    );
    assert_eq!(parsed["approval"]["approval_note"], "老爸批准写入 LIM 候选");
    assert_eq!(parsed["approval"]["writeback_scope"], "experiences");
    assert_eq!(parsed["approval"]["writes_automatically"], false);
    assert_eq!(parsed["requested_candidate_count"], 1);
    assert_eq!(parsed["duplicate_candidate_count"], 0);
    assert_eq!(parsed["selection_state"], "selected");
    assert_eq!(parsed["selection_reason"], "plan_candidates");
    assert!(parsed["approval"]["approved_at"]
        .as_str()
        .expect("approved_at")
        .contains('T'));
    assert_eq!(
        parsed["selected_candidates"]
            .as_array()
            .expect("selected candidates")
            .len(),
        1
    );
    assert_eq!(
        parsed["applied_candidate_ids"]
            .as_array()
            .expect("applied")
            .len(),
        1
    );

    let experiences = std::fs::read_to_string(root.join("experiences.md"))
        .expect("experiences file should be readable");
    assert!(experiences.contains("writeback=memory_maintenance_apply"));
    assert!(experiences.contains("approved_writeback=true"));
    assert!(experiences.contains("approval_source=cli --approve-writeback"));
    assert!(experiences.contains("approval_note=老爸批准写入 LIM 候选"));
    assert!(experiences.contains("provenance_preserved=true"));
    assert!(experiences.contains("source=lim_dry_run"));
    assert!(experiences.contains("source_record_id="));
}

#[test]
fn memory_maintenance_apply_repeated_writeback_skips_existing_candidate() {
    let root = temp_root("repeat-writeback");
    let config_path = write_fake_config(&root);
    seed_session_summary(&config_path, "maintenance", "重复写回应跳过已存在候选");

    let first = run_chuang(&[
        "memory",
        "maintenance",
        "apply",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "重复写回",
        "--session-id",
        "maintenance",
        "--approve-writeback",
        "--json",
    ]);
    assert!(
        first.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_chuang(&[
        "memory",
        "maintenance",
        "apply",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "重复写回",
        "--session-id",
        "maintenance",
        "--approve-writeback",
        "--json",
    ]);
    assert!(
        second.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&second.stdout)).expect("stdout json");
    assert_eq!(parsed["approved_writeback"], true);
    assert_eq!(parsed["selection_state"], "selected");
    assert_eq!(parsed["selection_reason"], "plan_candidates");
    assert_eq!(
        parsed["applied_candidate_ids"]
            .as_array()
            .expect("applied candidate ids")
            .len(),
        0
    );
    assert_eq!(
        parsed["skipped_candidate_ids"]
            .as_array()
            .expect("skipped candidate ids")
            .len(),
        1
    );
}

#[test]
fn memory_maintenance_apply_deduplicates_repeated_candidate_ids_and_tracks_selection_metrics() {
    let root = temp_root("dedupe-request");
    let config_path = write_fake_config(&root);
    seed_session_summary(&config_path, "maintenance", "重复候选应去重且只写一次");

    let report = run_chuang(&[
        "memory",
        "maintenance",
        "report",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "重复候选",
        "--session-id",
        "maintenance",
        "--json",
    ]);
    assert!(
        report.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&report.stderr)
    );
    let report_parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&report.stdout)).expect("stdout json");
    let candidate_id = report_parsed["lim_candidates"][0]["candidate_id"]
        .as_str()
        .expect("candidate id")
        .to_string();

    let output = run_chuang(&[
        "memory",
        "maintenance",
        "apply",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "重复候选",
        "--session-id",
        "maintenance",
        "--candidate-id",
        &candidate_id,
        "--candidate-id",
        &candidate_id,
        "--approve-writeback",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["requested_candidate_count"], 2);
    assert_eq!(parsed["duplicate_candidate_count"], 1);
    assert_eq!(parsed["selection_state"], "deduplicated");
    assert_eq!(
        parsed["selection_reason"],
        "duplicate_candidate_ids_deduplicated"
    );
    assert_eq!(
        parsed["duplicate_candidate_ids"]
            .as_array()
            .expect("duplicate candidate ids")
            .len(),
        1
    );
    assert_eq!(
        parsed["selected_candidate_ids"]
            .as_array()
            .expect("selected candidate ids")
            .len(),
        1
    );
    assert_eq!(
        parsed["applied_candidate_ids"]
            .as_array()
            .expect("applied")
            .len(),
        1
    );
}

#[test]
fn memory_maintenance_apply_noops_when_no_lim_candidates_exist() {
    let root = temp_root("empty-selection");
    let config_path = write_fake_config(&root);

    let output = run_chuang(&[
        "memory",
        "maintenance",
        "apply",
        "--config",
        config_path.to_str().expect("config path should be utf8"),
        "--identity-memory-root",
        root.to_str().expect("temp path should be utf8"),
        "--query",
        "no-match-for-lim",
        "--session-id",
        "maintenance",
        "--approve-writeback",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    assert_eq!(parsed["requested_candidate_count"], 0);
    assert_eq!(parsed["duplicate_candidate_count"], 0);
    assert_eq!(parsed["selection_state"], "empty");
    assert_eq!(parsed["selection_reason"], "no_lim_candidates");
    assert_eq!(
        parsed["selected_candidate_ids"]
            .as_array()
            .expect("selected candidate ids")
            .len(),
        0
    );
    assert_eq!(
        parsed["applied_candidate_ids"]
            .as_array()
            .expect("applied candidate ids")
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
