use std::collections::BTreeMap;

use crate::agent_runtime::{ContextDebugInfo, RuntimeResult};
use crate::subagent_report::{
    governance_metadata, ArtifactKind, ArtifactRef, ExecutionStatus, ReportBuilder,
    RuntimeReportInput, SubagentReport, SubagentReportBuilder,
};
use crate::tool_runtime::ToolLoopReport;

pub fn build_runtime_report(
    result: &RuntimeResult,
    report_id: impl Into<String>,
    task_id: impl Into<String>,
    agent_id: impl Into<String>,
    parent_agent_id: Option<String>,
) -> SubagentReport {
    let mut report = SubagentReportBuilder::from_runtime(RuntimeReportInput {
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
    .build();

    report.artifacts.extend(tool_report_artifacts(result));
    report.artifacts.extend(tool_events_artifacts(result));
    report
        .artifacts
        .extend(runtime_observability_artifacts(result));
    report
}

fn build_summary(result: &RuntimeResult) -> String {
    let mut summary = format!(
        "model={} recall_hits={} packed_tokens={}",
        result.response.model_name, result.recall_hit_count, result.packed_token_count
    );
    if let Some(tool_call_count) = result.response.meta.extra.get("tool_call_count") {
        summary.push_str(&format!(" tool_calls={tool_call_count}"));
    }
    if let Some(protocol_error_count) = result.response.meta.extra.get("tool_protocol_error_count")
    {
        summary.push_str(&format!(" tool_protocol_errors={protocol_error_count}"));
    }
    summary
}

fn tool_report_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let Some(raw_report) = result.response.meta.extra.get("tool_report_json") else {
        return Vec::new();
    };

    let description = serde_json::from_str::<ToolLoopReport>(raw_report)
        .map(|report| {
            format!(
                "tool_loop status={} calls={} rounds={} workspace={}",
                report.status, report.call_count, report.rounds, report.workspace_root
            )
        })
        .unwrap_or_else(|_| "tool_loop report present but could not be parsed".to_string());

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.tool_report_json".to_string(),
        description: Some(description),
    }]
}

fn tool_events_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let Some(raw_events) = result.response.meta.extra.get("tool_events_json") else {
        return Vec::new();
    };

    let description = serde_json::from_str::<Vec<serde_json::Value>>(raw_events)
        .map(|events| {
            let tool_call_count = events
                .iter()
                .filter(|event| {
                    event.get("kind").and_then(|value| value.as_str()) == Some("tool_call")
                })
                .count();
            let protocol_error_count = events
                .iter()
                .filter(|event| {
                    event.get("kind").and_then(|value| value.as_str()) == Some("protocol_error")
                })
                .count();
            format!(
                "tool_events count={} tool_calls={} protocol_errors={}",
                events.len(),
                tool_call_count,
                protocol_error_count
            )
        })
        .unwrap_or_else(|_| "tool_events present but could not be parsed".to_string());

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.tool_events_json".to_string(),
        description: Some(description),
    }]
}

pub fn runtime_observability_meta(result: &RuntimeResult) -> BTreeMap<String, String> {
    let extra = &result.response.meta.extra;
    let mut metadata = BTreeMap::new();
    metadata.insert("model_name".to_string(), result.response.model_name.clone());
    metadata.insert(
        "recall_hit_count".to_string(),
        result.recall_hit_count.to_string(),
    );
    metadata.insert(
        "packed_token_count".to_string(),
        result.packed_token_count.to_string(),
    );
    metadata.insert(
        "context_engine_kind".to_string(),
        result.context_engine_kind.clone(),
    );
    if let Some(provider) = &result.response.meta.provider {
        metadata.insert("provider".to_string(), provider.clone());
    }
    if let Some(finish_reason) = &result.response.meta.finish_reason {
        metadata.insert("finish_reason".to_string(), finish_reason.clone());
    }

    for key in [
        "transport",
        "transport_mode",
        "status_code",
        "provider_retryable",
        "provider_error_class",
        "provider_timeout_ms",
        "provider_fallback_used",
        "provider_fallback_from",
        "provider_fallback_reason",
        "governance_action_id",
        "governance_decision",
        "governance_reason",
        "goal_id",
        "goal_objective",
        "goal_context_injected",
        "session_id",
        "session_memory_scope",
        "session_memory_recall_isolated",
        "session_memory_recall_filter",
        "session_memory_recall_hit_count",
        "session_memory_write_requested",
        "session_memory_summary_kind",
        "session_memory_record_id",
        "tool_call_count",
        "tool_protocol_error_count",
    ] {
        if let Some(value) = extra.get(key) {
            metadata.insert(key.to_string(), value.clone());
        }
    }
    metadata
        .entry("tool_call_count".to_string())
        .or_insert_with(|| "0".to_string());
    metadata
        .entry("tool_protocol_error_count".to_string())
        .or_insert_with(|| "0".to_string());

    metadata
}

fn runtime_observability_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let observability = runtime_observability_meta(result);
    if observability.len() <= 4 {
        return Vec::new();
    }

    let description = format!(
        "runtime_observability model={} provider={} goal={} session={} tool_calls={} tool_protocol_errors={}",
        observability
            .get("model_name")
            .map(String::as_str)
            .unwrap_or("unknown"),
        observability
            .get("provider")
            .map(String::as_str)
            .or_else(|| observability.get("transport").map(String::as_str))
            .unwrap_or("unknown"),
        observability
            .get("goal_id")
            .map(String::as_str)
            .unwrap_or("none"),
        observability
            .get("session_id")
            .map(String::as_str)
            .unwrap_or("none"),
        observability
            .get("tool_call_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("tool_protocol_error_count")
            .map(String::as_str)
            .unwrap_or("0")
    );

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.observability".to_string(),
        description: Some(description),
    }]
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
    if let Some(decision) = &report.governance_decision {
        metadata.extend(governance_metadata(decision));
    }
    metadata
}
