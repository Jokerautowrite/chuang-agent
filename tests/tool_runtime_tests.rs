use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::governance::{RiskDecision, StaticRuleGovernance};
use chuang_agent::tool_runtime::{
    execute_tool_call, execute_tool_call_with_governance, parse_final_answer,
    parse_tool_action_envelope, parse_tool_call, parse_tool_model_output,
    proposed_action_for_tool_call, ExecutionSlot, ShellRiskRules, ToolActionEnvelope, ToolCall,
    ToolExecutionConfig, ToolLoopReport, ToolModelOutput, WriteOperation,
};

fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should move forward")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-tool-{name}-{nanos}"))
}

#[test]
fn parse_tool_protocol_roundtrip() {
    let call = parse_tool_call(r#"TOOL_CALL: {"tool":"list_dir","path":"."}"#)
        .expect("tool call should parse");
    assert!(matches!(call, ToolCall::ListDir { .. }));
    assert_eq!(
        parse_final_answer("FINAL: 已完成").as_deref(),
        Some("已完成")
    );
    assert!(matches!(
        parse_tool_model_output(r#"TOOL_CALL: {"tool":"list_dir","path":"."}"#),
        ToolModelOutput::ToolCall(ToolCall::ListDir { .. })
    ));
    assert_eq!(
        parse_tool_model_output("FINAL: 收口"),
        ToolModelOutput::FinalAnswer("收口".to_string())
    );
    assert_eq!(
        parse_tool_model_output("普通回复"),
        ToolModelOutput::PlainText("普通回复".to_string())
    );
}

#[test]
fn parse_structured_action_protocol_roundtrip() {
    let action = parse_tool_action_envelope(
        r#"ACTION: {"type":"tool_call","call":{"tool":"read_file","path":"src/main.rs"}}"#,
    )
    .expect("structured tool action should parse");

    assert!(matches!(
        action,
        ToolActionEnvelope::ToolCall {
            schema_version: None,
            call: ToolCall::ReadFile { .. }
        }
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"list_dir","path":"."}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::ListDir { .. })
    ));
    assert_eq!(
        parse_tool_model_output(r#"ACTION: {"type":"final","answer":"完成"}"#),
        ToolModelOutput::FinalAnswer("完成".to_string())
    );
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"list_dir","path":"."}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::ListDir { .. })
    ));
}

#[test]
fn parse_structured_action_accepts_ga_atomic_tool_names() {
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"file_read","path":"src/main.rs"}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::ReadFile { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"file_write","path":"notes/out.txt","content":"hello"}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::WriteFile { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"code_execute","command":"cargo test","cwd":"."}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::ShellExec { .. })
    ));
}

#[test]
fn parse_structured_action_does_not_accept_interface_only_desktop_tools_yet() {
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"mouse","x":10,"y":20}}"#
        ),
        ToolModelOutput::ProtocolError(_)
    ));
}

#[test]
fn parse_structured_action_reports_protocol_errors() {
    assert!(matches!(
        parse_tool_model_output(r#"ACTION: {"type":"tool_call","call":{"tool":"file_read"}}"#),
        ToolModelOutput::ProtocolError(error)
            if error.code == "invalid_action_json"
                && error.message.contains("missing field")
    ));
    assert!(matches!(
        parse_tool_model_output(r#"TOOL_CALL: {"tool":"file_write","path":"notes/a.txt"}"#),
        ToolModelOutput::ProtocolError(error)
            if error.code == "invalid_legacy_tool_call_json"
    ));
    assert!(matches!(
        parse_tool_model_output(r#"ACTION: {"type":"final","answer":""}"#),
        ToolModelOutput::ProtocolError(error) if error.code == "empty_final_answer"
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"schema_version":5,"type":"tool_call","call":{"tool":"list_dir","path":"."}}"#
        ),
        ToolModelOutput::ProtocolError(error)
            if error.code == "unsupported_action_schema_version"
    ));
}

#[test]
fn tool_instruction_block_prefers_ga_atomic_tool_names() {
    let root = temp_workspace("instruction");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let instructions = chuang_agent::tool_runtime::tool_instruction_block(&root);

    assert!(instructions.contains("file_read, file_write, code_execute"));
    assert!(instructions.contains("辅助工具：list_dir"));
    assert!(instructions.contains(r#""schema_version":1"#));
    assert!(instructions.contains(r#""tool":"file_read""#));
    assert!(instructions.contains("mouse/keyboard/screenshot/locate"));
    assert!(instructions.contains("进入工具往返"));
}

#[test]
fn tool_loop_report_exposes_schema_contract_fields() {
    assert_eq!(ToolLoopReport::schema_version(), 6);
    assert_eq!(
        ToolLoopReport::schema_fields(),
        &[
            "schema_version",
            "status",
            "workspace_root",
            "rounds",
            "call_count",
            "calls",
        ]
    );
    assert!(ToolLoopReport::call_schema_fields().contains(&"atomic_tool_name"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"failure_class"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"target_path"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"resolved_path"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"cwd"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"command"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"entries"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"output_bytes"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"output_lines"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"stderr_bytes"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"stderr_lines"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"write_operation"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"write_diff_preview"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"write_diff_truncated"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"output_redacted"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"stdout_redacted"));
    assert!(ToolLoopReport::call_schema_fields().contains(&"stderr_redacted"));
}

#[test]
fn tool_action_envelope_exposes_schema_contract_fields() {
    assert_eq!(ToolActionEnvelope::schema_version(), 1);
    assert_eq!(
        ToolActionEnvelope::schema_fields(),
        &["schema_version", "type", "call", "answer"]
    );
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"tool"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"path"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"content"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"command"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"cwd"));
}

#[test]
fn tool_runtime_can_read_write_list_and_shell_exec() {
    let root = temp_workspace("basic");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("input.txt"), "hello").expect("seed file should write");

    let write = execute_tool_call(
        &root,
        &ToolCall::WriteFile {
            path: "nested/output.txt".to_string(),
            content: "world".to_string(),
        },
    );
    assert!(write.ok, "write should succeed: {}", write.summary);
    assert_eq!(write.tool_name, "write_file");
    assert_eq!(write.atomic_tool_name.as_deref(), Some("file_write"));
    assert_eq!(write.failure_class, None);
    assert!(!write.retryable);
    assert_eq!(
        write.changed_files,
        vec![root.join("nested/output.txt").display().to_string()]
    );
    assert_eq!(write.write_before_bytes, None);
    assert_eq!(write.write_after_bytes, Some(5));
    assert_eq!(write.write_changed, Some(true));
    assert_eq!(write.write_operation, Some(WriteOperation::Created));
    assert_eq!(write.target_path.as_deref(), Some("nested/output.txt"));
    assert_eq!(
        write.resolved_path.as_deref(),
        Some(
            root.join("nested/output.txt")
                .display()
                .to_string()
                .as_str()
        )
    );
    assert!(write
        .write_diff_preview
        .as_deref()
        .is_some_and(|preview| preview.contains("+world")));
    assert!(!write.write_diff_truncated);

    let rewrite = execute_tool_call(
        &root,
        &ToolCall::WriteFile {
            path: "nested/output.txt".to_string(),
            content: "world".to_string(),
        },
    );
    assert!(rewrite.ok, "rewrite should succeed: {}", rewrite.summary);
    assert_eq!(rewrite.write_before_bytes, Some(5));
    assert_eq!(rewrite.write_after_bytes, Some(5));
    assert_eq!(rewrite.write_changed, Some(false));
    assert_eq!(rewrite.write_operation, Some(WriteOperation::Unchanged));
    assert_eq!(rewrite.write_diff_preview.as_deref(), Some("unchanged"));

    let list = execute_tool_call(
        &root,
        &ToolCall::ListDir {
            path: ".".to_string(),
        },
    );
    assert!(list.ok, "list should succeed: {}", list.summary);
    assert_eq!(list.atomic_tool_name, None);
    assert_eq!(list.target_path.as_deref(), Some("."));
    assert_eq!(
        list.resolved_path.as_deref(),
        Some(root.display().to_string().as_str())
    );
    assert!(list
        .entries
        .iter()
        .any(|entry| entry.name == "input.txt" && entry.kind == "file"));
    assert!(list
        .entries
        .iter()
        .any(|entry| entry.name == "nested" && entry.kind == "dir"));
    assert!(list.summary.contains("input.txt"));
    assert!(list.output_bytes.is_some());
    assert_eq!(list.output_lines, Some(1));

    let read = execute_tool_call(
        &root,
        &ToolCall::ReadFile {
            path: "nested/output.txt".to_string(),
        },
    );
    assert!(read.ok, "read should succeed: {}", read.summary);
    assert!(read.summary.contains("world"));
    assert_eq!(read.output.as_deref(), Some("world"));
    assert_eq!(read.target_path.as_deref(), Some("nested/output.txt"));
    assert_eq!(read.output_bytes, Some(5));
    assert_eq!(read.output_lines, Some(1));
    assert!(!read.output_truncated);

    let shell = execute_tool_call(
        &root,
        &ToolCall::ShellExec {
            command: "printf test-shell".to_string(),
            cwd: Some(".".to_string()),
        },
    );
    assert!(shell.ok, "shell should succeed: {}", shell.summary);
    assert!(shell.summary.contains("test-shell"));
    assert_eq!(shell.stdout.as_deref(), Some("test-shell"));
    assert_eq!(shell.stderr.as_deref(), Some(""));
    assert_eq!(shell.exit_code, Some(0));
    assert_eq!(
        shell.cwd.as_deref(),
        Some(root.display().to_string().as_str())
    );
    assert_eq!(shell.command.as_deref(), Some("printf test-shell"));
    assert_eq!(shell.output_bytes, Some(10));
    assert_eq!(shell.output_lines, Some(1));
    assert_eq!(shell.stderr_bytes, Some(0));
    assert_eq!(shell.stderr_lines, Some(0));
    assert!(!shell.output_redacted);
    assert!(!shell.stdout_redacted);
    assert!(!shell.stderr_redacted);
    assert!(!shell.stdout_truncated);
    assert!(!shell.stderr_truncated);
}

#[test]
fn write_file_diff_preview_redacts_secret_like_changes() {
    let root = temp_workspace("write-redact");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let record = execute_tool_call(
        &root,
        &ToolCall::WriteFile {
            path: ".env".to_string(),
            content: "API_KEY=secret-value".to_string(),
        },
    );

    assert!(record.ok);
    assert_eq!(
        record.write_diff_preview.as_deref(),
        Some("[redacted: secret-like path or content]")
    );
    assert_eq!(record.write_operation, Some(WriteOperation::Created));
    assert!(record.output_redacted);
    assert!(!record.write_diff_truncated);
}

#[test]
fn read_file_redacts_secret_like_content() {
    let root = temp_workspace("read-redact");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join(".env"), "API_KEY=secret-value").expect("seed secret file should write");

    let record = execute_tool_call(
        &root,
        &ToolCall::ReadFile {
            path: ".env".to_string(),
        },
    );

    assert!(record.ok);
    assert_eq!(
        record.output.as_deref(),
        Some("[redacted: secret-like path or content]")
    );
    assert!(record.output_redacted);
    assert_eq!(record.output_bytes, Some("API_KEY=secret-value".len()));
    assert_eq!(record.output_lines, Some(1));
}

#[test]
fn tool_runtime_marks_shell_nonzero_as_structured_failure() {
    let root = temp_workspace("shell-nonzero");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let shell = execute_tool_call(
        &root,
        &ToolCall::ShellExec {
            command: "printf nope >&2; exit 7".to_string(),
            cwd: Some(".".to_string()),
        },
    );

    assert!(!shell.ok);
    assert_eq!(shell.exit_code, Some(7));
    assert_eq!(shell.failure_class.as_deref(), Some("exit_nonzero"));
    assert_eq!(shell.stderr.as_deref(), Some("nope"));
    assert_eq!(shell.stderr_bytes, Some(4));
    assert_eq!(shell.stderr_lines, Some(1));
    assert!(!shell.retryable);
}

#[test]
fn tool_runtime_uses_configured_shell_timeout() {
    let root = temp_workspace("shell-timeout");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let record = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::ShellExec {
            command: "sleep 1".to_string(),
            cwd: Some(".".to_string()),
        },
        &ToolExecutionConfig {
            shell_timeout_ms: 20,
            ..ToolExecutionConfig::default()
        },
    );

    assert!(!record.ok);
    assert_eq!(record.failure_class.as_deref(), Some("timeout"));
    assert!(record.retryable);
}

#[test]
fn tool_runtime_governs_and_audits_tool_calls() {
    let root = temp_workspace("governed");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();

    let outcome = execute_tool_call_with_governance(
        &root,
        &mut governance,
        &ToolCall::WriteFile {
            path: "notes/audit.txt".to_string(),
            content: "copy, reuse, verify".to_string(),
        },
        "app-server",
        "turn-1:tool-1",
    )
    .expect("governed execution should succeed");

    assert!(matches!(outcome.decision, RiskDecision::Allowed { .. }));
    assert!(outcome.record.ok);
    assert!(outcome
        .record
        .decision
        .as_deref()
        .is_some_and(|decision| decision.starts_with("allowed:")));
    assert_eq!(governance.audit_records().len(), 1);
    assert_eq!(governance.audit_records()[0].operation, "tool.file_write");
    assert!(governance.audit_records()[0]
        .reason
        .contains("decision=allowed"));
}

#[test]
fn proposed_action_uses_ga_atomic_tool_identity() {
    let root = temp_workspace("proposed-action");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let write = proposed_action_for_tool_call(
        &root,
        &ToolCall::WriteFile {
            path: "notes/audit.txt".to_string(),
            content: "copy".to_string(),
        },
    );
    let list = proposed_action_for_tool_call(
        &root,
        &ToolCall::ListDir {
            path: ".".to_string(),
        },
    );

    assert_eq!(write.action_id, "tool:file_write");
    assert!(write.summary.contains("atomic_tool=file_write"));
    assert_eq!(list.action_id, "tool:list_dir");
    assert!(list.summary.contains("auxiliary_tool=list_dir"));
}

#[test]
fn execution_slot_wraps_registry_config_and_governed_execution() {
    let root = temp_workspace("execution-slot");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig {
        shell_timeout_ms: 30_000,
        ..ToolExecutionConfig::default()
    });

    assert_eq!(
        slot.registry().mapped_atomic_names(),
        vec!["file_read", "file_write", "code_execute",]
    );
    assert!(slot
        .tool_instruction_block(&root)
        .contains("file_read, file_write, code_execute"));

    let outcome = slot
        .execute_with_governance(
            &root,
            &mut governance,
            &ToolCall::ReadFile {
                path: "missing.txt".to_string(),
            },
            "cli",
            "turn-1:tool-1",
        )
        .expect("read failure is still a governed execution result");

    assert!(!outcome.record.ok);
    assert_eq!(
        outcome.record.atomic_tool_name.as_deref(),
        Some("file_read")
    );
    assert_eq!(governance.audit_records()[0].operation, "tool.file_read");
}

#[test]
fn tool_runtime_requires_approval_for_destructive_shell_commands() {
    let root = temp_workspace("governed-destructive");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();

    let error = execute_tool_call_with_governance(
        &root,
        &mut governance,
        &ToolCall::ShellExec {
            command: "rm -rf notes".to_string(),
            cwd: Some(".".to_string()),
        },
        "app-server",
        "turn-1:tool-1",
    )
    .expect_err("destructive shell command should require approval");

    assert!(error.starts_with("tool_needs_approval:"));
    assert_eq!(governance.audit_records().len(), 1);
    assert_eq!(
        governance.audit_records()[0].operation,
        "tool.code_execute.rejected"
    );
    assert!(governance.audit_records()[0]
        .reason
        .contains("decision=needs_approval"));
}

#[test]
fn tool_runtime_rejects_secret_shell_commands_as_draft_only() {
    let root = temp_workspace("governed-secret");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();

    let error = execute_tool_call_with_governance(
        &root,
        &mut governance,
        &ToolCall::ShellExec {
            command: "cat .env".to_string(),
            cwd: Some(".".to_string()),
        },
        "app-server",
        "turn-1:tool-1",
    )
    .expect_err("secret-bearing shell command should be draft-only");

    assert!(error.starts_with("tool_draft_only:"));
    assert_eq!(governance.audit_records().len(), 1);
    assert_eq!(
        governance.audit_records()[0].operation,
        "tool.code_execute.rejected"
    );
    assert!(governance.audit_records()[0]
        .reason
        .contains("decision=draft_only"));
}

#[test]
fn execution_slot_uses_configured_shell_risk_rules() {
    let root = temp_workspace("shell-risk-rules");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig {
        shell_timeout_ms: 30_000,
        shell_risk_rules: ShellRiskRules {
            network_change: vec![" make deploy".to_string()],
            ..ShellRiskRules::default()
        },
    });

    let error = slot
        .execute_with_governance(
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "make deploy".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-1:tool-1",
        )
        .expect_err("configured network-risk shell command should require approval");

    assert!(error.starts_with("tool_needs_approval:"));
    assert_eq!(
        governance.audit_records()[0].operation,
        "tool.code_execute.rejected"
    );
}

#[test]
fn execution_slot_can_return_governance_rejection_as_record() {
    let root = temp_workspace("governance-reject-record");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());

    let outcome = slot
        .execute_or_reject_with_governance(
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "cat .env".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-1:tool-1",
        )
        .expect("governance rejection should still become a tool record");

    assert!(matches!(outcome.decision, RiskDecision::DraftOnly { .. }));
    assert!(!outcome.record.ok);
    assert_eq!(
        outcome.record.failure_class.as_deref(),
        Some("governance_rejected")
    );
    assert!(outcome
        .record
        .decision
        .as_deref()
        .is_some_and(|decision| decision.starts_with("draft_only:")));
    assert_eq!(
        governance.audit_records()[0].operation,
        "tool.code_execute.rejected"
    );
}

#[test]
fn tool_runtime_reports_structured_failures() {
    let root = temp_workspace("failure");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let record = execute_tool_call(
        &root,
        &ToolCall::ReadFile {
            path: "../outside.txt".to_string(),
        },
    );

    assert!(!record.ok);
    assert_eq!(
        record.failure_class.as_deref(),
        Some("path_outside_workspace")
    );
    assert!(record.summary.contains("path_outside_workspace"));
    assert!(!record.retryable);
}
