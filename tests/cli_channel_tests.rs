use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};

use serde_json::Value;

fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-channel-{name}-{nanos}"))
}

fn write_workspace_config(workspace: &PathBuf) {
    fs::create_dir_all(workspace.join("identity")).expect("identity dir should create");
    fs::create_dir_all(workspace.join("rules")).expect("rules dir should create");
    fs::write(workspace.join("identity/SOUL.md"), "Channel test soul\n")
        .expect("soul should write");
    fs::write(workspace.join("identity/STORY.md"), "Channel test story\n")
        .expect("story should write");
    fs::write(
        workspace.join("identity/FIRST_WAKE.md"),
        "Channel test first wake\n",
    )
    .expect("first wake should write");
    fs::write(workspace.join("identity/agents.toml"), "[agents]\n").expect("agents should write");
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep channel replies concise.\n",
    )
    .expect("rules should write");
    fs::write(
        workspace.join("config.toml"),
        r#"
db_path = "./data/chuang-agent.db"
identity_memory_root = "./data/hermes-memory"
identity_root = "./identity"
soul_path = "./identity/SOUL.md"
story_path = "./identity/STORY.md"
first_wake_path = "./identity/FIRST_WAKE.md"
agents_registry_path = "./identity/agents.toml"
rules_root = "./rules"
rules_core_path = "./rules/core.md"

provider = "openai_compatible"
provider_id = "channel-openai"
base_url = "https://api.example.com/v1"
model = "gpt-channel-test"
api_key_env = "CHUANG_AGENT_CHANNEL_TEST_API_KEY"
transport = "stub"
"#,
    )
    .expect("config should write");
}

#[test]
fn cli_channel_simulate_runs_workspace_config_without_fake_responder() {
    let workspace = temp_workspace("simulate");
    write_workspace_config(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("simulate")
        .arg("--workspace-root")
        .arg(&workspace)
        .arg("--message-id")
        .arg("msg-1")
        .arg("--sender-id")
        .arg("user-1")
        .arg("--thread-id")
        .arg("thread-1")
        .arg("--text")
        .arg("还在吗？")
        .arg("--json")
        .env("CHUANG_AGENT_CHANNEL_TEST_API_KEY", "test-key")
        .output()
        .expect("channel simulate should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(parsed["inbound"]["channel"], "feishu-dedicated-chuang");
    assert_eq!(parsed["app_server_request"]["method"], "turn/start");
    assert_eq!(parsed["app_server_request"]["params"]["text"], "还在吗？");
    assert_eq!(parsed["outbound"]["thread_id"], "thread-1");
    assert_eq!(parsed["runtime_report_id"], "report-turn-1");
    assert_eq!(parsed["model_name"], "gpt-channel-test");
    assert_eq!(parsed["tool_call_count"], 0);
    assert_eq!(parsed["tool_protocol_error_count"], 0);
    assert_eq!(
        parsed["runtime_observability"]["model_name"],
        "gpt-channel-test"
    );
    assert_eq!(parsed["runtime_observability"]["session_id"], "thread-1");
    assert_eq!(
        parsed["runtime_observability"]["session_memory_scope"],
        "session"
    );
    assert_eq!(parsed["runtime_observability"]["tool_call_count"], "0");
    assert_eq!(
        parsed["runtime_observability"]["knowledge_context_preview_enabled"],
        "false"
    );
    assert_eq!(
        parsed["runtime_observability"]["knowledge_context_preview_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["knowledge_context_injected_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["knowledge_context_dropped_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["knowledge_context_dropped_segment_ids"],
        "[]"
    );
    assert_eq!(parsed["tool_trace"], "");
    assert!(parsed["tool_report"].is_null());
    assert_eq!(parsed["tool_surface"]["available"], true);
    assert_eq!(parsed["tool_surface"]["governed"], true);
    assert!(parsed["tool_surface"]["callable_tools"]
        .as_array()
        .expect("callable tools should be array")
        .iter()
        .any(|tool| tool == "file_read"));
    assert!(parsed["tool_surface"]["callable_tools"]
        .as_array()
        .expect("callable tools should be array")
        .iter()
        .any(|tool| tool == "list_dir"));
    assert_eq!(
        parsed["runtime_observability"]["tool_surface_available"],
        "true"
    );
    assert_eq!(
        parsed["runtime_observability"]["tool_surface_governed"],
        "true"
    );
    assert_eq!(
        parsed["tool_calls"]
            .as_array()
            .expect("tool calls should be array")
            .len(),
        0
    );
    assert_eq!(
        parsed["tool_protocol_errors"]
            .as_array()
            .expect("tool protocol errors should be array")
            .len(),
        0
    );
    assert_eq!(
        parsed["tool_events"]
            .as_array()
            .expect("tool events should be array")
            .len(),
        0
    );
    assert!(parsed["outbound"]["text"]
        .as_str()
        .expect("outbound text")
        .contains("stubbed_post_ok"));
    assert!(!parsed["outbound"]["text"]
        .as_str()
        .expect("outbound text")
        .contains("fake-responder"));
}

#[test]
fn cli_channel_simulate_can_forward_goal_context() {
    let workspace = temp_workspace("simulate-goal");
    write_workspace_config(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("simulate")
        .arg("--workspace-root")
        .arg(&workspace)
        .arg("--message-id")
        .arg("msg-goal-1")
        .arg("--sender-id")
        .arg("user-1")
        .arg("--thread-id")
        .arg("thread-goal-1")
        .arg("--text")
        .arg("继续推进")
        .arg("--goal")
        .arg("稳定完成 goal 通道接入")
        .arg("--json")
        .env("CHUANG_AGENT_CHANNEL_TEST_API_KEY", "test-key")
        .output()
        .expect("channel simulate should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(
        parsed["app_server_request"]["params"]["goal"],
        "稳定完成 goal 通道接入"
    );
    assert_eq!(parsed["provider_meta"]["goal_context_injected"], "true");
    assert_eq!(parsed["runtime_report_id"], "report-turn-1");
    assert_eq!(
        parsed["provider_meta"]["goal_objective"],
        "稳定完成 goal 通道接入"
    );
    assert_eq!(parsed["provider_meta"]["goal_id"], "mainline-mvp");
    assert_eq!(parsed["runtime_observability"]["goal_id"], "mainline-mvp");
    assert_eq!(
        parsed["runtime_observability"]["goal_objective"],
        "稳定完成 goal 通道接入"
    );
    assert_eq!(
        parsed["runtime_observability"]["session_id"],
        "thread-goal-1"
    );
    assert!(parsed["outbound"]["text"]
        .as_str()
        .expect("outbound text")
        .contains("stubbed_post_ok"));
}

#[test]
fn cli_channel_simulate_rejects_empty_text() {
    let workspace = temp_workspace("empty-text");
    write_workspace_config(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("simulate")
        .arg("--workspace-root")
        .arg(&workspace)
        .arg("--message-id")
        .arg("msg-1")
        .arg("--sender-id")
        .arg("user-1")
        .arg("--text")
        .arg(" ")
        .env("CHUANG_AGENT_CHANNEL_TEST_API_KEY", "test-key")
        .output()
        .expect("channel simulate should execute");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("channel message requires text"));
}

#[test]
fn cli_channel_feishu_check_validates_dedicated_env_without_leaking_values() {
    let workspace = temp_workspace("feishu-check");
    fs::create_dir_all(&workspace).expect("workspace should create");
    fs::write(workspace.join("config.toml"), "provider = \"fake\"\n").expect("config should write");
    let env_file = workspace.join("chuang-feishu.env");
    fs::write(
        &env_file,
        format!(
            r#"
CHUANG_AGENT_WORKSPACE_ROOT={}
CHUANG_FEISHU_APP_ID=cli_a_test
CHUANG_FEISHU_APP_SECRET=secret-value
"#,
            workspace.display()
        ),
    )
    .expect("env should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("feishu-check")
        .arg("--env-file")
        .arg(&env_file)
        .arg("--json")
        .output()
        .expect("feishu-check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["diagnostic_status"], "ready");
    assert!(parsed["diagnostic_summary"]
        .as_str()
        .expect("diagnostic summary")
        .contains("no live Feishu call"));
    assert_eq!(
        parsed["next_actions"]
            .as_array()
            .expect("next actions should be array")
            .len(),
        0
    );
    assert_eq!(parsed["workspace_root"], workspace.display().to_string());
    assert_eq!(parsed["workspace_root_exists"], true);
    assert_eq!(parsed["workspace_config_exists"], true);
    assert_eq!(parsed["env_file_is_chuang_scoped"], true);
    assert_eq!(
        parsed["env_file_scope_warnings"]
            .as_array()
            .expect("scope warnings should be array")
            .len(),
        0
    );
    assert_eq!(parsed["required_vars"]["CHUANG_FEISHU_APP_SECRET"], "<set>");
    assert_eq!(parsed["connection_mode"], "websocket");
    assert_eq!(parsed["connection_mode_ok"], true);
    assert_eq!(
        parsed["legacy_var_names"]
            .as_array()
            .expect("legacy vars should be array")
            .len(),
        0
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("secret-value"));
}

#[test]
fn cli_channel_feishu_check_rejects_legacy_env_file_path_scope() {
    let workspace = temp_workspace("feishu-check-legacy-path");
    fs::create_dir_all(&workspace).expect("workspace should create");
    fs::write(workspace.join("config.toml"), "provider = \"fake\"\n").expect("config should write");
    let env_root = workspace.join(".codex-im");
    fs::create_dir_all(&env_root).expect("env root should create");
    let env_file = env_root.join(".env");
    fs::write(
        &env_file,
        format!(
            r#"
CHUANG_AGENT_WORKSPACE_ROOT={}
CHUANG_FEISHU_APP_ID=cli_a_test
CHUANG_FEISHU_APP_SECRET=secret-value
"#,
            workspace.display()
        ),
    )
    .expect("env should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("feishu-check")
        .arg("--env-file")
        .arg(&env_file)
        .arg("--json")
        .output()
        .expect("feishu-check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["diagnostic_status"], "blocked");
    assert_eq!(parsed["env_file_is_chuang_scoped"], false);
    assert!(parsed["env_file_scope_warnings"]
        .as_array()
        .expect("scope warnings should be array")
        .iter()
        .any(|warning| warning == "env_file_looks_like_codex_im_default_env"));
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions should be array")
        .iter()
        .any(|action| action
            .as_str()
            .expect("next action should be string")
            .starts_with("use_chuang_scoped_env_file:")));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("secret-value"));
}

#[test]
fn cli_channel_feishu_check_rejects_legacy_env_names() {
    let workspace = temp_workspace("feishu-check-legacy");
    fs::create_dir_all(&workspace).expect("workspace should create");
    fs::write(workspace.join("config.toml"), "provider = \"fake\"\n").expect("config should write");
    let env_file = workspace.join("chuang-feishu.env");
    fs::write(
        &env_file,
        format!(
            r#"
CHUANG_AGENT_WORKSPACE_ROOT={}
CHUANG_FEISHU_APP_ID=cli_a_test
CHUANG_FEISHU_APP_SECRET=secret-value
FEISHU_APP_ID=legacy-codex
"#,
            workspace.display()
        ),
    )
    .expect("env should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("feishu-check")
        .arg("--env-file")
        .arg(&env_file)
        .arg("--json")
        .output()
        .expect("feishu-check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["has_legacy_names"], true);
    assert_eq!(
        parsed["legacy_var_names"],
        serde_json::json!(["FEISHU_APP_ID"])
    );
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions should be array")
        .iter()
        .any(|action| action == "remove_legacy_feishu_env_names"));
}

#[test]
fn cli_channel_feishu_check_rejects_missing_workspace_config_and_bad_mode() {
    let workspace = temp_workspace("feishu-check-bad-mode");
    fs::create_dir_all(&workspace).expect("workspace should create");
    let env_file = workspace.join("chuang-feishu.env");
    fs::write(
        &env_file,
        format!(
            r#"
CHUANG_AGENT_WORKSPACE_ROOT={}
CHUANG_FEISHU_APP_ID=cli_a_test
CHUANG_FEISHU_APP_SECRET=secret-value
CHUANG_FEISHU_CONNECTION_MODE=terminal
"#,
            workspace.display()
        ),
    )
    .expect("env should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("feishu-check")
        .arg("--env-file")
        .arg(&env_file)
        .arg("--json")
        .output()
        .expect("feishu-check should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["diagnostic_status"], "blocked");
    assert_eq!(parsed["workspace_root_exists"], true);
    assert_eq!(parsed["workspace_config_exists"], false);
    assert_eq!(parsed["connection_mode_ok"], false);
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions should be array")
        .iter()
        .any(|action| action == "add_or_fix_workspace_config_toml"));
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions should be array")
        .iter()
        .any(|action| action == "set_chuang_feishu_connection_mode_to_websocket_or_webhook"));
}
