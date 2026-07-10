use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-identity-memory-{name}-{nanos}"))
}

fn write_fake_config(root: &std::path::Path) -> PathBuf {
    write_fake_config_with_subagent(root, false)
}

fn write_fake_config_with_subagent(root: &std::path::Path, queued_external: bool) -> PathBuf {
    std::fs::create_dir_all(root).expect("config root should be created");
    let config_path = root.join("config.toml");
    let mut config = format!(
        "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
        root.join("memory.db").display(),
        root.join("identity-default").display()
    );
    if queued_external {
        config.push_str(&format!(
            "subagent = \"queued_external\"\nsubagent_queue_root = \"{}\"\n",
            root.join("subagent-queue").display()
        ));
    }
    std::fs::write(&config_path, config).expect("fake config should be written");
    config_path
}

#[test]
fn cli_identity_memory_show_append_and_compact_memory() {
    let root = temp_root("flow");
    let config_path = write_fake_config(&root);

    let append = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "append",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--id",
            "mem-1",
            "--content",
            "老爸偏好简洁中文进度",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        append.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&append.stderr)
    );

    let shown = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "show",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        shown.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&shown.stdout)).expect("stdout json");
    assert_eq!(parsed["user_chars"], 0);
    assert_eq!(parsed["user_max_chars"], 1375);
    assert_eq!(parsed["memory_max_chars"], 2200);
    assert_eq!(parsed["experiences_file"], "experiences.md");
    assert_eq!(parsed["experiences_chars"], 0);
    assert_eq!(parsed["experiences"], "");
    assert!(parsed["memory"]
        .as_str()
        .expect("memory string")
        .contains("## mem-1"));
    assert!(root.join("experiences.md").exists());

    let compact = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "write-memory",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--content",
            "## compact-1\n老爸偏好简洁中文进度\n",
            "--approve-overwrite",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        compact.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&compact.stderr)
    );
    let compacted: Value =
        serde_json::from_str(&String::from_utf8_lossy(&compact.stdout)).expect("stdout json");
    assert_eq!(compacted["scope"], "memory");
    assert_eq!(compacted["replaced"], true);

    let memory = std::fs::read_to_string(root.join("MEMORY.md")).expect("memory file");
    assert_eq!(memory, "## compact-1\n老爸偏好简洁中文进度\n");
}

#[test]
fn cli_identity_memory_can_append_experience_entry() {
    let root = temp_root("experience-flow");
    let config_path = write_fake_config(&root);

    let append = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "append-experience",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--id",
            "exp-1",
            "--content",
            "source=manual\nlesson=工具失败先看结构化 stderr",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        append.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&append.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&append.stdout)).expect("stdout json");
    assert_eq!(parsed["scope"], "experiences");
    assert_eq!(parsed["id"], "exp-1");

    let experiences = std::fs::read_to_string(root.join("experiences.md"))
        .expect("experiences file should be readable");
    assert!(experiences.contains("## exp-1"));
    assert!(experiences.contains("source=manual"));
    assert!(experiences.contains("lesson=工具失败先看结构化 stderr"));
}

#[test]
fn cli_run_remember_session_identity_experience_and_dispatch_succeeds_together() {
    let root = temp_root("remember-run-flow");
    let config_path = write_fake_config_with_subagent(&root, true);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--input",
            "把这轮沉淀到 session 和 identity",
            "--session-id",
            "alpha",
            "--remember-session",
            "--remember-identity",
            "--remember-experience",
            "--dispatch-subagent",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let memory = std::fs::read_to_string(root.join("identity-default").join("MEMORY.md"))
        .expect("memory file should exist");
    assert!(memory.contains("user=把这轮沉淀到 session 和 identity"));

    let experiences = std::fs::read_to_string(root.join("identity-default").join("experiences.md"))
        .expect("experiences file should exist");
    assert!(experiences.contains("source=runtime_turn"));
    assert!(experiences.contains("user=把这轮沉淀到 session 和 identity"));

    let dispatch_dir = root.join("subagent-queue").join("dispatch");
    let dispatch_entries: Vec<_> = std::fs::read_dir(&dispatch_dir)
        .expect("dispatch dir should exist")
        .collect::<Result<Vec<_>, _>>()
        .expect("dispatch dir should be readable");
    assert_eq!(dispatch_entries.len(), 1);

    let db_path = root.join("memory.db");
    let archive = chuang_agent::session_archive::SqliteSessionArchive::open(&db_path)
        .expect("session archive should open");
    let archived = archive.replay("alpha").expect("replay should succeed");
    assert_eq!(archived.len(), 1);
    assert_eq!(
        archived[0].raw_user_input,
        "把这轮沉淀到 session 和 identity"
    );
}

#[test]
fn cli_memory_session_search_filters_by_session_id() {
    let root = temp_root("session-search");
    let config_path = write_fake_config(&root);

    for (session_id, text) in [("alpha", "历史会话锚点A"), ("beta", "历史会话锚点B")] {
        let output = Command::new("cargo")
            .args([
                "run",
                "--quiet",
                "--",
                "run",
                "--config",
                config_path.to_str().expect("config path should be utf8"),
                "--input",
                text,
                "--session-id",
                session_id,
                "--remember-session",
            ])
            .output()
            .expect("cargo run should execute");
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let search = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "session",
            "search",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--query",
            "历史会话锚点B",
            "--session-id",
            "alpha",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        search.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&search.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&search.stdout)).expect("stdout json");
    assert_eq!(parsed["query"], "历史会话锚点B");
    assert_eq!(parsed["session_id"], "alpha");
    assert_eq!(parsed["hit_count"], 0);

    let same_session = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "session",
            "search",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--query",
            "历史会话锚点B",
            "--session-id",
            "beta",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        same_session.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&same_session.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&same_session.stdout)).expect("stdout json");
    assert_eq!(parsed["hit_count"], 1);
    assert!(parsed["hits"][0]["content"]
        .as_str()
        .expect("content should be string")
        .contains("历史会话锚点B"));
    assert_eq!(parsed["hits"][0]["metadata"]["session_id"], "beta");
}

#[test]
fn cli_memory_lim_extract_returns_dry_run_candidates() {
    let root = temp_root("lim-extract");
    let config_path = write_fake_config(&root);

    let run = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--input",
            "LIM候选经验：请求超时需要看 timeout_ms",
            "--session-id",
            "alpha",
            "--remember-session",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );

    let extract = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "lim",
            "extract",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--query",
            "timeout_ms",
            "--session-id",
            "alpha",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        extract.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&extract.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&extract.stdout)).expect("stdout json");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["candidate_count"], 1);
    assert!(parsed["candidates"][0]["candidate_id"]
        .as_str()
        .expect("candidate id string")
        .starts_with("lim-candidate-turn-memory-session-alpha-turn-1-"));
    assert_eq!(parsed["candidates"][0]["proposed_scope"], "experiences");
    assert!(parsed["candidates"][0]["content"]
        .as_str()
        .expect("content should be string")
        .contains("source=lim_dry_run"));
}

#[test]
fn cli_memory_maintenance_report_is_dry_run_and_reuses_lim_candidates() {
    let root = temp_root("maintenance-report");
    let config_path = write_fake_config(&root);

    let run = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--input",
            "维护报告候选：长期记忆写回必须人工确认",
            "--session-id",
            "maintenance-session",
            "--remember-session",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );

    let report = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "maintenance",
            "report",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--query",
            "人工确认",
            "--session-id",
            "maintenance-session",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        report.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&report.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&report.stdout)).expect("stdout json");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["session_id"], "maintenance-session");
    assert_eq!(
        parsed["identity_health"]["experiences_file"],
        "experiences.md"
    );
    assert_eq!(parsed["lim_candidate_count"], 1);
    assert_eq!(parsed["lim_candidates"][0]["proposed_scope"], "experiences");
    assert!(parsed["recommendations"]
        .as_array()
        .expect("recommendations")
        .iter()
        .any(|recommendation| recommendation
            .as_str()
            .expect("recommendation")
            .contains("manually")));
    assert!(
        !root.join("experiences.md").exists() || {
            std::fs::read_to_string(root.join("experiences.md"))
                .expect("experiences file")
                .is_empty()
        }
    );

    let apply = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "maintenance",
            "apply",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--query",
            "人工确认",
            "--session-id",
            "maintenance-session",
            "--approve-writeback",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        apply.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&apply.stdout)).expect("stdout json");
    assert_eq!(parsed["dry_run"], false);
    assert_eq!(parsed["approved_writeback"], true);
    assert_eq!(
        parsed["applied_candidate_ids"]
            .as_array()
            .expect("applied")
            .len(),
        1
    );
    assert_eq!(
        parsed["skipped_candidate_ids"]
            .as_array()
            .expect("skipped")
            .len(),
        0
    );

    let experiences = std::fs::read_to_string(root.join("experiences.md"))
        .expect("experiences file should be readable");
    assert!(experiences.contains("source=lim_dry_run"));
    assert!(experiences.contains("## lim-candidate-"));

    let apply_again = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "maintenance",
            "apply",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--query",
            "人工确认",
            "--session-id",
            "maintenance-session",
            "--approve-writeback",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        apply_again.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&apply_again.stderr)
    );
    let parsed_again: Value =
        serde_json::from_str(&String::from_utf8_lossy(&apply_again.stdout)).expect("stdout json");
    assert_eq!(
        parsed_again["applied_candidate_ids"]
            .as_array()
            .expect("applied")
            .len(),
        0
    );
    assert_eq!(
        parsed_again["skipped_candidate_ids"]
            .as_array()
            .expect("skipped")
            .len(),
        1
    );
}

#[test]
fn cli_memory_knowledge_status_is_read_only_contract() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "knowledge",
            "status",
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
    assert_eq!(parsed["adapter"], "external_knowledge");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["read_only"], true);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["runtime_retrieval_wired"], false);
    assert_eq!(parsed["doc"], "docs/external-knowledge-adapter.md");
    assert!(parsed["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .any(|source| source["name"] == "wiki" && source["state"] == "documented_only"));
    assert!(parsed["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .any(|source| source["name"] == "gbrain" && source["state"] == "documented_only"));
}

#[test]
fn cli_memory_knowledge_search_reads_local_docs_only() {
    let root = temp_root("knowledge-search");
    std::fs::create_dir_all(root.join("wiki")).expect("wiki dir should be created");
    std::fs::write(
        root.join("wiki").join("memory.md"),
        "外脑知识库用于 provenance 检索\n第二行不命中\n",
    )
    .expect("knowledge doc should write");
    std::fs::write(
        root.join("wiki").join("secret-token.md"),
        "外脑知识库 secret should stay unread\n",
    )
    .expect("sensitive doc should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "knowledge",
            "search",
            "--root",
            root.to_str().expect("temp path should be utf8"),
            "--query",
            "provenance",
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
    assert_eq!(parsed["adapter"], "local_external_knowledge");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["read_only"], true);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["runtime_retrieval_wired"], false);
    assert_eq!(parsed["query"], "provenance");
    assert_eq!(parsed["hit_count"], 1);
    assert_eq!(parsed["hits"][0]["source"], "local_file");
    assert_eq!(parsed["hits"][0]["path"], "wiki/memory.md");
    assert_eq!(parsed["hits"][0]["line"], 1);
    assert!(parsed["hits"][0]["preview"]
        .as_str()
        .expect("preview")
        .contains("provenance"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret should stay unread"));
}

#[test]
fn cli_memory_knowledge_preview_context_is_read_only_preview_contract() {
    let root = temp_root("knowledge-preview");
    std::fs::create_dir_all(root.join("wiki")).expect("wiki dir should be created");
    std::fs::write(
        root.join("wiki").join("memory.md"),
        "外脑 preview-context 用于 future runtime injection candidates\n第二行不命中\n",
    )
    .expect("knowledge doc should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "knowledge",
            "preview-context",
            "--root",
            root.to_str().expect("temp path should be utf8"),
            "--query",
            "preview-context",
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
    assert_eq!(parsed["adapter"], "local_external_knowledge");
    assert_eq!(parsed["preview"], true);
    assert_eq!(parsed["read_only"], true);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["runtime_injection_applied"], false);
    assert_eq!(parsed["runtime_retrieval_wired"], false);
    assert_eq!(parsed["segment_count"], 1);
    assert_eq!(parsed["segments"][0]["source"], "local_file");
    assert_eq!(parsed["segments"][0]["path"], "wiki/memory.md");
    assert_eq!(parsed["segments"][0]["line"], 1);
    assert_eq!(parsed["segments"][0]["read_only"], true);
    assert_eq!(parsed["segments"][0]["connects_real_service"], false);
    assert_eq!(parsed["segments"][0]["writes_automatically"], false);
    assert_eq!(parsed["segments"][0]["runtime_injection_applied"], false);
    assert_eq!(parsed["segments"][0]["runtime_retrieval_wired"], false);
    assert_eq!(
        parsed["segments"][0]["provenance"]["adapter"],
        "local_external_knowledge"
    );
    assert_eq!(parsed["segments"][0]["evidence"]["kind"], "line_match");
    assert!(parsed["segments"][0]["preview"]
        .as_str()
        .expect("preview")
        .contains("preview-context"));
    assert!(
        parsed["segments"][0]["token_estimate"]
            .as_u64()
            .expect("token estimate")
            > 0
    );

    let text = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "knowledge",
            "preview-context",
            "--root",
            root.to_str().expect("temp path should be utf8"),
            "--query",
            "preview-context",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        text.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&text.stderr)
    );
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(stdout.contains("memory_knowledge_preview_context"));
    assert!(stdout.contains("preview=true"));
    assert!(stdout.contains("runtime context preview only"));
    assert!(stdout.contains("runtime_injection_applied=false"));
}

#[test]
fn cli_run_knowledge_context_preview_is_model_facing_local_only_boundary() {
    let root = temp_root("knowledge-run-preview");
    let config_path = write_fake_config(&root);
    let knowledge_root = root.join("knowledge");
    std::fs::create_dir_all(&knowledge_root).expect("knowledge root should be created");
    std::fs::write(
        knowledge_root.join("wiki.md"),
        "model-facing knowledge preview marker remains local only\n",
    )
    .expect("knowledge doc should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--context-max-tokens",
            "5000",
            "--input",
            "检查外脑上下文边界",
            "--enable-knowledge-context-preview",
            "--knowledge-context-root",
            knowledge_root
                .to_str()
                .expect("knowledge root should be utf8"),
            "--knowledge-context-query",
            "model-facing",
        ])
        .output()
        .expect("cargo run should execute");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("knowledge_context_preview_enabled: true"));
    assert!(stdout.contains("knowledge_context_injected: true"));
    assert!(stdout.contains("knowledge_context_model_facing: true"));
    assert!(stdout.contains("knowledge_context_source_boundary: local_markdown_text_preview_only"));
    assert!(stdout.contains("knowledge_context_live_wiki_gbrain_connected: false"));
    assert!(stdout.contains("knowledge_context_connects_real_service: false"));
    assert!(stdout.contains("knowledge_context_runtime_retrieval_wired: false"));
}

#[test]
fn cli_memory_knowledge_source_contract_documents_wiki_gbrain_boundary() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "knowledge",
            "source-contract",
            "--source",
            "wiki",
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
    assert_eq!(parsed["source"], "wiki");
    assert_eq!(parsed["adapter"], "wiki_readonly_external_knowledge");
    assert_eq!(parsed["read_only"], true);
    assert_eq!(parsed["live_adapter_configured"], false);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_automatically"], false);
    assert_eq!(parsed["runtime_retrieval_wired"], false);
    assert_eq!(parsed["boundary"]["requires_operator_credentials"], true);
    assert_eq!(parsed["boundary"]["stores_secret_in_repo"], false);
    assert_eq!(parsed["boundary"]["writes_core_memory"], false);
    assert_eq!(parsed["boundary"]["requires_provenance"], true);
    assert_eq!(parsed["boundary"]["requires_evidence"], true);
    assert!(parsed["response_fields"]
        .as_array()
        .expect("response fields")
        .iter()
        .any(|field| field == "hits[].provenance"));
}

#[test]
fn cli_identity_memory_write_memory_rejects_over_limit_without_mutation() {
    let root = temp_root("write-memory-limit");
    let config_path = write_fake_config(&root);
    std::fs::write(root.join("MEMORY.md"), "## seed-1\nabc\n").expect("memory seed should write");
    let before = std::fs::read_to_string(root.join("MEMORY.md")).expect("memory file");
    let oversized_content = format!("## over-limit\n{}\n", "0".repeat(2300));

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "write-memory",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--content",
            oversized_content.as_str(),
            "--approve-overwrite",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("identity_memory_hard_limit_exceeded"));
    assert_eq!(
        std::fs::read_to_string(root.join("MEMORY.md")).expect("memory file"),
        before
    );
}

#[test]
fn cli_identity_memory_write_requires_explicit_overwrite_approval() {
    let root = temp_root("approval");
    let config_path = write_fake_config(&root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "memory",
            "identity",
            "write-user",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--content",
            "老爸",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("identity_memory_write_requires_approve_overwrite"));
}
