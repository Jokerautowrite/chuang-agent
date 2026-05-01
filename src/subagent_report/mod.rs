use std::collections::BTreeMap;

mod schema;
mod size_limit;
mod validation;

pub use schema::{
    ArtifactKind, ArtifactRef, ContextDebugSummary, ContextDropReasonSummary, ExecutionStatus,
    GovernanceDecisionSummary, ResourceUsage, SubagentReport, WorkingReservationDebug,
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
