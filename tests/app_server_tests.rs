use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
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
    assert_eq!(
        turn_response["result"]["turn"]["runtimeReportId"],
        "report-turn-1"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["runtime_report_id"],
        "report-turn-1"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["runtime_report_id"],
        "report-turn-1"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["knowledge_context_preview_enabled"],
        "false"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["knowledge_context_preview_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["knowledge_context_injected_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["knowledge_context_dropped_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["knowledge_context_dropped_segment_ids"],
        "[]"
    );
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
        turn_response["result"]["turn"]["toolSurface"]["available"],
        true
    );
    assert_eq!(
        turn_response["result"]["turn"]["toolSurface"]["governed"],
        true
    );
    assert_eq!(
        turn_response["result"]["thread"]["turns"][0]["toolSurface"]["available"],
        true
    );
    assert!(
        turn_response["result"]["turn"]["toolSurface"]["callable_tools"]
            .as_array()
            .expect("callable tools should be array")
            .iter()
            .any(|tool| tool == "file_read")
    );
    assert!(
        turn_response["result"]["turn"]["toolSurface"]["callable_tools"]
            .as_array()
            .expect("callable tools should be array")
            .iter()
            .any(|tool| tool == "list_dir")
    );
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
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_surface_available"],
        "true"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_surface_governed"],
        "true"
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_surface_callable_tools"]
            .as_str()
            .expect("callable tools metadata should be string")
            .contains("file_read")
    );
    let turn_completed = responses
        .iter()
        .find(|value| value["method"] == "turn/completed")
        .expect("turn completed event should be present");
    assert_eq!(turn_completed["params"]["turn"]["toolCallCount"], 0);
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeReportId"],
        "report-turn-1"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["runtime_report_id"],
        "report-turn-1"
    );
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
        turn_completed["params"]["turn"]["toolSurface"]["available"],
        true
    );
    assert_eq!(
        turn_completed["params"]["turn"]["toolSurface"]["governed"],
        true
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
fn app_server_turn_surfaces_provider_fallback_diagnostics() {
    let workspace = temp_workspace("provider-fallback");
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

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let mut buffer = [0u8; 4096];
        let _ = stream
            .read(&mut buffer)
            .expect("request should be readable");

        let body = r#"{"error":{"message":"rate limited"}}"#;
        let response = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
    });

    fs::write(
        workspace.join("config.toml"),
        format!(
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
provider_id = "app-server-primary"
base_url = "http://{address}/v1"
model = "gpt-app-server-primary"
api_key_env = "CHUANG_AGENT_APP_SERVER_TEST_API_KEY"
transport = "http"

fallback_provider = "fake"
fallback_provider_id = "app-server-fallback"
fallback_model = "gpt-app-server-fallback"
"#,
        ),
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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"触发 fallback"}}}}"#,
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
    server.join().expect("server thread should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let responses = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let turn_response = responses
        .iter()
        .find(|value| value["id"] == 1)
        .expect("turn/start response should be present");

    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_used"],
        "true"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_from"],
        "app-server-primary"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_reason"],
        "status_code=429"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_fallback_used"],
        "true"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_fallback_from"],
        "app-server-primary"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_fallback_reason"],
        "status_code=429"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_status_code"],
        "429"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_error_class"],
        "http_status"
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
    assert_eq!(parsed["diagnostic_mode"], false);
    assert_eq!(parsed["diagnostic_status"], "warning");
    assert!(parsed["diagnostic_summary"]
        .as_str()
        .expect("diagnostic summary")
        .contains("local warning"));
    assert_eq!(parsed["api_key_state"], "<set>");
    assert_eq!(parsed["goal_mode"]["ok"], true);
    assert_eq!(parsed["goal_mode"]["cli_entrypoint"], "run --goal TEXT");
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["update_progress_log"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["update_handoff"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["commit_checkpoint"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["final_report_policy"]["include_validation"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["final_report_policy"]["include_next_steps"],
        true
    );
    assert_eq!(parsed["goal_run"]["ok"], true);
    assert_eq!(parsed["goal_run"]["goal_id"], "mainline-mvp");
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions array")
        .iter()
        .any(|action| action
            .as_str()
            .expect("next action")
            .contains("switch provider transport to native or curl")));
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions array")
        .iter()
        .any(|action| action
            .as_str()
            .expect("next action")
            .contains("configure command-backed actuator")));
    assert_eq!(parsed["release_readiness"]["ok"], true);
    assert_eq!(
        parsed["release_readiness"]["overall_state"],
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        parsed["release_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["release_readiness"]["verifies_real_external_services"],
        false
    );
    assert!(parsed["release_readiness"]["acceptance"]
        .as_array()
        .expect("release acceptance array")
        .iter()
        .any(|item| item["name"] == "real_external_services"
            && item["state"] == "deferred"
            && item["connects_real_service"] == false));
    assert_eq!(parsed["third_test_candidate"]["ok"], true);
    assert_eq!(
        parsed["third_test_candidate"]["overall_state"],
        "local_gate_ready_requires_manual_live_check"
    );
    assert_eq!(parsed["third_test_candidate"]["local_gate_ready"], true);
    assert_eq!(
        parsed["third_test_candidate"]["smoke_script"],
        "scripts/chuang-third-test-smoke.sh"
    );
    assert_eq!(
        parsed["third_test_candidate"]["marker"],
        "third_test_candidate_smoke_ok"
    );
    assert_eq!(
        parsed["third_test_candidate"]["requires_manual_live_check"],
        true
    );
    assert_eq!(
        parsed["third_test_candidate"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["third_test_candidate"]["operator_env_blocks_100_percent"],
        true
    );
    assert_eq!(parsed["third_test_candidate"]["real_live_ready"], false);
    assert_eq!(parsed["local_contract_readiness"]["ok"], true);
    assert_eq!(parsed["local_contract_readiness"]["overall_state"], "ready");
    assert_eq!(parsed["local_contract_readiness"]["contract_count"], 5);
    assert_eq!(
        parsed["local_contract_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["local_contract_readiness"]["writes_core_memory"],
        false
    );
    assert_eq!(
        parsed["local_contract_readiness"]["executes_plugins"],
        false
    );
    assert_eq!(
        parsed["project_readiness"]["overall_state"],
        "mvp_ready_with_partial_modules"
    );
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

#[test]
fn app_server_health_diagnostic_reports_missing_provider_env_without_failing() {
    let workspace = temp_workspace("health-diagnostic");
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
api_key_env = "CHUANG_AGENT_APP_SERVER_MISSING_TEST_API_KEY"
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
            "--diagnostic",
            "--json",
        ])
        .env_remove("CHUANG_AGENT_APP_SERVER_MISSING_TEST_API_KEY")
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
    assert_eq!(parsed["diagnostic_mode"], true);
    assert_eq!(parsed["diagnostic_status"], "warning");
    assert!(parsed["diagnostic_summary"]
        .as_str()
        .expect("diagnostic summary")
        .contains("diagnostic mode"));
    assert_eq!(
        parsed["api_key_state"],
        "<missing:CHUANG_AGENT_APP_SERVER_MISSING_TEST_API_KEY>"
    );
    assert_eq!(parsed["goal_mode"]["ok"], true);
    assert_eq!(parsed["goal_mode"]["cli_entrypoint"], "run --goal TEXT");
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["update_progress_log"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["update_handoff"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["commit_checkpoint"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["final_report_policy"]["include_validation"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["final_report_policy"]["include_next_steps"],
        true
    );
    assert_eq!(parsed["goal_run"]["ok"], true);
    assert_eq!(parsed["goal_run"]["goal_id"], "mainline-mvp");
    assert!(parsed["placeholder_warnings"]
        .as_array()
        .expect("placeholder warnings")
        .iter()
        .any(|warning| warning
            .as_str()
            .expect("warning")
            .contains("provider api_key_env missing")));
    assert!(parsed["next_actions"]
        .as_array()
        .expect("next actions array")
        .iter()
        .any(|action| action
            .as_str()
            .expect("next action")
            .contains("set CHUANG_AGENT_APP_SERVER_MISSING_TEST_API_KEY")));
    assert_eq!(
        parsed["release_readiness"]["overall_state"],
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        parsed["release_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(parsed["third_test_candidate"]["local_gate_ready"], true);
    assert_eq!(
        parsed["third_test_candidate"]["connects_real_external_services"],
        false
    );
    assert_eq!(parsed["third_test_candidate"]["real_live_ready"], false);
    assert_eq!(parsed["local_contract_readiness"]["ok"], true);
    assert_eq!(
        parsed["local_contract_readiness"]["connects_real_external_services"],
        false
    );
}

#[test]
fn app_server_health_text_reports_diagnostic_summary_and_next_actions() {
    let workspace = temp_workspace("health-text");
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
api_key_env = "CHUANG_AGENT_APP_SERVER_TEXT_TEST_API_KEY"
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
            "--diagnostic",
        ])
        .env_remove("CHUANG_AGENT_APP_SERVER_TEXT_TEST_API_KEY")
        .output()
        .expect("app-server health should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("app_server_ok: true"));
    assert!(stdout.contains("diagnostic_status: warning"));
    assert!(stdout.contains("diagnostic_summary:"));
    assert!(stdout.contains("next_actions:"));
    assert!(stdout.contains(
        "goal_mode: ok=true kind=lightweight_runtime_context cli_entrypoint=run --goal TEXT context_source=goal default_goal_id=mainline-mvp allowed_slots=context,governance,execution,report,memory checkpoint_policy=progress_log:true handoff:true commit:true final_report_policy=validation:true next_steps:true bypasses_governance=false adds_core_slot=false"
    ));
    assert!(stdout.contains("goal_run: ok=true"));
    assert!(stdout.contains("goal_run_readiness: ok=true plan_exists=true goal_id=mainline-mvp"));
    assert!(stdout.contains("goal_run_checkpoint_log_complete:"));
    assert!(stdout.contains("goal_run_last_checkpoint:"));
    assert!(stdout.contains("goal_run_last_checkpoint_summary:"));
    assert!(stdout.contains("goal_run_last_checkpoint_created_at:"));
    assert!(stdout.contains("goal_run_last_checkpoint_completed_worker_ids:"));
    assert!(stdout.contains("goal_run_last_checkpoint_validation_notes:"));
    assert!(stdout.contains("goal_run_incomplete_reasons:"));
    assert!(stdout.contains(
        "third_test_candidate: ok=true state=local_gate_ready_requires_manual_live_check local_gate_ready=true smoke_script=scripts/chuang-third-test-smoke.sh marker=third_test_candidate_smoke_ok requires_manual_live_check=true connects_real_external_services=false operator_env_blocks_100_percent=true real_live_ready=false"
    ));
    assert!(stdout.contains(
        "local_contract_readiness: ok=true state=ready contracts=5 connects_real_external_services=false writes_core_memory=false executes_plugins=false"
    ));
    assert!(stdout.contains("set CHUANG_AGENT_APP_SERVER_TEXT_TEST_API_KEY"));
}
