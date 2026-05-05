use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::atomic_tool::AtomicToolRegistry;
use crate::common::{AgentId, AuditRecord, TaskId, Timestamp};
use crate::governance::{
    risk_decision_label, risk_decision_reason, ActionKind, Governance, ProposedAction, RiskDecision,
};
use crate::memory_recall::{MemoryRecallPipeline, RecallRequest};
use crate::memory_store_sqlite::SqliteMemoryStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolCall {
    ListDir {
        path: String,
    },
    #[serde(alias = "file_read")]
    ReadFile {
        path: String,
    },
    #[serde(alias = "file_write")]
    WriteFile {
        path: String,
        content: String,
    },
    #[serde(alias = "code_execute")]
    ShellExec {
        command: String,
        #[serde(default)]
        cwd: Option<String>,
    },
    MemoryRecall {
        query: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolModelOutput {
    ToolCall(ToolCall),
    FinalAnswer(String),
    ProtocolError(ToolProtocolError),
    PlainText(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProtocolError {
    pub code: String,
    pub message: String,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLoopEvent {
    pub round: usize,
    pub kind: String,
    pub tool_name: Option<String>,
    pub atomic_tool_name: Option<String>,
    pub decision: Option<String>,
    pub ok: Option<bool>,
    pub failure_class: Option<String>,
    pub duration_ms: Option<u64>,
    pub retryable: Option<bool>,
    pub summary: Option<String>,
    pub protocol_error_code: Option<String>,
    pub protocol_error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolActionEnvelope {
    ToolCall {
        #[serde(default)]
        schema_version: Option<u16>,
        call: ToolCall,
    },
    Final {
        #[serde(default)]
        schema_version: Option<u16>,
        answer: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub call: ToolCall,
    pub tool_name: String,
    pub atomic_tool_name: Option<String>,
    pub ok: bool,
    pub summary: String,
    pub decision: Option<String>,
    pub duration_ms: u64,
    pub retryable: bool,
    pub target_path: Option<String>,
    pub resolved_path: Option<String>,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub entries: Vec<ToolDirectoryEntry>,
    pub output_bytes: Option<usize>,
    pub output_lines: Option<usize>,
    pub stderr_bytes: Option<usize>,
    pub stderr_lines: Option<usize>,
    pub output: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub changed_files: Vec<String>,
    pub write_before_bytes: Option<usize>,
    pub write_after_bytes: Option<usize>,
    pub write_changed: Option<bool>,
    pub write_operation: Option<WriteOperation>,
    pub write_diff_preview: Option<String>,
    pub write_diff_truncated: bool,
    pub failure_class: Option<String>,
    pub output_redacted: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
    pub output_truncated: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDirectoryEntry {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperation {
    Created,
    Modified,
    Unchanged,
}

pub const TOOL_EXECUTION_RECORD_SCHEMA_FIELDS: &[&str] = &[
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
];

pub const TOOL_LOOP_REPORT_SCHEMA_VERSION: u16 = 6;
pub const TOOL_ACTION_SCHEMA_VERSION: u16 = 1;

pub const TOOL_LOOP_REPORT_SCHEMA_FIELDS: &[&str] = &[
    "schema_version",
    "status",
    "workspace_root",
    "rounds",
    "call_count",
    "calls",
];

pub const TOOL_ACTION_SCHEMA_FIELDS: &[&str] = &["schema_version", "type", "call", "answer"];

pub const TOOL_ACTION_CALL_FIELDS: &[&str] = &[
    "tool",
    "path",
    "content",
    "command",
    "cwd",
    "query",
    "session_id",
    "limit",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedToolExecutionRecord {
    pub decision: RiskDecision,
    pub record: ToolExecutionRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLoopReport {
    pub schema_version: u16,
    pub status: String,
    pub workspace_root: String,
    pub rounds: usize,
    pub call_count: usize,
    pub calls: Vec<ToolExecutionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSurfaceStatus {
    pub available: bool,
    pub governed: bool,
    pub source: String,
    pub workspace_root: String,
    pub callable_tools: Vec<String>,
    pub mapped_atomic_tools: Vec<String>,
    pub interface_only_atomic_tools: Vec<String>,
    pub action_schema_version: u16,
    pub report_schema_version: u16,
    pub instruction_context_injected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionConfig {
    pub shell_timeout_ms: u64,
    pub shell_risk_rules: ShellRiskRules,
    pub memory: Option<MemoryToolContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryToolContext {
    pub db_path: PathBuf,
    pub session_id: Option<String>,
    pub default_limit: usize,
    pub max_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSlot {
    registry: AtomicToolRegistry,
    config: ToolExecutionConfig,
}

impl Default for ToolExecutionConfig {
    fn default() -> Self {
        Self {
            shell_timeout_ms: 30_000,
            shell_risk_rules: ShellRiskRules::default(),
            memory: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellRiskRules {
    pub delete_or_cleanup: Vec<String>,
    pub service_change: Vec<String>,
    pub network_change: Vec<String>,
    pub secret_access: Vec<String>,
}

impl Default for ShellRiskRules {
    fn default() -> Self {
        Self {
            delete_or_cleanup: vec![
                " rm ".to_string(),
                " rm\t".to_string(),
                " rm\n".to_string(),
                " rm -".to_string(),
                " unlink ".to_string(),
                " rmdir ".to_string(),
                " git reset --hard".to_string(),
                " git checkout --".to_string(),
                " purge ".to_string(),
                " uninstall ".to_string(),
                " apt remove ".to_string(),
                " dnf remove ".to_string(),
                " pacman -r".to_string(),
            ],
            service_change: vec![
                " systemctl ".to_string(),
                " service ".to_string(),
                " loginctl ".to_string(),
                " reboot ".to_string(),
                " shutdown ".to_string(),
                " pkill ".to_string(),
                " killall ".to_string(),
            ],
            network_change: vec![
                " curl ".to_string(),
                " wget ".to_string(),
                " ssh ".to_string(),
                " scp ".to_string(),
                " rsync ".to_string(),
                " nc ".to_string(),
                " ncat ".to_string(),
                " telnet ".to_string(),
            ],
            secret_access: vec![
                " .env".to_string(),
                " id_rsa".to_string(),
                " id_ed25519".to_string(),
                " api_key".to_string(),
                " token".to_string(),
                " secret".to_string(),
                " password".to_string(),
            ],
        }
    }
}

impl ExecutionSlot {
    pub fn generic_agent_mvp(config: ToolExecutionConfig) -> Self {
        Self {
            registry: AtomicToolRegistry::generic_agent_mvp(),
            config,
        }
    }

    pub fn registry(&self) -> &AtomicToolRegistry {
        &self.registry
    }

    pub fn tool_instruction_block(&self, workspace_root: &Path) -> String {
        format!(
            "{}\n\
受治理只读记忆工具：memory_recall。仅可检索当前会话记忆；未配置会话 DB 或 session_id 时会返回结构化未配置结果，不会接外部知识库。\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"memory_recall\",\"query\":\"关键词\",\"limit\":3}}}}",
            self.registry.tool_instruction_block(workspace_root)
        )
    }

    pub fn execute_with_governance<G: Governance>(
        &self,
        workspace_root: &Path,
        governance: &mut G,
        call: &ToolCall,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Result<GovernedToolExecutionRecord, String> {
        execute_tool_call_with_registry_and_governance(
            workspace_root,
            governance,
            &self.registry,
            call,
            agent_id,
            task_id,
            &self.config,
        )
    }

    pub fn execute_or_reject_with_governance<G: Governance>(
        &self,
        workspace_root: &Path,
        governance: &mut G,
        call: &ToolCall,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Result<GovernedToolExecutionRecord, String> {
        execute_tool_call_or_reject_with_registry_and_governance(
            workspace_root,
            governance,
            &self.registry,
            call,
            agent_id,
            task_id,
            &self.config,
        )
    }
}

impl ToolLoopReport {
    pub fn completed(
        workspace_root: &Path,
        rounds: usize,
        calls: Vec<ToolExecutionRecord>,
    ) -> Self {
        Self {
            schema_version: TOOL_LOOP_REPORT_SCHEMA_VERSION,
            status: "completed".to_string(),
            workspace_root: workspace_root.display().to_string(),
            rounds,
            call_count: calls.len(),
            calls,
        }
    }

    pub fn schema_version() -> u16 {
        TOOL_LOOP_REPORT_SCHEMA_VERSION
    }

    pub fn schema_fields() -> &'static [&'static str] {
        TOOL_LOOP_REPORT_SCHEMA_FIELDS
    }

    pub fn call_schema_fields() -> &'static [&'static str] {
        TOOL_EXECUTION_RECORD_SCHEMA_FIELDS
    }
}

impl ToolSurfaceStatus {
    pub fn generic_agent_mvp(workspace_root: &Path) -> Self {
        let registry = AtomicToolRegistry::generic_agent_mvp();
        let mapped_atomic_tools = registry
            .mapped_atomic_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut callable_tools = mapped_atomic_tools.clone();
        callable_tools.push("list_dir".to_string());
        callable_tools.push("memory_recall".to_string());

        Self {
            available: true,
            governed: true,
            source: "GenericAgent".to_string(),
            workspace_root: workspace_root.display().to_string(),
            callable_tools,
            mapped_atomic_tools,
            interface_only_atomic_tools: registry
                .interface_only_atomic_names()
                .into_iter()
                .map(str::to_string)
                .collect(),
            action_schema_version: ToolActionEnvelope::schema_version(),
            report_schema_version: ToolLoopReport::schema_version(),
            instruction_context_injected: true,
        }
    }
}

impl ToolActionEnvelope {
    pub fn schema_version() -> u16 {
        TOOL_ACTION_SCHEMA_VERSION
    }

    pub fn schema_fields() -> &'static [&'static str] {
        TOOL_ACTION_SCHEMA_FIELDS
    }

    pub fn call_schema_fields() -> &'static [&'static str] {
        TOOL_ACTION_CALL_FIELDS
    }
}

pub fn tool_instruction_block(workspace_root: &Path) -> String {
    ExecutionSlot::generic_agent_mvp(ToolExecutionConfig::default())
        .tool_instruction_block(workspace_root)
}

pub fn parse_tool_action_envelope(body: &str) -> Option<ToolActionEnvelope> {
    parse_tool_action_envelope_result(body).ok()
}

pub fn parse_tool_action_envelope_result(
    body: &str,
) -> Result<ToolActionEnvelope, ToolProtocolError> {
    let trimmed = body.trim();
    let Some(json_text) = trimmed.strip_prefix("ACTION:").map(str::trim) else {
        return Err(protocol_error(
            "missing_action_prefix",
            "ACTION payload must start with ACTION:",
            trimmed,
        ));
    };
    serde_json::from_str(json_text).map_err(|error| {
        protocol_error(
            "invalid_action_json",
            &format!("ACTION payload is invalid or unsupported: {error}"),
            trimmed,
        )
    })
}

pub fn parse_tool_call(body: &str) -> Option<ToolCall> {
    let trimmed = body.trim();
    let json_text = trimmed.strip_prefix("TOOL_CALL:")?.trim();
    serde_json::from_str(json_text).ok()
}

pub fn parse_final_answer(body: &str) -> Option<String> {
    let trimmed = body.trim();
    let final_text = trimmed.strip_prefix("FINAL:")?.trim();
    if final_text.is_empty() {
        None
    } else {
        Some(final_text.to_string())
    }
}

pub fn parse_tool_model_output(body: &str) -> ToolModelOutput {
    let trimmed = body.trim();
    if trimmed.starts_with("ACTION:") {
        return match parse_tool_action_envelope_result(trimmed) {
            Ok(ToolActionEnvelope::ToolCall {
                schema_version,
                call,
            }) if is_supported_action_schema(schema_version) => ToolModelOutput::ToolCall(call),
            Ok(ToolActionEnvelope::ToolCall { schema_version, .. }) => {
                ToolModelOutput::ProtocolError(unsupported_action_schema_error(
                    schema_version,
                    trimmed,
                ))
            }
            Ok(ToolActionEnvelope::Final {
                schema_version,
                answer: _,
            }) if !is_supported_action_schema(schema_version) => ToolModelOutput::ProtocolError(
                unsupported_action_schema_error(schema_version, trimmed),
            ),
            Ok(ToolActionEnvelope::Final { answer, .. }) if !answer.trim().is_empty() => {
                ToolModelOutput::FinalAnswer(answer.trim().to_string())
            }
            Ok(ToolActionEnvelope::Final { .. }) => ToolModelOutput::ProtocolError(protocol_error(
                "empty_final_answer",
                "ACTION final answer is empty",
                trimmed,
            )),
            Err(error) => ToolModelOutput::ProtocolError(error),
        };
    }

    if let Some(json_text) = trimmed.strip_prefix("TOOL_CALL:").map(str::trim) {
        return match serde_json::from_str::<ToolCall>(json_text) {
            Ok(call) => ToolModelOutput::ToolCall(call),
            Err(error) => ToolModelOutput::ProtocolError(protocol_error(
                "invalid_legacy_tool_call_json",
                &format!("TOOL_CALL payload is invalid or unsupported: {error}"),
                trimmed,
            )),
        };
    }

    if let Some(final_text) = trimmed.strip_prefix("FINAL:").map(str::trim) {
        if final_text.is_empty() {
            return ToolModelOutput::ProtocolError(protocol_error(
                "empty_final_answer",
                "FINAL answer is empty",
                trimmed,
            ));
        }
        return ToolModelOutput::FinalAnswer(final_text.to_string());
    }

    ToolModelOutput::PlainText(trimmed.to_string())
}

fn protocol_error(code: &str, message: &str, raw: &str) -> ToolProtocolError {
    ToolProtocolError {
        code: code.to_string(),
        message: message.to_string(),
        raw: truncate_text_with_flag(raw, 1_000).text,
    }
}

fn is_supported_action_schema(schema_version: Option<u16>) -> bool {
    schema_version
        .map(|version| version == TOOL_ACTION_SCHEMA_VERSION)
        .unwrap_or(true)
}

fn unsupported_action_schema_error(schema_version: Option<u16>, raw: &str) -> ToolProtocolError {
    protocol_error(
        "unsupported_action_schema_version",
        &format!(
            "ACTION schema_version={} is not supported; current={}",
            schema_version
                .map(|version| version.to_string())
                .unwrap_or_else(|| "none".to_string()),
            TOOL_ACTION_SCHEMA_VERSION
        ),
        raw,
    )
}

pub fn execute_tool_call(workspace_root: &Path, call: &ToolCall) -> ToolExecutionRecord {
    execute_tool_call_with_config(workspace_root, call, &ToolExecutionConfig::default())
}

pub fn execute_tool_call_with_config(
    workspace_root: &Path,
    call: &ToolCall,
    config: &ToolExecutionConfig,
) -> ToolExecutionRecord {
    let registry = AtomicToolRegistry::generic_agent_mvp();
    execute_tool_call_with_registry_and_config(workspace_root, &registry, call, config)
}

fn execute_tool_call_with_registry_and_config(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    config: &ToolExecutionConfig,
) -> ToolExecutionRecord {
    let started = Instant::now();
    let mut record = match call {
        ToolCall::ListDir { path } => execute_list_dir(workspace_root, registry, call, path),
        ToolCall::ReadFile { path } => execute_read_file(workspace_root, registry, call, path),
        ToolCall::WriteFile { path, content } => {
            execute_write_file(workspace_root, registry, call, path, content)
        }
        ToolCall::ShellExec { command, cwd } => execute_shell_exec(
            workspace_root,
            registry,
            call,
            command,
            cwd,
            config.shell_timeout_ms,
        ),
        ToolCall::MemoryRecall {
            query,
            session_id,
            limit,
        } => execute_memory_recall(registry, call, query, session_id, *limit, &config.memory),
    };
    record.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    record.retryable = record
        .failure_class
        .as_deref()
        .is_some_and(is_retryable_failure);
    record
}

pub fn execute_tool_call_with_governance<G: Governance>(
    workspace_root: &Path,
    governance: &mut G,
    call: &ToolCall,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
) -> Result<GovernedToolExecutionRecord, String> {
    execute_tool_call_with_governance_and_config(
        workspace_root,
        governance,
        call,
        agent_id,
        task_id,
        &ToolExecutionConfig::default(),
    )
}

pub fn execute_tool_call_with_governance_and_config<G: Governance>(
    workspace_root: &Path,
    governance: &mut G,
    call: &ToolCall,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
    config: &ToolExecutionConfig,
) -> Result<GovernedToolExecutionRecord, String> {
    let registry = AtomicToolRegistry::generic_agent_mvp();
    execute_tool_call_with_registry_and_governance(
        workspace_root,
        governance,
        &registry,
        call,
        agent_id,
        task_id,
        config,
    )
}

fn execute_tool_call_with_registry_and_governance<G: Governance>(
    workspace_root: &Path,
    governance: &mut G,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
    config: &ToolExecutionConfig,
) -> Result<GovernedToolExecutionRecord, String> {
    let agent_id = agent_id.into();
    let task_id = task_id.into();
    let proposed = proposed_action_for_tool_call_with_registry(
        workspace_root,
        registry,
        call,
        &config.shell_risk_rules,
    );
    let decision = governance
        .classify(&proposed)
        .map_err(|e| format!("tool_governance_failed: {}", e.message))?;

    if is_rejected_tool_decision(&decision) {
        audit_tool_rejection(
            governance,
            registry,
            call,
            agent_id.clone(),
            task_id.clone(),
            &decision,
        )?;
        return Err(tool_rejection_error(&decision));
    }

    execute_allowed_tool_call_with_audit(
        workspace_root,
        governance,
        registry,
        call,
        agent_id,
        task_id,
        config,
        decision,
    )
}

fn execute_allowed_tool_call_with_audit<G: Governance>(
    workspace_root: &Path,
    governance: &mut G,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    agent_id: String,
    task_id: String,
    config: &ToolExecutionConfig,
    decision: RiskDecision,
) -> Result<GovernedToolExecutionRecord, String> {
    let mut record =
        execute_tool_call_with_registry_and_config(workspace_root, registry, call, config);
    record.decision = Some(risk_decision_label(&decision));
    let mapping = registry.mapping_for_call(call);
    let audit_record = AuditRecord {
        operation: mapping.audit_operation.to_string(),
        agent_id: AgentId(agent_id),
        task_id: TaskId(task_id),
        delta_bytes: record.summary.len() as i64,
        reason: format!(
            "decision={}; ok={}; {}",
            risk_decision_label(&decision),
            record.ok,
            record.summary
        ),
        timestamp: Timestamp(now_timestamp()),
    };
    governance
        .audit(audit_record)
        .map_err(|e| format!("tool_audit_failed: {}", e.message))?;

    Ok(GovernedToolExecutionRecord { decision, record })
}

fn execute_tool_call_or_reject_with_registry_and_governance<G: Governance>(
    workspace_root: &Path,
    governance: &mut G,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
    config: &ToolExecutionConfig,
) -> Result<GovernedToolExecutionRecord, String> {
    let agent_id = agent_id.into();
    let task_id = task_id.into();
    let proposed = proposed_action_for_tool_call_with_registry(
        workspace_root,
        registry,
        call,
        &config.shell_risk_rules,
    );
    let decision = governance
        .classify(&proposed)
        .map_err(|e| format!("tool_governance_failed: {}", e.message))?;

    if is_rejected_tool_decision(&decision) {
        audit_tool_rejection(governance, registry, call, agent_id, task_id, &decision)?;
        let record = governance_rejected_record(registry, call, &decision);
        return Ok(GovernedToolExecutionRecord { decision, record });
    }

    execute_allowed_tool_call_with_audit(
        workspace_root,
        governance,
        registry,
        call,
        agent_id,
        task_id,
        config,
        decision,
    )
}

fn audit_tool_rejection<G: Governance>(
    governance: &mut G,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    agent_id: String,
    task_id: String,
    decision: &RiskDecision,
) -> Result<(), String> {
    let mapping = registry.mapping_for_call(call);
    let audit_record = AuditRecord {
        operation: format!("{}.rejected", mapping.audit_operation),
        agent_id: AgentId(agent_id),
        task_id: TaskId(task_id),
        delta_bytes: 0,
        reason: format!("decision={}", risk_decision_label(decision)),
        timestamp: Timestamp(now_timestamp()),
    };
    governance
        .audit(audit_record)
        .map_err(|e| format!("tool_audit_failed: {}", e.message))
}

pub fn proposed_action_for_tool_call(workspace_root: &Path, call: &ToolCall) -> ProposedAction {
    let registry = AtomicToolRegistry::generic_agent_mvp();
    proposed_action_for_tool_call_with_registry(
        workspace_root,
        &registry,
        call,
        &ShellRiskRules::default(),
    )
}

fn proposed_action_for_tool_call_with_registry(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    shell_risk_rules: &ShellRiskRules,
) -> ProposedAction {
    let mapping = registry.mapping_for_call(call);
    let action_id = mapping
        .audit_operation
        .strip_prefix("tool.")
        .map(|name| format!("tool:{name}"))
        .unwrap_or_else(|| format!("tool:{}", tool_call_name(call)));
    let kind = tool_action_kind(call, shell_risk_rules);
    let target = tool_target(workspace_root, call);
    let summary = match mapping.atomic_tool_name {
        Some(name) => format!("atomic_tool={name} {}", tool_summary(call)),
        None => format!(
            "auxiliary_tool={} {}",
            mapping.protocol_tool_name,
            tool_summary(call)
        ),
    };

    ProposedAction {
        action_id,
        kind,
        target,
        summary,
    }
}

fn tool_call_name(call: &ToolCall) -> &'static str {
    match call {
        ToolCall::ListDir { .. } => "list_dir",
        ToolCall::ReadFile { .. } => "read_file",
        ToolCall::WriteFile { .. } => "write_file",
        ToolCall::ShellExec { .. } => "shell_exec",
        ToolCall::MemoryRecall { .. } => "memory_recall",
    }
}

fn execute_list_dir(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    path: &str,
) -> ToolExecutionRecord {
    let dir = match resolve_workspace_path(workspace_root, path) {
        Ok(dir) => dir,
        Err(error) => return failed_record(registry, call, error),
    };
    let mut entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => {
            return failed_record(
                registry,
                call,
                format!("list_dir_failed path={} error={error}", dir.display()),
            )
        }
    }
    .filter_map(|entry| entry.ok())
    .map(|entry| {
        let file_type = entry.file_type().ok();
        let kind = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
            "dir"
        } else if file_type.as_ref().is_some_and(|ft| ft.is_file()) {
            "file"
        } else {
            "other"
        };
        ToolDirectoryEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            kind: kind.to_string(),
        }
    })
    .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name).then(left.kind.cmp(&right.kind)));
    let output = format!(
        "entries=[{}]",
        entries
            .iter()
            .map(|entry| format!("{} ({})", entry.name, entry.kind))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut record = success_record(
        registry,
        call,
        format!("path={} {}", dir.display(), output),
        Some(output),
        false,
    )
    .with_paths(path, &dir);
    record.entries = entries;
    record
}

fn execute_read_file(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    path: &str,
) -> ToolExecutionRecord {
    let file = match resolve_workspace_path(workspace_root, path) {
        Ok(file) => file,
        Err(error) => return failed_record(registry, call, error),
    };
    let content = match fs::read_to_string(&file) {
        Ok(content) => content,
        Err(error) => {
            return failed_record(
                registry,
                call,
                format!("read_file_failed path={} error={error}", file.display()),
            )
        }
    };
    let redacted = should_redact_tool_output(path, &content);
    let display_content = if redacted {
        "[redacted: secret-like path or content]".to_string()
    } else {
        content.clone()
    };
    let truncated = truncate_text_with_flag(&display_content, 10_000);
    let mut record = success_record(
        registry,
        call,
        format!(
            "path={} bytes={} content=\n{}",
            file.display(),
            content.len(),
            truncated.text
        ),
        Some(truncated.text),
        truncated.truncated,
    )
    .with_paths(path, &file);
    record.output_bytes = Some(content.len());
    record.output_lines = Some(count_lines(&content));
    record.output_redacted = redacted;
    record
}

fn execute_write_file(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    path: &str,
    content: &str,
) -> ToolExecutionRecord {
    let file = match resolve_workspace_path(workspace_root, path) {
        Ok(file) => file,
        Err(error) => return failed_record(registry, call, error),
    };
    let previous_content = if file.exists() {
        match fs::read_to_string(&file) {
            Ok(value) => Some(value),
            Err(error) => {
                return failed_record(
                    registry,
                    call,
                    format!(
                        "write_file_read_existing_failed path={} error={error}",
                        file.display()
                    ),
                )
            }
        }
    } else {
        None
    };
    if let Some(parent) = file.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return failed_record(
                registry,
                call,
                format!(
                    "write_file_mkdir_failed path={} error={e}",
                    parent.display(),
                    e = error
                ),
            );
        }
    }
    if let Err(error) = fs::write(&file, content) {
        return failed_record(
            registry,
            call,
            format!("write_file_failed path={} error={error}", file.display()),
        );
    }
    let mut record = success_record(
        registry,
        call,
        format!("written path={} bytes={}", file.display(), content.len()),
        None,
        false,
    );
    record.target_path = Some(path.to_string());
    record.resolved_path = Some(file.display().to_string());
    record.changed_files.push(file.display().to_string());
    record.write_before_bytes = previous_content.as_ref().map(|value| value.len());
    record.write_after_bytes = Some(content.len());
    record.write_changed = Some(previous_content.as_deref() != Some(content));
    record.write_operation = Some(match previous_content.as_deref() {
        None => WriteOperation::Created,
        Some(previous) if previous == content => WriteOperation::Unchanged,
        Some(_) => WriteOperation::Modified,
    });
    let diff_preview = build_write_diff_preview(path, previous_content.as_deref(), content);
    record.write_diff_preview = diff_preview.text;
    record.write_diff_truncated = diff_preview.truncated;
    record.output_redacted = diff_preview.redacted;
    record
}

fn execute_shell_exec(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    command: &str,
    cwd: &Option<String>,
    timeout_ms: u64,
) -> ToolExecutionRecord {
    let cwd_path = match cwd {
        Some(value) if !value.trim().is_empty() => {
            match resolve_workspace_path(workspace_root, value) {
                Ok(path) => path,
                Err(error) => return failed_record(registry, call, error),
            }
        }
        _ => workspace_root.to_path_buf(),
    };
    let child = match Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(&cwd_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return failed_record(
                registry,
                call,
                format!(
                    "shell_exec_spawn_failed cwd={} error={e}",
                    cwd_path.display(),
                    e = error
                ),
            )
        }
    };

    let output = match wait_with_timeout(child, timeout_ms) {
        Ok(output) => output,
        Err(error) => {
            return failed_record(
                registry,
                call,
                format!(
                    "shell_exec_wait_failed cwd={} error={e}",
                    cwd_path.display(),
                    e = error
                ),
            )
        }
    };
    let stdout_raw = String::from_utf8_lossy(&output.stdout);
    let stderr_raw = String::from_utf8_lossy(&output.stderr);
    let stdout_redacted = should_redact_tool_output(command, &stdout_raw);
    let stderr_redacted = should_redact_tool_output(command, &stderr_raw);
    let stdout_display = if stdout_redacted {
        "[redacted: secret-like command or output]".to_string()
    } else {
        stdout_raw.to_string()
    };
    let stderr_display = if stderr_redacted {
        "[redacted: secret-like command or output]".to_string()
    } else {
        stderr_raw.to_string()
    };
    let stdout = truncate_text_with_flag(&stdout_display, 8_000);
    let stderr = truncate_text_with_flag(&stderr_display, 4_000);
    let mut record = success_record(
        registry,
        call,
        format!(
            "cwd={} status={:?} stdout=\n{}\nstderr=\n{}",
            cwd_path.display(),
            output.status.code(),
            stdout.text,
            stderr.text
        ),
        None,
        false,
    );
    record.cwd = Some(cwd_path.display().to_string());
    record.command = Some(command.to_string());
    record.ok = output.status.success();
    record.stdout = Some(stdout.text);
    record.stderr = Some(stderr.text);
    record.output_bytes = Some(output.stdout.len());
    record.output_lines = Some(count_lines(&stdout_raw));
    record.stderr_bytes = Some(output.stderr.len());
    record.stderr_lines = Some(count_lines(&stderr_raw));
    record.exit_code = output.status.code();
    record.output_redacted = stdout_redacted;
    record.stdout_redacted = stdout_redacted;
    record.stderr_redacted = stderr_redacted;
    record.stdout_truncated = stdout.truncated;
    record.stderr_truncated = stderr.truncated;
    if !record.ok {
        record.failure_class = Some("exit_nonzero".to_string());
    }
    record
}

fn execute_memory_recall(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    query: &str,
    requested_session_id: &Option<String>,
    requested_limit: Option<usize>,
    memory: &Option<MemoryToolContext>,
) -> ToolExecutionRecord {
    let Some(memory) = memory else {
        return memory_failed_record(registry, call, "memory_recall_unconfigured");
    };
    let Some(configured_session_id) = memory.session_id.as_deref() else {
        return memory_failed_record(registry, call, "memory_recall_session_unconfigured");
    };
    if let Some(requested) = requested_session_id.as_deref() {
        if requested != configured_session_id {
            return memory_failed_record(registry, call, "memory_recall_session_mismatch");
        }
    }
    if query.trim().is_empty() {
        return memory_failed_record(registry, call, "memory_recall_query_empty");
    }
    let limit = requested_limit.unwrap_or(memory.default_limit);
    if limit == 0 {
        return memory_failed_record(registry, call, "memory_recall_limit_must_be_positive");
    }
    let limit = limit.min(memory.max_limit.max(1));
    if !memory.db_path.exists() {
        return memory_failed_record(registry, call, "memory_recall_store_unavailable");
    }

    let store = match SqliteMemoryStore::open(&memory.db_path) {
        Ok(store) => store,
        Err(error) => {
            return memory_failed_record(
                registry,
                call,
                &format!("memory_recall_store_open_failed: {error:?}"),
            )
        }
    };
    let pipeline = MemoryRecallPipeline::new(store);
    let result = match pipeline.recall(&RecallRequest {
        query_text: query.trim().to_string(),
        metadata: std::collections::BTreeMap::from([
            ("memory_scope".to_string(), "session".to_string()),
            ("session_id".to_string(), configured_session_id.to_string()),
        ]),
        limit,
    }) {
        Ok(result) => result,
        Err(error) => {
            return memory_failed_record(
                registry,
                call,
                &format!("memory_recall_failed: {error:?}"),
            )
        }
    };

    let output = MemoryRecallToolOutput {
        scope: "session".to_string(),
        session_id: configured_session_id.to_string(),
        query: query.trim().to_string(),
        hit_count: result.hits.len(),
        hits: result
            .hits
            .into_iter()
            .map(|hit| MemoryRecallToolHit {
                rank: hit.rank,
                score: hit.score,
                id: hit.record.id,
                content: hit.record.content,
                metadata: hit.record.metadata,
                created_at: hit.record.created_at,
            })
            .collect(),
    };
    let output_json = match serde_json::to_string(&output) {
        Ok(value) => value,
        Err(error) => {
            return memory_failed_record(
                registry,
                call,
                &format!("memory_recall_output_json_failed: {error}"),
            )
        }
    };
    let truncated = truncate_text_with_flag(&output_json, 8_000);
    let mut record = success_record(
        registry,
        call,
        format!(
            "memory_recall scope=session session_id={} query={} hits={}",
            configured_session_id,
            query.trim(),
            output.hit_count
        ),
        Some(truncated.text),
        truncated.truncated,
    );
    record.output_lines = Some(output.hit_count);
    record
}

#[derive(Debug, Serialize)]
struct MemoryRecallToolOutput {
    scope: String,
    session_id: String,
    query: String,
    hit_count: usize,
    hits: Vec<MemoryRecallToolHit>,
}

#[derive(Debug, Serialize)]
struct MemoryRecallToolHit {
    rank: usize,
    score: u32,
    id: String,
    content: String,
    metadata: std::collections::BTreeMap<String, String>,
    created_at: String,
}

fn memory_failed_record(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    reason: &str,
) -> ToolExecutionRecord {
    let mut record = failed_record(registry, call, reason.to_string());
    record.failure_class = Some(reason.split(':').next().unwrap_or(reason).to_string());
    record.retryable = false;
    record
}

fn success_record(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    summary: String,
    output: Option<String>,
    output_truncated: bool,
) -> ToolExecutionRecord {
    let mapping = registry.mapping_for_call(call);
    ToolExecutionRecord {
        call: call.clone(),
        tool_name: tool_call_name(call).to_string(),
        atomic_tool_name: mapping.atomic_tool_name.map(str::to_string),
        ok: true,
        summary,
        decision: None,
        duration_ms: 0,
        retryable: false,
        target_path: None,
        resolved_path: None,
        cwd: None,
        command: None,
        entries: Vec::new(),
        output_bytes: output.as_ref().map(|value| value.len()),
        output_lines: output.as_ref().map(|value| count_lines(value)),
        stderr_bytes: None,
        stderr_lines: None,
        output,
        stdout: None,
        stderr: None,
        exit_code: None,
        changed_files: Vec::new(),
        write_before_bytes: None,
        write_after_bytes: None,
        write_changed: None,
        write_operation: None,
        write_diff_preview: None,
        write_diff_truncated: false,
        failure_class: None,
        output_redacted: false,
        stdout_redacted: false,
        stderr_redacted: false,
        output_truncated,
        stdout_truncated: false,
        stderr_truncated: false,
    }
}

fn failed_record(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    summary: String,
) -> ToolExecutionRecord {
    let mapping = registry.mapping_for_call(call);
    ToolExecutionRecord {
        call: call.clone(),
        tool_name: tool_call_name(call).to_string(),
        atomic_tool_name: mapping.atomic_tool_name.map(str::to_string),
        ok: false,
        failure_class: Some(classify_tool_failure(&summary).to_string()),
        summary,
        decision: None,
        duration_ms: 0,
        retryable: false,
        target_path: target_path_from_call(call),
        resolved_path: None,
        cwd: cwd_from_call(call),
        command: command_from_call(call),
        entries: Vec::new(),
        output_bytes: None,
        output_lines: None,
        stderr_bytes: None,
        stderr_lines: None,
        output: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        changed_files: Vec::new(),
        write_before_bytes: None,
        write_after_bytes: None,
        write_changed: None,
        write_operation: None,
        write_diff_preview: None,
        write_diff_truncated: false,
        output_truncated: false,
        output_redacted: false,
        stdout_redacted: false,
        stderr_redacted: false,
        stdout_truncated: false,
        stderr_truncated: false,
    }
}

impl ToolExecutionRecord {
    fn with_paths(mut self, target_path: &str, resolved_path: &Path) -> Self {
        self.target_path = Some(target_path.to_string());
        self.resolved_path = Some(resolved_path.display().to_string());
        self
    }
}

fn governance_rejected_record(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    decision: &RiskDecision,
) -> ToolExecutionRecord {
    let mut record = failed_record(registry, call, tool_rejection_error(decision));
    record.failure_class = Some("governance_rejected".to_string());
    record.decision = Some(risk_decision_label(decision));
    record.retryable = false;
    record
}

fn is_rejected_tool_decision(decision: &RiskDecision) -> bool {
    matches!(
        decision,
        RiskDecision::Blocked { .. }
            | RiskDecision::DraftOnly { .. }
            | RiskDecision::NeedsApproval { .. }
    )
}

fn tool_rejection_error(decision: &RiskDecision) -> String {
    let prefix = match decision {
        RiskDecision::Blocked { .. } => "tool_blocked",
        RiskDecision::DraftOnly { .. } => "tool_draft_only",
        RiskDecision::NeedsApproval { .. } => "tool_needs_approval",
        RiskDecision::Allowed { .. } => "tool_not_rejected",
    };
    format!("{prefix}: {}", risk_decision_reason(decision))
}

fn classify_tool_failure(summary: &str) -> &'static str {
    if summary.contains("path_outside_workspace") {
        "path_outside_workspace"
    } else if summary.contains("timed out") || summary.contains("shell_exec_wait_failed") {
        "timeout"
    } else if summary.contains("spawn_failed") {
        "spawn_failed"
    } else if summary.contains("read_file_failed") {
        "read_failed"
    } else if summary.contains("write_file") {
        "write_failed"
    } else if summary.contains("list_dir_failed") {
        "list_failed"
    } else {
        "tool_failed"
    }
}

fn is_retryable_failure(failure_class: &str) -> bool {
    matches!(failure_class, "timeout" | "spawn_failed")
}

fn tool_action_kind(call: &ToolCall, shell_risk_rules: &ShellRiskRules) -> ActionKind {
    match call {
        ToolCall::ListDir { .. } | ToolCall::ReadFile { .. } | ToolCall::MemoryRecall { .. } => {
            ActionKind::Observe
        }
        ToolCall::WriteFile { .. } => ActionKind::LocalFileWrite,
        ToolCall::ShellExec { command, .. } => {
            classify_shell_action_kind(command, shell_risk_rules)
        }
    }
}

fn classify_shell_action_kind(command: &str, rules: &ShellRiskRules) -> ActionKind {
    let normalized = command.to_ascii_lowercase();
    let padded = format!(" {normalized} ");

    if contains_any_pattern(&padded, &rules.delete_or_cleanup) {
        return ActionKind::DeleteOrCleanup;
    }

    if contains_any_pattern(&padded, &rules.service_change) {
        return ActionKind::ServiceChange;
    }

    if contains_any_pattern(&padded, &rules.network_change) {
        return ActionKind::NetworkChange;
    }

    if contains_any_pattern(&padded, &rules.secret_access) {
        return ActionKind::SecretAccess;
    }

    ActionKind::ShellCommand
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn contains_any_pattern(value: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| !pattern.is_empty() && value.contains(pattern))
}

fn tool_target(workspace_root: &Path, call: &ToolCall) -> String {
    match call {
        ToolCall::ListDir { path }
        | ToolCall::ReadFile { path }
        | ToolCall::WriteFile { path, .. } => {
            format!("{}::{}", workspace_root.display(), path.trim())
        }
        ToolCall::ShellExec { command, cwd } => format!(
            "{}::{}",
            cwd.as_deref().unwrap_or(".").trim(),
            command.trim()
        ),
        ToolCall::MemoryRecall {
            query, session_id, ..
        } => format!(
            "memory::session={}::{}",
            session_id.as_deref().unwrap_or("<configured>"),
            query.trim()
        ),
    }
}

fn tool_summary(call: &ToolCall) -> String {
    match call {
        ToolCall::ListDir { path } => format!("list_dir path={}", path.trim()),
        ToolCall::ReadFile { path } => format!("read_file path={}", path.trim()),
        ToolCall::WriteFile { path, content } => {
            format!("write_file path={} bytes={}", path.trim(), content.len())
        }
        ToolCall::ShellExec { command, cwd } => format!(
            "shell_exec cwd={} command={}",
            cwd.as_deref().unwrap_or(".").trim(),
            command.trim()
        ),
        ToolCall::MemoryRecall { query, limit, .. } => format!(
            "memory_recall query={} limit={}",
            query.trim(),
            limit
                .map(|value| value.to_string())
                .unwrap_or_else(|| "default".to_string())
        ),
    }
}

fn target_path_from_call(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::ListDir { path }
        | ToolCall::ReadFile { path }
        | ToolCall::WriteFile { path, .. } => Some(path.clone()),
        ToolCall::ShellExec { .. } | ToolCall::MemoryRecall { .. } => None,
    }
}

fn cwd_from_call(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::ShellExec { cwd, .. } => cwd.clone(),
        _ => None,
    }
}

fn command_from_call(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::ShellExec { command, .. } => Some(command.clone()),
        _ => None,
    }
}

fn count_lines(value: &str) -> usize {
    if value.is_empty() {
        0
    } else {
        value.lines().count()
    }
}

fn now_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn resolve_workspace_path(workspace_root: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let candidate = if raw_path.trim().is_empty() {
        workspace_root.to_path_buf()
    } else {
        let path = Path::new(raw_path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace_root.join(path)
        }
    };

    let normalized_root = fs::canonicalize(workspace_root).map_err(|e| {
        format!(
            "workspace_root_invalid path={} error={e}",
            workspace_root.display()
        )
    })?;
    let normalized_candidate = if candidate.exists() {
        fs::canonicalize(&candidate)
            .map_err(|e| format!("path_invalid path={} error={e}", candidate.display()))?
    } else {
        normalize_path_lexically(&candidate)
    };

    if !normalized_candidate.starts_with(&normalized_root) {
        return Err(format!(
            "path_outside_workspace path={} workspace_root={}",
            normalized_candidate.display(),
            normalized_root.display()
        ));
    }

    Ok(normalized_candidate)
}

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout_ms: u64,
) -> std::io::Result<std::process::Output> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "shell_exec timed out after {timeout_ms}ms status={:?}",
                    output.status.code()
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct TruncatedText {
    text: String,
    truncated: bool,
}

struct OptionalPreview {
    text: Option<String>,
    truncated: bool,
    redacted: bool,
}

fn build_write_diff_preview(path: &str, previous: Option<&str>, next: &str) -> OptionalPreview {
    if previous == Some(next) {
        return OptionalPreview {
            text: Some("unchanged".to_string()),
            truncated: false,
            redacted: false,
        };
    }

    if should_redact_write_preview(path, previous, next) {
        return OptionalPreview {
            text: Some("[redacted: secret-like path or content]".to_string()),
            truncated: false,
            redacted: true,
        };
    }

    let before_lines = previous.unwrap_or("").lines().collect::<Vec<_>>();
    let after_lines = next.lines().collect::<Vec<_>>();
    let max_len = before_lines.len().max(after_lines.len());
    let mut preview = String::from("--- before\n+++ after\n");
    let mut truncated = false;
    let mut emitted = 0usize;

    for index in 0..max_len {
        let before = before_lines.get(index).copied();
        let after = after_lines.get(index).copied();
        if before == after {
            continue;
        }
        match before {
            Some(line) => {
                push_diff_line(&mut preview, '-', line);
                emitted += 1;
            }
            None => {}
        }
        match after {
            Some(line) => {
                push_diff_line(&mut preview, '+', line);
                emitted += 1;
            }
            None => {}
        }
        if emitted >= 80 || preview.len() >= 4_000 {
            truncated = index + 1 < max_len;
            break;
        }
    }

    if preview.len() > 4_000 {
        let truncated_text = truncate_text_with_flag(&preview, 4_000);
        preview = truncated_text.text;
        truncated = true;
    }

    OptionalPreview {
        text: Some(preview),
        truncated,
        redacted: false,
    }
}

fn push_diff_line(preview: &mut String, prefix: char, line: &str) {
    preview.push(prefix);
    preview.push_str(line);
    preview.push('\n');
}

fn should_redact_write_preview(path: &str, previous: Option<&str>, next: &str) -> bool {
    should_redact_tool_output(path, previous.unwrap_or_default())
        || should_redact_tool_output(path, next)
}

fn should_redact_tool_output(locator: &str, content: &str) -> bool {
    let haystack = format!(
        "{}\n{}",
        locator.to_ascii_lowercase(),
        content.to_ascii_lowercase()
    );
    contains_any(
        &haystack,
        &[
            ".env",
            "id_rsa",
            "id_ed25519",
            "api_key",
            "apikey",
            "access_token",
            "refresh_token",
            "secret",
            "password",
            "passwd",
            "private_key",
            "client_secret",
        ],
    )
}

fn truncate_text_with_flag(value: &str, max_len: usize) -> TruncatedText {
    if value.len() <= max_len {
        return TruncatedText {
            text: value.to_string(),
            truncated: false,
        };
    }
    let mut truncated = value
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    TruncatedText {
        text: truncated,
        truncated: true,
    }
}
