use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;
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
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
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
    assert_eq!(
        parsed["runtime_observability"]["tool_protocol_error_count"],
        "0"
    );
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
        parsed["runtime_observability"]["tool_unified_execution_status"],
        "ok"
    );
    assert_eq!(
        parsed["runtime_observability"]["tool_unified_execution_failure_count"],
        "0"
    );
    assert_eq!(parsed["runtime_observability"]["runtime_event_count"], "0");
    assert_eq!(
        parsed["runtime_observability"]["runtime_event_tool_started_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["runtime_event_tool_finished_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["runtime_event_approval_requested_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["runtime_event_approval_resolved_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["runtime_event_elicitation_requested_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["goal_handoff_parent_context_handoff_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["goal_handoff_report_admission_ref_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["goal_handoff_report_admission_refs"],
        "none"
    );
    assert_eq!(
        parsed["runtime_observability"]["goal_handoff_report_admission_reason_codes"],
        "none"
    );
    assert_eq!(
        parsed["runtime_observability"]["subagent_children_child_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["subagent_children_accepted_report_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["subagent_children_report_admission_ref_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["subagent_children_missing_report_count"],
        "0"
    );
    assert_eq!(
        parsed["runtime_observability"]["subagent_children_report_admission_refs"],
        "none"
    );
    assert_eq!(
        parsed["runtime_observability"]["subagent_children_report_reason_codes"],
        "none"
    );
    assert!(
        parsed["runtime_observability"]["runtime_response_trace_chars"]
            .as_str()
            .expect("runtime response trace char count should be a string")
            .parse::<usize>()
            .expect("runtime response trace char count should parse")
            > 0
    );
    assert!(parsed["runtime_observability"]["context_pack_trace"]
        .as_str()
        .expect("context pack trace should be a string")
        .contains("normalize_tokens:"));
    assert!(parsed["runtime_observability"]["context_compaction_events"]
        .as_str()
        .expect("context compaction events should be a string")
        .contains("context_compaction_started"));
    assert!(
        parsed["runtime_observability"]["context_compaction_summary_json"]
            .as_str()
            .expect("context compaction summary should be a string")
            .contains("\"dropped_count\"")
    );
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
    assert!(
        parsed["runtime_observability"]["tool_surface_callable_tools"]
            .as_str()
            .expect("runtime tool surface callable tools should be string")
            .contains("file_read")
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
fn cli_channel_simulate_surfaces_nonzero_tool_protocol_errors() {
    let workspace = temp_workspace("simulate-tool-protocol-error");
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
                "id": "chatcmpl-channel-tool-protocol",
                "object": "chat.completion",
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
provider_id = "channel-protocol"
base_url = "http://{address}/v1"
model = "gpt-channel-protocol"
api_key_env = "CHUANG_AGENT_CHANNEL_TEST_API_KEY"
transport = "http"
"#,
        ),
    )
    .expect("config should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("simulate")
        .arg("--workspace-root")
        .arg(&workspace)
        .arg("--message-id")
        .arg("msg-protocol-1")
        .arg("--sender-id")
        .arg("user-1")
        .arg("--thread-id")
        .arg("thread-protocol-1")
        .arg("--text")
        .arg("读取文件")
        .arg("--json")
        .env("CHUANG_AGENT_CHANNEL_TEST_API_KEY", "test-key")
        .output()
        .expect("channel simulate should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().expect("server thread should finish");
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(parsed["tool_protocol_error_count"], 1);
    assert_eq!(
        parsed["runtime_observability"]["tool_protocol_error_count"],
        "1"
    );
    assert_eq!(
        parsed["runtime_observability"]["tool_unified_execution_status"],
        "ok"
    );
    let protocol_errors = parsed["tool_protocol_errors"]
        .as_array()
        .expect("tool protocol errors should be array");
    assert_eq!(protocol_errors.len(), 1);
    assert_eq!(protocol_errors[0]["code"], "invalid_action_json");
    assert!(parsed["provider_meta"]["tool_protocol_errors_json"]
        .as_str()
        .expect("provider meta protocol errors")
        .contains("invalid_action_json"));
    assert!(parsed["tool_events"]
        .as_array()
        .expect("tool events should be array")
        .iter()
        .any(|event| event["kind"] == "protocol_error"));
    assert!(parsed["outbound"]["text"]
        .as_str()
        .expect("outbound text")
        .contains("已修正协议错误"));
}

#[test]
fn cli_channel_simulate_text_surfaces_protocol_error_codes_without_raw_payload() {
    let workspace = temp_workspace("simulate-text-tool-protocol-error");
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
                "id": "chatcmpl-channel-text-tool-protocol",
                "object": "chat.completion",
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
provider_id = "channel-text-protocol"
base_url = "http://{address}/v1"
model = "gpt-channel-text-protocol"
api_key_env = "CHUANG_AGENT_CHANNEL_TEST_API_KEY"
transport = "http"
"#,
        ),
    )
    .expect("config should write");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("simulate")
        .arg("--workspace-root")
        .arg(&workspace)
        .arg("--message-id")
        .arg("msg-protocol-text-1")
        .arg("--sender-id")
        .arg("user-1")
        .arg("--thread-id")
        .arg("thread-protocol-text-1")
        .arg("--text")
        .arg("读取文件")
        .env("CHUANG_AGENT_CHANNEL_TEST_API_KEY", "test-key")
        .output()
        .expect("channel simulate should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().expect("server thread should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("tool_surface_available: true"));
    assert!(stdout.contains("tool_surface_governed: true"));
    assert!(stdout.contains("tool_surface_callable_tools:"));
    assert!(stdout.contains("file_read"));
    assert!(stdout.contains("tool_unified_execution_status: ok"));
    assert!(stdout.contains("tool_unified_execution_failure_count: 0"));
    assert!(stdout.contains("tool_protocol_error_count: 1"));
    assert!(stdout.contains("live_readiness_state: local_ready_live_pending"));
    assert!(stdout.contains("live_readiness_real_external_acceptance_pending: true"));
    assert!(stdout.contains("live_readiness_ready_does_not_mean_live: true"));
    assert!(stdout.contains("tool_protocol_error_codes: invalid_action_json"));
    assert!(stdout.contains("reply: 已修正协议错误。"));
    assert!(!stdout.contains("missing field"));
    assert!(!stdout.contains("ACTION:"));
    assert!(!stdout.contains(r#""type":"tool_call""#));
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
fn cli_channel_simulate_surfaces_tool_context_and_readonly_guidance() {
    let workspace = temp_workspace("simulate-tool-surface");
    write_workspace_config(&workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .arg("channel")
        .arg("simulate")
        .arg("--workspace-root")
        .arg(&workspace)
        .arg("--message-id")
        .arg("msg-surface-1")
        .arg("--sender-id")
        .arg("user-1")
        .arg("--thread-id")
        .arg("thread-surface-1")
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

    assert_eq!(parsed["tool_surface"]["available"], true);
    assert_eq!(parsed["tool_surface"]["governed"], true);
    assert_eq!(parsed["tool_surface"]["instruction_context_injected"], true);
    assert_eq!(
        parsed["tool_surface"]["mapped_atomic_tools"],
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
    assert!(parsed["tool_surface"]["callable_tools"]
        .as_array()
        .expect("callable tools should be array")
        .iter()
        .any(|tool| tool == "locate"));
    assert!(parsed["tool_surface"]["callable_tools"]
        .as_array()
        .expect("callable tools should be array")
        .iter()
        .any(|tool| tool == "screenshot"));
    assert!(parsed["tool_surface"]["callable_tools"]
        .as_array()
        .expect("callable tools should be array")
        .iter()
        .any(|tool| tool == "memory_recall"));
    assert_eq!(
        parsed["runtime_observability"]["tool_surface_available"],
        "true"
    );
    assert_eq!(
        parsed["runtime_observability"]["tool_surface_governed"],
        "true"
    );
    assert_eq!(
        parsed["runtime_observability"]["tool_instruction_context_injected"],
        "true"
    );
    assert!(
        parsed["runtime_observability"]["tool_surface_callable_tools"]
            .as_str()
            .expect("tool surface callable tools")
            .contains("locate")
    );
    assert!(
        parsed["runtime_observability"]["tool_surface_callable_tools"]
            .as_str()
            .expect("tool surface callable tools")
            .contains("screenshot")
    );
    assert!(
        parsed["runtime_observability"]["tool_surface_callable_tools"]
            .as_str()
            .expect("tool surface callable tools")
            .contains("memory_recall")
    );
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
FEISHU_ENCRYPT_KEY=legacy-encrypt
HERMES_FEISHU_APP_SECRET=legacy-hermes-secret
HERMES_FEISHU_VERIFICATION_TOKEN=legacy-hermes-token
CODEX_FEISHU_BOT_ID=legacy-codex-bot
CODEX_FEISHU_APP_SECRET=legacy-codex-secret
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
    let legacy_var_names = parsed["legacy_var_names"]
        .as_array()
        .expect("legacy vars should be array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("legacy var should be string")
                .to_string()
        })
        .collect::<Vec<_>>();
    for expected in [
        "FEISHU_APP_ID",
        "FEISHU_ENCRYPT_KEY",
        "HERMES_FEISHU_APP_SECRET",
        "HERMES_FEISHU_VERIFICATION_TOKEN",
        "CODEX_FEISHU_BOT_ID",
        "CODEX_FEISHU_APP_SECRET",
    ] {
        assert!(
            legacy_var_names.iter().any(|name| name == expected),
            "expected legacy var {expected} in {legacy_var_names:?}"
        );
    }
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
