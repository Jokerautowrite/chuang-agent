//! `subagent_report::size_limit` 模块。公开接口：struct ReportSizeLimit；const DEFAULT_REPORT_SIZE_LIMIT_BYTES。

pub const DEFAULT_REPORT_SIZE_LIMIT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportSizeLimit {
    pub max_bytes: usize,
}

impl Default for ReportSizeLimit {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_REPORT_SIZE_LIMIT_BYTES,
        }
    }
}
