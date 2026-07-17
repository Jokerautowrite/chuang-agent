use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::governance::{OperatorApprovalEvidence, RiskDecision, StaticRuleGovernance};
use chuang_agent::runtime_event_ledger::{
    InMemoryRuntimeEventLedger, RuntimeEventKind, RuntimeEventLedger,
};
use chuang_agent::tool_runtime::{
    execute_tool_call, execute_tool_call_with_governance,
    execute_tool_call_with_governance_and_ledger, parse_final_answer, parse_tool_action_envelope,
    parse_tool_action_envelope_result, parse_tool_call, parse_tool_model_output,
    proposed_action_for_tool_call, ExecutionSlot, MemoryToolContext, OperatorApprovalReceipt,
    PendingApproval, ShellRiskRules, ToolActionEnvelope, ToolCall, ToolExecutionConfig,
    ToolLoopReport, ToolModelOutput, ToolSurfaceStatus, WriteOperation,
};
use chuang_agent::workspace_file_adapter::WorkspaceFileAdapter;

fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should move forward")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-tool-{name}-{nanos}"))
}

fn register_operator_approval(
    governance: &mut StaticRuleGovernance,
    pending: &PendingApproval,
) -> OperatorApprovalReceipt {
    let operator_ref = "operator:test".to_string();
    let evidence_ref = format!("operator-evidence://{}", pending.approval_id);
    governance
        .register_operator_approval(OperatorApprovalEvidence {
            approval_id: pending.approval_id.clone(),
            operator_ref: operator_ref.clone(),
            evidence_ref: evidence_ref.clone(),
        })
        .expect("operator approval evidence should register");
    OperatorApprovalReceipt {
        approval_id: pending.approval_id.clone(),
        call_id: pending.call_id.clone(),
        call_fingerprint: pending.call_fingerprint.clone(),
        target_fingerprint: pending.target_fingerprint.clone(),
        workspace_fingerprint: pending.workspace_fingerprint.clone(),
        policy_marker: pending.policy_marker.clone(),
        approved: true,
        operator_ref,
        evidence_ref,
    }
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
fn parse_structured_action_recovers_from_concatenated_final() {
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"locate","target":"screen"}}FINAL: 已观察"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Locate { .. })
    ));

    assert!(matches!(
        parse_tool_action_envelope_result(
            r#"ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"open_app","app_name":"Chrome"}}FINAL: Chrome 已打开"#
        ),
        Ok(ToolActionEnvelope::ToolCall {
            call: ToolCall::OpenApp { .. },
            ..
        })
    ));
}

#[test]
fn parse_structured_action_result_reports_errors() {
    let missing_prefix = parse_tool_action_envelope_result(r#"{"type":"final","answer":"完成"}"#)
        .expect_err("missing ACTION prefix should be structured error");
    assert_eq!(missing_prefix.code, "missing_action_prefix");

    let invalid_json = parse_tool_action_envelope_result(r#"ACTION: {"type":"final""#)
        .expect_err("bad ACTION json should be structured error");
    assert_eq!(invalid_json.code, "invalid_action_json");
    assert!(invalid_json.message.contains("ACTION payload is invalid"));

    let trailing_text =
        parse_tool_action_envelope_result(r#"ACTION: {"type":"final","answer":"完成"} extra"#)
            .expect_err("arbitrary trailing text should stay invalid");
    assert_eq!(trailing_text.code, "invalid_action_json");
    assert!(trailing_text.message.contains("trailing text"));
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
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"memory_recall","query":"会话锚点","limit":2}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::MemoryRecall { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"spawn_subagent","task":"review tests","agent_name":"reviewer","policy":"analyze","timeout_ms":300000}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::SpawnSubagent { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"open_app","app_name":"Chrome"}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::OpenApp { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"mouse","x":1,"y":2}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Mouse { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"keyboard","text":"abc","secret":false}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Keyboard { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"screenshot","target":"screen"}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Screenshot { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"locate","target":"screen"}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Locate { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"wait","millis":5}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Wait { .. })
    ));
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"human_suspend","reason":"needs user confirmation","prompt":"approve?"}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::HumanSuspend { .. })
    ));
}

#[test]
fn mouse_and_keyboard_are_governed_as_local_desktop_interactions() {
    let workspace = temp_workspace("desktop-governance-kind");

    for call in [
        ToolCall::Mouse { x: 10, y: 20 },
        ToolCall::Keyboard {
            text: "hello".to_string(),
            secret: false,
        },
    ] {
        let action = proposed_action_for_tool_call(&workspace, &call);
        assert_eq!(
            action.kind,
            chuang_agent::governance::ActionKind::LocalDesktopInteraction
        );
        assert!(action.target.starts_with("actuator::"));
    }
}

#[test]
fn parse_structured_action_accepts_interface_only_desktop_tools() {
    assert!(matches!(
        parse_tool_model_output(
            r#"ACTION: {"type":"tool_call","call":{"tool":"mouse","x":10,"y":20}}"#
        ),
        ToolModelOutput::ToolCall(ToolCall::Mouse { .. })
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
    assert!(instructions.contains("辅助工具：list_dir, open_app"));
    assert!(instructions.contains("apply_patch"));
    assert!(instructions.contains("memory_recall"));
    assert!(instructions.contains("code_execute 使用 Bash"));
    assert!(instructions.contains(r#""tool":"open_app""#));
    assert!(instructions.contains(r#""app_name":"Chrome""#));
    assert!(instructions.contains(r#""schema_version":1"#));
    assert!(instructions.contains(r#""tool":"file_read""#));
    assert!(instructions.contains("open_app/mouse/keyboard/screenshot/locate"));
    assert!(instructions.contains("browser_navigate + browser_read") || instructions.contains("browser_navigate+browser_read") || instructions.contains("browser_navigate"));
    assert!(instructions.contains("open_app / mouse / keyboard 是交互工具"));
    assert!(instructions.contains("桌面只读观察：screenshot, locate"));
    assert!(instructions.contains("每次回复只能输出一个结构"));
    assert!(instructions.contains("输出 tool_call 后必须等待工具结果"));
    assert!(instructions.contains("不要把 ACTION 和 FINAL 粘在同一次输出里"));
    assert!(instructions.contains("进入工具往返"));
    // Progressive disclosure: short catalog + full detail both present in combined block.
    assert!(instructions.contains("工具面（薄目录）") || instructions.contains("薄目录"));
}

#[test]
fn tool_surface_status_exposes_read_only_desktop_browser_tools() {
    let root = temp_workspace("surface-status");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let surface = ToolSurfaceStatus::generic_agent_mvp(&root);

    assert_eq!(
        surface.desktop_browser_read_only_atomic_tools,
        vec!["screenshot".to_string(), "locate".to_string()]
    );

    let surface_json = serde_json::to_value(&surface).expect("surface should serialize");
    assert_eq!(
        surface_json["desktop_browser_read_only_atomic_tools"],
        serde_json::json!(["screenshot", "locate"])
    );
    assert!(surface
        .callable_tools
        .iter()
        .any(|tool| tool == "spawn_subagent"));
    assert!(surface
        .callable_tools
        .iter()
        .any(|tool| tool == "browser_read"));
    assert!(surface
        .callable_tools
        .iter()
        .any(|tool| tool == "browser_navigate"));
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
    assert_eq!(
        ToolLoopReport::call_schema_fields(),
        &[
            "call",
            "tool_name",
            "atomic_tool_name",
            "ok",
            "summary",
            "decision",
            "duration_ms",
            "retryable",
            "target_path",
            "resolved_path",
            "cwd",
            "command",
            "entries",
            "output_bytes",
            "output_lines",
            "stderr_bytes",
            "stderr_lines",
            "output",
            "stdout",
            "stderr",
            "exit_code",
            "changed_files",
            "write_before_bytes",
            "write_after_bytes",
            "write_changed",
            "write_operation",
            "write_diff_preview",
            "write_diff_truncated",
            "failure_class",
            "output_redacted",
            "stdout_redacted",
            "stderr_redacted",
            "output_truncated",
            "stdout_truncated",
            "stderr_truncated",
        ]
    );
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
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"patch"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"command"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"cwd"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"app_name"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"query"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"session_id"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"limit"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"task"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"agent_name"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"policy"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"token_budget"));
    assert!(ToolActionEnvelope::call_schema_fields().contains(&"timeout_ms"));
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

    let modify = execute_tool_call(
        &root,
        &ToolCall::WriteFile {
            path: "nested/output.txt".to_string(),
            content: "world again".to_string(),
        },
    );
    assert!(modify.ok, "modify should succeed: {}", modify.summary);
    assert_eq!(modify.write_before_bytes, Some(5));
    assert_eq!(modify.write_after_bytes, Some(11));
    assert_eq!(modify.write_changed, Some(true));
    assert_eq!(modify.write_operation, Some(WriteOperation::Modified));
    assert!(modify
        .write_diff_preview
        .as_deref()
        .is_some_and(|preview| preview.contains("-world") && preview.contains("+world again")));

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
    assert!(read.summary.contains("world again"));
    assert_eq!(read.output.as_deref(), Some("world again"));
    assert_eq!(read.target_path.as_deref(), Some("nested/output.txt"));
    assert_eq!(read.output_bytes, Some(11));
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
fn shell_exec_supports_bash_pipefail_commands() {
    let root = temp_workspace("bash-pipefail");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let result = execute_tool_call(
        &root,
        &ToolCall::ShellExec {
            command: "set -o pipefail; printf bash-ok | cat".to_string(),
            cwd: Some(".".to_string()),
        },
    );

    assert!(result.ok, "bash command should succeed: {}", result.summary);
    assert_eq!(result.stdout.as_deref(), Some("bash-ok"));
    assert_eq!(result.exit_code, Some(0));
}

#[test]
fn workspace_file_adapter_can_apply_patch_and_enforce_workspace_bounds() {
    let root = temp_workspace("patch");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("keep.txt"), "old\nvalue\n").expect("seed file should write");

    let adapter = WorkspaceFileAdapter::new(&root);
    let result = adapter
        .apply_patch("*** Begin Patch\n*** Update File: keep.txt\n@@\n-old\n+new\n*** End Patch")
        .expect("patch should apply");
    assert_eq!(result.operation_count, 1);
    assert_eq!(
        result.changed_files,
        vec![root.join("keep.txt").display().to_string()]
    );
    assert!(result.diff_preview.contains("keep.txt"));
    assert!(!result.backup_paths.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("keep.txt")).unwrap(),
        "new\nvalue\n"
    );

    let escape = adapter
        .list_dir("../")
        .expect_err("outside workspace should be rejected");
    assert!(escape.contains("path_outside_workspace"));
}

#[test]
fn workspace_file_adapter_write_file_creates_auditable_backup() {
    let root = temp_workspace("write-backup");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("note.txt"), "before\n").expect("seed file should write");

    let adapter = WorkspaceFileAdapter::new(&root);
    let result = adapter
        .write_file("note.txt", "after\n")
        .expect("write should succeed");

    assert_eq!(
        result.operation,
        chuang_agent::workspace_file_adapter::WorkspaceWriteOperation::Modified
    );
    assert_eq!(result.before_bytes, Some("before\n".len()));
    assert_eq!(result.after_bytes, "after\n".len());
    assert!(result.changed);
    assert_eq!(result.backup_paths.len(), 1);
    assert!(result.backup_paths[0].contains(".chuang-file-audit"));
    assert_eq!(
        fs::read_to_string(&result.backup_paths[0]).expect("backup should be readable"),
        "before\n"
    );
    assert!(result.diff_preview.contains("-before"));
    assert!(result.diff_preview.contains("+after"));
}

#[test]
fn workspace_file_adapter_rejects_patch_delete_without_removing_file() {
    let root = temp_workspace("patch-delete-rejected");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("keep.txt"), "keep\n").expect("seed file should write");

    let adapter = WorkspaceFileAdapter::new(&root);
    let error = adapter
        .apply_patch("*** Begin Patch\n*** Delete File: keep.txt\n*** End Patch")
        .expect_err("delete patch should be rejected");

    assert!(error.contains("apply_patch_delete_not_allowed"));
    assert_eq!(
        fs::read_to_string(root.join("keep.txt")).expect("file should remain"),
        "keep\n"
    );
}

#[test]
fn workspace_file_adapter_rejects_patch_without_partial_writes() {
    let root = temp_workspace("patch-no-partial-writes");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("keep.txt"), "keep\n").expect("seed file should write");

    let adapter = WorkspaceFileAdapter::new(&root);
    let error = adapter
        .apply_patch(
            "*** Begin Patch\n*** Add File: created.txt\n+created\n*** Delete File: keep.txt\n*** End Patch",
        )
        .expect_err("mixed patch should be rejected before writing");

    assert!(error.contains("apply_patch_delete_not_allowed"));
    assert!(
        !root.join("created.txt").exists(),
        "earlier add operation should not be committed"
    );
    assert_eq!(
        fs::read_to_string(root.join("keep.txt")).expect("existing file should remain"),
        "keep\n"
    );
}

#[test]
fn workspace_file_adapter_rejects_patch_move_without_removing_source() {
    let root = temp_workspace("patch-move-rejected");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("source.txt"), "old\n").expect("seed file should write");

    let adapter = WorkspaceFileAdapter::new(&root);
    let error = adapter
        .apply_patch(
            "*** Begin Patch\n*** Update File: source.txt\n*** Move to: moved.txt\n@@\n-old\n+new\n*** End Patch",
        )
        .expect_err("move patch should be rejected");

    assert!(error.contains("apply_patch_move_not_allowed"));
    assert_eq!(
        fs::read_to_string(root.join("source.txt")).expect("source should remain"),
        "old\n"
    );
    assert!(
        !root.join("moved.txt").exists(),
        "move target should not be created"
    );
}

#[cfg(unix)]
#[test]
fn workspace_file_adapter_rejects_writes_through_symlink_parent() {
    let root = temp_workspace("symlink-parent-escape");
    let outside = temp_workspace("symlink-parent-outside");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    std::os::unix::fs::symlink(&outside, root.join("linked")).expect("symlink should be created");

    let adapter = WorkspaceFileAdapter::new(&root);
    let error = adapter
        .write_file("linked/created.txt", "outside\n")
        .expect_err("symlink parent should not escape workspace");

    assert!(error.contains("path_outside_workspace"));
    assert!(
        !outside.join("created.txt").exists(),
        "outside file should not be created"
    );
}

#[cfg(unix)]
#[test]
fn workspace_file_adapter_rejects_patch_add_through_symlink_parent() {
    let root = temp_workspace("patch-symlink-parent-escape");
    let outside = temp_workspace("patch-symlink-parent-outside");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    std::os::unix::fs::symlink(&outside, root.join("linked")).expect("symlink should be created");

    let adapter = WorkspaceFileAdapter::new(&root);
    let error = adapter
        .apply_patch("*** Begin Patch\n*** Add File: linked/created.txt\n+outside\n*** End Patch")
        .expect_err("patch add through symlink parent should not escape workspace");

    assert!(error.contains("path_outside_workspace"));
    assert!(
        !outside.join("created.txt").exists(),
        "outside file should not be created"
    );
}

#[test]
fn workspace_file_adapter_read_file_redacts_secret_like_content() {
    let root = temp_workspace("read-redaction");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::write(root.join("config.txt"), "api_key = \"secret-value\"\n")
        .expect("seed file should write");

    let adapter = WorkspaceFileAdapter::new(&root);
    let result = adapter
        .read_file("config.txt")
        .expect("read should succeed");

    assert!(result.redacted);
    assert_eq!(result.content, "api_key = \"[REDACTED]\"\n");
    assert_eq!(result.bytes, "api_key = \"secret-value\"\n".len());
    assert_eq!(result.lines, 1);
}

#[test]
fn tool_runtime_can_execute_desktop_atomic_tools_with_fake_actuator() {
    let root = temp_workspace("desktop-tools");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let config = ToolExecutionConfig {
        actuator: Some(chuang_agent::runtime_config::ActuatorConfig::Fake),
        ..ToolExecutionConfig::default()
    };

    let mouse = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::Mouse { x: 10, y: 20 },
        &config,
    );
    assert!(mouse.ok, "mouse should succeed: {}", mouse.summary);

    let keyboard = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::Keyboard {
            text: "hello".to_string(),
            secret: false,
        },
        &config,
    );
    assert!(keyboard.ok, "keyboard should succeed: {}", keyboard.summary);

    let open_app = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::OpenApp {
            app_name: "Chrome".to_string(),
        },
        &config,
    );
    assert!(open_app.ok, "open_app should succeed: {}", open_app.summary);
    assert_eq!(open_app.atomic_tool_name, None);
    assert_eq!(open_app.tool_name, "open_app");
    assert!(open_app
        .output
        .as_deref()
        .expect("open_app output should include handle")
        .contains("app_name=Chrome"));

    let screenshot = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::Screenshot {
            target: Some("screen".to_string()),
        },
        &config,
    );
    assert!(
        screenshot.ok,
        "screenshot should succeed: {}",
        screenshot.summary
    );
    let screenshot_output: serde_json::Value = serde_json::from_str(
        screenshot
            .output
            .as_deref()
            .expect("screenshot should return structured evidence output"),
    )
    .expect("screenshot output should be json");
    assert_eq!(screenshot_output["evidence_uri"], "fake://screenshot");
    assert_eq!(
        screenshot_output["audit_message"],
        "fake actuator screenshot"
    );

    let locate = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::Locate {
            target: Some("screen".to_string()),
        },
        &config,
    );
    assert!(locate.ok, "locate should succeed: {}", locate.summary);
    let locate_output: serde_json::Value = serde_json::from_str(
        locate
            .output
            .as_deref()
            .expect("locate should return structured evidence output"),
    )
    .expect("locate output should be json");
    assert_eq!(locate_output["summary"], "fake observation");
    assert_eq!(locate_output["evidence_uri"], "fake://observation");
    assert_eq!(locate_output["audit_message"], "fake actuator observation");

    let wait = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::Wait { millis: 1 },
        &config,
    );
    assert!(wait.ok, "wait should succeed: {}", wait.summary);

    let human_suspend = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::HumanSuspend {
            reason: "uncertain desktop state".to_string(),
            prompt: Some("confirm next action".to_string()),
        },
        &config,
    );
    assert!(!human_suspend.ok);
    assert_eq!(
        human_suspend.failure_class.as_deref(),
        Some("human_input_required")
    );
    assert_eq!(
        human_suspend.atomic_tool_name.as_deref(),
        Some("human_suspend")
    );
    assert!(human_suspend
        .output
        .as_deref()
        .expect("human suspend should explain prompt")
        .contains("confirm next action"));
}

#[test]
fn governed_secret_keyboard_input_requires_approval() {
    let root = temp_workspace("governed-secret-keyboard");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();

    let error = execute_tool_call_with_governance(
        &root,
        &mut governance,
        &ToolCall::Keyboard {
            text: "123456".to_string(),
            secret: true,
        },
        "cli",
        "turn-1:tool-1",
    )
    .expect_err("secret keyboard input should require approval");

    assert!(error.starts_with("tool_needs_approval:"));
}

#[test]
fn memory_recall_returns_structured_unconfigured_result() {
    let root = temp_workspace("memory-unconfigured");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let record = execute_tool_call(
        &root,
        &ToolCall::MemoryRecall {
            query: "会话锚点".to_string(),
            session_id: None,
            limit: Some(3),
        },
    );

    assert!(!record.ok);
    assert_eq!(record.tool_name, "memory_recall");
    assert_eq!(record.atomic_tool_name, None);
    assert_eq!(
        record.failure_class.as_deref(),
        Some("memory_recall_unconfigured")
    );
    assert!(!record.retryable);
}

#[test]
fn memory_recall_searches_only_configured_session_memory() {
    use chuang_agent::memory_store::MemoryStore;

    let root = temp_workspace("memory-session");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let db_path = root.join("memory.db");
    let mut store =
        chuang_agent::memory_store_sqlite::SqliteMemoryStore::open(&db_path).expect("db opens");
    store
        .put(chuang_agent::memory_store::MemoryRecord {
            id: "alpha-hit".to_string(),
            content: "会话锚点A 要在 alpha 中召回".to_string(),
            metadata: std::collections::BTreeMap::from([
                ("memory_scope".to_string(), "session".to_string()),
                ("session_id".to_string(), "alpha".to_string()),
            ]),
            created_at: "2026-05-04T00:00:00Z".to_string(),
            expires_at: None,
        })
        .expect("alpha memory writes");
    store
        .put(chuang_agent::memory_store::MemoryRecord {
            id: "beta-hit".to_string(),
            content: "会话锚点A 不应跨会话召回".to_string(),
            metadata: std::collections::BTreeMap::from([
                ("memory_scope".to_string(), "session".to_string()),
                ("session_id".to_string(), "beta".to_string()),
            ]),
            created_at: "2026-05-04T00:00:01Z".to_string(),
            expires_at: None,
        })
        .expect("beta memory writes");

    let record = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::MemoryRecall {
            query: "会话锚点A".to_string(),
            session_id: Some("alpha".to_string()),
            limit: Some(5),
        },
        &ToolExecutionConfig {
            memory: Some(MemoryToolContext {
                db_path,
                session_id: Some("alpha".to_string()),
                default_limit: 3,
                max_limit: 5,
            }),
            actuator: None,
            ..ToolExecutionConfig::default()
        },
    );

    assert!(
        record.ok,
        "memory recall should succeed: {}",
        record.summary
    );
    assert_eq!(record.failure_class, None);
    let output = record.output.as_deref().expect("output json should exist");
    assert!(output.contains(r#""hit_count":1"#));
    assert!(output.contains("alpha-hit"));
    assert!(!output.contains("beta-hit"));
}

#[test]
fn memory_recall_rejects_cross_session_request() {
    let root = temp_workspace("memory-session-mismatch");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let record = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::MemoryRecall {
            query: "会话锚点".to_string(),
            session_id: Some("beta".to_string()),
            limit: None,
        },
        &ToolExecutionConfig {
            memory: Some(MemoryToolContext {
                db_path: root.join("memory.db"),
                session_id: Some("alpha".to_string()),
                default_limit: 3,
                max_limit: 5,
            }),
            actuator: None,
            ..ToolExecutionConfig::default()
        },
    );

    assert!(!record.ok);
    assert_eq!(
        record.failure_class.as_deref(),
        Some("memory_recall_session_mismatch")
    );
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
        Some("--- before\n+++ after\n+API_KEY=[REDACTED]\n")
    );
    assert_eq!(record.write_operation, Some(WriteOperation::Created));
    assert!(record.output_redacted);
    assert!(!record.write_diff_truncated);
}

#[cfg(unix)]
#[test]
fn tool_runtime_rejects_symlink_parent_writes() {
    let root = temp_workspace("runtime-symlink-parent-escape");
    let outside = temp_workspace("runtime-symlink-parent-outside");
    fs::create_dir_all(&root).expect("workspace root should be created");
    fs::create_dir_all(&outside).expect("outside dir should be created");
    std::os::unix::fs::symlink(&outside, root.join("linked")).expect("symlink should be created");

    let record = execute_tool_call(
        &root,
        &ToolCall::WriteFile {
            path: "linked/created.txt".to_string(),
            content: "outside".to_string(),
        },
    );

    assert!(!record.ok);
    assert!(record.summary.contains("path_outside_workspace"));
    assert!(
        !outside.join("created.txt").exists(),
        "outside file should not be created"
    );
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
    assert_eq!(record.output.as_deref(), Some("API_KEY=[REDACTED]"));
    assert!(record.output_redacted);
    assert_eq!(record.output_bytes, Some("API_KEY=secret-value".len()));
    assert_eq!(record.output_lines, Some(1));
}

#[test]
fn shell_exec_redacts_secret_like_stdout_and_stderr() {
    let root = temp_workspace("shell-redact");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let record = execute_tool_call(
        &root,
        &ToolCall::ShellExec {
            command: "printf 'API_KEY=secret-value'; printf 'password=hidden' >&2".to_string(),
            cwd: Some(".".to_string()),
        },
    );

    assert!(record.ok);
    assert_eq!(record.stdout.as_deref(), Some("API_KEY=[REDACTED]"));
    assert_eq!(record.stderr.as_deref(), Some("password=[REDACTED]"));
    assert!(record.output_redacted);
    assert!(record.stdout_redacted);
    assert!(record.stderr_redacted);
    assert_eq!(record.output_bytes, Some("API_KEY=secret-value".len()));
    assert_eq!(record.stderr_bytes, Some("password=hidden".len()));
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
        actuator: None,
        ..ToolExecutionConfig::default()
    });

    assert_eq!(
        slot.registry().mapped_atomic_names(),
        vec![
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "file_read",
            "file_write",
            "code_execute",
            "wait",
            "human_suspend",
        ]
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
fn execution_slot_records_runtime_ledger_events_for_tool_calls() {
    let root = temp_workspace("execution-slot-ledger");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());

    let outcome = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-1",
            "turn-1",
            &root,
            &mut governance,
            &ToolCall::WriteFile {
                path: "notes/out.txt".to_string(),
                content: "ledger".to_string(),
            },
            "cli",
            "turn-1:tool-1",
        )
        .expect("governed execution should succeed");

    assert!(outcome.record.ok);
    let events = ledger
        .query_by_turn("thread-1", "turn-1")
        .expect("ledger should list turn events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, RuntimeEventKind::ToolStarted);
    assert_eq!(events[1].event_type, RuntimeEventKind::ToolFinished);
    assert!(events[1]
        .risk_decision
        .as_ref()
        .is_some_and(|decision| decision.decision.starts_with("allowed")));
    assert!(events.iter().all(|event| event
        .evidence_ref
        .as_deref()
        .is_some_and(|value| value.starts_with("tool://"))));

    let call_id = "tool:write_file:cli:turn-1:tool-1";
    let by_call = ledger
        .query_by_call(call_id)
        .expect("ledger should query by call id");
    assert_eq!(by_call, events);
}

#[test]
fn execution_slot_records_runtime_ledger_events_for_rejected_tool_calls() {
    let root = temp_workspace("execution-slot-ledger-rejected");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());

    let outcome = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-2",
            "turn-2",
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-2:tool-1",
        )
        .expect("governance rejection should be captured as a tool record");

    assert!(!outcome.record.ok);
    assert_eq!(
        outcome.record.failure_class.as_deref(),
        Some("approval_pending")
    );
    assert!(outcome.pending_approval.is_some());
    let events = ledger
        .query_by_turn("thread-2", "turn-2")
        .expect("ledger should list rejected turn events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, RuntimeEventKind::ToolStarted);
    assert_eq!(events[1].event_type, RuntimeEventKind::ApprovalRequested);
    assert!(events[1]
        .risk_decision
        .as_ref()
        .is_some_and(|decision| decision.decision.starts_with("needs_approval")));
    assert!(events[0]
        .evidence_ref
        .as_deref()
        .is_some_and(|value| value.starts_with("tool://")));
    assert!(events[1]
        .evidence_ref
        .as_deref()
        .is_some_and(|value| value.starts_with("approval://")));
    assert_eq!(
        events[0].call_id.as_deref(),
        Some("tool:code_execute:cli:turn-2:tool-1")
    );
    assert_eq!(
        events[1].call_id.as_deref(),
        Some("tool:code_execute:cli:turn-2:tool-1")
    );
}

#[test]
fn governed_ledger_wrapper_returns_pending_without_tool_finished() {
    let root = temp_workspace("governed-ledger-pending");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();

    let outcome = execute_tool_call_with_governance_and_ledger(
        &mut ledger,
        "thread-wrapper",
        "turn-wrapper",
        &root,
        &mut governance,
        &ToolCall::ShellExec {
            command: "rm -rf notes".to_string(),
            cwd: Some(".".to_string()),
        },
        "cli",
        "turn-wrapper:tool-1",
        &ToolExecutionConfig::default(),
    )
    .expect("needs approval should return pending state");

    assert!(outcome.pending_approval.is_some());
    let events = ledger
        .query_by_turn("thread-wrapper", "turn-wrapper")
        .expect("events should list");
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_type.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeEventKind::ToolStarted,
            RuntimeEventKind::ApprovalRequested,
        ]
    );
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
fn tool_runtime_requires_approval_for_real_secret_shell_access() {
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
    .expect_err("real secret material access should require approval");

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
fn tool_runtime_allows_workspace_keyword_scans_without_secret_false_positive() {
    let root = temp_workspace("governed-secret-keyword-scan");
    fs::create_dir_all(root.join("src")).expect("workspace root should be created");
    fs::write(
        root.join("src/rules.rs"),
        "const MESSAGE: &str = \"secret-bearing action\";\n",
    )
    .expect("fixture should write");
    let mut governance = StaticRuleGovernance::new();

    let outcome = execute_tool_call_with_governance(
        &root,
        &mut governance,
        &ToolCall::ShellExec {
            command: "rg -n 'secret-bearing action|draft_only|token|password' src".to_string(),
            cwd: Some(".".to_string()),
        },
        "app-server",
        "turn-1:tool-1",
    )
    .expect("source keyword scans should execute in the workspace");

    assert!(matches!(outcome.decision, RiskDecision::Allowed { .. }));
    assert!(outcome.record.ok);
    assert!(outcome
        .record
        .stdout
        .as_deref()
        .is_some_and(|value| value.contains("secret-bearing action")));
}

#[test]
fn execution_slot_uses_configured_shell_risk_rules() {
    let root = temp_workspace("shell-risk-rules");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig {
        shell_timeout_ms: 30_000,
        shell_rtk_rewrite: true,
        shell_risk_rules: ShellRiskRules {
            network_change: vec![" make deploy".to_string()],
            ..ShellRiskRules::default()
        },
        memory: None,
        actuator: None,
        subagent: None,
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
fn spawn_subagent_tool_runs_existing_dispatch_runner_collect_chain() {
    let root = temp_workspace("spawn-subagent");
    fs::create_dir_all(root.join("scripts")).expect("scripts dir should be created");
    fs::write(root.join("config.toml"), "subagent = \"queued_external\"\n")
        .expect("config should write");
    let executable = root.join("fake-chuang");
    let runner = root.join("scripts/fake-runner");
    fs::write(
        &executable,
        r#"#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
queue = args[args.index("--subagent-queue-root") + 1]
os.makedirs(queue, exist_ok=True)
if args[:2] == ["subagent", "dispatch"]:
    json.dump({"run_id":"run-1","agent_id":"worker-1"}, sys.stdout)
elif args[:2] == ["subagent", "run-loop"]:
    json.dump({"ran_count":1}, sys.stdout)
elif args[:2] == ["subagent", "collect"]:
    json.dump({
        "report_admission":{"status":"Accepted"},
        "report":{"status":"Success","summary":"worker completed","stdout_preview":"verified"}
    }, sys.stdout)
else:
    raise SystemExit(2)
"#,
    )
    .expect("fake executable should write");
    fs::write(&runner, "#!/bin/sh\nexit 0\n").expect("runner should write");
    let mut permissions = fs::metadata(&executable)
        .expect("fake executable metadata")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake executable should chmod");
        let mut runner_permissions = fs::metadata(&runner)
            .expect("runner metadata")
            .permissions();
        runner_permissions.set_mode(0o755);
        fs::set_permissions(&runner, runner_permissions).expect("runner should chmod");
    }

    let record = chuang_agent::tool_runtime::execute_tool_call_with_config(
        &root,
        &ToolCall::SpawnSubagent {
            task: "inspect the workspace".to_string(),
            tasks: None,
            agent_name: Some("reviewer".to_string()),
            policy: Some("analyze".to_string()),
            token_budget: Some(1024),
            timeout_ms: Some(60_000),
            max_concurrency: None,
        },
        &ToolExecutionConfig {
            subagent: Some(chuang_agent::tool_runtime::SubagentToolContext {
                executable_path: executable,
                config_path: root.join("config.toml"),
                queue_root: root.join("queue"),
                runner_command: runner,
                worker_model: "gpt-5.6-luna".to_string(),
                worker_capability: "workspace".to_string(),
            }),
            ..ToolExecutionConfig::default()
        },
    );

    assert!(record.ok, "spawn should succeed: {}", record.summary);
    assert_eq!(record.tool_name, "spawn_subagent");
    assert!(
        record.summary.contains("admission=accepted")
            || record.summary.contains("subagent_batch_completed"),
        "summary={}",
        record.summary
    );
    let output = record.output.expect("subagent output should exist");
    assert!(
        output.contains("\"result_preview\":\"verified\"")
            || output.contains("verified")
            || output.contains("\"ok\":true"),
        "output={output}"
    );
    assert!(output.contains("\"worker_model\":\"gpt-5.6-luna\"") || output.contains("gpt-5.6-luna"));
}

#[test]
fn parse_spawn_subagent_accepts_parallel_tasks_array() {
    let output = parse_tool_model_output(
        r#"ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"spawn_subagent","tasks":["a","b"],"policy":"analyze","max_concurrency":2}}"#,
    );
    match output {
        ToolModelOutput::ToolCall(ToolCall::SpawnSubagent {
            tasks,
            max_concurrency,
            task,
            ..
        }) => {
            assert!(task.is_empty());
            assert_eq!(tasks.as_ref().map(|t| t.len()), Some(2));
            assert_eq!(max_concurrency, Some(2));
        }
        other => panic!("unexpected {other:?}"),
    }
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

    assert!(matches!(
        outcome.decision,
        RiskDecision::NeedsApproval { .. }
    ));
    assert!(!outcome.record.ok);
    assert_eq!(
        outcome.record.failure_class.as_deref(),
        Some("approval_pending")
    );
    assert!(outcome.record.retryable);
    assert!(outcome.pending_approval.is_some());
    assert!(outcome
        .record
        .decision
        .as_deref()
        .is_some_and(|decision| decision.starts_with("needs_approval:")));
    assert_eq!(
        governance.audit_records()[0].operation,
        "tool.code_execute.rejected"
    );
}

#[test]
fn execution_slot_can_return_needs_approval_rejection_as_record() {
    let root = temp_workspace("governance-needs-approval-record");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());

    let outcome = slot
        .execute_or_reject_with_governance(
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-1:tool-1",
        )
        .expect("needs approval rejection should still become a tool record");

    assert!(matches!(
        outcome.decision,
        RiskDecision::NeedsApproval { .. }
    ));
    assert!(!outcome.record.ok);
    assert_eq!(
        outcome.record.failure_class.as_deref(),
        Some("approval_pending")
    );
    assert!(outcome.record.retryable);
    assert!(outcome.pending_approval.is_some());
    assert!(outcome
        .record
        .decision
        .as_deref()
        .is_some_and(|decision| decision.starts_with("needs_approval:")));
    assert_eq!(
        outcome.record.atomic_tool_name.as_deref(),
        Some("code_execute")
    );
    assert_eq!(
        governance.audit_records()[0].operation,
        "tool.code_execute.rejected"
    );
}

#[test]
fn execution_slot_resumes_exact_pending_call_with_matching_operator_receipt() {
    let root = temp_workspace("approval-resume");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let call = ToolCall::ShellExec {
        command: "printf 'marker\\n' >> approved.txt; # rm -rf notes".to_string(),
        cwd: Some(".".to_string()),
    };
    let outcome = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-resume",
            "turn-resume",
            &root,
            &mut governance,
            &call,
            "cli",
            "turn-resume:tool-1",
        )
        .expect("approval request should return pending state");
    assert!(!root.join("approved.txt").exists());
    let pending = outcome
        .pending_approval
        .expect("pending approval should exist");
    assert_eq!(
        serde_json::from_str::<ToolCall>(&pending.serialized_tool_call)
            .expect("stored call should deserialize"),
        call
    );
    let receipt = register_operator_approval(&mut governance, &pending);
    let duplicate_pending = pending.clone();
    let approval_id = pending.approval_id.clone();

    let resumed = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-resume",
            "turn-resume",
            &root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect("matching approval should resume exact call");

    assert!(resumed.record.ok);
    assert!(resumed.pending_approval.is_none());
    assert!(duplicate_pending.call_fingerprint.starts_with("sha256:"));
    assert!(duplicate_pending.target_fingerprint.starts_with("sha256:"));
    assert!(duplicate_pending
        .workspace_fingerprint
        .starts_with("sha256:"));
    assert!(duplicate_pending.policy_marker.starts_with("sha256:"));
    assert_eq!(
        fs::read_to_string(root.join("approved.txt")).expect("approved call should execute"),
        "marker\n"
    );
    let restarted_slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let duplicate_error = restarted_slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-resume",
            "turn-resume",
            &root,
            &mut governance,
            duplicate_pending,
            &receipt,
        )
        .expect_err("the same approval must execute only once across slot restarts");
    assert_eq!(duplicate_error, "approval_already_consumed");
    assert_eq!(
        fs::read_to_string(root.join("approved.txt")).expect("approved output should remain"),
        "marker\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let marker = root
            .join(".chuang/runtime/consumed-approvals")
            .join(format!("{approval_id}.json"));
        assert_eq!(
            fs::metadata(marker)
                .expect("consumption marker should exist")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let events = ledger
        .query_by_turn("thread-resume", "turn-resume")
        .expect("events should list");
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_type.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeEventKind::ToolStarted,
            RuntimeEventKind::ApprovalRequested,
            RuntimeEventKind::ApprovalResolved,
            RuntimeEventKind::ToolFinished,
        ]
    );
}

#[test]
fn execution_slot_rejects_mismatched_approval_receipt_without_execution() {
    let root = temp_workspace("approval-mismatch");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let pending = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-mismatch",
            "turn-mismatch",
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "printf marker > must-not-exist.txt; # rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-mismatch:tool-1",
        )
        .expect("approval request should return pending state")
        .pending_approval
        .expect("pending approval should exist");
    let receipt = OperatorApprovalReceipt {
        approval_id: pending.approval_id.clone(),
        call_id: pending.call_id.clone(),
        call_fingerprint: pending.call_fingerprint.clone(),
        target_fingerprint: pending.target_fingerprint.clone(),
        workspace_fingerprint: pending.workspace_fingerprint.clone(),
        policy_marker: "tampered-policy".to_string(),
        approved: true,
        operator_ref: "operator:test".to_string(),
        evidence_ref: "operator-evidence://tampered".to_string(),
    };

    let error = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-mismatch",
            "turn-mismatch",
            &root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect_err("mismatched receipt must not resume");

    assert_eq!(error, "operator_approval_receipt_mismatch");
    assert!(!root.join("must-not-exist.txt").exists());
    assert!(!ledger
        .query_by_turn("thread-mismatch", "turn-mismatch")
        .expect("events should list")
        .iter()
        .any(|event| event.event_type == RuntimeEventKind::ApprovalResolved));
}

#[test]
fn execution_slot_rejects_pending_approval_copied_to_another_workspace() {
    let original_root = temp_workspace("approval-original-workspace");
    let other_root = temp_workspace("approval-other-workspace");
    fs::create_dir_all(&original_root).expect("original workspace should be created");
    fs::create_dir_all(&other_root).expect("other workspace should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let pending = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-workspace",
            "turn-workspace",
            &original_root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "printf marker > must-not-exist.txt; # rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-workspace:tool-1",
        )
        .expect("approval request should return pending state")
        .pending_approval
        .expect("pending approval should exist");
    let receipt = register_operator_approval(&mut governance, &pending);

    let error = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-workspace",
            "turn-workspace",
            &other_root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect_err("approval must stay bound to its original workspace");

    assert_eq!(error, "approval_pending_revalidation_failed");
    assert!(!other_root.join("must-not-exist.txt").exists());
}

#[test]
fn execution_slot_rejects_tampered_pending_approval_id() {
    let root = temp_workspace("approval-tampered-id");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let mut pending = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-tampered-id",
            "turn-tampered-id",
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "printf marker > must-not-exist.txt; # rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-tampered-id:tool-1",
        )
        .expect("approval request should return pending state")
        .pending_approval
        .expect("pending approval should exist");
    pending.approval_id = "approval-tampered".to_string();
    let receipt = register_operator_approval(&mut governance, &pending);

    let error = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-tampered-id",
            "turn-tampered-id",
            &root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect_err("tampered approval id must not pass revalidation");

    assert_eq!(error, "approval_pending_revalidation_failed");
    assert!(!root.join("must-not-exist.txt").exists());
}

#[test]
fn execution_slot_rejects_matching_but_unregistered_operator_receipt() {
    let root = temp_workspace("approval-unregistered");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let pending = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-unregistered",
            "turn-unregistered",
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "printf marker > must-not-exist.txt; # rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-unregistered:tool-1",
        )
        .expect("approval request should return pending state")
        .pending_approval
        .expect("pending approval should exist");
    let receipt = OperatorApprovalReceipt {
        approval_id: pending.approval_id.clone(),
        call_id: pending.call_id.clone(),
        call_fingerprint: pending.call_fingerprint.clone(),
        target_fingerprint: pending.target_fingerprint.clone(),
        workspace_fingerprint: pending.workspace_fingerprint.clone(),
        policy_marker: pending.policy_marker.clone(),
        approved: true,
        operator_ref: "operator:forged".to_string(),
        evidence_ref: "operator-evidence://forged".to_string(),
    };

    let error = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-unregistered",
            "turn-unregistered",
            &root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect_err("matching caller-authored fields must not self-authorize");

    assert_eq!(error, "operator_approval_not_authorized");
    assert!(!root.join("must-not-exist.txt").exists());
}

#[test]
fn execution_slot_rejects_empty_operator_ref_without_execution() {
    let root = temp_workspace("approval-empty-operator");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let pending = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-empty-operator",
            "turn-empty-operator",
            &root,
            &mut governance,
            &ToolCall::ShellExec {
                command: "printf marker > must-not-exist.txt; # rm -rf notes".to_string(),
                cwd: Some(".".to_string()),
            },
            "cli",
            "turn-empty-operator:tool-1",
        )
        .expect("approval request should return pending state")
        .pending_approval
        .expect("pending approval should exist");
    let receipt = OperatorApprovalReceipt {
        approval_id: pending.approval_id.clone(),
        call_id: pending.call_id.clone(),
        call_fingerprint: pending.call_fingerprint.clone(),
        target_fingerprint: pending.target_fingerprint.clone(),
        workspace_fingerprint: pending.workspace_fingerprint.clone(),
        policy_marker: pending.policy_marker.clone(),
        approved: true,
        operator_ref: " ".to_string(),
        evidence_ref: "operator-evidence://empty-operator".to_string(),
    };

    let error = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-empty-operator",
            "turn-empty-operator",
            &root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect_err("empty operator ref must be rejected");

    assert_eq!(error, "operator_approval_operator_ref_required");
    assert!(!root.join("must-not-exist.txt").exists());
}

#[test]
fn failed_approved_call_is_consumed_before_partial_side_effect_can_be_replayed() {
    let root = temp_workspace("approval-failed-consumed");
    fs::create_dir_all(&root).expect("workspace root should be created");
    let mut governance = StaticRuleGovernance::new();
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let call = ToolCall::ShellExec {
        command: "printf 'partial\\n' >> partial.txt; exit 7; # rm -rf notes".to_string(),
        cwd: Some(".".to_string()),
    };
    let pending = slot
        .execute_or_reject_with_governance_and_ledger(
            &mut ledger,
            "thread-partial",
            "turn-partial",
            &root,
            &mut governance,
            &call,
            "cli",
            "turn-partial:tool-1",
        )
        .expect("approval request should return pending state")
        .pending_approval
        .expect("pending approval should exist");
    let receipt = register_operator_approval(&mut governance, &pending);
    let duplicate_pending = pending.clone();
    let pending_call_id = pending.call_id.clone();

    let first = slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-partial",
            "turn-partial",
            &root,
            &mut governance,
            pending,
            &receipt,
        )
        .expect("failed command should return its tool record");
    assert!(!first.record.ok);
    assert_eq!(
        fs::read_to_string(root.join("partial.txt")).expect("partial effect should exist"),
        "partial\n"
    );

    let restarted_slot = ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default());
    let retry_error = restarted_slot
        .resume_approved_with_ledger(
            &mut ledger,
            "thread-partial",
            "turn-partial",
            &root,
            &mut governance,
            duplicate_pending,
            &receipt,
        )
        .expect_err("failed approved execution must not replay after restart");
    assert_eq!(retry_error, "approval_already_consumed");
    assert_eq!(
        fs::read_to_string(root.join("partial.txt")).expect("partial effect should not repeat"),
        "partial\n"
    );
    let events = ledger
        .query_by_turn("thread-partial", "turn-partial")
        .expect("events should list");
    let resolved = events
        .iter()
        .find(|event| event.event_type == RuntimeEventKind::ApprovalResolved)
        .expect("failed approved execution should still have a terminal approval event");
    assert!(resolved
        .risk_decision
        .as_ref()
        .is_some_and(|decision| decision.reason.contains("tool execution failed")));
    assert!(resolved
        .evidence_ref
        .as_deref()
        .is_some_and(|reference| reference.ends_with("/failed")));
    let finished = events
        .iter()
        .filter(|event| event.event_type == RuntimeEventKind::ToolFinished)
        .collect::<Vec<_>>();
    assert_eq!(finished.len(), 1);
    assert_eq!(
        finished[0].call_id.as_deref(),
        Some(pending_call_id.as_str())
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

#[test]
fn browser_navigate_and_read_tools_work_when_cdp_live() {
    std::env::set_var("CHUANG_CDP_PORT", "9222");
    let root = temp_workspace("browser-tools");
    std::fs::create_dir_all(&root).expect("root");
    let nav = execute_tool_call(
        &root,
        &ToolCall::BrowserNavigate {
            url: "https://example.com".to_string(),
        },
    );
    if !nav.ok {
        // Skip hard fail if chrome not up in CI/sandbox.
        eprintln!("browser_navigate skipped/failed: {}", nav.summary);
        return;
    }
    assert_eq!(nav.tool_name, "browser_navigate");
    assert!(nav.output.as_deref().unwrap_or("").contains("example"), "{}", nav.summary);
    let read = execute_tool_call(&root, &ToolCall::BrowserRead {});
    assert!(read.ok, "{}", read.summary);
    assert_eq!(read.tool_name, "browser_read");
}
