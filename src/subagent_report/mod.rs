mod schema;
mod size_limit;
mod validation;

pub use schema::{ArtifactKind, ArtifactRef, ExecutionStatus, ResourceUsage, SubagentReport};
pub use size_limit::{ReportSizeLimit, DEFAULT_REPORT_SIZE_LIMIT_BYTES};
pub use validation::{
    ReportBuilder, ReportRejectReason, ReportValidator, SubagentReportBuilder,
    SubagentReportValidator,
};
