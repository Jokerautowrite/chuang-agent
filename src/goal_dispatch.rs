use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{AgentId, TaskId, Timestamp};
use crate::goal_run::{GoalRun, GoalRunDiagnostics, GoalRunError, GoalWorkerPlan};
use crate::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use crate::subagent_report::{
    build_parent_context_handoff, ExecutionStatus, ParentContextHandoff, ReportAdmissionStatus,
    SubagentReport, SubagentReportValidator,
};
use crate::subagent_spawner::{
    ContextIsolation, QueuedSubagentSpawner, RunId, SpawnRequest, SubagentToolPolicy,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDispatchReceipt {
    pub goal_id: String,
    pub goal_objective: String,
    pub queue_root: String,
    pub goal_root: String,
    pub dispatch_count: usize,
    pub dispatch_diagnostics: GoalDispatchDiagnostics,
    pub dispatches: Vec<GoalWorkerDispatchReceipt>,
    pub dispatch_manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDispatchManifest {
    pub goal_id: String,
    pub goal_objective: String,
    pub goal_root: String,
    pub queue_root: String,
    pub parent_agent_id: String,
    pub created_at: String,
    pub dispatch_count: usize,
    pub dispatch_diagnostics: GoalDispatchDiagnostics,
    pub dispatches: Vec<GoalWorkerDispatchReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDispatchCollectionReceipt {
    pub goal_id: String,
    pub goal_objective: String,
    pub goal_root: String,
    pub queue_root: String,
    pub dispatch_count: usize,
    pub available_report_count: usize,
    pub missing_run_ids: Vec<String>,
    pub report_run_ids: Vec<String>,
    pub blocked_report_run_ids: Vec<String>,
    pub blocked_report_reasons: Vec<String>,
    pub completed_worker_ids: Vec<String>,
    pub report_summaries: Vec<String>,
    pub parent_context_handoffs: Vec<ParentContextHandoff>,
    #[serde(default)]
    pub handoff_query_summary: GoalDispatchHandoffSummary,
    pub ready_to_checkpoint: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_suggestion: Option<GoalCheckpointSuggestion>,
    pub manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalCheckpointSuggestion {
    pub summary: String,
    pub completed_worker_ids: Vec<String>,
    pub validation_notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDispatchHandoffSummary {
    #[serde(default)]
    pub parent_context_handoff_count: usize,
    #[serde(default)]
    pub parent_context_handoff_refs: Vec<String>,
    #[serde(default)]
    pub report_admission_ref_count: usize,
    #[serde(default)]
    pub report_admission_reason_codes: BTreeMap<String, usize>,
    #[serde(default)]
    pub report_admission_refs: Vec<GoalReportAdmissionRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalReportAdmissionRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission_id: Option<String>,
    pub report_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub admission_status: String,
    pub reason_code: String,
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDispatchDiagnostics {
    pub worker_scope_complete: bool,
    pub worker_validation_complete: bool,
    pub validation_plan_complete: bool,
    pub ready_to_dispatch: bool,
    pub incomplete_reasons: Vec<String>,
    pub goal_run_diagnostics: GoalRunDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalWorkerDispatchReceipt {
    pub worker_id: String,
    pub worker_objective: String,
    pub task_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub dispatch_path: String,
    pub write_scope_ids: Vec<String>,
    pub validation_checks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDispatchError {
    pub field: String,
    pub message: String,
}

pub fn dispatch_goal_run_to_queue(
    goal_run: &GoalRun,
    goal_root: impl Into<PathBuf>,
    queue: &FileSubagentQueue,
    parent_agent_id: impl Into<String>,
) -> Result<GoalDispatchReceipt, GoalDispatchError> {
    goal_run
        .validate()
        .map_err(goal_run_error_to_dispatch_error)?;

    let mut spawner = QueuedSubagentSpawner::new();
    let goal_root = goal_root.into();
    let parent_agent_id = parent_agent_id.into();
    let goal_id = goal_run.goal_spec.goal_id.clone();
    let goal_objective = goal_run.goal_spec.objective.clone();
    let queue_root = queue.queue_root().display().to_string();
    let dispatch_token = current_dispatch_token();

    let mut dispatches = Vec::with_capacity(goal_run.worker_plan.len());
    for (index, worker) in goal_run.worker_plan.iter().enumerate() {
        let ids = build_dispatch_ids(&goal_id, &worker.worker_id, &dispatch_token, index);
        let spawn = build_spawn_request(goal_run, worker, &parent_agent_id, &dispatch_token, index);
        spawner
            .spawn_with_ids(spawn, ids.run_id.clone(), ids.agent_id.clone())
            .map_err(dispatch_error_from_subagent)?;
        dispatches.push(GoalWorkerDispatchReceipt {
            worker_id: worker.worker_id.clone(),
            worker_objective: worker.objective.clone(),
            task_id: ids.task_id.0.clone(),
            run_id: ids.run_id.0.clone(),
            agent_id: ids.agent_id.0.clone(),
            dispatch_path: queue.dispatch_path(&ids.run_id).display().to_string(),
            write_scope_ids: worker.write_scope_ids.clone(),
            validation_checks: worker.validation_checks.clone(),
        });
    }

    queue
        .flush_pending_dispatches(&spawner)
        .map_err(dispatch_error_from_queue)?;

    let manifest = GoalDispatchManifest {
        goal_id: goal_id.clone(),
        goal_objective: goal_objective.clone(),
        goal_root: goal_root.display().to_string(),
        queue_root: queue_root.clone(),
        parent_agent_id: parent_agent_id.clone(),
        created_at: current_rfc3339_timestamp(),
        dispatch_count: dispatches.len(),
        dispatch_diagnostics: build_dispatch_diagnostics(goal_run),
        dispatches: dispatches.clone(),
    };
    let manifest_path = write_goal_dispatch_manifest(&goal_root, &manifest)?;

    Ok(GoalDispatchReceipt {
        goal_id,
        goal_objective,
        queue_root,
        goal_root: goal_root.display().to_string(),
        dispatch_count: dispatches.len(),
        dispatch_diagnostics: manifest.dispatch_diagnostics.clone(),
        dispatches,
        dispatch_manifest_path: manifest_path.display().to_string(),
    })
}

pub fn dispatch_goal_run(
    goal_run: &GoalRun,
    goal_root: impl Into<PathBuf>,
    queue_root: impl Into<PathBuf>,
    parent_agent_id: impl Into<String>,
) -> Result<GoalDispatchReceipt, GoalDispatchError> {
    let goal_root = goal_root.into();
    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(queue_root))
        .map_err(dispatch_error_from_queue)?;
    dispatch_goal_run_to_queue(goal_run, goal_root, &queue, parent_agent_id)
}

pub fn load_goal_dispatch_manifest(
    goal_root: impl Into<PathBuf>,
    goal_id: &str,
) -> Result<GoalDispatchManifest, GoalDispatchError> {
    let goal_root = goal_root.into();
    let path = goal_dispatch_manifest_path(&goal_root, goal_id)?;
    let payload = fs::read_to_string(&path).map_err(|error| GoalDispatchError {
        field: "goal_dispatch_manifest.path".to_string(),
        message: format!(
            "goal dispatch manifest read failed path={} error={error}",
            path.display()
        ),
    })?;
    let manifest = serde_json::from_str::<GoalDispatchManifest>(&payload).map_err(|error| {
        GoalDispatchError {
            field: "goal_dispatch_manifest.json".to_string(),
            message: format!(
                "goal dispatch manifest parse failed path={} error={error}",
                path.display()
            ),
        }
    })?;
    Ok(manifest)
}

pub fn collect_goal_dispatch_reports(
    goal_root: impl Into<PathBuf>,
    queue_root: impl Into<PathBuf>,
    goal_id: &str,
) -> Result<GoalDispatchCollectionReceipt, GoalDispatchError> {
    let goal_root = goal_root.into();
    let queue_root = queue_root.into();
    let manifest = load_goal_dispatch_manifest(&goal_root, goal_id)?;
    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
        .map_err(dispatch_error_from_queue)?;
    collect_goal_dispatch_reports_with_reader(&goal_root, &manifest, |run_id| {
        read_report_for_collect(&queue, run_id)
    })
}

pub fn collect_goal_dispatch_reports_read_only(
    goal_root: impl Into<PathBuf>,
    queue_root: impl Into<PathBuf>,
    goal_id: &str,
) -> Result<GoalDispatchCollectionReceipt, GoalDispatchError> {
    let goal_root = goal_root.into();
    let queue_root = queue_root.into();
    let manifest = load_goal_dispatch_manifest(&goal_root, goal_id)?;
    collect_goal_dispatch_reports_with_reader(&goal_root, &manifest, |run_id| {
        read_report_for_collect_read_only(&queue_root, run_id)
    })
}

enum GoalReportReadOutcome {
    Missing,
    Report(SubagentReport),
    Blocked(String),
}

fn collect_goal_dispatch_reports_with_reader<F>(
    goal_root: &Path,
    manifest: &GoalDispatchManifest,
    mut read_report: F,
) -> Result<GoalDispatchCollectionReceipt, GoalDispatchError>
where
    F: FnMut(&RunId) -> Result<GoalReportReadOutcome, GoalDispatchError>,
{
    let mut available_report_count = 0usize;
    let mut missing_run_ids = Vec::new();
    let mut report_run_ids = Vec::new();
    let mut blocked_report_run_ids = Vec::new();
    let mut blocked_report_reasons = Vec::new();
    let mut completed_worker_ids = Vec::new();
    let mut report_summaries = Vec::new();
    let mut parent_context_handoffs = Vec::new();
    let mut report_admission_refs = Vec::new();
    let mut report_admission_reason_codes = BTreeMap::new();

    for dispatch in &manifest.dispatches {
        let run_id = RunId(dispatch.run_id.clone());
        match read_report(&run_id)? {
            GoalReportReadOutcome::Report(report) => {
                available_report_count += 1;
                report_run_ids.push(dispatch.run_id.clone());
                if report.task_id.0 != dispatch.task_id
                    || report.agent_id.0 != dispatch.agent_id
                    || report
                        .parent_agent_id
                        .as_ref()
                        .map(|agent| agent.0.as_str())
                        != Some(manifest.parent_agent_id.as_str())
                {
                    blocked_report_run_ids.push(dispatch.run_id.clone());
                    blocked_report_reasons.push(format!(
                        "report identity does not match dispatch for run_id={}",
                        dispatch.run_id
                    ));
                    continue;
                }
                if report.status != ExecutionStatus::Success {
                    blocked_report_run_ids.push(dispatch.run_id.clone());
                    blocked_report_reasons.push(format!(
                        "report status is not success for run_id={}",
                        dispatch.run_id
                    ));
                    continue;
                }
                let admission = build_goal_collect_report_admission(&report)?;
                if admission.status != ReportAdmissionStatus::Accepted {
                    blocked_report_run_ids.push(dispatch.run_id.clone());
                    blocked_report_reasons.push(format!(
                        "report admission rejected for run_id={} reason_code={}",
                        dispatch.run_id, admission.reason_code
                    ));
                    continue;
                }
                let report_admission_ref = build_goal_report_admission_ref(&report, &admission);
                *report_admission_reason_codes
                    .entry(report_admission_ref.reason_code.clone())
                    .or_insert(0) += 1;
                report_admission_refs.push(report_admission_ref);
                completed_worker_ids.push(dispatch.worker_id.clone());
                parent_context_handoffs.push(build_parent_context_handoff(&report, &admission));
                report_summaries.push(report.summary);
            }
            GoalReportReadOutcome::Blocked(reason) => {
                available_report_count += 1;
                report_run_ids.push(dispatch.run_id.clone());
                blocked_report_run_ids.push(dispatch.run_id.clone());
                blocked_report_reasons.push(reason);
            }
            GoalReportReadOutcome::Missing => missing_run_ids.push(dispatch.run_id.clone()),
        }
    }

    let ready_to_checkpoint = manifest.dispatch_diagnostics.ready_to_dispatch
        && available_report_count == manifest.dispatch_count
        && missing_run_ids.is_empty()
        && blocked_report_run_ids.is_empty();
    let handoff_query_summary = GoalDispatchHandoffSummary {
        parent_context_handoff_count: parent_context_handoffs.len(),
        parent_context_handoff_refs: parent_context_handoff_refs(&parent_context_handoffs),
        report_admission_ref_count: report_admission_refs.len(),
        report_admission_reason_codes,
        report_admission_refs,
    };
    let checkpoint_suggestion = if ready_to_checkpoint {
        Some(build_checkpoint_suggestion(
            &manifest,
            &completed_worker_ids,
            &report_summaries,
        ))
    } else {
        None
    };

    Ok(GoalDispatchCollectionReceipt {
        goal_id: manifest.goal_id.clone(),
        goal_objective: manifest.goal_objective.clone(),
        goal_root: manifest.goal_root.clone(),
        queue_root: manifest.queue_root.clone(),
        dispatch_count: manifest.dispatch_count,
        available_report_count,
        missing_run_ids,
        report_run_ids,
        blocked_report_run_ids,
        blocked_report_reasons,
        completed_worker_ids,
        report_summaries,
        parent_context_handoffs,
        handoff_query_summary,
        ready_to_checkpoint,
        checkpoint_suggestion,
        manifest_path: goal_dispatch_manifest_path(goal_root, &manifest.goal_id)?
            .display()
            .to_string(),
    })
}

fn build_goal_collect_report_admission(
    report: &SubagentReport,
) -> Result<crate::subagent_report::ReportAdmission, GoalDispatchError> {
    let raw = serde_json::to_vec(report).map_err(|error| GoalDispatchError {
        field: "goal_dispatch_report.admission".to_string(),
        message: format!("goal dispatch report admission encode failed: {error}"),
    })?;
    Ok(SubagentReportValidator::default().admit_raw(
        &raw,
        AgentId("goal-collect-controller".to_string()),
        Timestamp(current_rfc3339_timestamp()),
    ))
}

fn build_goal_report_admission_ref(
    report: &SubagentReport,
    admission: &crate::subagent_report::ReportAdmission,
) -> GoalReportAdmissionRef {
    GoalReportAdmissionRef {
        admission_id: Some(format!(
            "goal-report-admission://{}/{}",
            report.agent_id.0, report.report_id.0
        )),
        report_id: admission
            .report_id
            .as_ref()
            .unwrap_or(&report.report_id)
            .0
            .clone(),
        task_id: admission
            .task_id
            .as_ref()
            .unwrap_or(&report.task_id)
            .0
            .clone(),
        agent_id: admission
            .agent_id
            .as_ref()
            .unwrap_or(&report.agent_id)
            .0
            .clone(),
        admission_status: match admission.status {
            ReportAdmissionStatus::Accepted => "Accepted".to_string(),
            ReportAdmissionStatus::Rejected => "Rejected".to_string(),
        },
        reason_code: admission.reason_code.clone(),
        evidence_ref: Some(format!(
            "report://{}/{}",
            report.agent_id.0, report.report_id.0
        )),
    }
}

fn parent_context_handoff_refs(handoffs: &[ParentContextHandoff]) -> Vec<String> {
    handoffs
        .iter()
        .map(|handoff| {
            handoff.provenance_ref.clone().unwrap_or_else(|| {
                format!("proposal_only:{}", handoff.admission_reason_code.as_str())
            })
        })
        .collect()
}

fn read_report_for_collect(
    queue: &FileSubagentQueue,
    run_id: &RunId,
) -> Result<GoalReportReadOutcome, GoalDispatchError> {
    let Some(payload) = queue
        .read_report_raw(run_id)
        .map_err(dispatch_error_from_queue)?
    else {
        return Ok(GoalReportReadOutcome::Missing);
    };
    match serde_json::from_slice::<SubagentReport>(&payload) {
        Ok(report) => Ok(GoalReportReadOutcome::Report(report)),
        Err(error) => Ok(GoalReportReadOutcome::Blocked(format!(
            "report parse failed for run_id={} error={error}",
            run_id.0
        ))),
    }
}

fn read_report_for_collect_read_only(
    queue_root: &Path,
    run_id: &RunId,
) -> Result<GoalReportReadOutcome, GoalDispatchError> {
    let path = queue_root
        .join("reports")
        .join(format!("{}.json", run_id.0));
    if !path.exists() {
        return Ok(GoalReportReadOutcome::Missing);
    }
    let payload = fs::read(&path).map_err(|error| GoalDispatchError {
        field: "goal_dispatch_report.path".to_string(),
        message: format!(
            "goal dispatch report read failed path={} error={error}",
            path.display()
        ),
    })?;
    match serde_json::from_slice::<SubagentReport>(&payload) {
        Ok(report) => Ok(GoalReportReadOutcome::Report(report)),
        Err(error) => Ok(GoalReportReadOutcome::Blocked(format!(
            "report parse failed path={} run_id={} error={error}",
            path.display(),
            run_id.0
        ))),
    }
}

fn build_spawn_request(
    goal_run: &GoalRun,
    worker: &GoalWorkerPlan,
    parent_agent_id: &str,
    dispatch_token: &str,
    index: usize,
) -> SpawnRequest {
    let mut metadata = BTreeMap::new();
    metadata.insert("source".to_string(), "goal-dispatch".to_string());
    metadata.insert("goal_id".to_string(), goal_run.goal_spec.goal_id.clone());
    metadata.insert(
        "goal_objective".to_string(),
        goal_run.goal_spec.objective.clone(),
    );
    metadata.insert("worker_id".to_string(), worker.worker_id.clone());
    metadata.insert("worker_objective".to_string(), worker.objective.clone());
    metadata.insert(
        "write_scope_ids".to_string(),
        worker.write_scope_ids.join(","),
    );
    metadata.insert(
        "validation_checks".to_string(),
        worker.validation_checks.join(" || "),
    );
    metadata.insert(
        "acceptance_checks".to_string(),
        goal_run.goal_spec.acceptance_checks.join(" || "),
    );
    metadata.insert("dispatch_token".to_string(), dispatch_token.to_string());
    metadata.insert("dispatch_index".to_string(), index.to_string());

    SpawnRequest {
        task_id: TaskId(format!(
            "goal-{}-{}-{}-{}-task",
            sanitize_identifier(&goal_run.goal_spec.goal_id),
            sanitize_identifier(&worker.worker_id),
            dispatch_token,
            index
        )),
        parent_agent_id: AgentId(parent_agent_id.to_string()),
        agent_name: format!(
            "goal-{}-{}",
            sanitize_identifier(&goal_run.goal_spec.goal_id),
            sanitize_identifier(&worker.worker_id)
        ),
        task: build_worker_task(goal_run, worker),
        tool_policy: SubagentToolPolicy::Execute,
        context_isolation: ContextIsolation::Forked {
            max_parent_tokens: 256,
        },
        token_budget: dispatch_token_token_budget(goal_run),
        idle_timeout_ms: 30_000,
        recursive_spawn: false,
        metadata,
    }
}

fn build_worker_task(goal_run: &GoalRun, worker: &GoalWorkerPlan) -> String {
    let goal_block = goal_run
        .goal_spec
        .render_context_block()
        .unwrap_or_else(|_| "GOAL_SPEC\nrender_error: invalid goal spec".to_string());

    format!(
        "{goal_block}\n\
WORKER\n\
worker_id: {}\n\
worker_objective: {}\n\
write_scope_ids: {}\n\
validation_checks:\n{}\n\
integration_policy: main_process_owns_integration={} workers_may_commit={} workers_may_touch_secrets={} require_worker_reports={}",
        worker.worker_id,
        worker.objective,
        worker.write_scope_ids.join(","),
        render_list(&worker.validation_checks),
        goal_run.integration_policy.main_process_owns_integration,
        goal_run.integration_policy.workers_may_commit,
        goal_run.integration_policy.workers_may_touch_secrets,
        goal_run.integration_policy.require_worker_reports
    )
}

fn dispatch_token_token_budget(goal_run: &GoalRun) -> u16 {
    goal_run
        .goal_spec
        .budget
        .max_tool_rounds
        .and_then(|rounds| u16::try_from(rounds).ok())
        .map(|rounds| rounds.saturating_mul(64).max(512))
        .unwrap_or(1024)
}

fn build_dispatch_ids(
    goal_id: &str,
    worker_id: &str,
    dispatch_token: &str,
    index: usize,
) -> DispatchIds {
    DispatchIds {
        run_id: RunId(format!(
            "goal-{}-{}-{}-{}-run",
            sanitize_identifier(goal_id),
            sanitize_identifier(worker_id),
            dispatch_token,
            index
        )),
        agent_id: AgentId(format!(
            "goal-{}-{}-{}-{}-agent",
            sanitize_identifier(goal_id),
            sanitize_identifier(worker_id),
            dispatch_token,
            index
        )),
        task_id: TaskId(format!(
            "goal-{}-{}-{}-{}-task",
            sanitize_identifier(goal_id),
            sanitize_identifier(worker_id),
            dispatch_token,
            index
        )),
    }
}

fn build_dispatch_diagnostics(goal_run: &GoalRun) -> GoalDispatchDiagnostics {
    let goal_run_diagnostics = goal_run.diagnostics();
    let worker_scope_complete = goal_run_diagnostics.worker_scope_complete;
    let worker_validation_complete = goal_run_diagnostics.worker_validation_complete;
    let validation_plan_complete = goal_run_diagnostics.validation_plan_complete;
    let ready_to_dispatch =
        worker_scope_complete && worker_validation_complete && validation_plan_complete;
    let incomplete_reasons = if ready_to_dispatch {
        Vec::new()
    } else {
        let mut reasons = Vec::new();
        if !worker_scope_complete {
            reasons.push("worker scopes do not cover declared disjoint scopes".to_string());
        }
        if !worker_validation_complete {
            reasons.push("one or more workers have no validation checks".to_string());
        }
        if !validation_plan_complete {
            reasons.push("validation plan has no runnable command".to_string());
        }
        reasons
    };

    GoalDispatchDiagnostics {
        worker_scope_complete,
        worker_validation_complete,
        validation_plan_complete,
        ready_to_dispatch,
        incomplete_reasons,
        goal_run_diagnostics,
    }
}

fn build_checkpoint_suggestion(
    manifest: &GoalDispatchManifest,
    completed_worker_ids: &[String],
    report_summaries: &[String],
) -> GoalCheckpointSuggestion {
    let validation_notes = manifest
        .dispatches
        .iter()
        .zip(report_summaries.iter())
        .map(|(dispatch, report_summary)| {
            format!(
                "report available for worker_id={} run_id={} summary={}",
                dispatch.worker_id, dispatch.run_id, report_summary
            )
        })
        .collect::<Vec<_>>();

    GoalCheckpointSuggestion {
        summary: format!(
            "checkpoint ready for goal_id={} workers={}",
            manifest.goal_id,
            if completed_worker_ids.is_empty() {
                "none".to_string()
            } else {
                completed_worker_ids.join(" | ")
            }
        ),
        completed_worker_ids: completed_worker_ids.to_vec(),
        validation_notes,
    }
}

fn write_goal_dispatch_manifest(
    goal_root: &Path,
    manifest: &GoalDispatchManifest,
) -> Result<PathBuf, GoalDispatchError> {
    fs::create_dir_all(goal_root).map_err(|error| GoalDispatchError {
        field: "goal_dispatch_manifest.root".to_string(),
        message: format!(
            "goal dispatch manifest root create failed path={} error={error}",
            goal_root.display()
        ),
    })?;
    let path = goal_dispatch_manifest_path(goal_root, &manifest.goal_id)?;
    let payload = serde_json::to_string_pretty(manifest).map_err(|error| GoalDispatchError {
        field: "goal_dispatch_manifest.json".to_string(),
        message: format!("goal dispatch manifest render failed: {error}"),
    })?;
    fs::write(&path, payload).map_err(|error| GoalDispatchError {
        field: "goal_dispatch_manifest.path".to_string(),
        message: format!(
            "goal dispatch manifest write failed path={} error={error}",
            path.display()
        ),
    })?;
    Ok(path)
}

fn goal_dispatch_manifest_path(
    goal_root: &Path,
    goal_id: &str,
) -> Result<PathBuf, GoalDispatchError> {
    Ok(goal_root.join(format!("{}.dispatch.json", sanitize_identifier(goal_id))))
}

struct DispatchIds {
    run_id: RunId,
    agent_id: AgentId,
    task_id: TaskId,
}

fn sanitize_identifier(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').to_string()
}

fn current_dispatch_token() -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{pid}-{nanos}")
}

fn current_rfc3339_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn render_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn goal_run_error_to_dispatch_error(error: GoalRunError) -> GoalDispatchError {
    GoalDispatchError {
        field: error.field,
        message: error.message,
    }
}

fn dispatch_error_from_subagent(
    error: crate::subagent_spawner::SubagentError,
) -> GoalDispatchError {
    GoalDispatchError {
        field: "subagent".to_string(),
        message: format!("{error:?}"),
    }
}

fn dispatch_error_from_queue(
    error: crate::subagent_queue::FileSubagentQueueError,
) -> GoalDispatchError {
    GoalDispatchError {
        field: "subagent_queue".to_string(),
        message: format!("{error:?}"),
    }
}
