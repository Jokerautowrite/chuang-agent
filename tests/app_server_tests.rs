use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};

fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-app-server-{name}-{nanos}"))
}

#[test]
fn app_server_turn_uses_workspace_provider_config() {
    let workspace = temp_workspace("provider-config");
    fs::create_dir_all(workspace.join("identity")).expect("identity dir should create");
    fs::create_dir_all(workspace.join("rules")).expect("rules dir should create");
    fs::write(workspace.join("identity/SOUL.md"), "Chuang test soul\n").expect("soul should write");
    fs::write(workspace.join("identity/STORY.md"), "Chuang test story\n")
        .expect("story should write");
    fs::write(
        workspace.join("identity/FIRST_WAKE.md"),
        "Chuang test first wake\n",
    )
    .expect("first wake should write");
    fs::write(workspace.join("identity/agents.toml"), "[agents]\n")
        .expect("agents registry should write");
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
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
provider_id = "app-server-openai"
base_url = "https://api.example.com/v1"
model = "gpt-app-server-test"
api_key_env = "CHUANG_AGENT_APP_SERVER_TEST_API_KEY"
transport = "stub"
"#,
    )
    .expect("config should write");

    let mut child = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("app-server")
        .env("CHUANG_AGENT_APP_SERVER_TEST_API_KEY", "test-key")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("app-server should spawn");

    let mut stdin = child.stdin.take().expect("stdin should exist");
    writeln!(
        stdin,
        r#"{{"id":1,"method":"model/list","params":{{"workspaceRoot":"{}"}}}}"#,
        workspace.display()
    )
    .expect("model/list should write");
    writeln!(
        stdin,
        r#"{{"id":2,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"还在吗？"}}}}"#,
        workspace.display()
    )
    .expect("turn/start should write");
    drop(stdin);

    let output = child.wait_with_output().expect("app-server should exit");
    assert!(
        output.status.success(),
        "app-server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("gpt-app-server-test"));
    assert!(stdout.contains("stubbed_post_ok"));
    assert!(!stdout.contains("fake-responder"));

    let responses = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let turn_response = responses
        .iter()
        .find(|value| value["id"] == 2)
        .expect("turn/start response should be present");
    assert_eq!(turn_response["result"]["turn"]["toolCallCount"], 0);
    assert_eq!(turn_response["result"]["turn"]["toolProtocolErrorCount"], 0);
    assert_eq!(
        turn_response["result"]["turn"]["toolCalls"]
            .as_array()
            .expect("tool calls should be array")
            .len(),
        0
    );
    assert_eq!(
        turn_response["result"]["turn"]["toolEvents"]
            .as_array()
            .expect("tool events should be array")
            .len(),
        0
    );
    assert!(turn_response["result"]["turn"]["toolReport"].is_null());
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["session_id"],
        "chuang-thread-1"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["model_name"],
        "gpt-app-server-test"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_id"],
        "chuang-thread-1"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["governance_action_id"],
        "run-turn-1"
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["governance_decision"]
            .as_str()
            .expect("governance decision should be string")
            .starts_with("allowed:")
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["governance_reason"]
            .as_str()
            .expect("governance reason should be string")
            .contains("read-only or draft action")
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_memory_scope"],
        "session"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_call_count"],
        "0"
    );
    let turn_completed = responses
        .iter()
        .find(|value| value["method"] == "turn/completed")
        .expect("turn completed event should be present");
    assert_eq!(turn_completed["params"]["turn"]["toolCallCount"], 0);
    assert_eq!(
        turn_completed["params"]["turn"]["toolProtocolErrorCount"],
        0
    );
    assert_eq!(
        turn_completed["params"]["turn"]["toolCalls"]
            .as_array()
            .expect("event tool calls should be array")
            .len(),
        0
    );
    assert_eq!(
        turn_completed["params"]["turn"]["toolEvents"]
            .as_array()
            .expect("event tool events should be array")
            .len(),
        0
    );
    assert_eq!(
        turn_completed["params"]["turn"]["providerMeta"]["session_id"],
        "chuang-thread-1"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["model_name"],
        "gpt-app-server-test"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["session_id"],
        "chuang-thread-1"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["governance_action_id"],
        "run-turn-1"
    );
    assert!(
        turn_completed["params"]["turn"]["runtimeObservability"]["governance_decision"]
            .as_str()
            .expect("event governance decision should be string")
            .starts_with("allowed:")
    );
}

#[test]
fn app_server_health_reports_workspace_runtime() {
    let workspace = temp_workspace("health");
    fs::create_dir_all(workspace.join("identity")).expect("identity dir should create");
    fs::create_dir_all(workspace.join("rules")).expect("rules dir should create");
    fs::write(workspace.join("identity/SOUL.md"), "Chuang health soul\n")
        .expect("soul should write");
    fs::write(workspace.join("identity/STORY.md"), "Chuang health story\n")
        .expect("story should write");
    fs::write(
        workspace.join("identity/FIRST_WAKE.md"),
        "Chuang health first wake\n",
    )
    .expect("first wake should write");
    fs::write(workspace.join("identity/agents.toml"), "[agents]\n")
        .expect("agents registry should write");
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
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
provider_id = "app-server-health-openai"
base_url = "https://api.example.com/v1"
model = "gpt-app-server-health"
api_key_env = "CHUANG_AGENT_APP_SERVER_TEST_API_KEY"
transport = "stub"
"#,
    )
    .expect("config should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "app-server",
            "health",
            "--workspace-root",
            workspace.to_str().expect("workspace path should be utf8"),
            "--json",
        ])
        .env("CHUANG_AGENT_APP_SERVER_TEST_API_KEY", "test-key")
        .output()
        .expect("app-server health should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["server"], "chuang-agent-app-server");
    assert_eq!(parsed["model"], "gpt-app-server-health");
    assert_eq!(parsed["workspace_root"], workspace.display().to_string());
    assert!(parsed["identity_memory_root"]
        .as_str()
        .expect("identity memory root")
        .ends_with("data/hermes-memory"));
    assert_eq!(
        parsed["identity_soul_path"],
        workspace.join("identity/SOUL.md").display().to_string()
    );
    assert_eq!(
        parsed["rules_core_path"],
        workspace.join("rules/core.md").display().to_string()
    );
}
