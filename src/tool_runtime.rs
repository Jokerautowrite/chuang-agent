//! `tool_runtime` 模块。公开接口：struct ToolProtocolError, ToolLoopEvent, ToolExecutionRecord, ToolDirectoryEntry, GovernedToolExecutionRecord, PendingApproval, PendingRiskDecision, OperatorApprovalReceipt；enum ToolCall, ToolModelOutput, ToolActionEnvelope, WriteOperation；fn build_subagent_tool_context, generic_agent_mvp, registry, tool_catalog_block, tool_detail_block, tool_instruction_block, execute_with_governance, execute_or_reject_with_governance；const TOOL_EXECUTION_RECORD_SCHEMA_FIELDS, TOOL_LOOP_REPORT_SCHEMA_VERSION, TOOL_ACTION_SCHEMA_VERSION, TOOL_LOOP_REPORT_SCHEMA_FIELDS, TOOL_ACTION_SCHEMA_FIELDS, TOOL_ACTION_CALL_FIELDS, PENDING_APPROVAL_MAX_CALL_BYTES, DEFAULT_SUBAGENT_WORKER_MODEL。

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::actuator::{
    Actuator, ClickTarget, CommandActuator, FakeActuator, InputTarget, ObserveTarget,
    OpenAppRequest, ScreenshotTarget, SecretOrPlainText,
};
use crate::atomic_tool::AtomicToolRegistry;
use crate::browser_read::{ensure_cdp_browser_read_adapter, BrowserReadAdapter, BrowserReadError};
use crate::common::{AgentId, AuditRecord, TaskId, Timestamp};
use crate::governance::{
    risk_decision_label, risk_decision_reason, ActionKind, Governance, OperatorApprovalEvidence,
    ProposedAction, RiskDecision,
};
use crate::memory_recall::{MemoryRecallPipeline, RecallRequest};
use crate::memory_store_sqlite::SqliteMemoryStore;
use crate::path_utils::{path_to_display_string, resolve_candidate_preserving_existing_symlinks};
use crate::runtime_config::ActuatorConfig;
use crate::runtime_event_ledger::{
    RuntimeEvent, RuntimeEventKind, RuntimeEventLedger, RuntimeRiskDecision,
};
use crate::secret_redaction::{is_secret_material_path, redact_sensitive_text};
use crate::workspace_file_adapter::WorkspaceFileAdapter;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    Mouse {
        x: i32,
        y: i32,
    },
    Keyboard {
        text: String,
        #[serde(default)]
        secret: bool,
    },
    Screenshot {
        #[serde(default)]
        target: Option<String>,
    },
    Locate {
        #[serde(default)]
        target: Option<String>,
    },
    OpenApp {
        app_name: String,
    },
    Wait {
        millis: u64,
    },
    HumanSuspend {
        reason: String,
        #[serde(default)]
        prompt: Option<String>,
    },
    ApplyPatch {
        patch: String,
    },
    #[serde(rename = "code_execute", alias = "shell_exec")]
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
    SpawnSubagent {
        /// Single task (legacy / simple).
        #[serde(default)]
        task: String,
        /// Parallel tasks: when non-empty, each item is one worker job (preferred for speed).
        #[serde(default)]
        tasks: Option<Vec<String>>,
        #[serde(default)]
        agent_name: Option<String>,
        #[serde(default)]
        policy: Option<String>,
        #[serde(default)]
        token_budget: Option<u16>,
        #[serde(default)]
        timeout_ms: Option<u64>,
        /// How many workers to run at once (default min(n,4), max 8).
        #[serde(default)]
        max_concurrency: Option<u8>,
    },
    /// Read current page URL/title/DOM via managed headless Chrome CDP.
    BrowserRead {},
    /// Navigate managed headless Chrome to a URL, then return page state.
    BrowserNavigate {
        url: String,
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
    "x",
    "y",
    "text",
    "secret",
    "target",
    "app_name",
    "millis",
    "reason",
    "prompt",
    "patch",
    "command",
    "cwd",
    "query",
    "session_id",
    "limit",
    "task",
    "agent_name",
    "policy",
    "token_budget",
    "timeout_ms",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedToolExecutionRecord {
    pub decision: RiskDecision,
    pub record: ToolExecutionRecord,
    pub pending_approval: Option<PendingApproval>,
}

pub const PENDING_APPROVAL_MAX_CALL_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingApproval {
    pub approval_id: String,
    pub call_id: String,
    pub agent_id: String,
    pub task_id: String,
    pub serialized_tool_call: String,
    pub call_fingerprint: String,
    pub target_fingerprint: String,
    pub workspace_fingerprint: String,
    pub policy_marker: String,
    pub risk_decision: PendingRiskDecision,
}

impl fmt::Debug for PendingApproval {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingApproval")
            .field("approval_id", &self.approval_id)
            .field("call_id", &self.call_id)
            .field("agent_id", &self.agent_id)
            .field("task_id", &self.task_id)
            .field("serialized_tool_call", &"<redacted>")
            .field("call_fingerprint", &self.call_fingerprint)
            .field("target_fingerprint", &self.target_fingerprint)
            .field("workspace_fingerprint", &self.workspace_fingerprint)
            .field("policy_marker", &self.policy_marker)
            .field("risk_decision", &self.risk_decision)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRiskDecision {
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorApprovalReceipt {
    pub approval_id: String,
    pub call_id: String,
    pub call_fingerprint: String,
    pub target_fingerprint: String,
    pub workspace_fingerprint: String,
    pub policy_marker: String,
    pub approved: bool,
    pub operator_ref: String,
    pub evidence_ref: String,
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
    pub desktop_browser_read_only_atomic_tools: Vec<String>,
    pub interface_only_atomic_tools: Vec<String>,
    pub action_schema_version: u16,
    pub report_schema_version: u16,
    pub instruction_context_injected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionConfig {
    pub shell_timeout_ms: u64,
    /// When true (default), rewrite supported shell commands through `rtk`
    /// (Rust Token Killer) before execution to shrink tool output tokens.
    pub shell_rtk_rewrite: bool,
    pub shell_risk_rules: ShellRiskRules,
    pub memory: Option<MemoryToolContext>,
    pub actuator: Option<ActuatorConfig>,
    pub subagent: Option<SubagentToolContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryToolContext {
    pub db_path: PathBuf,
    pub session_id: Option<String>,
    pub default_limit: usize,
    pub max_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentToolContext {
    pub executable_path: PathBuf,
    pub config_path: PathBuf,
    pub queue_root: PathBuf,
    pub runner_command: PathBuf,
    pub worker_model: String,
    pub worker_capability: String,
}

/// Default worker model for queued subagent Codex runners.
/// Aligned with the example-provider/zen-sub2 account group's actually supported model
/// (deepseek-v4-flash); gpt-5.6-luna/terra both return 404 there.
/// Pluggable: override with `CHUANG_CODEX_RUNNER_MODEL`.
pub const DEFAULT_SUBAGENT_WORKER_MODEL: &str = "deepseek-v4-flash";

/// Default capability advertised to the subagent dispatch/run-loop chain.
pub const DEFAULT_SUBAGENT_WORKER_CAPABILITY: &str = "workspace";

/// 工具执行层空闲看门狗告警阈值（蓝本 docs/reference-dig-20260810.md §2.5）：
/// 45s 未完成即给执行记录打告警标记。硬中断沿用各工具现有超时配置
/// （shell_timeout_ms / subagent timeout_ms / provider request_timeout_ms）。
const TOOL_IDLE_WATCHDOG_WARN_MS: u64 = 45_000;

const SUBAGENT_RUNNER_REL: &str = "scripts/chuang-codex-runner.py";
const RUNTIME_CONFIG_PATH_META: &str = "config_path";

/// Build production `SubagentToolContext` from runtime config + current process.
///
/// Always returns a context so production tool loops can call `spawn_subagent`.
/// A missing runner script is still recorded as a path; execution fails clearly later.
/// Does **not** enable the live_worker adapter — this is the queued_external + codex runner path.
pub fn build_subagent_tool_context(
    config: &crate::runtime_config::RuntimeConfig,
) -> SubagentToolContext {
    SubagentToolContext {
        executable_path: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("chuang-agent")),
        config_path: resolve_subagent_config_path(config),
        queue_root: config.subagent_queue.root.clone(),
        runner_command: resolve_codex_runner_command(config),
        worker_model: resolve_subagent_worker_model(),
        worker_capability: DEFAULT_SUBAGENT_WORKER_CAPABILITY.to_string(),
    }
}

fn resolve_subagent_config_path(config: &crate::runtime_config::RuntimeConfig) -> PathBuf {
    if let Some(raw) = config.metadata.get(RUNTIME_CONFIG_PATH_META) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let workspace_config = config.permission.workspace_root.join("config.toml");
    if workspace_config.is_file() {
        return workspace_config;
    }
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_config = cwd.join("config.toml");
        if cwd_config.is_file() {
            return cwd_config;
        }
    }
    PathBuf::from("config.toml")
}

fn resolve_codex_runner_command(config: &crate::runtime_config::RuntimeConfig) -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(root) = std::env::var("CHUANG_AGENT_ROOT") {
        candidates.push(PathBuf::from(root).join(SUBAGENT_RUNNER_REL));
    }
    candidates.push(config.permission.workspace_root.join(SUBAGENT_RUNNER_REL));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SUBAGENT_RUNNER_REL));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(SUBAGENT_RUNNER_REL));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join(SUBAGENT_RUNNER_REL));
            // cargo run layout: target/debug/chuang-agent → repo root
            candidates.push(parent.join("../..").join(SUBAGENT_RUNNER_REL));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(SUBAGENT_RUNNER_REL))
}

fn resolve_subagent_worker_model() -> String {
    std::env::var("CHUANG_CODEX_RUNNER_MODEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SUBAGENT_WORKER_MODEL.to_string())
}

#[derive(Debug, Clone)]
pub struct ExecutionSlot {
    registry: AtomicToolRegistry,
    config: ToolExecutionConfig,
}

impl Default for ToolExecutionConfig {
    fn default() -> Self {
        Self {
            shell_timeout_ms: 120_000,
            shell_rtk_rewrite: true,
            shell_risk_rules: ShellRiskRules::default(),
            memory: None,
            actuator: None,
            subagent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellRiskRules {
    pub delete_or_cleanup: Vec<String>,
    pub privilege_escalation: Vec<String>,
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
                " git clean ".to_string(),
                " purge ".to_string(),
                " uninstall ".to_string(),
                " apt remove ".to_string(),
                " dnf remove ".to_string(),
                " pacman -r".to_string(),
            ],
            privilege_escalation: vec![
                " sudo ".to_string(),
                " doas ".to_string(),
                " su -".to_string(),
                " chmod -r 777".to_string(),
                " chmod -r 0777".to_string(),
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
                " nmcli ".to_string(),
                " ip link ".to_string(),
                " ip route ".to_string(),
                " ifconfig ".to_string(),
                " route add ".to_string(),
                " route del ".to_string(),
                " resolvectl ".to_string(),
                " iptables ".to_string(),
                " nft ".to_string(),
            ],
            secret_access: vec![
                " .env".to_string(),
                " id_rsa".to_string(),
                " id_ed25519".to_string(),
                " auth.json".to_string(),
                " credentials.json".to_string(),
                " .npmrc".to_string(),
                " .pypirc".to_string(),
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

    pub fn tool_catalog_block(&self, workspace_root: &Path) -> String {
        self.registry.tool_catalog_block(workspace_root)
    }

    pub fn tool_detail_block(&self, workspace_root: &Path) -> String {
        format!(
            "{}\n\
受治理只读记忆工具：memory_recall。仅可检索当前会话记忆；未配置会话 DB 或 session_id 时会返回结构化未配置结果，不会接外部知识库。\n\
ACTION: {{\"schema_version\":1,\"type\":\"tool_call\",\"call\":{{\"tool\":\"memory_recall\",\"query\":\"关键词\",\"limit\":3}}}}",
            self.registry.tool_detail_block(workspace_root)
        )
    }

    pub fn tool_instruction_block(&self, workspace_root: &Path) -> String {
        format!(
            "{}\n{}",
            self.tool_catalog_block(workspace_root),
            self.tool_detail_block(workspace_root)
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

    pub fn execute_or_reject_with_governance_and_ledger<L, G>(
        &self,
        ledger: &mut L,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        workspace_root: &Path,
        governance: &mut G,
        call: &ToolCall,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Result<GovernedToolExecutionRecord, String>
    where
        L: RuntimeEventLedger,
        G: Governance,
    {
        execute_tool_call_or_reject_with_registry_governance_and_ledger(
            ledger,
            thread_id,
            turn_id,
            workspace_root,
            governance,
            &self.registry,
            call,
            agent_id,
            task_id,
            &self.config,
        )
    }

    pub fn resume_approved_with_ledger<L, G>(
        &self,
        ledger: &mut L,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        workspace_root: &Path,
        governance: &mut G,
        pending: PendingApproval,
        approval: &OperatorApprovalReceipt,
    ) -> Result<GovernedToolExecutionRecord, String>
    where
        L: RuntimeEventLedger,
        G: Governance,
    {
        resume_pending_tool_call_with_registry_and_ledger(
            ledger,
            thread_id,
            turn_id,
            workspace_root,
            governance,
            &self.registry,
            pending,
            approval,
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
        Self::with_status(workspace_root, rounds, calls, "completed")
    }

    pub fn with_status(
        workspace_root: &Path,
        rounds: usize,
        calls: Vec<ToolExecutionRecord>,
        status: &str,
    ) -> Self {
        Self {
            schema_version: TOOL_LOOP_REPORT_SCHEMA_VERSION,
            status: status.to_string(),
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
        let desktop_browser_read_only_atomic_tools = registry
            .desktop_browser_read_only_atomic_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut callable_tools = mapped_atomic_tools.clone();
        callable_tools.push("list_dir".to_string());
        callable_tools.push("open_app".to_string());
        callable_tools.push("apply_patch".to_string());
        callable_tools.push("memory_recall".to_string());
        callable_tools.push("spawn_subagent".to_string());
        callable_tools.push("browser_read".to_string());
        callable_tools.push("browser_navigate".to_string());

        Self {
            available: true,
            governed: true,
            source: "GenericAgent".to_string(),
            workspace_root: workspace_root.display().to_string(),
            callable_tools,
            mapped_atomic_tools,
            desktop_browser_read_only_atomic_tools,
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
        // 容错：模型可能在说明文字后输出 "ACTION: {...}" 混合体，
        // 找不到前缀时先在全文中搜索 ACTION: 标记，提取其后 JSON 再解析。
        if let Some(pos) = trimmed.find("ACTION:") {
            let action_part = trimmed[pos + "ACTION:".len()..].trim_start();
            if action_part.starts_with('{') {
                return parse_action_json_prefix(action_part, trimmed);
            }
        }
        return Err(protocol_error(
            "missing_action_prefix",
            "ACTION payload must start with ACTION:",
            trimmed,
        ));
    };
    parse_action_json_prefix(json_text, trimmed)
}

fn parse_action_json_prefix(
    json_text: &str,
    raw: &str,
) -> Result<ToolActionEnvelope, ToolProtocolError> {
    if let Ok(envelope) = serde_json::from_str::<ToolActionEnvelope>(json_text) {
        return Ok(envelope);
    }
    let mut stream =
        serde_json::Deserializer::from_str(json_text).into_iter::<ToolActionEnvelope>();
    let envelope = stream
        .next()
        .transpose()
        .map_err(|error| {
            let (code, message) = if error.is_eof() {
                ("truncated_action_json", format!("ACTION payload truncated mid-JSON ({error}); retry with a shorter command on the next attempt"))
            } else {
                ("invalid_action_json", format!("ACTION payload is invalid or unsupported: {error}"))
            };
            protocol_error(code, &message, raw)
        })?
        .ok_or_else(|| protocol_error("invalid_action_json", "ACTION payload is empty", raw))?;
    let trailing = json_text[stream.byte_offset()..].trim();
    if trailing.is_empty() || is_recoverable_concatenated_tool_output(trailing) {
        Ok(envelope)
    } else {
        Err(protocol_error(
            "invalid_action_json",
            "ACTION payload has trailing text; output only one ACTION or FINAL per response",
            raw,
        ))
    }
}

fn is_recoverable_concatenated_tool_output(trailing: &str) -> bool {
    trailing.starts_with("FINAL:")
        || trailing.starts_with("ACTION:")
        || trailing.starts_with("TOOL_CALL:")
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
    if trimmed.starts_with("ACTION:") || (!trimmed.starts_with("FINAL:") && trimmed.contains("ACTION: {")) {
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

    // 兼容层：部分推理模型（如 deepseek-v4-flash 经 ccswitch）偶发输出
    // Anthropic/OpenAI 风格的 XML 工具调用（<antml:invoke>/<invoke>），
    // 而不是创要求的 ACTION JSON。这里识别并转换成正式 ToolCall，
    // 避免把“开始语 + XML 调用”当成最终答案导致一句话断流。
    if let Some(call) = parse_xml_tool_call(trimmed) {
        return ToolModelOutput::ToolCall(call);
    }
    if let Some(call) = parse_loose_colon_tool_call(trimmed) {
        return ToolModelOutput::ToolCall(call);
    }

    ToolModelOutput::PlainText(trimmed.to_string())
}

/// 解析 XML 风格工具调用，返回正式 ToolCall。支持两类常见输出：
/// 1. antml:invoke / invoke（参数用 parameter 标签）
/// 2. <ACTION><tool_call><name>..</name><args>{json}</args>..（参数用 name+args）
///    参数名做小范围映射（cmd -> command），数字/布尔参数自动转换，tasks 支持多行数组。
fn parse_xml_tool_call(body: &str) -> Option<ToolCall> {
    if let Some(call) = parse_xml_invoke_tool_call(body) {
        return Some(call);
    }
    parse_xml_named_args_tool_call(body)
}

fn parse_xml_invoke_tool_call(body: &str) -> Option<ToolCall> {
    let invoke_re = regex::Regex::new(r#"<(?:\w+:)?invoke\s+name=["']([^"']+)["'][^>]*>"#).ok()?;
    let tool_name = invoke_re.captures(body)?.get(1)?.as_str().to_string();

    let param_re = regex::Regex::new(
        r#"<(?:\w+:)?parameter\s+name=["']([^"']+)["'][^>]*>([\s\S]*?)</(?:\w+:)?parameter>"#,
    )
    .ok()?;
    let mut params: Vec<(String, serde_json::Value)> = Vec::new();
    for captures in param_re.captures_iter(body) {
        let Some(name) = captures.get(1) else {
            continue;
        };
        let Some(value) = captures.get(2) else {
            continue;
        };
        let raw = value.as_str().trim().to_string();
        params.push((
            name.as_str().to_string(),
            coerce_xml_param(name.as_str(), &raw),
        ));
    }
    if params.is_empty() {
        return None;
    }

    let mut object = serde_json::Map::new();
    object.insert("tool".to_string(), serde_json::Value::String(tool_name));
    for (name, value) in params {
        // 兼容常见别名：deepseek 系输出 code_execute 参数为 cmd。
        let key = if name == "cmd" {
            "command"
        } else {
            name.as_str()
        };
        object.insert(key.to_string(), value);
    }
    serde_json::from_value::<ToolCall>(serde_json::Value::Object(object)).ok()
}

/// <ACTION><tool_call><name>code_execute</name><args>{"command":"pwd"}</args></tool_call></ACTION>
fn parse_xml_named_args_tool_call(body: &str) -> Option<ToolCall> {
    let name_re = regex::Regex::new(r#"<(?:\w+:)?name>([^<]+)</(?:\w+:)?name>"#).ok()?;
    let args_re = regex::Regex::new(r#"<(?:\w+:)?args>([\s\S]*?)</(?:\w+:)?args>"#).ok()?;
    let tool_name = name_re.captures(body)?.get(1)?.as_str().trim().to_string();
    let args_raw = args_re.captures(body)?.get(1)?.as_str().trim().to_string();
    let mut args_value: serde_json::Map<String, serde_json::Value> = if args_raw.is_empty() {
        serde_json::Map::new()
    } else {
        serde_json::from_str(&args_raw).ok()?
    };
    if let Some(cmd) = args_value.remove("cmd") {
        args_value.insert("command".to_string(), cmd);
    }
    let mut object = serde_json::Map::new();
    object.insert("tool".to_string(), serde_json::Value::String(tool_name));
    for (key, value) in args_value {
        object.insert(key, value);
    }
    serde_json::from_value::<ToolCall>(serde_json::Value::Object(object)).ok()
}

/// 松散 key:value 风格工具调用，例如：
/// ACTION
/// tool_call: code_execute
/// arguments: {"command": "pwd"}
fn parse_loose_colon_tool_call(body: &str) -> Option<ToolCall> {
    let name_re =
        regex::Regex::new(r#"(?m)^\s*(?:tool_call|tool)\s*[:：]\s*([A-Za-z_][A-Za-z0-9_]*)"#)
            .ok()?;
    let tool_name = name_re.captures(body)?.get(1)?.as_str().trim().to_string();
    let args_re = regex::Regex::new(r#"(?m)^\s*arguments?\s*[:：]\s*(\{[\s\S]*\})"#).ok()?;
    let args_raw = args_re.captures(body)?.get(1)?.as_str().trim().to_string();
    let mut args_value: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&args_raw).ok()?;
    if let Some(cmd) = args_value.remove("cmd") {
        args_value.insert("command".to_string(), cmd);
    }
    let mut object = serde_json::Map::new();
    object.insert("tool".to_string(), serde_json::Value::String(tool_name));
    for (key, value) in args_value {
        object.insert(key, value);
    }
    serde_json::from_value::<ToolCall>(serde_json::Value::Object(object)).ok()
}

/// 把 XML 字符串参数按目标字段类型转成 JSON 值（数字/布尔），其余保持字符串。
fn coerce_xml_param(name: &str, raw: &str) -> serde_json::Value {
    match name {
        "x" | "y" | "millis" | "limit" | "timeout_ms" | "max_concurrency" | "token_budget" => raw
            .parse::<i64>()
            .map(|number| serde_json::Value::Number(number.into()))
            .unwrap_or_else(|_| serde_json::Value::String(raw.to_string())),
        "secret" => match raw.to_lowercase().as_str() {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            _ => serde_json::Value::String(raw.to_string()),
        },
        "tasks" => {
            let trimmed = raw.trim_start();
            if trimmed.starts_with('[') {
                serde_json::from_str::<serde_json::Value>(trimmed)
                    .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
            } else {
                let items: Vec<String> = raw
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect();
                if items.is_empty() {
                    serde_json::Value::String(raw.to_string())
                } else {
                    serde_json::Value::Array(
                        items.into_iter().map(serde_json::Value::String).collect(),
                    )
                }
            }
        }
        _ => serde_json::Value::String(raw.to_string()),
    }
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
        ToolCall::Mouse { x, y } => execute_mouse(registry, call, config.actuator.as_ref(), *x, *y),
        ToolCall::Keyboard { text, secret } => {
            execute_keyboard(registry, call, config.actuator.as_ref(), text, *secret)
        }
        ToolCall::Screenshot { target } => {
            execute_screenshot(registry, call, config.actuator.as_ref(), target)
        }
        ToolCall::Locate { target } => {
            execute_locate(registry, call, config.actuator.as_ref(), target)
        }
        ToolCall::OpenApp { app_name } => {
            execute_open_app(registry, call, config.actuator.as_ref(), app_name)
        }
        ToolCall::Wait { millis } => {
            execute_wait(registry, call, config.actuator.as_ref(), *millis)
        }
        ToolCall::HumanSuspend { reason, prompt } => {
            execute_human_suspend(registry, call, reason, prompt.as_deref())
        }
        ToolCall::ApplyPatch { patch } => {
            execute_apply_patch(workspace_root, registry, call, patch)
        }
        ToolCall::ShellExec { command, cwd } => execute_shell_exec(
            workspace_root,
            registry,
            call,
            command,
            cwd,
            config.shell_timeout_ms,
            config.shell_rtk_rewrite,
        ),
        ToolCall::MemoryRecall {
            query,
            session_id,
            limit,
        } => execute_memory_recall(registry, call, query, session_id, *limit, &config.memory),
        ToolCall::SpawnSubagent {
            task,
            tasks,
            agent_name,
            policy,
            token_budget,
            timeout_ms,
            max_concurrency,
        } => execute_spawn_subagent(
            workspace_root,
            registry,
            call,
            task,
            tasks.as_deref(),
            agent_name.as_deref(),
            policy.as_deref(),
            *token_budget,
            *timeout_ms,
            *max_concurrency,
            &config.subagent,
        ),
        ToolCall::BrowserRead {} => execute_browser_read(registry, call),
        ToolCall::BrowserNavigate { url } => execute_browser_navigate(registry, call, url),
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

pub fn execute_tool_call_with_governance_and_ledger<L, G>(
    ledger: &mut L,
    thread_id: impl Into<String>,
    turn_id: impl Into<String>,
    workspace_root: &Path,
    governance: &mut G,
    call: &ToolCall,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
    config: &ToolExecutionConfig,
) -> Result<GovernedToolExecutionRecord, String>
where
    L: RuntimeEventLedger,
    G: Governance,
{
    let thread_id = thread_id.into();
    let turn_id = turn_id.into();
    let agent_id = agent_id.into();
    let task_id = task_id.into();
    let call_id = tool_call_event_id(call, &agent_id, &task_id);

    ledger
        .append(
            RuntimeEvent::at(
                RuntimeEventKind::ToolStarted,
                thread_id.clone(),
                now_timestamp(),
            )
            .with_turn_id(turn_id.clone())
            .with_call_id(call_id.clone())
            .with_evidence_ref(format!("tool://{call_id}/started")),
        )
        .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;

    let registry = AtomicToolRegistry::generic_agent_mvp();
    let mut outcome = execute_tool_call_or_reject_with_registry_and_governance(
        workspace_root,
        governance,
        &registry,
        call,
        agent_id,
        task_id,
        config,
    );

    if let Ok(outcome) = &outcome {
        if let Some(pending) = &outcome.pending_approval {
            ledger
                .append(
                    RuntimeEvent::at(
                        RuntimeEventKind::ApprovalRequested,
                        thread_id,
                        now_timestamp(),
                    )
                    .with_turn_id(turn_id)
                    .with_call_id(call_id)
                    .with_risk_decision(
                        RuntimeRiskDecision::new(
                            pending.risk_decision.decision.clone(),
                            pending.risk_decision.reason.clone(),
                        )
                        .with_policy_ref(pending.policy_marker.clone()),
                    )
                    .with_evidence_ref(format!("approval://{}/requested", pending.approval_id)),
                )
                .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;
            return Ok(outcome.clone());
        }
    }
    if let Ok(governed) = &outcome {
        if is_permanent_tool_rejection(&governed.decision) {
            outcome = Err(tool_rejection_error(&governed.decision));
        }
    }

    let finished_event = match &outcome {
        Ok(outcome) => RuntimeEvent::at(
            RuntimeEventKind::ToolFinished,
            thread_id.clone(),
            now_timestamp(),
        )
        .with_turn_id(turn_id.clone())
        .with_call_id(call_id.clone())
        .with_risk_decision(RuntimeRiskDecision::new(
            risk_decision_label(&outcome.decision),
            risk_decision_reason(&outcome.decision),
        ))
        .with_evidence_ref(format!("tool://{call_id}/finished")),
        Err(_) => RuntimeEvent::at(RuntimeEventKind::ToolFinished, thread_id, now_timestamp())
            .with_turn_id(turn_id)
            .with_call_id(call_id.clone())
            .with_evidence_ref(format!("tool://{call_id}/finished")),
    };

    ledger
        .append(finished_event)
        .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;

    outcome
}

fn execute_tool_call_or_reject_with_registry_governance_and_ledger<L, G>(
    ledger: &mut L,
    thread_id: impl Into<String>,
    turn_id: impl Into<String>,
    workspace_root: &Path,
    governance: &mut G,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
    config: &ToolExecutionConfig,
) -> Result<GovernedToolExecutionRecord, String>
where
    L: RuntimeEventLedger,
    G: Governance,
{
    let thread_id = thread_id.into();
    let turn_id = turn_id.into();
    let agent_id = agent_id.into();
    let task_id = task_id.into();
    let call_id = tool_call_event_id(call, &agent_id, &task_id);

    ledger
        .append(
            RuntimeEvent::at(
                RuntimeEventKind::ToolStarted,
                thread_id.clone(),
                now_timestamp(),
            )
            .with_turn_id(turn_id.clone())
            .with_call_id(call_id.clone())
            .with_evidence_ref(format!("tool://{call_id}/started")),
        )
        .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;

    let outcome = execute_tool_call_or_reject_with_registry_and_governance(
        workspace_root,
        governance,
        registry,
        call,
        agent_id,
        task_id,
        config,
    );

    if let Ok(outcome) = &outcome {
        if let Some(pending) = &outcome.pending_approval {
            ledger
                .append(
                    RuntimeEvent::at(
                        RuntimeEventKind::ApprovalRequested,
                        thread_id,
                        now_timestamp(),
                    )
                    .with_turn_id(turn_id)
                    .with_call_id(call_id)
                    .with_risk_decision(
                        RuntimeRiskDecision::new(
                            pending.risk_decision.decision.clone(),
                            pending.risk_decision.reason.clone(),
                        )
                        .with_policy_ref(pending.policy_marker.clone()),
                    )
                    .with_evidence_ref(format!("approval://{}/requested", pending.approval_id)),
                )
                .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;
            return Ok(outcome.clone());
        }
    }

    let finished_event = match &outcome {
        Ok(outcome) => RuntimeEvent::at(
            RuntimeEventKind::ToolFinished,
            thread_id.clone(),
            now_timestamp(),
        )
        .with_turn_id(turn_id.clone())
        .with_call_id(call_id.clone())
        .with_risk_decision(RuntimeRiskDecision::new(
            risk_decision_label(&outcome.decision),
            risk_decision_reason(&outcome.decision),
        ))
        .with_evidence_ref(format!("tool://{call_id}/finished")),
        Err(_) => RuntimeEvent::at(RuntimeEventKind::ToolFinished, thread_id, now_timestamp())
            .with_turn_id(turn_id)
            .with_call_id(call_id.clone())
            .with_evidence_ref(format!("tool://{call_id}/finished")),
    };

    ledger
        .append(finished_event)
        .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;

    outcome
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

    Ok(GovernedToolExecutionRecord {
        decision,
        record,
        pending_approval: None,
    })
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

    if matches!(decision, RiskDecision::NeedsApproval { .. }) {
        audit_tool_rejection(
            governance,
            registry,
            call,
            agent_id.clone(),
            task_id.clone(),
            &decision,
        )?;
        let pending = build_pending_approval(
            workspace_root,
            registry,
            call,
            &agent_id,
            &task_id,
            &proposed,
            &decision,
        )?;
        let record = governance_pending_record(registry, call, &decision);
        return Ok(GovernedToolExecutionRecord {
            decision,
            record,
            pending_approval: Some(pending),
        });
    }

    if is_permanent_tool_rejection(&decision) {
        audit_tool_rejection(governance, registry, call, agent_id, task_id, &decision)?;
        let record = governance_rejected_record(registry, call, &decision);
        return Ok(GovernedToolExecutionRecord {
            decision,
            record,
            pending_approval: None,
        });
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

#[allow(clippy::too_many_arguments)]
fn resume_pending_tool_call_with_registry_and_ledger<L, G>(
    ledger: &mut L,
    thread_id: impl Into<String>,
    turn_id: impl Into<String>,
    workspace_root: &Path,
    governance: &mut G,
    registry: &AtomicToolRegistry,
    pending: PendingApproval,
    approval: &OperatorApprovalReceipt,
    config: &ToolExecutionConfig,
) -> Result<GovernedToolExecutionRecord, String>
where
    L: RuntimeEventLedger,
    G: Governance,
{
    let thread_id = thread_id.into();
    let turn_id = turn_id.into();
    validate_approval_receipt(&pending, approval)?;
    let call: ToolCall = serde_json::from_str(&pending.serialized_tool_call)
        .map_err(|_| "approval_pending_call_invalid".to_string())?;
    let proposed = proposed_action_for_tool_call_with_registry(
        workspace_root,
        registry,
        &call,
        &config.shell_risk_rules,
    );
    let current_decision = governance
        .classify(&proposed)
        .map_err(|e| format!("tool_governance_failed: {}", e.message))?;
    if !matches!(current_decision, RiskDecision::NeedsApproval { .. }) {
        return Err("approval_policy_no_longer_resumable".to_string());
    }
    let recalculated = build_pending_approval(
        workspace_root,
        registry,
        &call,
        &pending.agent_id,
        &pending.task_id,
        &proposed,
        &current_decision,
    )?;
    if recalculated.approval_id != pending.approval_id
        || recalculated.call_id != pending.call_id
        || recalculated.call_fingerprint != pending.call_fingerprint
        || recalculated.target_fingerprint != pending.target_fingerprint
        || recalculated.workspace_fingerprint != pending.workspace_fingerprint
        || recalculated.policy_marker != pending.policy_marker
    {
        return Err("approval_pending_revalidation_failed".to_string());
    }
    governance
        .verify_operator_approval(&OperatorApprovalEvidence {
            approval_id: approval.approval_id.clone(),
            operator_ref: approval.operator_ref.clone(),
            evidence_ref: approval.evidence_ref.clone(),
        })
        .map_err(|_| "operator_approval_not_authorized".to_string())?;
    persist_approval_consumption(workspace_root, &pending, approval)?;

    let decision = RiskDecision::Allowed {
        reason: format!("operator_approval_receipt:{}", approval.operator_ref),
    };
    let outcome = execute_allowed_tool_call_with_audit(
        workspace_root,
        governance,
        registry,
        &call,
        pending.agent_id.clone(),
        pending.task_id.clone(),
        config,
        decision,
    )?;
    let (resolution_reason, evidence_suffix) = if outcome.record.ok {
        ("operator approval receipt accepted", "resolved")
    } else {
        (
            "operator approval receipt accepted; tool execution failed",
            "failed",
        )
    };
    let call_id = pending.call_id.clone();
    let policy_marker = pending.policy_marker.clone();
    let approval_id = pending.approval_id.clone();
    ledger
        .append(
            RuntimeEvent::at(
                RuntimeEventKind::ApprovalResolved,
                thread_id.clone(),
                now_timestamp(),
            )
            .with_turn_id(turn_id.clone())
            .with_call_id(call_id.clone())
            .with_risk_decision(
                RuntimeRiskDecision::new("allowed", resolution_reason)
                    .with_policy_ref(policy_marker),
            )
            .with_evidence_ref(format!("approval://{approval_id}/{evidence_suffix}")),
        )
        .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;
    ledger
        .append(
            RuntimeEvent::at(RuntimeEventKind::ToolFinished, thread_id, now_timestamp())
                .with_turn_id(turn_id)
                .with_call_id(call_id.clone())
                .with_risk_decision(RuntimeRiskDecision::new(
                    risk_decision_label(&outcome.decision),
                    risk_decision_reason(&outcome.decision),
                ))
                .with_evidence_ref(format!("tool://{call_id}/finished")),
        )
        .map_err(|e| format!("tool_event_ledger_failed: {e}"))?;
    Ok(outcome)
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
        ToolCall::Mouse { .. } => "mouse",
        ToolCall::Keyboard { .. } => "keyboard",
        ToolCall::Screenshot { .. } => "screenshot",
        ToolCall::Locate { .. } => "locate",
        ToolCall::OpenApp { .. } => "open_app",
        ToolCall::Wait { .. } => "wait",
        ToolCall::HumanSuspend { .. } => "human_suspend",
        ToolCall::ApplyPatch { .. } => "apply_patch",
        ToolCall::ShellExec { .. } => "code_execute",
        ToolCall::MemoryRecall { .. } => "memory_recall",
        ToolCall::SpawnSubagent { .. } => "spawn_subagent",
        ToolCall::BrowserRead { .. } => "browser_read",
        ToolCall::BrowserNavigate { .. } => "browser_navigate",
    }
}

fn tool_call_event_id(call: &ToolCall, agent_id: &str, task_id: &str) -> String {
    format!("tool:{}:{}:{}", tool_call_name(call), agent_id, task_id)
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
    let redaction = redact_sensitive_text(path, &content);
    let truncated = truncate_text_with_flag(&redaction.text, 10_000);
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
    record.output_redacted = redaction.redacted;
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
    record.resolved_path = Some(path_to_display_string(&file));
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
    rtk_rewrite: bool,
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
    // RTK emits POSIX shell rewrites. Keep it enabled on Unix, but execute the
    // operator's original command through Windows PowerShell on Windows.
    let (run_command, rtk_applied) = apply_rtk_shell_rewrite(command, rtk_rewrite && cfg!(unix));
    #[cfg(unix)]
    let mut shell = {
        let mut command = Command::new("bash");
        command.arg("-lc");
        command
    };
    #[cfg(windows)]
    let mut shell = {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ]);
        command
    };
    shell
        .arg(&run_command)
        .current_dir(&cwd_path)
        .env("CHUANG_GOVERNED_TOOL_PROCESS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path_env_with_rtk() {
        shell.env("PATH", path);
    }
    let child = match shell.spawn() {
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

    let wait_outcome =
        wait_with_timeout_with_watchdog(child, timeout_ms, Some(TOOL_IDLE_WATCHDOG_WARN_MS));
    let watchdog_note = idle_watchdog_note(wait_outcome.warn_elapsed_ms);
    let output = match wait_outcome.output {
        Ok(output) => output,
        Err(error) => {
            return failed_record(
                registry,
                call,
                format!(
                    "shell_exec_wait_failed cwd={} error={e}{watchdog_note}",
                    cwd_path.display(),
                    e = error
                ),
            )
        }
    };
    let stdout_raw = String::from_utf8_lossy(&output.stdout);
    let stderr_raw = String::from_utf8_lossy(&output.stderr);
    let stdout_redaction = redact_sensitive_text("stdout", &stdout_raw);
    let stderr_redaction = redact_sensitive_text("stderr", &stderr_raw);
    let stdout = truncate_text_with_flag(&stdout_redaction.text, 8_000);
    let stderr = truncate_text_with_flag(&stderr_redaction.text, 4_000);
    let rtk_note = if rtk_applied {
        format!(
            " rtk_rewrite=true original={}",
            redact_sensitive_text("command", command).text
        )
    } else {
        String::new()
    };
    let mut record = success_record(
        registry,
        call,
        format!(
            "cwd={} status={:?}{rtk_note}{watchdog_note} stdout=\n{}\nstderr=\n{}",
            cwd_path.display(),
            output.status.code(),
            stdout.text,
            stderr.text
        ),
        None,
        false,
    );
    record.cwd = Some(path_to_display_string(&cwd_path));
    record.command = Some(redact_sensitive_text("command", &run_command).text);
    record.ok = output.status.success();
    record.stdout = Some(stdout.text);
    record.stderr = Some(stderr.text);
    record.output_bytes = Some(output.stdout.len());
    record.output_lines = Some(count_lines(&stdout_raw));
    record.stderr_bytes = Some(output.stderr.len());
    record.stderr_lines = Some(count_lines(&stderr_raw));
    record.exit_code = output.status.code();
    record.output_redacted = stdout_redaction.redacted || stderr_redaction.redacted;
    record.stdout_redacted = stdout_redaction.redacted;
    record.stderr_redacted = stderr_redaction.redacted;
    record.stdout_truncated = stdout.truncated;
    record.stderr_truncated = stderr.truncated;
    if !record.ok {
        record.failure_class = Some("exit_nonzero".to_string());
    }
    record
}

/// Discover `rtk` binary: `RTK_BIN`, `PATH`, then common install locations.
pub fn discover_rtk_bin() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("RTK_BIN") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            for name in ["rtk", "rtk.exe"] {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    for rel in [
        ".cargo/bin/rtk",
        ".cargo/bin/rtk.exe",
        ".local/bin/rtk",
        ".local/bin/rtk.exe",
    ] {
        let cand = home.join(rel);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn path_env_with_rtk() -> Option<OsString> {
    let rtk = discover_rtk_bin()?;
    let dir = rtk.parent()?.to_path_buf();
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = std::env::split_paths(&current).collect::<Vec<_>>();
    if !paths.iter().any(|path| path == &dir) {
        paths.insert(0, dir);
    }
    std::env::join_paths(paths).ok()
}

fn rtk_rewrite_env_disabled() -> bool {
    match std::env::var("CHUANG_SHELL_RTK_REWRITE") {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            matches!(v.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

/// Parse `rtk hook check` stdout into an optional rewritten command.
pub fn parse_rtk_hook_check_output(stdout: &str, original: &str) -> Option<String> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if line.starts_with("No rewrite for:") {
        return None;
    }
    let original = original.trim();
    if line == original {
        return None;
    }
    // Rewrites are `rtk …` or embed `rtk` mid-pipeline (`cd x && rtk ls`).
    if line.starts_with("rtk ") || line.contains(" rtk ") || line.contains("&& rtk ") {
        return Some(line.to_string());
    }
    None
}

/// When enabled and `rtk` is available, rewrite supported commands for compact output.
/// Returns `(command_to_run, rewritten)`.
pub fn apply_rtk_shell_rewrite(command: &str, enabled: bool) -> (String, bool) {
    if !enabled || rtk_rewrite_env_disabled() {
        return (command.to_string(), false);
    }
    let original = command.trim();
    if original.is_empty() {
        return (command.to_string(), false);
    }
    let Some(rtk) = discover_rtk_bin() else {
        return (command.to_string(), false);
    };
    let output = match Command::new(&rtk)
        .args(["hook", "check", "--agent", "claude"])
        .arg(original)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return (command.to_string(), false),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    match parse_rtk_hook_check_output(&stdout, original) {
        Some(rewritten) => (rewritten, true),
        None => (command.to_string(), false),
    }
}

fn execute_browser_read(registry: &AtomicToolRegistry, call: &ToolCall) -> ToolExecutionRecord {
    match ensure_cdp_browser_read_adapter() {
        Ok(adapter) => match adapter.read_current_page() {
            Ok(page) => {
                let output = format_browser_page_output(
                    &page.url,
                    &page.title,
                    &page.dom_text,
                    &page.source,
                    &page.read_at,
                );
                success_record(
                    registry,
                    call,
                    format!(
                        "browser_read ok title={} url={}",
                        compact_tool_preview(&page.title, 40),
                        compact_tool_preview(&page.url, 80)
                    ),
                    Some(output),
                    page.dom_text.chars().count() > 12_000,
                )
            }
            Err(err) => browser_failed_record(registry, call, &err),
        },
        Err(err) => browser_failed_record(registry, call, &err),
    }
}

fn execute_browser_navigate(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    url: &str,
) -> ToolExecutionRecord {
    match ensure_cdp_browser_read_adapter() {
        Ok(adapter) => match adapter.navigate_and_read(url) {
            Ok(page) => {
                let output = format_browser_page_output(
                    &page.url,
                    &page.title,
                    &page.dom_text,
                    &page.source,
                    &page.read_at,
                );
                success_record(
                    registry,
                    call,
                    format!(
                        "browser_navigate ok title={} url={}",
                        compact_tool_preview(&page.title, 40),
                        compact_tool_preview(&page.url, 80)
                    ),
                    Some(output),
                    page.dom_text.chars().count() > 12_000,
                )
            }
            Err(err) => browser_failed_record(registry, call, &err),
        },
        Err(err) => browser_failed_record(registry, call, &err),
    }
}

fn format_browser_page_output(
    url: &str,
    title: &str,
    dom_text: &str,
    source: &str,
    read_at: &str,
) -> String {
    format!("url: {url}\ntitle: {title}\nsource: {source}\nread_at: {read_at}\n\n{dom_text}")
}

fn browser_failed_record(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    err: &BrowserReadError,
) -> ToolExecutionRecord {
    let mut record = failed_record(registry, call, format!("{}: {}", err.code, err.message));
    record.failure_class = Some(err.code.clone());
    record.retryable = err.retryable;
    record
}

fn compact_tool_preview(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim().replace('\n', " ");
    if trimmed.chars().count() <= max_chars {
        return trimmed;
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
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

#[allow(clippy::too_many_arguments)]
fn execute_spawn_subagent(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    task: &str,
    tasks: Option<&[String]>,
    agent_name: Option<&str>,
    policy: Option<&str>,
    token_budget: Option<u16>,
    timeout_ms: Option<u64>,
    max_concurrency: Option<u8>,
    subagent: &Option<SubagentToolContext>,
) -> ToolExecutionRecord {
    let Some(subagent) = subagent else {
        return failed_record(
            registry,
            call,
            "subagent_runtime_unavailable: 子代理工具上下文未装配（不是权限拦截）。本机自检请用 code_execute 跑 chuang doctor 或 SKIP_LIVE=1 chuang field-accept".to_string(),
        );
    };

    let mut job_list: Vec<String> = tasks
        .unwrap_or(&[])
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if job_list.is_empty() && !task.trim().is_empty() {
        job_list.push(task.trim().to_string());
    }
    if job_list.is_empty() {
        return failed_record(registry, call, "subagent_task_empty".to_string());
    }
    // Safety cap: one tool call should not fork an unbounded number of workers.
    // 2026-08-11: raised 8 -> 32 per dispatch principle (maximize parallelism).
    if job_list.len() > 32 {
        job_list.truncate(32);
    }

    let policy = policy.unwrap_or("analyze").trim().to_ascii_lowercase();
    if !matches!(policy.as_str(), "analyze" | "execute") {
        return failed_record(
            registry,
            call,
            format!("subagent_policy_unsupported policy={policy}"),
        );
    }
    let agent_name = sanitize_subagent_name(agent_name.unwrap_or("worker"));
    let token_budget = token_budget.unwrap_or(2048).clamp(256, 16_384);
    let timeout_ms = timeout_ms.unwrap_or(300_000).clamp(30_000, 900_000);
    let concurrency = max_concurrency
        .map(|v| v as usize)
        .unwrap_or_else(|| job_list.len().min(4))
        .clamp(1, 32)
        .min(job_list.len());

    let run_suffix = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}-{}", std::process::id(), duration.as_nanos()),
        Err(error) => {
            return failed_record(registry, call, format!("subagent_clock_failed: {error}"))
        }
    };
    let queue_root = subagent.queue_root.join(&run_suffix);
    let config_path = subagent.config_path.display().to_string();
    let queue_root_text = queue_root.display().to_string();
    let runner_command = subagent.runner_command.display().to_string();
    let token_budget_text = token_budget.to_string();
    let timeout_text = timeout_ms.to_string();

    // Prepend dispatch-only worker brief (category C — not main-session doctrine dump).
    let wrapped_jobs: Vec<String> = job_list
        .iter()
        .map(|t| crate::norm_layer::wrap_task_for_worker(t))
        .collect();

    let mut run_ids: Vec<String> = Vec::new();
    let mut task_ids: Vec<String> = Vec::new();
    for (index, wrapped) in wrapped_jobs.iter().enumerate() {
        let task_id = format!("tool-subagent-{run_suffix}-{index}");
        let name = if job_list.len() == 1 {
            agent_name.clone()
        } else {
            format!("{agent_name}-{}", index + 1)
        };
        let dispatch_args = vec![
            "subagent".to_string(),
            "dispatch".to_string(),
            "--config".to_string(),
            config_path.clone(),
            "--subagent-queue-root".to_string(),
            queue_root_text.clone(),
            "--task".to_string(),
            wrapped.clone(),
            "--task-id".to_string(),
            task_id.clone(),
            "--agent-name".to_string(),
            name,
            "--policy".to_string(),
            policy.clone(),
            "--token-budget".to_string(),
            token_budget_text.clone(),
            "--idle-timeout-ms".to_string(),
            timeout_text.clone(),
            "--requires-capability".to_string(),
            subagent.worker_capability.clone(),
            "--json".to_string(),
        ];
        let dispatch_refs: Vec<&str> = dispatch_args.iter().map(String::as_str).collect();
        let dispatch =
            match run_subagent_cli_json(workspace_root, &dispatch_refs, timeout_ms, subagent) {
                Ok(value) => value,
                Err(error) => return failed_record(registry, call, error),
            };
        let run_id = dispatch
            .get("run_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if run_id.is_empty() {
            return failed_record(
                registry,
                call,
                format!("subagent_dispatch_missing_run_id index={index}"),
            );
        }
        run_ids.push(run_id);
        task_ids.push(task_id);
    }

    let max_runs_text = job_list.len().to_string();
    let concurrency_text = concurrency.to_string();
    let run_args = vec![
        "subagent",
        "run-loop",
        "--config",
        config_path.as_str(),
        "--subagent-queue-root",
        queue_root_text.as_str(),
        "--runner",
        "command",
        "--runner-command",
        runner_command.as_str(),
        "--allow-runner-command",
        runner_command.as_str(),
        "--capability",
        subagent.worker_capability.as_str(),
        "--max-runs",
        max_runs_text.as_str(),
        "--max-concurrency",
        concurrency_text.as_str(),
        "--approve-exec",
        "--json",
    ];
    let loop_timeout = timeout_ms
        .saturating_mul(job_list.len() as u64)
        .saturating_div(concurrency as u64)
        .saturating_add(60_000);
    let run = match run_subagent_cli_json(workspace_root, &run_args, loop_timeout, subagent) {
        Ok(value) => value,
        Err(error) => return failed_record(registry, call, error),
    };
    let ran = run
        .get("ran_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if ran < job_list.len() as u64 {
        return failed_record(
            registry,
            call,
            format!(
                "subagent_runner_incomplete expected={} ran={ran} concurrency={concurrency}",
                job_list.len()
            ),
        );
    }

    let mut results = Vec::new();
    let mut all_ok = true;
    for (index, run_id) in run_ids.iter().enumerate() {
        let collect_args = vec![
            "subagent",
            "collect",
            "--config",
            config_path.as_str(),
            "--subagent-queue-root",
            queue_root_text.as_str(),
            "--run-id",
            run_id.as_str(),
            "--json",
        ];
        let collected = match run_subagent_cli_json(workspace_root, &collect_args, 30_000, subagent)
        {
            Ok(value) => value,
            Err(error) => return failed_record(registry, call, error),
        };
        let accepted = collected
            .pointer("/report_admission/status")
            .and_then(enum_json_name)
            .as_deref()
            == Some("Accepted");
        let admission_reason_code = collected
            .pointer("/report_admission/reason_code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let admission_reason = collected
            .pointer("/report_admission/reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let report_status = collected
            .pointer("/report/status")
            .and_then(enum_json_name)
            .unwrap_or_else(|| "missing".to_string());
        let summary = collected
            .pointer("/report/summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("subagent returned no summary");
        let stdout_preview = collected
            .pointer("/report/stdout_preview")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let ok = accepted && report_status == "Success";
        if !ok {
            all_ok = false;
        }
        results.push(serde_json::json!({
            "run_id": run_id,
            "task_id": task_ids.get(index),
            "task_preview": redact_sensitive_text("subagent_task", &job_list[index]).text,
            "status": report_status,
            "admission": if accepted { "accepted" } else { "rejected" },
            "admission_reason_code": admission_reason_code,
            "admission_reason": redact_sensitive_text("subagent_admission_reason", &admission_reason).text,
            "ok": ok,
            "summary": redact_sensitive_text("subagent_summary", summary).text,
            "result_preview": redact_sensitive_text("subagent_output", stdout_preview).text,
        }));
    }

    let output = serde_json::json!({
        "worker_count": job_list.len(),
        "max_concurrency": concurrency,
        "worker_model": subagent.worker_model,
        "workspace_root": workspace_root.display().to_string(),
        "queue_root": queue_root_text,
        "results": results,
    })
    .to_string();

    if !all_ok {
        let failed: Vec<&serde_json::Value> = results
            .iter()
            .filter(|item| !item.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
            .collect();
        let first = failed.first().copied();
        let first_status = first
            .and_then(|item| item.get("status").and_then(|v| v.as_str()))
            .unwrap_or("unknown");
        let first_admission = first
            .and_then(|item| item.get("admission").and_then(|v| v.as_str()))
            .unwrap_or("unknown");
        let first_reason_code = first
            .and_then(|item| item.get("admission_reason_code").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim();
        let first_summary = first
            .and_then(|item| item.get("summary").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim();
        // Keep first= a single whitespace-free token so CLI humanizers can parse it.
        let first_summary = if first_summary.is_empty() {
            if !first_reason_code.is_empty() {
                first_reason_code.to_string()
            } else {
                first_status.to_string()
            }
        } else {
            first_summary
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("_")
                .chars()
                .take(96)
                .collect::<String>()
        };
        let short = format!(
            "subagent_batch_partial_failure workers={} failed={} concurrency={concurrency} first_status={first_status} first_admission={first_admission} first={first_summary}",
            job_list.len(),
            failed.len(),
        );
        return failed_record_with_output(registry, call, short, Some(output));
    }

    // 把每个工人的产出片段嵌进 summary，保证模型在主摘要里就能看到 worker 结果
    // （不依赖模型解析嵌套 output JSON；codex 输出格式不稳定时也能可靠回流）。
    let mut worker_summary_lines: Vec<String> = Vec::new();
    for (index, item) in results.iter().enumerate() {
        let result_preview = item
            .get("result_preview")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .take(180)
            .collect::<String>();
        let item_summary = item
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        let worker_text = if result_preview.is_empty() {
            item_summary.to_string()
        } else {
            result_preview
        };
        if !worker_text.is_empty() {
            worker_summary_lines.push(format!("#{} {}", index + 1, worker_text));
        }
    }
    let worker_summary_suffix = if worker_summary_lines.is_empty() {
        String::new()
    } else {
        format!("; workers=[{}]", worker_summary_lines.join(" | "))
    };

    let first = results.first();
    let first_run = first
        .and_then(|item| item.get("run_id").and_then(|v| v.as_str()))
        .unwrap_or("-");
    let first_summary = first
        .and_then(|item| item.get("summary").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim();
    let first_summary_token = if first_summary.is_empty() {
        "ok".to_string()
    } else {
        first_summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("_")
            .chars()
            .take(72)
            .collect::<String>()
    };
    success_record(
        registry,
        call,
        format!(
            "subagent_batch_completed workers={} concurrency={concurrency} admission=accepted first_run={first_run} first={first_summary_token}{worker_summary_suffix}",
            job_list.len(),
        ),
        Some(output),
        false,
    )
}

fn enum_json_name(value: &serde_json::Value) -> Option<String> {
    value.as_str().map(ToString::to_string).or_else(|| {
        value
            .as_object()
            .and_then(|object| object.keys().next().cloned())
    })
}

fn run_subagent_cli_json(
    workspace_root: &Path,
    args: &[&str],
    timeout_ms: u64,
    subagent: &SubagentToolContext,
) -> Result<serde_json::Value, String> {
    let child = Command::new(&subagent.executable_path)
        .args(args)
        .current_dir(workspace_root)
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .env("CHUANG_CODEX_RUNNER_WORKSPACE", workspace_root)
        .env("CHUANG_CODEX_RUNNER_MODEL", &subagent.worker_model)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("subagent_cli_spawn_failed: {error}"))?;
    let wait_outcome =
        wait_with_timeout_with_watchdog(child, timeout_ms, Some(TOOL_IDLE_WATCHDOG_WARN_MS));
    let watchdog_note = idle_watchdog_note(wait_outcome.warn_elapsed_ms);
    let output = wait_outcome
        .output
        .map_err(|error| format!("subagent_cli_wait_failed: {error}{watchdog_note}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "subagent_cli_failed exit_code={:?} stderr={}",
            output.status.code(),
            redact_sensitive_text("subagent_stderr", &stderr).text
        ));
    }
    serde_json::from_str(stdout.trim())
        .map_err(|error| format!("subagent_cli_json_invalid: {error}"))
}

fn sanitize_subagent_name(raw: &str) -> String {
    let name = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let name = name.trim_matches('-');
    if name.is_empty() {
        "worker".to_string()
    } else {
        name.chars().take(48).collect()
    }
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
        call: redacted_tool_call(call),
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
    failed_record_with_output(registry, call, summary, None)
}

fn failed_record_with_output(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    summary: String,
    output: Option<String>,
) -> ToolExecutionRecord {
    let mapping = registry.mapping_for_call(call);
    ToolExecutionRecord {
        call: redacted_tool_call(call),
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
        output_truncated: false,
        output_redacted: false,
        stdout_redacted: false,
        stderr_redacted: false,
        stdout_truncated: false,
        stderr_truncated: false,
    }
}

fn redacted_tool_call(call: &ToolCall) -> ToolCall {
    match call {
        ToolCall::WriteFile { path, content } => ToolCall::WriteFile {
            path: path.clone(),
            content: redact_sensitive_text(path, content).text,
        },
        ToolCall::Keyboard { text, secret } => ToolCall::Keyboard {
            text: if *secret {
                "[REDACTED]".to_string()
            } else {
                redact_sensitive_text("keyboard", text).text
            },
            secret: *secret,
        },
        ToolCall::ApplyPatch { patch } => ToolCall::ApplyPatch {
            patch: redact_sensitive_text("patch", patch).text,
        },
        ToolCall::ShellExec { command, cwd } => ToolCall::ShellExec {
            command: redact_sensitive_text("command", command).text,
            cwd: cwd.clone(),
        },
        ToolCall::SpawnSubagent {
            task,
            tasks,
            agent_name,
            policy,
            token_budget,
            timeout_ms,
            max_concurrency,
        } => ToolCall::SpawnSubagent {
            task: redact_sensitive_text("subagent_task", task).text,
            tasks: tasks.as_ref().map(|items| {
                items
                    .iter()
                    .map(|item| redact_sensitive_text("subagent_task", item).text)
                    .collect()
            }),
            agent_name: agent_name.clone(),
            policy: policy.clone(),
            token_budget: *token_budget,
            timeout_ms: *timeout_ms,
            max_concurrency: *max_concurrency,
        },
        _ => call.clone(),
    }
}

impl ToolExecutionRecord {
    pub fn auxiliary_success(
        call: &ToolCall,
        summary: impl Into<String>,
        output: Option<String>,
    ) -> Self {
        success_record(
            &AtomicToolRegistry::generic_agent_mvp(),
            call,
            summary.into(),
            output,
            false,
        )
    }

    pub fn auxiliary_failure(call: &ToolCall, summary: impl Into<String>) -> Self {
        failed_record(
            &AtomicToolRegistry::generic_agent_mvp(),
            call,
            summary.into(),
        )
    }

    fn with_paths(mut self, target_path: &str, resolved_path: &Path) -> Self {
        self.target_path = Some(target_path.to_string());
        self.resolved_path = Some(path_to_display_string(resolved_path));
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

fn governance_pending_record(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    decision: &RiskDecision,
) -> ToolExecutionRecord {
    let mut record = failed_record(registry, call, "tool_approval_pending".to_string());
    record.failure_class = Some("approval_pending".to_string());
    record.decision = Some(risk_decision_label(decision));
    record.retryable = true;
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

fn is_permanent_tool_rejection(decision: &RiskDecision) -> bool {
    matches!(
        decision,
        RiskDecision::Blocked { .. } | RiskDecision::DraftOnly { .. }
    )
}

fn build_pending_approval(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    agent_id: &str,
    task_id: &str,
    proposed: &ProposedAction,
    decision: &RiskDecision,
) -> Result<PendingApproval, String> {
    let serialized_tool_call =
        serde_json::to_string(call).map_err(|_| "approval_call_serialize_failed".to_string())?;
    if serialized_tool_call.len() > PENDING_APPROVAL_MAX_CALL_BYTES {
        return Err("approval_call_exceeds_bounded_receipt".to_string());
    }
    let call_id = tool_call_event_id(call, agent_id, task_id);
    let call_fingerprint = stable_fingerprint(&serialized_tool_call);
    let target_fingerprint = stable_fingerprint(&proposed.target);
    let workspace_fingerprint = stable_fingerprint(
        &fs::canonicalize(workspace_root)
            .map_err(|error| format!("approval_workspace_invalid: {error}"))?
            .display()
            .to_string(),
    );
    let policy_marker = stable_fingerprint(&format!(
        "{}|{:?}|{}|{}",
        proposed.action_id,
        proposed.kind,
        registry.mapping_for_call(call).audit_operation,
        risk_decision_label(decision)
    ));
    let approval_id = format!(
        "approval-{}",
        stable_fingerprint(&format!(
            "{call_id}|{call_fingerprint}|{target_fingerprint}|{workspace_fingerprint}|{policy_marker}"
        ))
        .trim_start_matches("sha256:")
    );
    Ok(PendingApproval {
        approval_id,
        call_id,
        agent_id: agent_id.to_string(),
        task_id: task_id.to_string(),
        serialized_tool_call,
        call_fingerprint,
        target_fingerprint,
        workspace_fingerprint,
        policy_marker,
        risk_decision: PendingRiskDecision {
            decision: "needs_approval".to_string(),
            reason: risk_decision_reason(decision),
        },
    })
}

fn validate_approval_receipt(
    pending: &PendingApproval,
    approval: &OperatorApprovalReceipt,
) -> Result<(), String> {
    if !approval.approved {
        return Err("operator_approval_denied".to_string());
    }
    if approval.operator_ref.trim().is_empty() {
        return Err("operator_approval_operator_ref_required".to_string());
    }
    if approval.evidence_ref.trim().is_empty() {
        return Err("operator_approval_evidence_ref_required".to_string());
    }
    if approval.approval_id != pending.approval_id
        || approval.call_id != pending.call_id
        || approval.call_fingerprint != pending.call_fingerprint
        || approval.target_fingerprint != pending.target_fingerprint
        || approval.workspace_fingerprint != pending.workspace_fingerprint
        || approval.policy_marker != pending.policy_marker
    {
        return Err("operator_approval_receipt_mismatch".to_string());
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ApprovalConsumptionMarker<'a> {
    approval_id: &'a str,
    call_id: &'a str,
    call_fingerprint: &'a str,
    operator_ref: &'a str,
    evidence_ref: &'a str,
    started_at: String,
}

fn persist_approval_consumption(
    workspace_root: &Path,
    pending: &PendingApproval,
    approval: &OperatorApprovalReceipt,
) -> Result<(), String> {
    let relative_path = format!(
        ".chuang/runtime/consumed-approvals/{}.json",
        pending.approval_id
    );
    let marker_path = resolve_workspace_path(workspace_root, &relative_path)?;
    let parent = marker_path
        .parent()
        .ok_or_else(|| "approval_consumption_parent_invalid".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("approval_consumption_store_unavailable: {error}"))?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = match options.open(&marker_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err("approval_already_consumed".to_string());
        }
        Err(error) => {
            return Err(format!("approval_consumption_store_unavailable: {error}"));
        }
    };
    let marker = ApprovalConsumptionMarker {
        approval_id: &pending.approval_id,
        call_id: &pending.call_id,
        call_fingerprint: &pending.call_fingerprint,
        operator_ref: &approval.operator_ref,
        evidence_ref: &approval.evidence_ref,
        started_at: now_timestamp(),
    };
    let bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|_| "approval_consumption_marker_serialize_failed".to_string())?;
    file.write_all(&bytes)
        .map_err(|error| format!("approval_consumption_marker_write_failed: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("approval_consumption_marker_sync_failed: {error}"))
}

fn stable_fingerprint(value: &str) -> String {
    let hash = Sha256::digest(value.as_bytes());
    format!("sha256:{hash:x}")
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
    } else if summary.contains("apply_patch_") {
        "write_failed"
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
    } else if summary.contains("subagent_runtime_unavailable") {
        "subagent_runtime_unavailable"
    } else if summary.contains("subagent_batch_partial_failure") {
        "subagent_batch_partial_failure"
    } else if summary.contains("subagent_runner_incomplete") {
        "subagent_runner_incomplete"
    } else if summary.contains("subagent_cli_failed")
        || summary.contains("subagent_cli_spawn_failed")
        || summary.contains("subagent_cli_wait_failed")
    {
        "subagent_cli_failed"
    } else if summary.contains("subagent_") {
        "subagent_failed"
    } else {
        "tool_failed"
    }
}

fn is_retryable_failure(failure_class: &str) -> bool {
    matches!(
        failure_class,
        "timeout"
            | "spawn_failed"
            | "subagent_batch_partial_failure"
            | "subagent_runner_incomplete"
            | "subagent_cli_failed"
    )
}

fn tool_action_kind(call: &ToolCall, shell_risk_rules: &ShellRiskRules) -> ActionKind {
    match call {
        ToolCall::ListDir { .. } | ToolCall::MemoryRecall { .. } | ToolCall::BrowserRead { .. } => {
            ActionKind::Observe
        }
        ToolCall::ReadFile { path } if is_secret_material_path(path) => ActionKind::SecretAccess,
        ToolCall::ReadFile { .. } => ActionKind::Observe,
        ToolCall::Keyboard { secret: true, .. } => ActionKind::SecretAccess,
        ToolCall::Mouse { .. }
        | ToolCall::Keyboard { secret: false, .. }
        | ToolCall::OpenApp { .. }
        | ToolCall::BrowserNavigate { .. } => ActionKind::LocalDesktopInteraction,
        ToolCall::Screenshot { .. }
        | ToolCall::Locate { .. }
        | ToolCall::Wait { .. }
        | ToolCall::HumanSuspend { .. } => ActionKind::Observe,
        ToolCall::WriteFile { .. } => ActionKind::LocalFileWrite,
        ToolCall::ApplyPatch { .. } => ActionKind::LocalFileWrite,
        ToolCall::ShellExec { command, .. } => {
            classify_shell_action_kind(command, shell_risk_rules)
        }
        ToolCall::SpawnSubagent { task, .. } => {
            let nested_kind = classify_shell_action_kind(task, shell_risk_rules);
            if nested_kind == ActionKind::ShellCommand {
                ActionKind::SubagentDispatch
            } else {
                nested_kind
            }
        }
    }
}

fn classify_shell_action_kind(command: &str, rules: &ShellRiskRules) -> ActionKind {
    let normalized = command.to_ascii_lowercase();
    let padded = format!(" {normalized} ");

    if contains_any_pattern(&padded, &rules.delete_or_cleanup) {
        return ActionKind::DeleteOrCleanup;
    }

    if contains_any_pattern(&padded, &rules.privilege_escalation) {
        return ActionKind::PrivilegeEscalation;
    }

    if contains_any_pattern(&padded, &rules.service_change) {
        return ActionKind::ServiceChange;
    }

    if command_reads_secret_environment(&normalized)
        || (contains_any_pattern(&padded, &rules.secret_access)
            && command_accesses_secret_material(&normalized))
    {
        return ActionKind::SecretAccess;
    }

    if is_external_commit_command(&normalized) {
        return ActionKind::ExternalSend;
    }

    if contains_any_pattern(&padded, &rules.network_change)
        || is_download_pipe_to_shell(&normalized)
    {
        return ActionKind::NetworkChange;
    }

    ActionKind::ShellCommand
}

fn command_reads_secret_environment(command: &str) -> bool {
    contains_any(command, &["printenv ", " env ", "export "])
        && contains_any(
            command,
            &[
                "_api_key",
                "_token",
                "_secret",
                "_password",
                "api_key",
                "access_token",
                "private_key",
            ],
        )
}

fn is_download_pipe_to_shell(command: &str) -> bool {
    let compact = command.split_whitespace().collect::<Vec<_>>().join(" ");
    (compact.contains("curl ") || compact.contains("wget "))
        && (compact.contains("| sh")
            || compact.contains("| bash")
            || compact.contains("| zsh")
            || compact.contains("| fish"))
}

fn is_external_commit_command(command: &str) -> bool {
    let compact = command.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.starts_with("git push")
        || compact.starts_with("ssh ")
        || compact.starts_with("scp ")
        || (compact.starts_with("rsync ") && compact.contains(':'))
        || compact.starts_with("npm publish")
        || compact.starts_with("cargo publish")
        || ((compact.starts_with("curl ") || compact.starts_with("wget "))
            && contains_any(
                &compact,
                &[
                    " -x post",
                    " --request post",
                    " --data",
                    " -d ",
                    " --form",
                    " -f ",
                    " --upload-file",
                    " -t ",
                ],
            ))
}

fn command_accesses_secret_material(command: &str) -> bool {
    let tokens = command
        .split_whitespace()
        .map(|token| {
            token.trim_matches(|character: char| {
                matches!(
                    character,
                    '\'' | '"' | '`' | ';' | ',' | '(' | ')' | '[' | ']'
                )
            })
        })
        .collect::<Vec<_>>();
    let program = tokens
        .first()
        .and_then(|token| token.rsplit('/').next())
        .unwrap_or_default();

    if matches!(program, "rg" | "grep") {
        let positional = tokens
            .iter()
            .skip(1)
            .filter(|token| !token.starts_with('-'))
            .copied()
            .collect::<Vec<_>>();
        return positional
            .iter()
            .skip(1)
            .any(|token| is_secret_material_path(token));
    }

    contains_any(
        command,
        &[
            "cat ", "head ", "tail ", "less ", "more ", "sed ", "awk ", "grep ", "rg ", "source ",
            ". ", "cp ", "scp ", "rsync ", "curl ", "wget ", "tar ", "zip ", "base64 ", "openssl ",
            "pbcopy", "xclip", "python ", "python3 ", "node ", "bash ", "sh ",
        ],
    ) || command.contains('<')
        || command.contains('>')
        || command.contains("export ")
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
        ToolCall::Mouse { x, y } => format!("actuator::mouse x={} y={}", x, y),
        ToolCall::Keyboard { text, secret } => {
            format!("actuator::keyboard secret={} bytes={}", secret, text.len())
        }
        ToolCall::Screenshot { target } => format!(
            "actuator::screenshot target={}",
            target.as_deref().unwrap_or("screen")
        ),
        ToolCall::Locate { target } => format!(
            "actuator::locate target={}",
            target.as_deref().unwrap_or("screen")
        ),
        ToolCall::OpenApp { app_name } => format!("actuator::open_app app={}", app_name.trim()),
        ToolCall::Wait { millis } => format!("actuator::wait millis={}", millis),
        ToolCall::HumanSuspend { reason, .. } => {
            format!("human::suspend reason={}", reason.trim())
        }
        ToolCall::ApplyPatch { patch } => format!(
            "{}::apply_patch bytes={}",
            workspace_root.display(),
            patch.len()
        ),
        ToolCall::ShellExec { command, cwd } => format!(
            "{}::{}",
            cwd.as_deref().unwrap_or(".").trim(),
            redact_sensitive_text("command", command).text.trim()
        ),
        ToolCall::MemoryRecall {
            query, session_id, ..
        } => format!(
            "memory::session={}::{}",
            session_id.as_deref().unwrap_or("<configured>"),
            redact_sensitive_text("memory_query", query).text.trim()
        ),
        ToolCall::SpawnSubagent {
            task,
            tasks,
            agent_name,
            policy,
            max_concurrency,
            ..
        } => {
            let n = tasks
                .as_ref()
                .map(|t| t.len())
                .filter(|n| *n > 0)
                .unwrap_or(if task.trim().is_empty() { 0 } else { 1 });
            format!(
                "{}::subagent name={} policy={} workers={} concurrency={} task={}",
                workspace_root.display(),
                agent_name.as_deref().unwrap_or("worker"),
                policy.as_deref().unwrap_or("analyze"),
                n,
                max_concurrency
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "auto".to_string()),
                redact_sensitive_text("subagent_task", task).text.trim()
            )
        }
        ToolCall::BrowserRead {} => "browser::read_current_page".to_string(),
        ToolCall::BrowserNavigate { url } => format!("browser::navigate url={}", url.trim()),
    }
}

fn tool_summary(call: &ToolCall) -> String {
    match call {
        ToolCall::ListDir { path } => format!("list_dir path={}", path.trim()),
        ToolCall::ReadFile { path } => format!("read_file path={}", path.trim()),
        ToolCall::WriteFile { path, content } => {
            format!("write_file path={} bytes={}", path.trim(), content.len())
        }
        ToolCall::Mouse { x, y } => format!("mouse x={} y={}", x, y),
        ToolCall::Keyboard { text, secret } => {
            format!("keyboard secret={} bytes={}", secret, text.len())
        }
        ToolCall::Screenshot { target } => format!(
            "screenshot target={}",
            target.as_deref().unwrap_or("screen")
        ),
        ToolCall::Locate { target } => {
            format!("locate target={}", target.as_deref().unwrap_or("screen"))
        }
        ToolCall::OpenApp { app_name } => format!("open_app app={}", app_name.trim()),
        ToolCall::Wait { millis } => format!("wait millis={}", millis),
        ToolCall::HumanSuspend { reason, prompt } => format!(
            "human_suspend reason={} prompt={}",
            reason.trim(),
            prompt.as_deref().unwrap_or("none").trim()
        ),
        ToolCall::ApplyPatch { patch } => {
            format!("apply_patch bytes={}", patch.len())
        }
        ToolCall::ShellExec { command, cwd } => format!(
            "code_execute cwd={} command={}",
            cwd.as_deref().unwrap_or(".").trim(),
            redact_sensitive_text("command", command).text.trim()
        ),
        ToolCall::MemoryRecall { query, limit, .. } => format!(
            "memory_recall query={} limit={}",
            redact_sensitive_text("memory_query", query).text.trim(),
            limit
                .map(|value| value.to_string())
                .unwrap_or_else(|| "default".to_string())
        ),
        ToolCall::SpawnSubagent {
            task,
            tasks,
            agent_name,
            policy,
            token_budget,
            timeout_ms,
            max_concurrency,
        } => {
            let n = tasks
                .as_ref()
                .map(|t| t.iter().filter(|s| !s.trim().is_empty()).count())
                .filter(|n| *n > 0)
                .unwrap_or(if task.trim().is_empty() { 0 } else { 1 });
            format!(
                "spawn_subagent name={} policy={} workers={} concurrency={} token_budget={} timeout_ms={} task={}",
                agent_name.as_deref().unwrap_or("worker"),
                policy.as_deref().unwrap_or("analyze"),
                n,
                max_concurrency
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "auto".to_string()),
                token_budget
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "2048".to_string()),
                timeout_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "300000".to_string()),
                redact_sensitive_text("subagent_task", task).text.trim()
            )
        }
        ToolCall::BrowserRead {} => "browser_read".to_string(),
        ToolCall::BrowserNavigate { url } => format!("browser_navigate url={}", url.trim()),
    }
}

fn target_path_from_call(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::ListDir { path }
        | ToolCall::ReadFile { path }
        | ToolCall::WriteFile { path, .. } => Some(path.clone()),
        ToolCall::Mouse { .. }
        | ToolCall::Keyboard { .. }
        | ToolCall::Screenshot { .. }
        | ToolCall::Locate { .. }
        | ToolCall::OpenApp { .. }
        | ToolCall::Wait { .. }
        | ToolCall::HumanSuspend { .. }
        | ToolCall::ApplyPatch { .. }
        | ToolCall::ShellExec { .. }
        | ToolCall::MemoryRecall { .. }
        | ToolCall::SpawnSubagent { .. }
        | ToolCall::BrowserRead { .. }
        | ToolCall::BrowserNavigate { .. } => None,
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
        ToolCall::ShellExec { command, .. } => Some(redact_sensitive_text("command", command).text),
        _ => None,
    }
}

fn build_actuator(config: Option<&ActuatorConfig>) -> Option<Box<dyn Actuator>> {
    match config? {
        ActuatorConfig::Fake => Some(Box::new(FakeActuator::new())),
        ActuatorConfig::Command(command) => Some(Box::new(CommandActuator::new(command.clone()))),
    }
}

fn execute_mouse(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    actuator: Option<&ActuatorConfig>,
    x: i32,
    y: i32,
) -> ToolExecutionRecord {
    let mut record = success_record(
        registry,
        call,
        format!("mouse x={} y={}", x, y),
        None,
        false,
    );
    let Some(mut actuator) = build_actuator(actuator) else {
        record.ok = false;
        record.failure_class = Some("actuator_unconfigured".to_string());
        return record;
    };
    if let Err(error) = actuator.click(ClickTarget::Coordinates { x, y }) {
        record.ok = false;
        record.failure_class = Some("actuator_failed".to_string());
        record.summary = format!("actuator_click_failed: {}", error.message);
    }
    record
}

fn execute_keyboard(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    actuator: Option<&ActuatorConfig>,
    text: &str,
    secret: bool,
) -> ToolExecutionRecord {
    let mut record = success_record(
        registry,
        call,
        format!("keyboard secret={} bytes={}", secret, text.len()),
        None,
        false,
    );
    let Some(mut actuator) = build_actuator(actuator) else {
        record.ok = false;
        record.failure_class = Some("actuator_unconfigured".to_string());
        return record;
    };
    let text_value = if secret {
        SecretOrPlainText::Secret {
            label: "keyboard_secret".to_string(),
        }
    } else {
        SecretOrPlainText::Plain(text.to_string())
    };
    if let Err(error) = actuator.input_text(InputTarget::Focused, text_value) {
        record.ok = false;
        record.failure_class = Some("actuator_failed".to_string());
        record.summary = format!("actuator_input_failed: {}", error.message);
    }
    record
}

fn execute_open_app(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    actuator: Option<&ActuatorConfig>,
    app_name: &str,
) -> ToolExecutionRecord {
    let mut record = success_record(
        registry,
        call,
        format!("open_app app={}", app_name.trim()),
        None,
        false,
    );
    let Some(mut actuator) = build_actuator(actuator) else {
        record.ok = false;
        record.failure_class = Some("actuator_unconfigured".to_string());
        return record;
    };
    match actuator.open_app(OpenAppRequest {
        app_name: app_name.trim().to_string(),
    }) {
        Ok(handle) => {
            record.output = Some(format!(
                "app_name={} handle_id={}",
                handle.app_name, handle.handle_id
            ));
            record.output_bytes = record.output.as_ref().map(|value| value.len());
            record.output_lines = record.output.as_ref().map(|value| count_lines(value));
        }
        Err(error) => {
            record.ok = false;
            record.failure_class = Some("actuator_failed".to_string());
            record.summary = format!("actuator_open_app_failed: {}", error.message);
        }
    }
    record
}

fn execute_screenshot(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    actuator: Option<&ActuatorConfig>,
    target: &Option<String>,
) -> ToolExecutionRecord {
    let mut record = success_record(
        registry,
        call,
        format!(
            "screenshot target={}",
            target.as_deref().unwrap_or("screen")
        ),
        None,
        false,
    );
    let Some(mut actuator) = build_actuator(actuator) else {
        record.ok = false;
        record.failure_class = Some("actuator_unconfigured".to_string());
        return record;
    };
    let screenshot_target = match target.as_deref() {
        Some(value) if !value.trim().is_empty() && value.trim() != "screen" => {
            ScreenshotTarget::Window(value.trim().to_string())
        }
        _ => ScreenshotTarget::Screen,
    };
    match actuator.screenshot(screenshot_target) {
        Ok(evidence_ref) => {
            record.output = Some(actuator_evidence_output(
                None,
                Some(&evidence_ref.uri),
                evidence_ref.audit_message.as_deref(),
            ));
            refresh_record_output_stats(&mut record);
        }
        Err(error) => {
            record.ok = false;
            record.failure_class = Some("actuator_failed".to_string());
            record.summary = format!("actuator_screenshot_failed: {}", error.message);
        }
    }
    record
}

fn execute_locate(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    actuator: Option<&ActuatorConfig>,
    target: &Option<String>,
) -> ToolExecutionRecord {
    let mut record = success_record(
        registry,
        call,
        format!("locate target={}", target.as_deref().unwrap_or("screen")),
        None,
        false,
    );
    let Some(mut actuator) = build_actuator(actuator) else {
        record.ok = false;
        record.failure_class = Some("actuator_unconfigured".to_string());
        return record;
    };
    let observe_target = match target.as_deref() {
        Some(value) if !value.trim().is_empty() && value.trim() != "screen" => {
            ObserveTarget::Window(value.trim().to_string())
        }
        _ => ObserveTarget::Screen,
    };
    match actuator.observe(observe_target) {
        Ok(observation) => {
            let evidence_uri = observation
                .evidence_ref
                .as_ref()
                .map(|evidence_ref| evidence_ref.uri.as_str());
            let audit_message = observation.audit_message.as_deref().or_else(|| {
                observation
                    .evidence_ref
                    .as_ref()
                    .and_then(|evidence_ref| evidence_ref.audit_message.as_deref())
            });
            record.output = Some(actuator_evidence_output(
                Some(&observation.summary),
                evidence_uri,
                audit_message,
            ));
            refresh_record_output_stats(&mut record);
        }
        Err(error) => {
            record.ok = false;
            record.failure_class = Some("actuator_failed".to_string());
            record.summary = format!("actuator_observe_failed: {}", error.message);
        }
    }
    record
}

fn actuator_evidence_output(
    summary: Option<&str>,
    evidence_uri: Option<&str>,
    audit_message: Option<&str>,
) -> String {
    serde_json::json!({
        "summary": summary,
        "evidence_uri": evidence_uri,
        "audit_message": audit_message,
    })
    .to_string()
}

fn refresh_record_output_stats(record: &mut ToolExecutionRecord) {
    record.output_bytes = record.output.as_ref().map(|value| value.len());
    record.output_lines = record.output.as_ref().map(|value| count_lines(value));
}

fn execute_wait(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    _actuator: Option<&ActuatorConfig>,
    millis: u64,
) -> ToolExecutionRecord {
    std::thread::sleep(std::time::Duration::from_millis(millis.min(50)));
    success_record(
        registry,
        call,
        format!("wait millis={}", millis),
        None,
        false,
    )
}

fn execute_human_suspend(
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    reason: &str,
    prompt: Option<&str>,
) -> ToolExecutionRecord {
    let prompt = prompt.unwrap_or("none").trim();
    let mut record = success_record(
        registry,
        call,
        format!("human_suspend reason={}", reason.trim()),
        Some(format!(
            "human_input_required reason={} prompt={}",
            reason.trim(),
            prompt
        )),
        false,
    );
    record.ok = false;
    record.failure_class = Some("human_input_required".to_string());
    record.retryable = false;
    record
}

fn execute_apply_patch(
    workspace_root: &Path,
    registry: &AtomicToolRegistry,
    call: &ToolCall,
    patch: &str,
) -> ToolExecutionRecord {
    let adapter = WorkspaceFileAdapter::new(workspace_root);
    let result = match adapter.apply_patch(patch) {
        Ok(result) => result,
        Err(error) => return failed_record(registry, call, error),
    };
    let mut record = success_record(
        registry,
        call,
        format!(
            "apply_patch changed_files={} operations={}",
            result.changed_files.len(),
            result.operation_count
        ),
        Some(result.diff_preview.clone()),
        result.diff_truncated,
    );
    record.changed_files = result.changed_files;
    record.write_diff_preview = Some(result.diff_preview);
    record.write_diff_truncated = result.diff_truncated;
    record.output_redacted = false;
    if !result.backup_paths.is_empty() {
        record.summary = format!(
            "{} backups={}",
            record.summary,
            result.backup_paths.join(",")
        );
    }
    record
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
    let normalized_candidate = resolve_candidate_preserving_existing_symlinks(&candidate)?;

    if !normalized_candidate.starts_with(&normalized_root) {
        return Err(format!(
            "path_outside_workspace path={} workspace_root={}",
            normalized_candidate.display(),
            normalized_root.display()
        ));
    }

    Ok(normalized_candidate)
}

#[allow(dead_code)] // 已被 wait_with_timeout_with_watchdog 取代；保留旧签名供回滚对照。
fn wait_with_timeout(
    child: std::process::Child,
    timeout_ms: u64,
) -> std::io::Result<std::process::Output> {
    wait_with_timeout_with_watchdog(child, timeout_ms, None).output
}

/// 空闲看门狗等待结果：`warn_elapsed_ms` 在跨过告警阈值时记录实际耗时，
/// 硬中断仍由 `timeout_ms`（各工具现有超时配置）决定。
struct WatchdogWaitOutcome {
    output: std::io::Result<std::process::Output>,
    warn_elapsed_ms: Option<u64>,
}

/// `wait_with_timeout` 的看门狗变体：`warn_ms` 阈值到达时打告警标记
/// （蓝本 §2.5 空闲看门狗 45s 告警；kill 沿用既有 timeout）。
fn wait_with_timeout_with_watchdog(
    mut child: std::process::Child,
    timeout_ms: u64,
    warn_ms: Option<u64>,
) -> WatchdogWaitOutcome {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(timeout_ms);
    let mut warn_elapsed_ms = None;
    loop {
        match child.try_wait() {
            Err(error) => {
                return WatchdogWaitOutcome {
                    output: Err(error),
                    warn_elapsed_ms,
                };
            }
            Ok(Some(_)) => {
                let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                if warn_elapsed_ms.is_none() {
                    warn_elapsed_ms = check_watchdog_warn(elapsed_ms, warn_ms);
                }
                return WatchdogWaitOutcome {
                    output: child.wait_with_output(),
                    warn_elapsed_ms,
                };
            }
            Ok(None) => {}
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            if warn_elapsed_ms.is_none() {
                warn_elapsed_ms = check_watchdog_warn(elapsed_ms, warn_ms);
            }
            let output = match child.wait_with_output() {
                Ok(output) => output,
                Err(error) => {
                    return WatchdogWaitOutcome {
                        output: Err(error),
                        warn_elapsed_ms,
                    };
                }
            };
            return WatchdogWaitOutcome {
                output: Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "shell_exec timed out after {timeout_ms}ms status={:?}",
                        output.status.code()
                    ),
                )),
                warn_elapsed_ms,
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn check_watchdog_warn(elapsed_ms: u64, warn_ms: Option<u64>) -> Option<u64> {
    match warn_ms {
        Some(threshold) if elapsed_ms >= threshold => Some(elapsed_ms),
        _ => None,
    }
}

fn idle_watchdog_note(warn_elapsed_ms: Option<u64>) -> String {
    match warn_elapsed_ms {
        Some(elapsed) => format!(" idle_watchdog_warned_after_ms={elapsed}"),
        None => String::new(),
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
        if let Some(line) = before {
            push_diff_line(&mut preview, '-', line);
            emitted += 1;
        }
        if let Some(line) = after {
            push_diff_line(&mut preview, '+', line);
            emitted += 1;
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

    let redaction = redact_sensitive_text(path, &preview);
    OptionalPreview {
        text: Some(redaction.text),
        truncated,
        redacted: redaction.redacted,
    }
}

fn push_diff_line(preview: &mut String, prefix: char, line: &str) {
    preview.push(prefix);
    preview.push_str(line);
    preview.push('\n');
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

#[cfg(test)]
mod rtk_rewrite_tests {
    use super::{apply_rtk_shell_rewrite, parse_rtk_hook_check_output};

    #[test]
    fn parse_rtk_check_output_detects_rewrite() {
        assert_eq!(
            parse_rtk_hook_check_output("rtk ls -la\n", "ls -la").as_deref(),
            Some("rtk ls -la")
        );
        assert_eq!(
            parse_rtk_hook_check_output("cd /tmp && rtk ls\n", "cd /tmp && ls").as_deref(),
            Some("cd /tmp && rtk ls")
        );
        assert_eq!(
            parse_rtk_hook_check_output("No rewrite for: echo hi\n", "echo hi"),
            None
        );
    }

    #[test]
    fn apply_rtk_respects_disable_flag() {
        let (cmd, applied) = apply_rtk_shell_rewrite("ls -la", false);
        assert_eq!(cmd, "ls -la");
        assert!(!applied);
    }

    #[test]
    fn apply_rtk_rewrites_when_available() {
        if super::discover_rtk_bin().is_none() {
            return;
        }
        // Avoid env pollution from parallel tests by not setting CHUANG_SHELL_RTK_REWRITE.
        let (cmd, applied) = apply_rtk_shell_rewrite("git status", true);
        if applied {
            assert!(
                cmd.contains("rtk"),
                "rewritten command should use rtk: {cmd}"
            );
        }
    }
}

#[cfg(test)]
mod subagent_context_builder_tests {
    use super::{
        build_subagent_tool_context, DEFAULT_SUBAGENT_WORKER_CAPABILITY,
        DEFAULT_SUBAGENT_WORKER_MODEL,
    };
    use crate::runtime_config::RuntimeConfig;
    use std::path::PathBuf;

    #[test]
    fn production_builder_yields_subagent_tool_context() {
        let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
        config.metadata.insert(
            "config_path".to_string(),
            "/tmp/chuang-agent-test-config.toml".to_string(),
        );
        config.subagent_queue.root = PathBuf::from("./data/subagent-queue");
        config.permission.workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let ctx = build_subagent_tool_context(&config);

        assert_eq!(
            ctx.config_path,
            PathBuf::from("/tmp/chuang-agent-test-config.toml")
        );
        assert_eq!(ctx.queue_root, PathBuf::from("./data/subagent-queue"));
        assert_eq!(ctx.worker_model, DEFAULT_SUBAGENT_WORKER_MODEL);
        assert_eq!(ctx.worker_capability, DEFAULT_SUBAGENT_WORKER_CAPABILITY);
        assert!(!ctx.executable_path.as_os_str().is_empty());
        // Prefer real checked-in runner when discoverable from workspace/manifest.
        let runner = ctx.runner_command.display().to_string();
        assert!(
            runner.contains("chuang-codex-runner.py"),
            "runner_command should point at codex runner: {runner}"
        );
    }

    #[test]
    fn production_builder_uses_env_worker_model_override() {
        let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
        // Safety: only assert override path when env is already set by the harness.
        // We do not mutate process env here (parallel test risk).
        let ctx = build_subagent_tool_context(&config);
        if let Ok(model) = std::env::var("CHUANG_CODEX_RUNNER_MODEL") {
            let trimmed = model.trim();
            if !trimmed.is_empty() {
                assert_eq!(ctx.worker_model, trimmed);
                return;
            }
        }
        assert_eq!(ctx.worker_model, DEFAULT_SUBAGENT_WORKER_MODEL);
    }
}

#[cfg(test)]
mod xml_tool_call_parser_tests {
    use super::{parse_tool_model_output, parse_xml_tool_call, ToolCall, ToolModelOutput};

    #[test]
    fn parses_antml_invoke_with_cmd_alias() {
        let body = r#"我先查一下目录。

<antml:invoke name="code_execute">
<antml:parameter name="cmd" string="true">git status --short</antml:parameter>
</antml:invoke>"#;
        let call = parse_xml_tool_call(body).expect("antml invoke should parse");
        match call {
            ToolCall::ShellExec { command, cwd } => {
                assert_eq!(command, "git status --short");
                assert_eq!(cwd, None);
            }
            other => panic!("expected code_execute, got {other:?}"),
        }
    }

    #[test]
    fn parses_openai_style_invoke_with_command() {
        let body = r#"<tool_calls>
<invoke name="list_dir">
<parameter name="path">src</parameter>
</invoke>
</tool_calls>"#;
        let call = parse_xml_tool_call(body).expect("openai invoke should parse");
        match call {
            ToolCall::ListDir { path } => assert_eq!(path, "src"),
            other => panic!("expected list_dir, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_model_output_accepts_xml_instead_of_plain_text() {
        let body = "我先看看文件。\n\n<antml:invoke name=\"file_read\">\n<antml:parameter name=\"path\">Cargo.toml</antml:parameter>\n</antml:invoke>";
        let parsed = parse_tool_model_output(body);
        assert!(
            matches!(parsed, ToolModelOutput::ToolCall(ToolCall::ReadFile { .. })),
            "xml tool call should become ToolCall, got {parsed:?}"
        );
    }

    #[test]
    fn coerces_numeric_and_boolean_params() {
        let body = r#"<antml:invoke name="wait">
<antml:parameter name="millis">250</antml:parameter>
</antml:invoke>"#;
        let call = parse_xml_tool_call(body).expect("wait invoke should parse");
        match call {
            ToolCall::Wait { millis } => assert_eq!(millis, 250),
            other => panic!("expected wait, got {other:?}"),
        }

        let body = r#"<antml:invoke name="keyboard">
<antml:parameter name="text">hi</antml:parameter>
<antml:parameter name="secret">true</antml:parameter>
</antml:invoke>"#;
        let call = parse_xml_tool_call(body).expect("keyboard invoke should parse");
        match call {
            ToolCall::Keyboard { text, secret } => {
                assert_eq!(text, "hi");
                assert!(secret);
            }
            other => panic!("expected keyboard, got {other:?}"),
        }
    }

    #[test]
    fn keeps_plain_text_when_no_xml_invoke() {
        let parsed = parse_tool_model_output("今天天气不错。");
        assert!(matches!(parsed, ToolModelOutput::PlainText(_)));
    }

    #[test]
    fn parses_action_xml_with_name_and_args() {
        let body = r#"<ACTION>
<tool_call>
<name>code_execute</name>
<args>{"command": "pwd"}</args>
</tool_call>
</ACTION>"#;
        let parsed = parse_tool_model_output(body);
        match parsed {
            ToolModelOutput::ToolCall(ToolCall::ShellExec { command, .. }) => {
                assert_eq!(command, "pwd");
            }
            other => panic!("expected code_execute, got {other:?}"),
        }
    }

    #[test]
    fn parses_action_xml_with_cmd_alias_in_args() {
        let body = r#"<ACTION><tool_call><name>code_execute</name><args>{"cmd": "ls -la"}</args></tool_call></ACTION>"#;
        let call = parse_xml_tool_call(body).expect("action xml should parse");
        match call {
            ToolCall::ShellExec { command, .. } => assert_eq!(command, "ls -la"),
            other => panic!("expected code_execute, got {other:?}"),
        }
    }

    #[test]
    fn parses_loose_colon_style_tool_call() {
        let body = "ACTION\ntool_call: code_execute\narguments: {\"command\": \"pwd\"}";
        let parsed = parse_tool_model_output(body);
        match parsed {
            ToolModelOutput::ToolCall(ToolCall::ShellExec { command, .. }) => {
                assert_eq!(command, "pwd");
            }
            other => panic!("expected code_execute, got {other:?}"),
        }
    }
}

#[test]
fn rejects_action_json_with_arbitrary_trailing_text() {
    // 完整 JSON 后跟任意解释文字必须保持协议错误，
    // 不能被逆推修复误判为合法 ACTION。
    let body = r#"ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"code_execute","command":"ls -la"}} 后面是一些解释文字"#;
    let error = parse_tool_action_envelope_result(body).expect_err("尾部解释文字应保持协议错误");
    assert_eq!(error.code, "invalid_action_json");
    assert!(error.message.contains("trailing text"));
}

#[test]
fn parses_action_embedded_after_prose() {
    let body = r#"好的，我先看一下。
ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"code_execute","command":"pwd"}}"#;
    let envelope = parse_tool_action_envelope_result(body).expect("说明文字+ACTION 应解析成功");
    let reparsed = serde_json::to_string(&envelope).unwrap();
    assert!(
        reparsed.contains("\"tool\":\"code_execute\""),
        "reparsed={reparsed}"
    );
}

#[cfg(test)]
mod idle_watchdog_tests {
    use super::{
        check_watchdog_warn, idle_watchdog_note, wait_with_timeout_with_watchdog,
        TOOL_IDLE_WATCHDOG_WARN_MS,
    };

    #[test]
    fn warn_threshold_follows_blueprint_45s() {
        assert_eq!(TOOL_IDLE_WATCHDOG_WARN_MS, 45_000);
    }

    #[test]
    fn check_watchdog_warn_marks_only_after_threshold() {
        assert_eq!(check_watchdog_warn(44_999, Some(45_000)), None);
        assert_eq!(check_watchdog_warn(45_000, Some(45_000)), Some(45_000));
        assert_eq!(check_watchdog_warn(90_000, Some(45_000)), Some(90_000));
        // 未配置告警阈值时永不告警。
        assert_eq!(check_watchdog_warn(120_000, None), None);
    }

    #[test]
    fn idle_watchdog_note_renders_only_when_warned() {
        assert!(idle_watchdog_note(None).is_empty());
        let note = idle_watchdog_note(Some(45_123));
        assert!(
            note.contains("idle_watchdog_warned_after_ms=45123"),
            "note={note}"
        );
    }

    #[test]
    fn wait_with_watchdog_marks_real_elapsed_for_slow_child_only() {
        // 快速命令：不告警。
        #[cfg(unix)]
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg("true")
            .spawn()
            .expect("spawn true");
        #[cfg(windows)]
        let child = std::process::Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("spawn true");
        let fast = wait_with_timeout_with_watchdog(child, 10_000, Some(45_000));
        assert!(fast.output.is_ok());
        assert_eq!(fast.warn_elapsed_ms, None, "fast child must not warn");

        // 慢命令（2s）vs 告警阈值 1s：按实际耗时（>=1000ms）告警，而非 0/差值。
        #[cfg(unix)]
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 2")
            .spawn()
            .expect("spawn sleep");
        #[cfg(windows)]
        let child = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 2"])
            .spawn()
            .expect("spawn sleep");
        let slow = wait_with_timeout_with_watchdog(child, 10_000, Some(1_000));
        assert!(slow.output.is_ok());
        let warned = slow
            .warn_elapsed_ms
            .expect("slow child must warn with 1s threshold");
        assert!(warned >= 1_000, "warned={warned} must be real elapsed");
    }
}
