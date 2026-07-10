use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};

fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-app-server-{name}-{nanos}"))
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    let mut expected_len = None;
    loop {
        let read = stream
            .read(&mut buffer)
            .expect("request should be readable");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if expected_len.is_none() {
            if let Some(header_end) = find_header_end(&request) {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_len = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                expected_len = Some(header_end + 4 + content_len);
            }
        }
        if expected_len
            .map(|len| request.len() >= len)
            .unwrap_or(false)
        {
            break;
        }
    }
    request
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_identity_registry(workspace: &PathBuf) {
    fs::write(
        workspace.join("identity/agents.toml"),
        r#"memory_body_id = "chuang-local-body"
active_agent_id = "chuang"

[[agents]]
agent_id = "chuang"
display_name = "Chuang"
shell_kind = "codex-rust"
role = "local-agent-os-kernel"
memory_body_id = "chuang-local-body"
allowed_channels = ["chuang-feishu", "cli", "app-server"]
"#,
    )
    .expect("agents registry should write");
}

fn write_basic_stub_workspace(workspace: &PathBuf) {
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
    write_identity_registry(workspace);
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
}

#[test]
fn app_server_turn_rejects_identity_without_app_server_channel_permission() {
    let workspace = temp_workspace("channel-not-allowed");
    write_basic_stub_workspace(&workspace);
    let registry_path = workspace.join("identity/agents.toml");
    let registry = fs::read_to_string(&registry_path).expect("registry should read");
    fs::write(
        &registry_path,
        registry.replace(
            r#"allowed_channels = ["chuang-feishu", "cli", "app-server"]"#,
            r#"allowed_channels = ["chuang-feishu", "cli"]"#,
        ),
    )
    .expect("registry should remove app-server channel");

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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"不应进入运行时"}}}}"#,
        workspace.display()
    )
    .expect("turn/start should write");
    drop(stdin);

    let output = child.wait_with_output().expect("app-server should exit");
    assert!(output.status.success());
    let responses = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let response = responses
        .iter()
        .find(|value| value["id"] == 1)
        .expect("turn/start error response should exist");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(message.contains("identity_registry_invalid"));
    assert!(message.contains("ChannelNotAllowed"));
    assert!(message.contains("app-server"));
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
    write_identity_registry(&workspace);
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
    assert_eq!(turn_response["result"]["turn"]["liveReadiness"]["ok"], true);
    assert_eq!(
        turn_response["result"]["turn"]["liveReadiness"]["overall_state"],
        "local_ready_live_pending"
    );
    assert_eq!(
        turn_response["result"]["turn"]["liveReadiness"]["real_external_acceptance_pending"],
        true
    );
    assert_eq!(
        turn_response["result"]["turn"]["liveReadiness"]
            ["provider_live_request_verified_by_status"],
        false
    );
    assert_eq!(
        turn_response["result"]["turn"]["liveReadiness"]["ready_does_not_mean_live"],
        true
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_unified_execution_status"],
        "ok"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["tool_unified_execution_failure_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["runtime_event_count"],
        "5"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["runtime_event_tool_started_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["runtime_event_tool_finished_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["runtime_event_approval_requested_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["runtime_event_approval_resolved_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["runtime_event_elicitation_requested_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["goal_handoff_parent_context_handoff_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["goal_handoff_report_admission_ref_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["goal_handoff_report_admission_refs"],
        "none"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["goal_handoff_report_admission_reason_codes"],
        "none"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["goal_handoff_query_summary_json"],
        "none"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["subagent_children_child_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["subagent_children_accepted_report_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["subagent_children_report_admission_ref_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["subagent_children_missing_report_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["subagent_children_report_admission_refs"],
        "none"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["subagent_children_report_reason_codes"],
        "none"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["subagent_children_summary_json"],
        "none"
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["runtime_response_trace_chars"]
            .as_str()
            .expect("runtime response trace char count should be a string")
            .parse::<usize>()
            .expect("runtime response trace char count should parse")
            > 0
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["context_pack_trace"]
            .as_str()
            .expect("context pack trace should be a string")
            .contains("normalize_tokens:")
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["context_compaction_events"]
            .as_str()
            .expect("context compaction events should be a string")
            .contains("context_compaction_started")
    );
    assert!(turn_response["result"]["turn"]["runtimeObservability"]
        ["context_compaction_summary_json"]
        .as_str()
        .expect("context compaction summary should be a string")
        .contains("\"dropped_count\""));
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
        turn_response["result"]["turn"]["runtimeObservability"]["tool_protocol_error_count"],
        "0"
    );
    assert_eq!(
        turn_response["result"]["turn"]["toolProtocolErrors"]
            .as_array()
            .expect("tool protocol errors should be array")
            .len(),
        0
    );
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
        turn_completed["params"]["turn"]["liveReadiness"]["ok"],
        true
    );
    assert_eq!(
        turn_completed["params"]["turn"]["liveReadiness"]["overall_state"],
        "local_ready_live_pending"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["liveReadiness"]["real_external_acceptance_pending"],
        true
    );
    assert_eq!(
        turn_completed["params"]["turn"]["liveReadiness"]
            ["provider_live_request_verified_by_status"],
        false
    );
    assert_eq!(
        turn_completed["params"]["turn"]["liveReadiness"]["ready_does_not_mean_live"],
        true
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["tool_unified_execution_status"],
        "ok"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["tool_unified_execution_failure_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["runtime_event_count"],
        "5"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["runtime_event_tool_started_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["runtime_event_tool_finished_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["runtime_event_approval_requested_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["runtime_event_approval_resolved_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["runtime_event_elicitation_requested_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["goal_handoff_parent_context_handoff_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["goal_handoff_report_admission_ref_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["goal_handoff_report_admission_refs"],
        "none"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["goal_handoff_report_admission_reason_codes"],
        "none"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["subagent_children_child_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["subagent_children_accepted_report_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["subagent_children_report_admission_ref_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["subagent_children_missing_report_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["subagent_children_report_admission_refs"],
        "none"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]
            ["subagent_children_report_reason_codes"],
        "none"
    );
    assert!(
        turn_completed["params"]["turn"]["runtimeObservability"]["runtime_response_trace_chars"]
            .as_str()
            .expect("completed runtime response trace char count should be a string")
            .parse::<usize>()
            .expect("completed runtime response trace char count should parse")
            > 0
    );
    assert!(
        turn_completed["params"]["turn"]["runtimeObservability"]["context_pack_trace"]
            .as_str()
            .expect("completed context pack trace should be a string")
            .contains("normalize_tokens:")
    );
    assert!(
        turn_completed["params"]["turn"]["runtimeObservability"]["context_compaction_events"]
            .as_str()
            .expect("completed context compaction events should be a string")
            .contains("context_compaction_started")
    );
    assert!(turn_completed["params"]["turn"]["runtimeObservability"]
        ["context_compaction_summary_json"]
        .as_str()
        .expect("completed context compaction summary should be a string")
        .contains("\"dropped_count\""));
    assert_eq!(
        turn_completed["params"]["turn"]["toolProtocolErrorCount"],
        0
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["tool_protocol_error_count"],
        "0"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["toolProtocolErrors"]
            .as_array()
            .expect("event tool protocol errors should be array")
            .len(),
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
    assert!(
        turn_completed["params"]["turn"]["toolSurface"]["callable_tools"]
            .as_array()
            .expect("event callable tools should be array")
            .iter()
            .any(|tool| tool == "file_read")
    );
    assert!(
        turn_completed["params"]["turn"]["toolSurface"]["callable_tools"]
            .as_array()
            .expect("event callable tools should be array")
            .iter()
            .any(|tool| tool == "list_dir")
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["tool_surface_available"],
        "true"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["tool_surface_governed"],
        "true"
    );
    assert!(turn_completed["params"]["turn"]["runtimeObservability"]
        ["tool_surface_callable_tools"]
        .as_str()
        .expect("event callable tools metadata should be string")
        .contains("file_read"));
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
fn app_server_turn_surfaces_nonzero_tool_protocol_errors() {
    let workspace = temp_workspace("tool-protocol-error");
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
    write_identity_registry(&workspace);
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
    )
    .expect("rules should write");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let scripted_outputs = [
            r#"ACTION: {"type":"tool_call","call":{"tool":"file_read"}}"#,
            r#"ACTION: {"type":"final","answer":"已修正协议错误。"}"#,
        ];
        for content in scripted_outputs {
            let (mut stream, _) = listener.accept().expect("connection should be accepted");
            let _ = read_http_request(&mut stream);
            let body = serde_json::json!({
                "id": "chatcmpl-tool-protocol",
                "object": "response",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": content},
                    "finish_reason": "stop"
                }]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be writable");
        }
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
provider_id = "app-server-protocol"
base_url = "http://{address}/v1"
model = "gpt-app-server-protocol"
api_key_env = "CHUANG_AGENT_APP_SERVER_TEST_API_KEY"
transport = "http"
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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"测试工具协议纠错"}}}}"#,
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

    assert_eq!(turn_response["result"]["turn"]["status"], "completed");
    assert_eq!(turn_response["result"]["turn"]["toolProtocolErrorCount"], 1);
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_protocol_error_count"],
        "1"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["tool_unified_execution_status"],
        "ok"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["tool_unified_execution_failure_count"],
        "0"
    );
    let protocol_errors = turn_response["result"]["turn"]["toolProtocolErrors"]
        .as_array()
        .expect("tool protocol errors should be array");
    assert_eq!(protocol_errors.len(), 1);
    assert_eq!(protocol_errors[0]["code"], "invalid_action_json");
    assert!(protocol_errors[0]["message"]
        .as_str()
        .expect("protocol error message")
        .contains("missing field"));
    assert!(
        turn_response["result"]["turn"]["providerMeta"]["tool_protocol_errors_json"]
            .as_str()
            .expect("provider meta protocol errors")
            .contains("invalid_action_json")
    );
    assert!(turn_response["result"]["turn"]["toolEvents"]
        .as_array()
        .expect("tool events should be array")
        .iter()
        .any(|event| event["kind"] == "protocol_error"));

    let turn_completed = responses
        .iter()
        .find(|value| value["method"] == "turn/completed")
        .expect("turn completed event should be present");
    assert_eq!(
        turn_completed["params"]["turn"]["toolProtocolErrorCount"],
        1
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["tool_protocol_error_count"],
        "1"
    );
    let completed_protocol_errors = turn_completed["params"]["turn"]["toolProtocolErrors"]
        .as_array()
        .expect("event tool protocol errors should be array");
    assert_eq!(completed_protocol_errors.len(), 1);
    assert_eq!(completed_protocol_errors[0]["code"], "invalid_action_json");
    assert!(
        turn_completed["params"]["turn"]["providerMeta"]["tool_protocol_errors_json"]
            .as_str()
            .expect("completed provider meta protocol errors")
            .contains("invalid_action_json")
    );
    assert!(turn_completed["params"]["turn"]["toolEvents"]
        .as_array()
        .expect("completed tool events should be array")
        .iter()
        .any(|event| event["kind"] == "protocol_error"));
}

#[test]
fn app_server_second_turn_injects_recent_thread_history() {
    let workspace = temp_workspace("recent-thread-history");
    write_basic_stub_workspace(&workspace);

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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"我给你一个暗号：anchor_alpha"}}}}"#,
        workspace.display()
    )
    .expect("first turn/start should write");
    writeln!(
        stdin,
        r#"{{"id":2,"method":"turn/start","params":{{"workspaceRoot":"{}","threadId":"chuang-thread-1","text":"刚才那个暗号是什么？"}}}}"#,
        workspace.display()
    )
    .expect("second turn/start should write");
    drop(stdin);

    let output = child.wait_with_output().expect("app-server should exit");
    assert!(
        output.status.success(),
        "app-server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let responses = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let first_turn = responses
        .iter()
        .find(|value| value["id"] == 1)
        .expect("first turn response should be present");
    assert_eq!(
        first_turn["result"]["turn"]["runtimeObservability"]
            ["recent_conversation_history_item_count"],
        "0"
    );
    assert_eq!(
        first_turn["result"]["turn"]["runtimeObservability"]
            ["recent_conversation_history_injected"],
        "false"
    );

    let second_turn = responses
        .iter()
        .find(|value| value["id"] == 2)
        .expect("second turn response should be present");
    assert_eq!(
        second_turn["result"]["turn"]["providerMeta"]["recent_conversation_history_item_count"],
        "2"
    );
    assert_eq!(
        second_turn["result"]["turn"]["providerMeta"]["recent_conversation_history_turn_count"],
        "1"
    );
    assert_eq!(
        second_turn["result"]["turn"]["providerMeta"]["recent_conversation_history_injected"],
        "true"
    );
    assert_eq!(
        second_turn["result"]["turn"]["runtimeObservability"]
            ["recent_conversation_history_item_count"],
        "2"
    );
    assert_eq!(
        second_turn["result"]["turn"]["runtimeObservability"]
            ["recent_conversation_history_model_facing"],
        "true"
    );
    assert_eq!(
        second_turn["result"]["thread"]["turns"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn app_server_rejects_workspace_change_for_existing_thread() {
    let workspace = temp_workspace("workspace-sticky");
    let other_workspace = temp_workspace("workspace-sticky-other");
    write_basic_stub_workspace(&workspace);
    write_basic_stub_workspace(&other_workspace);

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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"先绑定这个工作区"}}}}"#,
        workspace.display()
    )
    .expect("first turn/start should write");
    writeln!(
        stdin,
        r#"{{"id":2,"method":"turn/start","params":{{"workspaceRoot":"{}","threadId":"chuang-thread-1","text":"尝试换工作区"}}}}"#,
        other_workspace.display()
    )
    .expect("second turn/start should write");
    drop(stdin);

    let output = child.wait_with_output().expect("app-server should exit");
    assert!(
        output.status.success(),
        "app-server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let responses = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let second_turn = responses
        .iter()
        .find(|value| value["id"] == 2)
        .expect("second turn response should be present");
    assert!(second_turn["error"]["message"]
        .as_str()
        .expect("error message should be string")
        .contains("workspace_root_mismatch"));
}

#[test]
fn app_server_turn_compacts_session_memory_hard_limit_without_failing_turn() {
    let workspace = temp_workspace("session-memory-limit");
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
    write_identity_registry(&workspace);
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

    let oversized_input = "超限".repeat(1200);
    let mut stdin = child.stdin.take().expect("stdin should exist");
    writeln!(
        stdin,
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","threadId":"chuang-thread-1","text":"{}"}}}}"#,
        workspace.display(),
        oversized_input
    )
    .expect("turn/start should write");
    drop(stdin);

    let output = child.wait_with_output().expect("app-server should exit");
    assert!(
        output.status.success(),
        "app-server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let responses = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let turn_response = responses
        .iter()
        .find(|value| value["id"] == 1)
        .expect("turn/start response should be present");
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_memory_write_requested"],
        "true"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_memory_write_status"],
        "compacted"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_memory_summary_kind"],
        "compacted_turn_summary"
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_memory_record_id"]
            .as_str()
            .expect("session memory record id should be string")
            .starts_with("turn-memory-session-chuang-thread-1-turn-1-")
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["session_memory_write_status"],
        "compacted"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["session_memory_summary_kind"],
        "compacted_turn_summary"
    );
    assert!(
        turn_response["result"]["turn"]["runtimeObservability"]["session_memory_write_error"]
            .is_null()
    );
    let turn_completed = responses
        .iter()
        .find(|value| value["method"] == "turn/completed")
        .expect("turn/completed event should be present");
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["session_memory_write_status"],
        "compacted"
    );
    assert_eq!(
        turn_completed["params"]["turn"]["runtimeObservability"]["session_memory_summary_kind"],
        "compacted_turn_summary"
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
    write_identity_registry(&workspace);
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
    )
    .expect("rules should write");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let _ = read_http_request(&mut stream);

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
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_request_url"],
        format!("http://{address}/v1/responses")
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_request_method"],
        "POST"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_error_message"],
        "rate limited"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_primary_request_url"],
        format!("http://{address}/v1/responses")
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_primary_request_method"],
        "POST"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_primary_error_message"],
        "rate limited"
    );
}

#[test]
fn app_server_turn_surfaces_provider_fallback_primary_config_error_metadata() {
    let workspace = temp_workspace("provider-fallback-config-error");
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
    write_identity_registry(&workspace);
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
provider_id = "app-server-primary"
base_url = "ftp://api.example.com/v1"
model = "gpt-app-server-primary"
api_key_env = "CHUANG_AGENT_APP_SERVER_TEST_API_KEY"
transport = "http"
provider_timeout_ms = 12345

fallback_provider = "fake"
fallback_provider_id = "app-server-fallback"
fallback_model = "gpt-app-server-fallback"
fallback_error_classes = "config"
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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"触发 config fallback"}}}}"#,
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
        turn_response["result"]["turn"]["providerMeta"]
            ["provider_fallback_primary_config_error_field"],
        "base_url"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_primary_timeout_ms"],
        "12345"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_primary_request_url"],
        "ftp://api.example.com/v1/responses"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_fallback_primary_request_method"],
        "POST"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_config_error_field"],
        "base_url"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_timeout_ms"],
        "12345"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_request_url"],
        "ftp://api.example.com/v1/responses"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]
            ["provider_fallback_primary_request_method"],
        "POST"
    );
}

#[test]
fn app_server_turn_surfaces_capacity_metadata_on_plain_text_429() {
    let workspace = temp_workspace("provider-capacity");
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
    write_identity_registry(&workspace);
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
    )
    .expect("rules should write");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let _ = read_http_request(&mut stream);

        let body = "at capacity";
        let response = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"触发 at capacity"}}}}"#,
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

    assert_eq!(turn_response["result"]["turn"]["status"], "provider_error");
    assert_eq!(
        turn_response["result"]["turn"]["finishReason"],
        "http-error-429"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_error_message"],
        "at capacity"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_failure_reason_code"],
        "model_capacity"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_failure_category"],
        "capacity"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_failure_reason_code"],
        "model_capacity"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_failure_category"],
        "capacity"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_error_message"],
        "at capacity"
    );
}

#[test]
fn app_server_turn_marks_200_missing_content_as_provider_error() {
    let workspace = temp_workspace("provider-missing-content");
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
    write_identity_registry(&workspace);
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
    )
    .expect("rules should write");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let _ = read_http_request(&mut stream);

        let body = r#"{"id":"chatcmpl-empty","object":"response","choices":[{"index":0,"message":{"role":"assistant","content":""},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"触发 missing content"}}}}"#,
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

    assert_eq!(turn_response["result"]["turn"]["status"], "provider_error");
    assert_eq!(
        turn_response["result"]["turn"]["finishReason"],
        "provider-error-missing-content"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_response_ok"],
        "false"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_failure_reason_code"],
        "missing_content"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_failure_category"],
        "response"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["finish_reason"],
        "provider-error-missing-content"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_error_message"],
        "missing assistant content in successful provider response"
    );
}

#[test]
fn app_server_turn_surfaces_provider_timeout_reason_codes() {
    let workspace = temp_workspace("provider-timeout");
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
    write_identity_registry(&workspace);
    fs::write(
        workspace.join("rules/core.md"),
        "- Keep the response minimal and testable.\n",
    )
    .expect("rules should write");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection should be accepted");
        let _ = read_http_request(&mut stream);
        thread::sleep(Duration::from_millis(1000));
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
provider_id = "app-server-timeout-openai"
base_url = "http://{address}/v1"
model = "gpt-app-server-timeout"
api_key_env = "CHUANG_AGENT_APP_SERVER_TEST_API_KEY"
provider_timeout_ms = 20
transport = "curl"
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
        r#"{{"id":1,"method":"turn/start","params":{{"workspaceRoot":"{}","text":"触发 timeout"}}}}"#,
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

    assert_eq!(turn_response["result"]["turn"]["status"], "provider_error");
    assert_eq!(
        turn_response["result"]["turn"]["finishReason"],
        "invalid-config"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["config_error_field"],
        "curl_wait"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["config_error_field"],
        "curl_wait"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_timeout_reason_code"],
        "request_timeout"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_timeout_category"],
        "timeout"
    );
    assert_eq!(
        turn_response["result"]["turn"]["providerMeta"]["provider_timeout_ms"],
        "20"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_timeout_reason_code"],
        "request_timeout"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_timeout_category"],
        "timeout"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["provider_timeout_ms"],
        "20"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["request_url"],
        format!("http://{address}/v1/responses")
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["request_method"],
        "POST"
    );
    assert_eq!(
        turn_response["result"]["turn"]["runtimeObservability"]["request_message_count"],
        "2"
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
    write_identity_registry(&workspace);
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
        .env(
            "CHUANG_AGENT_WORKSPACE_ROOT",
            workspace.display().to_string(),
        )
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_ACTUATOR_ENABLE")
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
    assert_eq!(parsed["subagent_live_worker"]["enabled"], false);
    assert_eq!(parsed["subagent_live_worker"]["adapter_kind"], "none");
    assert_eq!(parsed["subagent_live_worker"]["starts_worker"], false);
    assert_eq!(parsed["subagent_live_worker"]["available"], false);
    assert!(parsed["subagent_live_worker"]["reason"]
        .as_str()
        .expect("app-server subagent live worker reason")
        .contains("no live worker is started"));
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
    assert_eq!(
        parsed["policy_tool_status"]["active_permission_profile"],
        "local_ga"
    );
    let health_tool_descriptors = parsed["policy_tool_status"]["ga_tool_descriptors"]
        .as_array()
        .expect("app-server health tool descriptors should be array");
    let file_write_descriptor = health_tool_descriptors
        .iter()
        .find(|descriptor| descriptor["name"] == "file_write")
        .expect("file_write descriptor should be surfaced");
    assert_eq!(file_write_descriptor["read_only"], false);
    assert_eq!(file_write_descriptor["mutating"], true);
    assert_eq!(file_write_descriptor["destructive"], false);
    assert_eq!(file_write_descriptor["external_commit"], false);
    assert_eq!(file_write_descriptor["requires_approval"], false);
    assert_eq!(
        file_write_descriptor["local_ga_decision"],
        "allow_with_audit"
    );
    assert!(file_write_descriptor["risk_tags"]
        .as_array()
        .expect("file_write risk tags should be array")
        .iter()
        .any(|tag| tag == "write"));
    assert!(file_write_descriptor["risk_tags"]
        .as_array()
        .expect("file_write risk tags should be array")
        .iter()
        .any(|tag| tag == "audit"));
    assert_eq!(parsed["runtime_report_surface"]["ok"], true);
    assert_eq!(parsed["runtime_report_surface"]["artifact_count"], 11);
    assert_eq!(
        parsed["runtime_report_surface"]["observability_field_count"],
        26
    );
    assert!(parsed["runtime_report_surface"]["artifact_locators"]
        .as_array()
        .expect("runtime report artifact locators should be array")
        .iter()
        .any(|locator| locator == "runtime_meta.tool_protocol_errors_json"));
    assert!(parsed["runtime_report_surface"]["artifact_locators"]
        .as_array()
        .expect("runtime report artifact locators should be array")
        .iter()
        .any(|locator| locator == "runtime_meta.runtime_event_ledger_json"));
    assert!(parsed["runtime_report_surface"]["artifact_locators"]
        .as_array()
        .expect("runtime report artifact locators should be array")
        .iter()
        .any(|locator| locator == "runtime_meta.context_compaction_summary_json"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "tool_protocol_error_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "tool_unified_execution_status"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "tool_unified_execution_failure_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "tool_unified_execution_failure_classes"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "runtime_event_tool_started_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "runtime_event_tool_finished_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "runtime_event_approval_requested_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "runtime_event_approval_resolved_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "runtime_event_elicitation_requested_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "goal_handoff_report_admission_refs"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "goal_handoff_parent_context_handoff_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "goal_handoff_report_admission_ref_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "goal_handoff_report_admission_reason_codes"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "subagent_children_report_admission_refs"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "subagent_children_child_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "subagent_children_accepted_report_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "subagent_children_report_admission_ref_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "subagent_children_missing_report_count"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "subagent_children_report_reason_codes"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "context_compaction_summary_json"));
    assert!(parsed["runtime_report_surface"]["observability_fields"]
        .as_array()
        .expect("runtime report observability fields should be array")
        .iter()
        .any(|field| field == "context_compaction_events"));
    assert_eq!(parsed["provider_readiness"]["ok"], true);
    assert_eq!(
        parsed["provider_readiness"]["provider_kind"],
        "openai_compatible"
    );
    assert_eq!(parsed["provider_readiness"]["transport"], "stub");
    assert_eq!(parsed["provider_readiness"]["fallback_configured"], false);
    assert_eq!(parsed["provider_readiness"]["api_key_state"], "<set>");
    assert_eq!(
        parsed["provider_readiness"]["provider_id"],
        "app-server-health-openai"
    );
    assert_eq!(
        parsed["provider_readiness"]["model_name"],
        "gpt-app-server-health"
    );
    assert_eq!(
        parsed["workspace"]["workspace_root"],
        workspace.display().to_string()
    );
    assert_eq!(
        parsed["workspace"]["app_server_child_root"],
        workspace.display().to_string()
    );
    assert_eq!(
        parsed["workspace"]["config_root"],
        workspace.display().to_string()
    );
    assert_eq!(parsed["workspace"]["matches_config"], true);
    assert_eq!(
        parsed["workspace"]["config_source"],
        "CHUANG_AGENT_WORKSPACE_ROOT"
    );
    assert_eq!(
        parsed["workspace"]["config_path"],
        workspace.join("config.toml").display().to_string()
    );
    assert!(parsed["provider_readiness"]["current"]
        .as_str()
        .expect("provider current should be text")
        .contains("transport=stub"));
    assert!(parsed["provider_readiness"]["next_action"]
        .as_str()
        .expect("provider next action should be text")
        .contains("real provider transport"));
    assert!(parsed["runtime_capability_primer"]
        .as_str()
        .expect("runtime capability primer should be text")
        .contains("file_read/file_write/code_execute/list_dir"));
    assert!(parsed["runtime_capability_primer"]
        .as_str()
        .expect("runtime capability primer should be text")
        .contains("默认注入能力 primer"));
    assert!(parsed["runtime_capability_primer"]
        .as_str()
        .expect("runtime capability primer should be text")
        .contains("locate/screenshot=只读观察"));
    assert!(parsed["runtime_capability_primer"]
        .as_str()
        .expect("runtime capability primer should be text")
        .contains("subagent=dispatch/list/run-once/run-loop/report/collect"));
    assert_eq!(parsed["atomic_tools"]["ok"], true);
    assert_eq!(
        parsed["atomic_tools"]["governed_executable_atomic_tool_names"],
        serde_json::json!([
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "file_read",
            "file_write",
            "code_execute",
            "wait",
            "human_suspend"
        ])
    );
    assert_eq!(
        parsed["atomic_tools"]["desktop_browser_interface_only_atomic_tool_names"],
        serde_json::json!([])
    );
    assert_eq!(
        parsed["atomic_tools"]["desktop_browser_live_gated_atomic_tool_names"],
        serde_json::json!(["mouse", "keyboard", "screenshot", "locate"])
    );
    assert!(parsed["atomic_tools"]["interface_only_reason"]
        .as_str()
        .expect("interface only reason should be text")
        .contains("all GA atoms are mapped to governed runtime ports"));
    assert_eq!(
        parsed["atomic_tools"]["local_cli_self_check_entrypoints"],
        serde_json::json!([
            "status --json",
            "doctor --json",
            "app-server health --diagnostic --json"
        ])
    );
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
    assert_eq!(parsed["local_contract_readiness"]["contract_count"], 6);
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
    assert_eq!(parsed["subagent_readiness"]["ok"], true);
    assert_eq!(
        parsed["subagent_readiness"]["overall_state"],
        "queued_protocol_partial"
    );
    assert_eq!(parsed["subagent_readiness"]["live_worker_available"], false);
    assert_eq!(
        parsed["subagent_readiness"]["worker_runtime_state"],
        "local_contract_only"
    );
    assert!(parsed["subagent_readiness"]["worker_runtime_reason"]
        .as_str()
        .expect("subagent worker runtime reason should be text")
        .contains("subagent slot is fake"));
    assert!(
        parsed["subagent_readiness"]["worker_runtime_blocked_reason"]
            .as_str()
            .expect("subagent worker runtime blocked reason should be text")
            .contains("live_worker_unavailable")
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_route_state"],
        "requires_dispatch_required_capabilities"
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_mismatch_blocks_live"],
        true
    );
    assert!(parsed["subagent_readiness"]["capability_mismatch_reason"]
        .as_str()
        .expect("subagent capability mismatch reason should be text")
        .contains("required_capabilities"));
    assert_eq!(parsed["subagent_readiness"]["local_contract_ready"], true);
    assert_eq!(
        parsed["subagent_readiness"]["local_contract_state"],
        "ready"
    );
    assert!(parsed["subagent_readiness"]["local_contract_reason"]
        .as_str()
        .expect("subagent local reason should be text")
        .contains("protocol-ready"));
    assert_eq!(parsed["subagent_readiness"]["live_adapter_ready"], false);
    assert_eq!(
        parsed["subagent_readiness"]["live_adapter_state"],
        "partial"
    );
    assert!(parsed["subagent_readiness"]["live_adapter_reason"]
        .as_str()
        .expect("subagent live adapter reason should be text")
        .contains("not yet connected"));
    assert!(parsed["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "live_runner_rehearsal"
            && layer["local_contract_ready"] == true
            && layer["live_adapter_ready"] == false
            && layer["live_worker_available"] == false
            && layer["worker_runtime_state"] == "local_contract_only"
            && layer["blocked_reason"]
                .as_str()
                .expect("layer blocked reason should be text")
                .contains("required_capabilities")
            && layer["capability_route_state"] == "requires_dispatch_required_capabilities"
            && layer["capability_mismatch_blocks_live"] == true
            && layer["capability_mismatch_reason"]
                .as_str()
                .expect("layer capability mismatch reason should be text")
                .contains("required_capabilities")));
    assert_eq!(parsed["live_readiness"]["ok"], true);
    assert_eq!(
        parsed["live_readiness"]["overall_state"],
        "local_ready_live_pending"
    );
    assert_eq!(parsed["live_readiness"]["ga_local_mapped_only"], true);
    assert_eq!(parsed["live_readiness"]["desktop_browser_live_gated"], true);
    assert_eq!(parsed["live_readiness"]["browser_worker_frozen"], true);
    assert_eq!(parsed["live_readiness"]["live_worker_available"], false);
    assert_eq!(
        parsed["live_readiness"]["real_external_acceptance_pending"],
        true
    );
    assert_eq!(
        parsed["live_readiness"]["provider_live_request_verified_by_status"],
        false
    );
    assert_eq!(parsed["live_readiness"]["ready_does_not_mean_live"], true);
    assert_eq!(parsed["live_adapter_gates"]["ok"], true);
    assert_eq!(
        parsed["live_adapter_gates"]["overall_state"],
        "disabled_by_default"
    );
    assert_eq!(parsed["live_adapter_gates"]["gate_count"], 3);
    assert_eq!(parsed["live_adapter_gates"]["enabled_count"], 0);
    let live_adapter_gates = parsed["live_adapter_gates"]["gates"]
        .as_array()
        .expect("live adapter gates array");
    assert!(live_adapter_gates.iter().any(|gate| {
        gate["name"] == "subagent_runner"
            && gate["state"] == "disabled"
            && gate["enabled"] == false
            && gate["default_enabled"] == false
            && gate["env_value_state"] == "unset"
            && gate["required_env"] == "CHUANG_CODEX_RUNNER_ENABLE"
            && gate["audit_label"] == "subagent.runner.live"
            && gate["preflight_checks"]
                .as_array()
                .expect("preflight checks should be array")
                .iter()
                .any(|check| {
                    check
                        .as_str()
                        .expect("preflight check should be text")
                        .contains("capability routing")
                })
            && gate["must_reject_capabilities"]
                .as_array()
                .expect("must reject capabilities should be array")
                .iter()
                .any(|capability| {
                    capability
                        .as_str()
                        .expect("capability should be text")
                        .contains("core-memory write")
                })
            && gate["reason"]
                .as_str()
                .expect("gate reason should be text")
                .contains("disabled by default")
            && gate["next_action"]
                .as_str()
                .expect("gate next action should be text")
                .contains("preflight evidence")
    }));
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
fn app_server_health_reports_workspace_config_mismatch() {
    let workspace = temp_workspace("health-workspace-mismatch");
    let configured_workspace = temp_workspace("health-configured-workspace");
    write_basic_stub_workspace(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "app-server",
            "health",
            "--workspace-root",
            workspace.to_str().expect("workspace path should be utf8"),
            "--json",
        ])
        .env("CHUANG_AGENT_APP_SERVER_TEST_API_KEY", "test-key")
        .env(
            "CHUANG_AGENT_WORKSPACE_ROOT",
            configured_workspace.display().to_string(),
        )
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_ACTUATOR_ENABLE")
        .output()
        .expect("app-server health should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(
        parsed["workspace"]["workspace_root"],
        workspace.display().to_string()
    );
    assert_eq!(
        parsed["workspace"]["app_server_child_root"],
        workspace.display().to_string()
    );
    assert_eq!(
        parsed["workspace"]["config_root"],
        configured_workspace.display().to_string()
    );
    assert_eq!(parsed["workspace"]["matches_config"], false);
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
    write_identity_registry(&workspace);
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
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_ACTUATOR_ENABLE")
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
    assert_eq!(parsed["subagent_readiness"]["live_adapter_ready"], false);
    assert_eq!(parsed["subagent_readiness"]["live_worker_available"], false);
    assert_eq!(
        parsed["subagent_readiness"]["worker_runtime_state"],
        "local_contract_only"
    );
    assert!(parsed["subagent_readiness"]["worker_runtime_reason"]
        .as_str()
        .expect("subagent worker runtime reason should be text")
        .contains("subagent slot is fake"));
    assert!(
        parsed["subagent_readiness"]["worker_runtime_blocked_reason"]
            .as_str()
            .expect("subagent worker runtime blocked reason should be text")
            .contains("live_worker_unavailable")
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_route_state"],
        "requires_dispatch_required_capabilities"
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_mismatch_blocks_live"],
        true
    );
    assert!(parsed["subagent_readiness"]["capability_mismatch_reason"]
        .as_str()
        .expect("subagent capability mismatch reason should be text")
        .contains("required_capabilities"));
    assert!(parsed["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "command_runner"
            && layer["local_contract_state"] == "ready"
            && layer["live_adapter_state"] == "deferred"
            && layer["live_worker_available"] == false
            && layer["worker_runtime_state"] == "local_contract_only"
            && layer["blocked_reason"]
                .as_str()
                .expect("layer blocked reason should be text")
                .contains("local contract evidence only")
            && layer["capability_route_state"] == "not_live_routed"
            && layer["capability_mismatch_blocks_live"] == true
            && layer["capability_mismatch_reason"]
                .as_str()
                .expect("layer capability mismatch reason should be text")
                .contains("live-preflight")));
    assert_eq!(
        parsed["live_adapter_gates"]["overall_state"],
        "disabled_by_default"
    );
    assert!(parsed["live_adapter_gates"]["gates"]
        .as_array()
        .expect("live adapter gates array")
        .iter()
        .any(|gate| gate["name"] == "control_apply"
            && gate["required_env"] == "CHUANG_REAL_CONTROL_ENABLE"
            && gate["enabled"] == false));
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
    write_identity_registry(&workspace);
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
        .env(
            "CHUANG_AGENT_WORKSPACE_ROOT",
            workspace.display().to_string(),
        )
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .env_remove("CHUANG_REAL_CONTROL_ENABLE")
        .env_remove("CHUANG_REAL_ACTUATOR_ENABLE")
        .output()
        .expect("app-server health should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("app_server_ok: true"));
    assert!(stdout.contains(&format!("workspace_config_root: {}", workspace.display())));
    assert!(stdout.contains(&format!(
        "workspace_app_server_child_root: {}",
        workspace.display()
    )));
    assert!(stdout.contains("workspace_matches_config: true"));
    assert!(stdout.contains("diagnostic_status: warning"));
    assert!(stdout.contains("diagnostic_summary:"));
    assert!(stdout.contains("next_actions:"));
    assert!(stdout.contains(
        "subagent_live_worker: enabled=false adapter_kind=none status=disabled starts_worker=false available=false reason=subagent_live_worker disabled by default; no live worker is started"
    ));
    assert!(stdout.contains(
        "provider_readiness: ok=true state=ready kind=openai_compatible transport=stub fallback_configured=false"
    ));
    assert!(stdout.contains(
        "atomic_tools: source=GenericAgent ok=true total=9 mapped=9 interface_only=0 manifest_schema_version=1 action_schema_version=1 report_schema_version=6"
    ));
    assert!(stdout.contains("atomic_tools_executable: mouse,keyboard,screenshot,locate,file_read,file_write,code_execute,wait,human_suspend"));
    assert!(stdout.contains("atomic_tools_interface_only: none"));
    assert!(stdout.contains(
        "atomic_tools_desktop_browser_interface_only: none reason=all GA atoms are mapped to governed runtime ports; real desktop/browser execution still requires an audited actuator adapter, live gate, allowlist, and receipt"
    ));
    assert!(stdout.contains(
        "atomic_tools_desktop_browser_live_gated: mouse,keyboard,screenshot,locate required=adapter,live_gate,allowlist,audit_receipt"
    ));
    assert!(stdout.contains(
        "atomic_tools_self_check_entrypoints: status --json,doctor --json,app-server health --diagnostic --json"
    ));
    assert!(stdout.contains("policy_tool_status: active_profile=local_ga normal_local_action_default=file_write/code_execute/open_app/click/input=allow_with_audit"));
    assert!(stdout.contains("high_risk_boundary=external_send=require_approval"));
    assert!(stdout.contains("ga_tool_descriptors=9/12 missing=0"));
    assert!(stdout.contains(
        "goal_mode: ok=true kind=lightweight_runtime_context cli_entrypoint=run --goal TEXT context_source=goal default_goal_id=mainline-mvp allowed_slots=context,governance,execution,report,memory checkpoint_policy=progress_log:true handoff:true commit:true final_report_policy=validation:true next_steps:true bypasses_governance=false adds_core_slot=false"
    ));
    assert!(stdout.contains("goal_run: ok=true"));
    assert!(stdout.contains("goal_run_readiness: ok=true plan_exists=true goal_id=mainline-mvp"));
    assert!(stdout.contains("runtime_report_surface: ok=true artifacts=11 observability_fields=26"));
    assert!(stdout.contains("runtime_event_tool_started_count"));
    assert!(stdout.contains("runtime_event_tool_finished_count"));
    assert!(stdout.contains("runtime_event_approval_requested_count"));
    assert!(stdout.contains("runtime_event_approval_resolved_count"));
    assert!(stdout.contains("runtime_event_elicitation_requested_count"));
    assert!(stdout.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(stdout.contains("tool_protocol_error_count"));
    assert!(stdout.contains("tool_unified_execution_status"));
    assert!(stdout.contains("tool_unified_execution_failure_count"));
    assert!(stdout.contains("tool_unified_execution_failure_classes"));
    assert!(stdout.contains("runtime_meta.runtime_event_ledger_json"));
    assert!(stdout.contains("runtime_meta.context_compaction_events"));
    assert!(stdout.contains("runtime_meta.context_compaction_summary_json"));
    assert!(stdout.contains("runtime_response.trace"));
    assert!(stdout.contains("runtime_response_trace_chars"));
    assert!(stdout.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(stdout.contains("runtime_meta.subagent_children_summary_json"));
    assert!(stdout.contains("goal_handoff_parent_context_handoff_count"));
    assert!(stdout.contains("goal_handoff_report_admission_ref_count"));
    assert!(stdout.contains("goal_handoff_report_admission_refs"));
    assert!(stdout.contains("goal_handoff_report_admission_reason_codes"));
    assert!(stdout.contains("subagent_children_child_count"));
    assert!(stdout.contains("subagent_children_accepted_report_count"));
    assert!(stdout.contains("subagent_children_report_admission_ref_count"));
    assert!(stdout.contains("subagent_children_report_admission_refs"));
    assert!(stdout.contains("subagent_children_missing_report_count"));
    assert!(stdout.contains("subagent_children_report_reason_codes"));
    assert!(stdout.contains("context_compaction_summary_json"));
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
        "local_contract_readiness: ok=true state=ready contracts=6 connects_real_external_services=false writes_core_memory=false executes_plugins=false"
    ));
    assert!(stdout.contains(
        "subagent_readiness: ok=true state=queued_protocol_partial mode=fake local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=partial"
    ));
    assert!(stdout.contains("live_worker_available=false worker_runtime_state=local_contract_only"));
    assert!(stdout.contains("worker_runtime_blocked_reason=live_worker_unavailable"));
    assert!(stdout.contains("capability_route_state=requires_dispatch_required_capabilities"));
    assert!(stdout.contains("capability_mismatch_blocks_live=true"));
    assert!(stdout.contains("capability_mismatch_reason=live subagent preflight"));
    assert!(stdout.contains("subagent_worker_runtime_reason: subagent slot is fake"));
    assert!(stdout.contains("subagent_readiness_local_contract_reason:"));
    assert!(stdout.contains("protocol-ready"));
    assert!(stdout.contains("subagent_readiness_live_adapter_reason:"));
    assert!(stdout.contains("not yet connected"));
    assert!(stdout.contains(
        "subagent_layer name=live_runner_rehearsal state=ready local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=deferred live_worker_available=false worker_runtime_state=local_contract_only blocked_reason=live_runner_rehearsal is read-only"
    ));
    assert!(stdout.contains("capability_mismatch_blocks_live=true"));
    assert!(stdout.contains("capability mismatch or missing dispatch required_capabilities"));
    assert!(stdout.contains(
        "live_adapter_gates: ok=true state=disabled_by_default gates=3 enabled=0 disabled=3"
    ));
    assert!(stdout.contains("live_readiness: ok=true state=local_ready_live_pending"));
    assert!(stdout.contains("local_ready_scope=ready/local-ready only covers local contracts"));
    assert!(stdout.contains("ga_local_mapped_only=true"));
    assert!(stdout.contains("desktop_browser_live_gated=true"));
    assert!(stdout.contains("browser_worker_frozen=true"));
    assert!(stdout.contains("real_external_acceptance_pending=true"));
    assert!(stdout.contains("provider_live_request_verified_by_status=false"));
    assert!(stdout.contains("ready_does_not_mean_live=true"));
    assert!(stdout.contains(
        "live_adapter_gate name=subagent_runner state=disabled enabled=false default_enabled=false env_value_state=unset required_env=CHUANG_CODEX_RUNNER_ENABLE audit_label=subagent.runner.live"
    ));
    assert!(stdout.contains("preflight=confirm CHUANG_CODEX_RUNNER_ENABLE=1"));
    assert!(stdout.contains("must_reject=unscoped external worker pool"));
    assert!(
        stdout.contains("reason=live adapter execution for subagent_runner is disabled by default")
    );
    assert!(stdout
        .contains("next=keep disabled until the operator approves exact live adapter targets"));
    assert!(stdout.contains("set CHUANG_AGENT_APP_SERVER_TEXT_TEST_API_KEY"));
}
