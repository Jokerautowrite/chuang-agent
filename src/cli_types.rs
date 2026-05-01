use std::path::PathBuf;

use chuang_agent::common::{AgentId, TaskId};
use chuang_agent::control_intent::ControlIntentInput;
use chuang_agent::runtime_config::{ConfigSummary, RuntimeConfig};
use chuang_agent::subagent_report::SubagentReport;
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
    pub(crate) remember: bool,
    pub(crate) remember_identity: bool,
    pub(crate) dispatch_subagent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RememberedRecords {
    pub(crate) sqlite_record_id: Option<String>,
    pub(crate) identity_record_id: Option<String>,
    pub(crate) runtime_report_id: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentReportCliOutput {
    pub(crate) run_id: String,
    pub(crate) available: bool,
    pub(crate) report: Option<SubagentReport>,
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
    pub(crate) has_report: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentRunOnceCliRequest {
    pub(crate) options: CliOptions,
    pub(crate) output: ControlOutputFormat,
    pub(crate) runner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SubagentRunOnceCliOutput {
    pub(crate) runner: String,
    pub(crate) ran: bool,
    pub(crate) run_id: Option<String>,
    pub(crate) report_path: Option<String>,
    pub(crate) summary: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ConfigInitCliOutput {
    pub(crate) written: bool,
    pub(crate) path: String,
}
