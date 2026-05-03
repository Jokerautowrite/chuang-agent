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
            "LIM候选经验：命令超时需要看 timeout_ms",
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
