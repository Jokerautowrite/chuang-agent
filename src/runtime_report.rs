use std::collections::BTreeMap;

use crate::agent_runtime::{ContextDebugInfo, RuntimeResult};
use crate::subagent_report::{
    ExecutionStatus, ReportBuilder, RuntimeReportInput, SubagentReport, SubagentReportBuilder,
};

pub fn build_runtime_report(
    result: &RuntimeResult,
    report_id: impl Into<String>,
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    parent_agent_id: Option<String>,
) -> SubagentReport {
    SubagentReportBuilder::from_runtime(RuntimeReportInput {
        report_id: report_id.into(),
        task_id: task_id.into(),
        agent_id: agent_id.into(),
        parent_agent_id,
        summary: build_summary(result),
        response_body: result.response.body.clone(),
        response_trace: result.response.trace.clone(),
        dropped_segment_ids: result.dropped_segment_ids.clone(),
        drop_reasons: map_drop_reasons(&result.context_debug),
        budget_exceeded: result.context_debug.budget_exceeded,
        budget_exceeded_reasons: map_budget_exceeded_reasons(&result.context_debug),
        working_reservation: map_working_reservation(&result.context_debug),
    })
    .build()
}

fn build_summary(result: &RuntimeResult) -> String {
    format!(
        "model={} recall_hits={} packed_tokens={}",
        result.response.model_name, result.recall_hit_count, result.packed_token_count
    )
}

fn map_drop_reasons(debug: &ContextDebugInfo) -> Vec<(String, String)> {
    debug
        .drop_reasons
        .iter()
        .map(|reason| {
            (
                reason.segment_id.clone(),
                reason.reason.as_str().to_string(),
            )
        })
        .collect()
}

fn map_budget_exceeded_reasons(debug: &ContextDebugInfo) -> Vec<String> {
    debug
        .budget_exceeded_reasons
        .iter()
        .map(|reason| reason.as_str().to_string())
        .collect()
}

fn map_working_reservation(
    debug: &ContextDebugInfo,
) -> Option<crate::subagent_report::WorkingReservationDebug> {
    debug.working_reservation.as_ref().map(|reservation| {
        crate::subagent_report::WorkingReservationDebug {
            reserved_segment_id: reservation.reserved_segment_id.clone(),
            reserved_tokens: reservation.reserved_tokens,
            dropped_segment_ids: reservation.dropped_segment_ids.clone(),
            reason: reservation.reason.as_str().to_string(),
        }
    })
}

pub fn report_metadata(report: &SubagentReport) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("schema_version".to_string(), report.schema_version.clone());
    metadata.insert(
        "status".to_string(),
        match report.status {
            ExecutionStatus::Success => "Success".to_string(),
            ExecutionStatus::Failed => "Failed".to_string(),
            ExecutionStatus::TimedOut => "TimedOut".to_string(),
            ExecutionStatus::Cancelled => "Cancelled".to_string(),
        },
    );
    metadata.insert("summary".to_string(), report.summary.clone());
    metadata.insert("report_id".to_string(), report.report_id.0.clone());
    metadata.insert("task_id".to_string(), report.task_id.0.clone());
    metadata.insert("agent_id".to_string(), report.agent_id.0.clone());
    if let Some(parent) = &report.parent_agent_id {
        metadata.insert("parent_agent_id".to_string(), parent.0.clone());
    }
    metadata
}
