//! `cli_types` 模块。内部实现模块（无公开顶层项）。

use std::path::PathBuf;

use chuang_agent::common::{AgentId, TaskId};
use chuang_agent::control_intent::ControlIntentInput;
use chuang_agent::genesis_actuator::GenesisAskResponse;
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::kernel_status::ChuangMvpStatus;
use chuang_agent::live_subagent_rehearsal::LiveSubagentRehearsalReport;
use chuang_agent::runtime_config::{ConfigSummary, RuntimeConfig};
use chuang_agent::subagent_report::{ReportAdmission, SubagentReport};
use chuang_agent::subagent_spawner::{RunId, SpawnRequest};
use serde::Serialize;

use crate::cli_output::ControlOutputFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOptions {
    pub(crate) runtime: RuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) user_input: String,
    pub(crate) workspace_root: Option<PathBuf>,
    pub(crate) remember: bool,
    pub(crate) session_id: Option<String>,
    pub(crate) remember_session: bool,
    pub(crate) conversation_history: Vec<ConversationHistoryItem>,
    pub(crate) remember_identity: bool,
    pub(crate) remember_experience: bool,
    pub(crate) dispatch_subagent: bool,
    pub(crate) goal_spec: Option<GoalSpec>,
    pub(crate) knowledge_context: Option<KnowledgeContextCliRequest>,
    pub(crate) live_guidance_path: Option<PathBuf>,
    pub(crate) progress_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConversationHistoryItem {
    pub(crate) role: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeContextCliRequest {
    pub(crate) root: PathBuf,
    pub(crate) query: String,
    pub(crate) limit: usize,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RememberedRecords {
    pub(crate) sqlite_record_id: Option<String>,
    pub(crate) session_record_id: Option<String>,
    pub(crate) diary_seq: Option<String>,
    pub(crate) identity_record_id: Option<String>,
    pub(crate) experience_record_id: Option<String>,
    pub(crate) runtime_report_id: Option<String>,
    pub(crate) governance_decision: Option<String>,
    pub(crate) subagent_dispatch_run_id: Option<String>,
    pub(crate) subagent_dispatch_agent_id: Option<String>,
    pub(crate) subagent_dispatch_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControlApplyCliRequest {
    pub(crate) intent: ControlIntentInput,
    pub(crate) approve: bool,
    pub(crate) output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentDispatchCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
    pub(crate) task_id: TaskId,
    pub(crate) spawn: SpawnRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentDispatchCliOutput {
    pub(crate) run_id: String,
    pub(crate) agent_id: String,
    pub(crate) task_id: String,
    pub(crate) dispatch_path: String,
    pub(crate) queue_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliSubagentIds {
    pub(crate) run_id: RunId,
    pub(crate) agent_id: AgentId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentReportCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
    pub(crate) run_id: RunId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentReleaseClaimCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
    pub(crate) run_id: RunId,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentReportCliOutput {
    pub(crate) run_id: String,
    pub(crate) available: bool,
    pub(crate) report: Option<SubagentReport>,
    pub(crate) report_admission: Option<ReportAdmission>,
    pub(crate) parent_context_handoff: Option<chuang_agent::subagent_report::ParentContextHandoff>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentReleaseClaimCliOutput {
    pub(crate) run_id: String,
    pub(crate) released: bool,
    pub(crate) release_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentCollectCliOutput {
    pub(crate) run_id: String,
    pub(crate) dispatch_available: bool,
    pub(crate) report_available: bool,
    pub(crate) report: Option<SubagentReport>,
    pub(crate) report_admission: Option<ReportAdmission>,
    pub(crate) parent_context_handoff: Option<chuang_agent::subagent_report::ParentContextHandoff>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentListCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentListCliOutput {
    pub(crate) queue_root: String,
    pub(crate) dispatch_count: usize,
    pub(crate) report_count: usize,
    pub(crate) items: Vec<SubagentListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentListItem {
    pub(crate) run_id: String,
    pub(crate) agent_id: String,
    pub(crate) task_id: String,
    pub(crate) agent_name: String,
    pub(crate) tool_policy: String,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) is_claimed: bool,
    pub(crate) is_claim_stale: bool,
    pub(crate) has_report: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentRunOnceCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
    pub(crate) runner: String,
    pub(crate) runner_command: Option<String>,
    pub(crate) runner_args: Vec<String>,
    pub(crate) worker_capabilities: Vec<String>,
    pub(crate) approve_exec: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentRunLoopCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
    pub(crate) runner: String,
    pub(crate) runner_command: Option<String>,
    pub(crate) runner_args: Vec<String>,
    pub(crate) worker_capabilities: Vec<String>,
    pub(crate) approve_exec: bool,
    pub(crate) max_runs: usize,
    pub(crate) max_concurrency: usize,
    pub(crate) require_live_gate: bool,
    pub(crate) allowed_runner_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentLivePreflightCliRequest {
    pub(crate) output: ControlOutputFormat,
    pub(crate) runner: String,
    pub(crate) runner_command: String,
    pub(crate) allowed_runner_commands: Vec<String>,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) worker_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentLivePreflightCliOutput {
    pub(crate) rehearsal: LiveSubagentRehearsalReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentRunOnceCliOutput {
    pub(crate) runner: String,
    pub(crate) evolution_kind: String,
    pub(crate) evolution_source: String,
    pub(crate) worker_capabilities: Vec<String>,
    pub(crate) ran: bool,
    pub(crate) run_id: Option<String>,
    pub(crate) report_path: Option<String>,
    pub(crate) report_admission: Option<ReportAdmission>,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentRunLoopCliOutput {
    pub(crate) runner: String,
    pub(crate) evolution_kind: String,
    pub(crate) evolution_source: String,
    pub(crate) worker_capabilities: Vec<String>,
    pub(crate) max_runs: usize,
    pub(crate) max_concurrency: usize,
    pub(crate) ran_count: usize,
    pub(crate) idle: bool,
    pub(crate) run_ids: Vec<String>,
    pub(crate) report_paths: Vec<String>,
    pub(crate) report_admissions: Vec<ReportAdmission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigCheckCliOutput {
    pub(crate) ok: bool,
    pub(crate) source: String,
    pub(crate) summary: ConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigInitCliRequest {
    pub(crate) output: ControlOutputFormat,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenesisAskCliRequest {
    pub(crate) output: ControlOutputFormat,
    pub(crate) prompt: String,
    pub(crate) program: String,
    pub(crate) profile_dir: PathBuf,
    pub(crate) cdp_port: u16,
    pub(crate) timeout_ms: u64,
    pub(crate) approve_exec: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GenesisAskCliOutput {
    pub(crate) response: GenesisAskResponse,
    pub(crate) governance_decision: String,
    pub(crate) audit_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GenesisDryRunCliOutput {
    pub(crate) primary: chuang_agent::genesis_actuator::GenesisCommandSpec,
    pub(crate) fallback: chuang_agent::genesis_actuator::GenesisCommandSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigInitCliOutput {
    pub(crate) written: bool,
    pub(crate) path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DoctorCliOutput {
    pub(crate) ok: bool,
    pub(crate) checks: Vec<DoctorCheck>,
    pub(crate) status: ChuangMvpStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DoctorCheck {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}
