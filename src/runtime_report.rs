use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::agent_runtime::{ContextDebugInfo, RuntimeResult};
use crate::context_engine::ContextCompactionSummary;
use crate::goal_dispatch::GoalDispatchHandoffSummary;
use crate::subagent_report::{
    governance_metadata, ArtifactKind, ArtifactRef, ExecutionStatus, ReportBuilder,
    RuntimeReportInput, SubagentReport, SubagentReportBuilder,
};
use crate::subagent_tree_ledger::SubagentChildrenSummary;
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
    report
        .artifacts
        .extend(tool_protocol_error_artifacts(result));
    report.artifacts.extend(tool_events_artifacts(result));
    report
        .artifacts
        .extend(runtime_event_ledger_artifacts(result));
    report.artifacts.extend(runtime_trace_artifacts(result));
    report
        .artifacts
        .extend(context_compaction_artifacts(result));
    report
        .artifacts
        .extend(handoff_query_summary_artifacts(result));
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

fn tool_protocol_error_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let Some(raw_errors) = result.response.meta.extra.get("tool_protocol_errors_json") else {
        return Vec::new();
    };

    let description = serde_json::from_str::<Vec<serde_json::Value>>(raw_errors)
        .map(|errors| {
            let codes = errors
                .iter()
                .filter_map(|error| error.get("code").and_then(|value| value.as_str()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "tool_protocol_errors count={} codes={}",
                errors.len(),
                if codes.is_empty() { "none" } else { &codes }
            )
        })
        .unwrap_or_else(|_| "tool_protocol_errors present but could not be parsed".to_string());

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.tool_protocol_errors_json".to_string(),
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
            let typed_failure_count = events
                .iter()
                .filter(|event| {
                    event.get("kind").and_then(|value| value.as_str()) == Some("tool_call")
                        && event.get("ok").and_then(|value| value.as_bool()) == Some(false)
                        && event
                            .get("failure_class")
                            .and_then(|value| value.as_str())
                            .is_some_and(is_typed_failure_class)
                })
                .count();
            format!(
                "tool_events count={} tool_calls={} protocol_errors={} typed_failures={}",
                events.len(),
                tool_call_count,
                protocol_error_count,
                typed_failure_count
            )
        })
        .unwrap_or_else(|_| "tool_events present but could not be parsed".to_string());

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.tool_events_json".to_string(),
        description: Some(description),
    }]
}

fn runtime_event_ledger_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let Some(raw_ledger) = result.response.meta.extra.get("runtime_event_ledger_json") else {
        return Vec::new();
    };

    let description = runtime_event_ledger_summary_from_json(raw_ledger)
        .map(|summary| {
            format!(
                "runtime_event_ledger count={} tool_started={} tool_finished={} approval_requested={} approval_resolved={} elicitation_requested={}",
                summary.event_count,
                summary.tool_started_count,
                summary.tool_finished_count,
                summary.approval_requested_count,
                summary.approval_resolved_count,
                summary.elicitation_requested_count
            )
        })
        .unwrap_or_else(|_| "runtime_event_ledger present but could not be parsed".to_string());

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.runtime_event_ledger_json".to_string(),
        description: Some(description),
    }]
}

fn runtime_trace_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    if result.response.trace.is_empty() {
        return Vec::new();
    }

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_response.trace".to_string(),
        description: Some(format!(
            "runtime_response_trace chars={}",
            result.response.trace.chars().count()
        )),
    }]
}

fn context_compaction_summary_artifact(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let Some(raw_summary) = result
        .response
        .meta
        .extra
        .get("context_compaction_summary_json")
    else {
        return Vec::new();
    };

    let description = serde_json::from_str::<ContextCompactionSummary>(raw_summary)
        .map(|summary| {
            format!(
                "context_compaction_summary events={} started={} completed={} dropped={} trace_steps={}",
                summary.event_count,
                summary.started_count,
                summary.completed_count,
                summary.dropped_count,
                summary.trace_steps.join(",")
            )
        })
        .unwrap_or_else(|_| {
            "context_compaction_summary present but could not be parsed".to_string()
        });

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.context_compaction_summary_json".to_string(),
        description: Some(description),
    }]
}

pub fn runtime_observability_meta(result: &RuntimeResult) -> BTreeMap<String, String> {
    let extra = &result.response.meta.extra;
    let mut metadata = BTreeMap::new();
    let typed_failures = collect_typed_failures(extra);
    let unified_failures = crate::tool_loop_meta::collect_unified_execution_failure_classes(extra);
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
    metadata.insert(
        "runtime_response_trace_chars".to_string(),
        result.response.trace.chars().count().to_string(),
    );
    if let Some(pack_trace) = packed_context_field(&result.packed_context_preview, "pack_trace") {
        metadata.insert("context_pack_trace".to_string(), pack_trace);
    }
    if let Some(compaction_events) =
        packed_context_field(&result.packed_context_preview, "compaction_events")
    {
        metadata.insert("context_compaction_events".to_string(), compaction_events);
    }
    if let Some(summary) = extra.get("context_compaction_summary_json") {
        metadata.insert(
            "context_compaction_summary_json".to_string(),
            summary.clone(),
        );
    }

    for key in [
        "transport",
        "transport_mode",
        "request_url",
        "request_method",
        "request_message_count",
        "config_error_field",
        "status_code",
        "provider_response_ok",
        "provider_retryable",
        "provider_error_class",
        "provider_error_message",
        "provider_failure_reason_code",
        "provider_failure_category",
        "provider_timeout_reason_code",
        "provider_timeout_category",
        "provider_timeout_ms",
        "provider_fallback_configured",
        "provider_fallback_used",
        "provider_fallback_from",
        "provider_fallback_reason",
        "provider_fallback_primary_retryable",
        "provider_fallback_primary_status_code",
        "provider_fallback_primary_error_class",
        "provider_fallback_primary_config_error_field",
        "provider_fallback_primary_timeout_ms",
        "provider_fallback_primary_error_message",
        "provider_fallback_primary_request_url",
        "provider_fallback_primary_request_method",
        "provider_fallback_primary_request_message_count",
        "provider_fallback_primary_transport",
        "provider_fallback_primary_transport_mode",
        "provider_fallback_primary_response_ok",
        "provider_fallback_primary_failure_reason_code",
        "provider_fallback_primary_failure_category",
        "runtime_report_id",
        "runtime_report_task_id",
        "runtime_report_agent_id",
        "runtime_report_status",
        "governance_action_id",
        "governance_decision",
        "governance_reason",
        "goal_id",
        "goal_objective",
        "goal_context_injected",
        "knowledge_context_preview_enabled",
        "knowledge_context_injected",
        "knowledge_context_preview_count",
        "knowledge_context_injected_count",
        "knowledge_context_dropped_count",
        "knowledge_context_dropped_segment_ids",
        "knowledge_context_model_facing",
        "knowledge_context_source_boundary",
        "knowledge_context_live_wiki_gbrain_connected",
        "knowledge_context_read_only",
        "knowledge_context_connects_real_service",
        "knowledge_context_writes_automatically",
        "knowledge_context_runtime_retrieval_wired",
        "recent_conversation_history_item_count",
        "recent_conversation_history_turn_count",
        "recent_conversation_history_injected",
        "recent_conversation_history_dropped",
        "recent_conversation_history_model_facing",
        "session_id",
        "session_memory_scope",
        "session_memory_recall_isolated",
        "session_memory_recall_filter",
        "session_memory_recall_hit_count",
        "session_memory_write_requested",
        "session_memory_summary_kind",
        "session_memory_record_id",
        "session_memory_write_status",
        "session_memory_write_error",
        "session_memory_compacted_from_chars",
        "session_memory_compacted_to_chars",
        "tool_call_count",
        "tool_protocol_error_count",
        "tool_surface_available",
        "tool_surface_governed",
        "tool_surface_source",
        "tool_surface_callable_tools",
        "tool_surface_mapped_atomic_tools",
        "tool_surface_interface_only_atomic_tools",
        "tool_action_schema_version",
        "tool_report_schema_version",
        "tool_instruction_context_injected",
        "runtime_event_count",
        "goal_handoff_query_summary_json",
        "subagent_children_summary_json",
        "context_pack_trace",
        "context_compaction_events",
        "context_compaction_summary_json",
    ] {
        if let Some(value) = extra.get(key) {
            metadata.insert(key.to_string(), value.clone());
        }
    }
    if let Some(summary) = extra
        .get("runtime_event_ledger_json")
        .and_then(|raw| runtime_event_ledger_summary_from_json(raw).ok())
    {
        metadata.insert(
            "runtime_event_count".to_string(),
            summary.event_count.to_string(),
        );
        metadata.insert(
            "runtime_event_tool_started_count".to_string(),
            summary.tool_started_count.to_string(),
        );
        metadata.insert(
            "runtime_event_tool_finished_count".to_string(),
            summary.tool_finished_count.to_string(),
        );
        metadata.insert(
            "runtime_event_approval_requested_count".to_string(),
            summary.approval_requested_count.to_string(),
        );
        metadata.insert(
            "runtime_event_approval_resolved_count".to_string(),
            summary.approval_resolved_count.to_string(),
        );
        metadata.insert(
            "runtime_event_elicitation_requested_count".to_string(),
            summary.elicitation_requested_count.to_string(),
        );
    }
    if let Some(summary) = extra
        .get("goal_handoff_query_summary_json")
        .and_then(|raw| serde_json::from_str::<GoalDispatchHandoffSummary>(raw).ok())
    {
        metadata.insert(
            "goal_handoff_parent_context_handoff_count".to_string(),
            summary.parent_context_handoff_count.to_string(),
        );
        metadata.insert(
            "goal_handoff_report_admission_ref_count".to_string(),
            summary.report_admission_ref_count.to_string(),
        );
        metadata.insert(
            "goal_handoff_report_admission_reason_codes".to_string(),
            format_reason_code_counts(&summary.report_admission_reason_codes),
        );
        metadata.insert(
            "goal_handoff_report_admission_refs".to_string(),
            format_goal_admission_refs(&summary.report_admission_refs),
        );
    }
    if let Some(summary) = extra
        .get("subagent_children_summary_json")
        .and_then(|raw| serde_json::from_str::<SubagentChildrenSummary>(raw).ok())
    {
        metadata.insert(
            "subagent_children_child_count".to_string(),
            summary.child_count.to_string(),
        );
        metadata.insert(
            "subagent_children_accepted_report_count".to_string(),
            summary.accepted_report_count.to_string(),
        );
        metadata.insert(
            "subagent_children_report_admission_ref_count".to_string(),
            summary.report_admission_refs.len().to_string(),
        );
        metadata.insert(
            "subagent_children_missing_report_count".to_string(),
            summary.missing_report_count.to_string(),
        );
        metadata.insert(
            "subagent_children_report_reason_codes".to_string(),
            format_reason_code_counts(&summary.report_reason_codes),
        );
        metadata.insert(
            "subagent_children_report_admission_refs".to_string(),
            format_subagent_admission_refs(&summary.report_admission_refs),
        );
    }
    metadata
        .entry("tool_call_count".to_string())
        .or_insert_with(|| "0".to_string());
    metadata
        .entry("tool_protocol_error_count".to_string())
        .or_insert_with(|| "0".to_string());
    for key in [
        "runtime_event_count",
        "runtime_event_tool_started_count",
        "runtime_event_tool_finished_count",
        "runtime_event_approval_requested_count",
        "runtime_event_approval_resolved_count",
        "runtime_event_elicitation_requested_count",
        "goal_handoff_parent_context_handoff_count",
        "goal_handoff_report_admission_ref_count",
        "subagent_children_child_count",
        "subagent_children_accepted_report_count",
        "subagent_children_report_admission_ref_count",
        "subagent_children_missing_report_count",
    ] {
        metadata
            .entry(key.to_string())
            .or_insert_with(|| "0".to_string());
    }
    for key in [
        "goal_handoff_report_admission_refs",
        "goal_handoff_report_admission_reason_codes",
        "subagent_children_report_admission_refs",
        "subagent_children_report_reason_codes",
    ] {
        metadata
            .entry(key.to_string())
            .or_insert_with(|| "none".to_string());
    }
    metadata.insert(
        "tool_typed_failure_count".to_string(),
        typed_failures.len().to_string(),
    );
    if !typed_failures.is_empty() {
        metadata.insert(
            "tool_typed_failure_classes".to_string(),
            typed_failures.into_iter().collect::<Vec<_>>().join(","),
        );
    }
    metadata.insert(
        "tool_unified_execution_failure_count".to_string(),
        unified_failures.len().to_string(),
    );
    metadata.insert(
        "tool_unified_execution_status".to_string(),
        if unified_failures.is_empty() {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !unified_failures.is_empty() {
        metadata.insert(
            "tool_unified_execution_failure_classes".to_string(),
            unified_failures.into_iter().collect::<Vec<_>>().join(","),
        );
    }

    metadata
}

fn context_compaction_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let mut artifacts = Vec::new();
    if let Some(pack_trace) = packed_context_field(&result.packed_context_preview, "pack_trace") {
        artifacts.push(ArtifactRef {
            kind: ArtifactKind::Log,
            locator: "runtime_meta.context_pack_trace".to_string(),
            description: Some(format!("pack_trace {pack_trace}")),
        });
    }
    if let Some(compaction_events) =
        packed_context_field(&result.packed_context_preview, "compaction_events")
    {
        artifacts.push(ArtifactRef {
            kind: ArtifactKind::Log,
            locator: "runtime_meta.context_compaction_events".to_string(),
            description: Some(format!("compaction_events {compaction_events}")),
        });
    }
    artifacts.extend(context_compaction_summary_artifact(result));
    artifacts
}

fn handoff_query_summary_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let mut artifacts = Vec::new();
    if let Some(raw_summary) = result
        .response
        .meta
        .extra
        .get("goal_handoff_query_summary_json")
    {
        let description = serde_json::from_str::<GoalDispatchHandoffSummary>(raw_summary)
            .map(|summary| {
                format!(
                    "goal_handoff_query_summary parent_context_handoffs={} report_admission_refs={} reason_codes={}",
                    summary.parent_context_handoff_count,
                    summary.report_admission_ref_count,
                    format_reason_code_counts(&summary.report_admission_reason_codes)
                )
            })
            .unwrap_or_else(|_| {
                "goal_handoff_query_summary present but could not be parsed".to_string()
            });
        artifacts.push(ArtifactRef {
            kind: ArtifactKind::Log,
            locator: "runtime_meta.goal_handoff_query_summary_json".to_string(),
            description: Some(description),
        });
    }
    if let Some(raw_summary) = result
        .response
        .meta
        .extra
        .get("subagent_children_summary_json")
    {
        let description = serde_json::from_str::<SubagentChildrenSummary>(raw_summary)
            .map(|summary| {
                format!(
                    "subagent_children_summary children={} open={} reported={} closed={} accepted_reports={} report_admission_refs={} rejected_reports={} missing_reports={} reason_codes={}",
                    summary.child_count,
                    summary.open_child_count,
                    summary.reported_child_count,
                    summary.closed_child_count,
                    summary.accepted_report_count,
                    summary.report_admission_refs.len(),
                    summary.rejected_report_count,
                    summary.missing_report_count,
                    format_reason_code_counts(&summary.report_reason_codes)
                )
            })
            .unwrap_or_else(|_| {
                "subagent_children_summary present but could not be parsed".to_string()
            });
        artifacts.push(ArtifactRef {
            kind: ArtifactKind::Log,
            locator: "runtime_meta.subagent_children_summary_json".to_string(),
            description: Some(description),
        });
    }
    artifacts
}

fn format_reason_code_counts(map: &BTreeMap<String, usize>) -> String {
    if map.is_empty() {
        return "none".to_string();
    }
    map.iter()
        .map(|(reason, count)| format!("{reason}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_goal_admission_refs(refs: &[crate::goal_dispatch::GoalReportAdmissionRef]) -> String {
    if refs.is_empty() {
        return "none".to_string();
    }

    refs.iter()
        .map(|admission| {
            admission
                .admission_id
                .as_deref()
                .unwrap_or(admission.report_id.as_str())
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn format_subagent_admission_refs(
    refs: &[crate::subagent_tree_ledger::ReportAdmissionRef],
) -> String {
    if refs.is_empty() {
        return "none".to_string();
    }

    refs.iter()
        .map(|admission| admission.admission_id.clone())
        .collect::<Vec<_>>()
        .join(",")
}

fn packed_context_field(preview: &str, key: &str) -> Option<String> {
    preview
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .map(str::to_string)
}

fn runtime_observability_artifacts(result: &RuntimeResult) -> Vec<ArtifactRef> {
    let observability = runtime_observability_meta(result);
    if observability.len() <= 4 {
        return Vec::new();
    }

    let description = format!(
        "runtime_observability model={} provider={} goal={} session={} tool_calls={} tool_protocol_errors={} tool_started={} tool_finished={} tool_typed_failures={} tool_unified_execution_status={} tool_unified_execution_failures={} runtime_events={} approval_requested={} approval_resolved={} elicitation_requested={} goal_handoffs={} goal_admissions={} goal_admission_ref_locators={} goal_admission_reason_codes={} subagent_children={} subagent_accepted_reports={} subagent_admission_refs={} subagent_admission_ref_locators={} subagent_missing_reports={} subagent_reason_codes={}",
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
            .unwrap_or("0"),
        observability
            .get("runtime_event_tool_started_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("runtime_event_tool_finished_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("tool_typed_failure_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("tool_unified_execution_status")
            .map(String::as_str)
            .unwrap_or("ok"),
        observability
            .get("tool_unified_execution_failure_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("runtime_event_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("runtime_event_approval_requested_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("runtime_event_approval_resolved_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("runtime_event_elicitation_requested_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("goal_handoff_parent_context_handoff_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("goal_handoff_report_admission_ref_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("goal_handoff_report_admission_refs")
            .map(String::as_str)
            .unwrap_or("none"),
        observability
            .get("goal_handoff_report_admission_reason_codes")
            .map(String::as_str)
            .unwrap_or("none"),
        observability
            .get("subagent_children_child_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("subagent_children_accepted_report_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("subagent_children_report_admission_ref_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("subagent_children_report_admission_refs")
            .map(String::as_str)
            .unwrap_or("none"),
        observability
            .get("subagent_children_missing_report_count")
            .map(String::as_str)
            .unwrap_or("0"),
        observability
            .get("subagent_children_report_reason_codes")
            .map(String::as_str)
            .unwrap_or("none")
    );

    vec![ArtifactRef {
        kind: ArtifactKind::Log,
        locator: "runtime_meta.observability".to_string(),
        description: Some(description),
    }]
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RuntimeEventLedgerJsonSummary {
    event_count: usize,
    tool_started_count: usize,
    tool_finished_count: usize,
    approval_requested_count: usize,
    approval_resolved_count: usize,
    elicitation_requested_count: usize,
}

fn runtime_event_ledger_summary_from_json(
    raw_ledger: &str,
) -> Result<RuntimeEventLedgerJsonSummary, serde_json::Error> {
    let events = serde_json::from_str::<Vec<serde_json::Value>>(raw_ledger)?;
    let mut summary = RuntimeEventLedgerJsonSummary {
        event_count: events.len(),
        ..RuntimeEventLedgerJsonSummary::default()
    };
    for event in &events {
        match event.get("event_type").and_then(|value| value.as_str()) {
            Some("tool_started") => summary.tool_started_count += 1,
            Some("tool_finished") => summary.tool_finished_count += 1,
            Some("approval_requested") => summary.approval_requested_count += 1,
            Some("approval_resolved") => summary.approval_resolved_count += 1,
            Some("elicitation_requested") => summary.elicitation_requested_count += 1,
            _ => {}
        }
    }
    Ok(summary)
}

fn collect_typed_failures(extra: &BTreeMap<String, String>) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    if let Some(class) = extra.get("tool_protocol_typed_failure_code") {
        if is_typed_failure_class(class) {
            classes.insert(class.clone());
        }
    }
    if let Ok(Some(report)) = crate::tool_loop_meta::parse_json_value(extra, "tool_report_json") {
        append_typed_failures_from_report(&mut classes, &report);
    }
    if let Ok(calls) = crate::tool_loop_meta::parse_json_vec_value(extra, "tool_calls_json") {
        append_typed_failures_from_calls(&mut classes, &calls);
    }
    if let Ok(events) = crate::tool_loop_meta::parse_json_vec_value(extra, "tool_events_json") {
        append_typed_failures_from_events(&mut classes, &events);
        if events.iter().any(|event| {
            event.get("kind").and_then(|value| value.as_str()) == Some("protocol_error")
        }) {
            classes.insert("protocol_error".to_string());
        }
    }
    classes
}

fn append_typed_failures_from_report(classes: &mut BTreeSet<String>, report: &serde_json::Value) {
    let calls = report
        .get("calls")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    append_typed_failures_from_calls(classes, &calls);
}

fn append_typed_failures_from_calls(classes: &mut BTreeSet<String>, calls: &[serde_json::Value]) {
    for call in calls {
        if call.get("ok").and_then(|value| value.as_bool()) == Some(false) {
            if let Some(class) = call.get("failure_class").and_then(|value| value.as_str()) {
                if is_typed_failure_class(class) {
                    classes.insert(class.to_string());
                }
            }
        }
    }
}

fn append_typed_failures_from_events(classes: &mut BTreeSet<String>, events: &[serde_json::Value]) {
    for event in events {
        if event.get("kind").and_then(|value| value.as_str()) == Some("tool_call")
            && event.get("ok").and_then(|value| value.as_bool()) == Some(false)
        {
            if let Some(class) = event.get("failure_class").and_then(|value| value.as_str()) {
                if is_typed_failure_class(class) {
                    classes.insert(class.to_string());
                }
            }
        }
    }
}

fn is_typed_failure_class(class: &str) -> bool {
    matches!(
        class,
        "adapter_unavailable"
            | "permission_denied"
            | "protocol_error"
            | "timeout"
            | "invalid_output"
            | "nonzero_exit"
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
    if let Some(decision) = &report.governance_decision {
        metadata.extend(governance_metadata(decision));
    }
    metadata
}
