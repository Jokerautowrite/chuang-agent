use chuang_agent::mcp_fake_adapter::{
    mcp_call_runtime_events, mcp_tool_descriptor_risk, mcp_tool_risk_view, McpAdapterError,
    McpFakeServer, McpRuntimeEventInput, McpToolCall, McpToolSpec,
};
use chuang_agent::runtime_event_ledger::{
    InMemoryRuntimeEventLedger, RuntimeEventKind, RuntimeEventLedger,
};
use serde_json::{json, Value};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" }
        }
    })
}

fn read_only_spec(name: &str) -> McpToolSpec {
    McpToolSpec::new(name, "Read-only MCP tool", schema())
        .read_only(true)
        .risk_tags(["read"])
}

#[test]
fn fake_server_lists_registered_tools_without_real_mcp_process() {
    let server = McpFakeServer::new()
        .with_tool(read_only_spec("local.search"), json!({"items": []}))
        .with_tool(
            McpToolSpec::new("local.write_note", "Write local note", schema())
                .read_only(false)
                .destructive(false)
                .open_world(false)
                .external_commit(false)
                .risk_tags(["file_write"]),
            json!({"written": true}),
        );

    let response = server
        .handle_request(r#"{"method":"tools/list"}"#)
        .expect("tools/list should work");
    let tools = response["tools"].as_array().expect("tools should be array");

    assert_eq!(response["method"], "tools/list");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "local.search");
    assert_eq!(tools[0]["read_only"], true);
    assert_eq!(tools[1]["name"], "local.write_note");
    assert_eq!(tools[1]["destructive"], false);
    assert_eq!(response["stderr_preview"], Value::Null);
}

#[test]
fn fake_server_calls_registered_tool_and_returns_clean_result_shape() {
    let server = McpFakeServer::new().with_tool(
        read_only_spec("local.search"),
        json!({"items": [{"title": "result"}]}),
    );

    let result = server
        .call_tool(McpToolCall {
            name: "local.search".to_string(),
            arguments: json!({"query": "hello"}),
        })
        .expect("registered tool should return result");

    assert_eq!(result.tool_name, "local.search");
    assert!(result.ok);
    assert_eq!(result.risk.name, "local.search");
    assert!(result.risk.read_only);
    assert!(!result.arguments_redacted);
    assert_eq!(result.arguments["query"], "hello");
    assert_eq!(result.content["items"][0]["title"], "result");
    assert_eq!(result.stderr_preview, None);
    assert!(!result.output_redacted);

    let rpc_result = server
        .handle_request(
            r#"{"method":"tools/call","params":{"name":"local.search","arguments":{"query":"hello"}}}"#,
        )
        .expect("tools/call request should work");
    assert_eq!(rpc_result["tool_name"], "local.search");
    assert_eq!(rpc_result["ok"], true);
    assert_eq!(rpc_result["risk"]["name"], "local.search");
    assert_eq!(rpc_result["arguments"]["query"], "hello");
}

#[test]
fn fake_server_reports_unknown_tool_and_malformed_json_as_structured_errors() {
    let server = McpFakeServer::new().with_tool(read_only_spec("local.search"), json!({}));

    let unknown = server
        .handle_request(
            r#"{"method":"tools/call","params":{"name":"missing.tool","arguments":{}}}"#,
        )
        .expect_err("missing tool should be rejected");
    assert_eq!(
        unknown,
        McpAdapterError::UnknownTool {
            name: "missing.tool".to_string(),
            stderr_preview: None,
        }
    );

    let malformed = server
        .handle_request(r#"{"method":"tools/list""#)
        .expect_err("bad json should be rejected");
    match malformed {
        McpAdapterError::MalformedJson {
            message,
            stderr_preview,
        } => {
            assert!(message.contains("malformed"));
            assert_eq!(stderr_preview, None);
        }
        other => panic!("expected malformed json error, got {other:?}"),
    }

    let unsupported = server
        .handle_request(r#"{"method":"tools/unknown"}"#)
        .expect_err("unsupported method should be rejected");
    assert_eq!(
        unsupported,
        McpAdapterError::UnsupportedMethod {
            method: "tools/unknown".to_string(),
            stderr_preview: None,
        }
    );
}

#[test]
fn fake_server_supports_timeout_like_error_and_stderr_noise_capture() {
    let server = McpFakeServer::new()
        .with_stderr_noise("debug: server warmed up")
        .with_stderr_noise("warning: token=stderr-secret-value should not leak")
        .with_timeout_tool(read_only_spec("slow.tool"), 25);

    let error = server
        .call_tool(McpToolCall {
            name: "slow.tool".to_string(),
            arguments: json!({}),
        })
        .expect_err("timeout tool should return fake timeout");

    match error {
        McpAdapterError::FakeTimeout {
            tool_name,
            timeout_ms,
            stderr_preview,
        } => {
            let preview = stderr_preview.expect("stderr preview should be captured");
            assert_eq!(tool_name, "slow.tool");
            assert_eq!(timeout_ms, 25);
            assert!(preview.contains("server warmed up"));
            assert!(preview.contains("token=<redacted>"));
            assert!(!preview.contains("stderr-secret-value"));
        }
        other => panic!("expected fake timeout, got {other:?}"),
    }
}

#[test]
fn fake_server_captures_tool_error_without_leaking_secret_noise() {
    let server = McpFakeServer::new()
        .with_stderr_noise("noise api_key=stderr-key-value")
        .with_error_tool(
            read_only_spec("failing.tool"),
            "provider returned secret=tool-error-secret",
        );

    let error = server
        .handle_request(r#"{"method":"tools/call","params":{"name":"failing.tool"}}"#)
        .expect_err("fake tool error should be structured");

    match error {
        McpAdapterError::ToolFailed {
            tool_name,
            message,
            stderr_preview,
        } => {
            assert_eq!(tool_name, "failing.tool");
            assert!(message.contains("secret=<redacted>"));
            assert!(!message.contains("tool-error-secret"));

            let preview = stderr_preview.expect("stderr preview should exist");
            assert!(preview.contains("api_key=<redacted>"));
            assert!(!preview.contains("stderr-key-value"));
        }
        other => panic!("expected tool failure, got {other:?}"),
    }
}

#[test]
fn mcp_tool_specs_convert_to_descriptor_like_risk_flags() {
    let read_only = mcp_tool_risk_view(&read_only_spec("local.search"));
    assert_eq!(read_only.name, "local.search");
    assert!(read_only.read_only);
    assert!(!read_only.destructive);
    assert!(!read_only.open_world);
    assert!(!read_only.external_commit);
    assert!(!read_only.requires_approval);
    assert!(!read_only.omitted_risk_defaults_tightened);
    assert_eq!(read_only.permission_decision_hint, "allow");

    let local_mutation = McpToolSpec::new("local.write_note", "Write local note", schema())
        .read_only(false)
        .destructive(false)
        .open_world(false)
        .external_commit(false)
        .risk_tags(["file_write"])
        .risk_view();
    assert!(!local_mutation.read_only);
    assert!(!local_mutation.destructive);
    assert!(!local_mutation.open_world);
    assert!(!local_mutation.external_commit);
    assert!(!local_mutation.requires_approval);
    assert!(!local_mutation.omitted_risk_defaults_tightened);
    assert_eq!(local_mutation.permission_decision_hint, "allow_with_audit");

    let destructive = McpToolSpec::new("local.delete_note", "Delete local note", schema())
        .read_only(false)
        .destructive(true)
        .open_world(false)
        .external_commit(false)
        .risk_tags(["delete"])
        .risk_view();
    assert!(destructive.destructive);
    assert!(destructive.requires_approval);
    assert!(!destructive.omitted_risk_defaults_tightened);
    assert_eq!(destructive.permission_decision_hint, "require_approval");

    let omitted_risk = McpToolSpec::new("unknown.mutator", "Risk omitted", schema())
        .read_only(false)
        .risk_view();
    assert!(omitted_risk.destructive);
    assert!(omitted_risk.open_world);
    assert!(omitted_risk.requires_approval);
    assert!(omitted_risk.omitted_risk_defaults_tightened);
    assert_eq!(omitted_risk.permission_decision_hint, "require_approval");

    let external = McpToolSpec::new("external.send", "Send message", schema())
        .read_only(false)
        .destructive(false)
        .open_world(true)
        .external_commit(true)
        .risk_tags(["external_send"])
        .risk_view();
    assert!(external.open_world);
    assert!(external.external_commit);
    assert!(external.requires_approval);
    assert!(!external.omitted_risk_defaults_tightened);
    assert_eq!(external.permission_decision_hint, "require_approval");

    let omitted_external_commit =
        McpToolSpec::new("local.unknown_commit", "Risk omitted", schema())
            .read_only(false)
            .destructive(false)
            .open_world(false)
            .risk_view();
    assert!(omitted_external_commit.external_commit);
    assert!(omitted_external_commit.requires_approval);
    assert!(omitted_external_commit.omitted_risk_defaults_tightened);
    assert_eq!(
        omitted_external_commit.permission_decision_hint,
        "require_approval"
    );
}

#[test]
fn mcp_descriptor_converts_into_tool_descriptor_risk_view_without_runtime_side_effects() {
    let spec = McpToolSpec::new("local.write_note", "Write local note", schema())
        .read_only(false)
        .destructive(false)
        .open_world(false)
        .external_commit(false)
        .risk_tags(["file_write"]);
    let mut tags = Vec::new();
    let risk = mcp_tool_descriptor_risk(&spec, &mut tags);

    assert_eq!(risk.name, "local.write_note");
    assert!(!risk.read_only);
    assert!(risk.mutating);
    assert!(!risk.destructive);
    assert!(!risk.external_commit);
    assert!(!risk.requires_approval);
    assert_eq!(risk.risk_tags, ["file_write"]);
}

#[test]
fn mcp_descriptor_conversion_marks_open_world_as_approval_required_even_without_explicit_tag() {
    let spec = McpToolSpec::new("net.search", "Open-world search", schema())
        .read_only(false)
        .destructive(false)
        .open_world(true)
        .external_commit(false);
    let mut tags = Vec::new();
    let risk = mcp_tool_descriptor_risk(&spec, &mut tags);

    assert!(risk.requires_approval);
    assert!(risk.risk_tags.contains(&"open_world"));
}

#[test]
fn tool_results_redact_secret_like_content_and_sensitive_keys() {
    let server = McpFakeServer::new().with_tool(
        read_only_spec("secret.echo"),
        json!({
            "visible": "safe",
            "api_key": "json-key-value",
            "nested": {
                "line": "token=inline-token-value",
                "raw": "sk-live-secret-value"
            }
        }),
    );

    let result = server
        .call_tool(McpToolCall {
            name: "secret.echo".to_string(),
            arguments: json!({"token": "caller-secret-token"}),
        })
        .expect("tool should return redacted content");
    let rendered = serde_json::to_string(&result).expect("result should serialize");

    assert!(result.output_redacted);
    assert!(result.arguments_redacted);
    assert_eq!(result.arguments["token"], "<redacted>");
    assert_eq!(result.content["visible"], "safe");
    assert_eq!(result.content["api_key"], "<redacted>");
    assert!(rendered.contains("token=<redacted>"));
    assert!(!rendered.contains("json-key-value"));
    assert!(!rendered.contains("inline-token-value"));
    assert!(!rendered.contains("sk-live-secret-value"));
}

#[test]
fn tools_list_includes_structured_descriptors_for_audit() {
    let server = McpFakeServer::new().with_tool(
        McpToolSpec::new("external.send", "Send", schema())
            .read_only(false)
            .destructive(false)
            .open_world(true)
            .external_commit(true)
            .risk_tags(["external_send"]),
        json!({"ok": true}),
    );

    let response = server
        .handle_request(r#"{"method":"tools/list"}"#)
        .expect("tools/list should work");
    let descriptors = response["descriptors"]
        .as_array()
        .expect("descriptors should be array");
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0]["spec"]["name"], "external.send");
    assert_eq!(descriptors[0]["risk"]["requires_approval"], true);
    assert_eq!(
        descriptors[0]["risk"]["omitted_risk_defaults_tightened"],
        false
    );
    assert_eq!(
        descriptors[0]["risk"]["permission_decision_hint"],
        "require_approval"
    );
}

#[test]
fn mcp_call_runtime_events_record_result_error_and_approval_without_payload_leakage() {
    let risk = McpToolSpec::new("external.send", "Send", schema())
        .read_only(false)
        .destructive(false)
        .open_world(true)
        .external_commit(true)
        .risk_tags(["external_send"])
        .risk_view();
    let input = McpRuntimeEventInput {
        thread_id: "thread-mcp".to_string(),
        turn_id: Some("turn-mcp".to_string()),
        call_id: "call-mcp-secret-value".to_string(),
        tool_name: "external.send".to_string(),
        risk,
    };

    let events = mcp_call_runtime_events(input, false);
    let approval = events
        .approval_required
        .clone()
        .expect("external MCP tool should request approval");
    assert_eq!(approval.event_type, RuntimeEventKind::ApprovalRequested);
    assert_eq!(approval.thread_id, "thread-mcp");
    assert_eq!(approval.turn_id.as_deref(), Some("turn-mcp"));
    assert_eq!(approval.call_id.as_deref(), Some("call-mcp-secret-value"));
    assert_eq!(
        approval
            .risk_decision
            .as_ref()
            .map(|risk| risk.decision.as_str()),
        Some("require_approval")
    );
    assert_eq!(
        events.tool_started.event_type,
        RuntimeEventKind::ToolStarted
    );
    assert_eq!(
        events.tool_finished.event_type,
        RuntimeEventKind::ToolFinished
    );
    assert_eq!(
        events
            .tool_finished
            .risk_decision
            .as_ref()
            .map(|risk| risk.decision.as_str()),
        Some("tool_error")
    );
    assert_eq!(
        events.tool_finished.evidence_ref.as_deref(),
        Some("mcp://tool/external.send/call-mcp-secret-value/error")
    );

    let mut ledger = InMemoryRuntimeEventLedger::new();
    ledger
        .append(approval)
        .expect("approval event should append");
    ledger
        .append(events.tool_started)
        .expect("started event should append");
    ledger
        .append(events.tool_finished)
        .expect("finished event should append");
    let turn_events = ledger
        .query_by_turn("thread-mcp", "turn-mcp")
        .expect("turn query should work");
    assert_eq!(turn_events.len(), 3);
    let call_events = ledger
        .query_by_call("call-mcp-secret-value")
        .expect("call query should work");
    assert_eq!(call_events.len(), 3);

    let rendered = serde_json::to_string(&call_events).expect("events should serialize");
    assert!(rendered.contains("approval_required"));
    assert!(rendered.contains("tool_error"));
    assert!(!rendered.contains("api_key"));
    assert!(!rendered.contains("payload-secret-value"));
}
