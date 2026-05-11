use std::collections::BTreeMap;

mod schema;
mod size_limit;
mod validation;

pub use schema::{
    ArtifactKind, ArtifactRef, ContextDebugSummary, ContextDropReasonSummary, ExecutionStatus,
    GovernanceDecisionSummary, ParentContextHandoff, ReportAdmission, ReportAdmissionStatus,
    ResourceUsage, SubagentReport, WorkingReservationDebug,
};
pub use size_limit::{ReportSizeLimit, DEFAULT_REPORT_SIZE_LIMIT_BYTES};
pub use validation::{
    ReportBuilder, ReportRejectReason, ReportValidator, RuntimeReportInput, SubagentReportBuilder,
    SubagentReportValidator,
};

pub fn governance_metadata(decision: &GovernanceDecisionSummary) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "governance_action_id".to_string(),
            decision.action_id.clone(),
        ),
        (
            "governance_decision".to_string(),
            format!("{}:{}", decision.decision, decision.reason),
        ),
        ("governance_reason".to_string(), decision.reason.clone()),
    ])
}

pub fn build_parent_context_handoff(
    report: &SubagentReport,
    admission: &ReportAdmission,
) -> ParentContextHandoff {
    let accepted = matches!(admission.status, ReportAdmissionStatus::Accepted);
    ParentContextHandoff {
        schema_version: "1.0.0".to_string(),
        accepted,
        report_id: if accepted {
            admission
                .report_id
                .clone()
                .or(Some(report.report_id.clone()))
        } else {
            None
        },
        task_id: if accepted {
            admission.task_id.clone().or(Some(report.task_id.clone()))
        } else {
            None
        },
        agent_id: if accepted {
            admission.agent_id.clone().or(Some(report.agent_id.clone()))
        } else {
            None
        },
        admission_reason_code: admission.reason_code.clone(),
        provenance_ref: if accepted {
            Some(format!(
                "report://{}/{}",
                report.agent_id.0, report.report_id.0
            ))
        } else {
            None
        },
        summary: if accepted {
            Some(report.summary.clone())
        } else {
            None
        },
        context_debug: if accepted {
            report.context_debug.clone()
        } else {
            None
        },
        memory_proposal_only: !accepted,
    }
}
