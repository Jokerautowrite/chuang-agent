use super::schema::{ExecutionStatus, SubagentReport};
use super::size_limit::DEFAULT_REPORT_SIZE_LIMIT_BYTES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportRejectReason {
    UnsupportedSchemaVersion {
        required_major: u64,
        current: String,
    },
    MissingRequiredField {
        field: &'static str,
    },
    InvalidEnumFormat {
        field: &'static str,
        found: String,
    },
    InvalidTimestampFormat {
        field: &'static str,
        found: String,
    },
    SizeLimitExceeded {
        limit_bytes: usize,
        actual: usize,
    },
    TruncationFailed {
        after_truncate: usize,
    },
}

pub trait ReportValidator {
    type Report;

    fn validate(&self, raw: &[u8]) -> Result<(), ReportRejectReason>;
    fn apply_optional_defaults(&self, report: &mut Self::Report);
}

pub trait ReportBuilder {
    fn build(self) -> SubagentReport;
    fn truncate_previews(self, max_total_bytes: usize) -> Self;
}

#[derive(Debug, Clone)]
pub struct SubagentReportValidator {
    max_bytes: usize,
}

impl Default for SubagentReportValidator {
    fn default() -> Self {
        Self::new(DEFAULT_REPORT_SIZE_LIMIT_BYTES)
    }
}

impl SubagentReportValidator {
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    fn extract_string(raw: &str, field: &'static str) -> Result<String, ReportRejectReason> {
        let needle = format!("\"{}\":\"", field);
        let start = raw
            .find(&needle)
            .ok_or(ReportRejectReason::MissingRequiredField { field })?
            + needle.len();
        let tail = &raw[start..];
        let end = tail
            .find('"')
            .ok_or(ReportRejectReason::MissingRequiredField { field })?;
        Ok(tail[..end].to_string())
    }

    fn contains_bool(raw: &str, field: &'static str) -> Result<(), ReportRejectReason> {
        let true_needle = format!("\"{}\":true", field);
        let false_needle = format!("\"{}\":false", field);
        if raw.contains(&true_needle) || raw.contains(&false_needle) {
            Ok(())
        } else {
            Err(ReportRejectReason::MissingRequiredField { field })
        }
    }

    fn contains_object(raw: &str, field: &'static str) -> Result<(), ReportRejectReason> {
        let needle = format!("\"{}\":{{", field);
        if raw.contains(&needle) {
            Ok(())
        } else {
            Err(ReportRejectReason::MissingRequiredField { field })
        }
    }

    fn contains_array(raw: &str, field: &'static str) -> Result<(), ReportRejectReason> {
        let needle = format!("\"{}\":[", field);
        if raw.contains(&needle) {
            Ok(())
        } else {
            Err(ReportRejectReason::MissingRequiredField { field })
        }
    }

    fn validate_timestamp(field: &'static str, value: &str) -> Result<(), ReportRejectReason> {
        let valid = value.len() >= 24
            && value.contains('T')
            && value.ends_with('Z')
            && value.matches(':').count() >= 2
            && value.contains('.');
        if valid {
            Ok(())
        } else {
            Err(ReportRejectReason::InvalidTimestampFormat {
                field,
                found: value.to_string(),
            })
        }
    }
}

impl ReportValidator for SubagentReportValidator {
    type Report = SubagentReport;

    fn validate(&self, raw: &[u8]) -> Result<(), ReportRejectReason> {
        let actual = raw.len();
        if actual > self.max_bytes {
            return Err(ReportRejectReason::SizeLimitExceeded {
                limit_bytes: self.max_bytes,
                actual,
            });
        }

        let text = std::str::from_utf8(raw)
            .map_err(|_| ReportRejectReason::MissingRequiredField { field: "raw_utf8" })?;

        let schema_version = Self::extract_string(text, "schema_version")?;
        let major = schema_version
            .split('.')
            .next()
            .and_then(|part| part.parse::<u64>().ok())
            .unwrap_or(0);
        if major != 1 {
            return Err(ReportRejectReason::UnsupportedSchemaVersion {
                required_major: 1,
                current: schema_version,
            });
        }

        Self::extract_string(text, "report_id")?;
        Self::extract_string(text, "task_id")?;
        Self::extract_string(text, "agent_id")?;
        let status = Self::extract_string(text, "status")?;
        if ExecutionStatus::from_str(&status).is_none() {
            return Err(ReportRejectReason::InvalidEnumFormat {
                field: "status",
                found: status,
            });
        }

        let started_at = Self::extract_string(text, "started_at")?;
        Self::validate_timestamp("started_at", &started_at)?;
        let finished_at = Self::extract_string(text, "finished_at")?;
        Self::validate_timestamp("finished_at", &finished_at)?;
        Self::extract_string(text, "summary")?;
        Self::contains_object(text, "resource_usage")?;
        Self::contains_array(text, "artifacts")?;
        Self::contains_bool(text, "truncated")?;

        Ok(())
    }

    fn apply_optional_defaults(&self, report: &mut Self::Report) {
        if report.stdout_preview.is_none() {
            report.stdout_preview = Some(String::new());
        }
        if report.stderr_preview.is_none() {
            report.stderr_preview = Some(String::new());
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubagentReportBuilder {
    report: SubagentReport,
}

impl SubagentReportBuilder {
    pub fn new(report: SubagentReport) -> Self {
        Self { report }
    }
}

impl ReportBuilder for SubagentReportBuilder {
    fn build(self) -> SubagentReport {
        self.report
    }

    fn truncate_previews(mut self, max_total_bytes: usize) -> Self {
        let stdout_len = self
            .report
            .stdout_preview
            .as_ref()
            .map(|value| value.len())
            .unwrap_or(0);
        let stderr_len = self
            .report
            .stderr_preview
            .as_ref()
            .map(|value| value.len())
            .unwrap_or(0);
        let total = stdout_len + stderr_len;

        if total > max_total_bytes {
            if let Some(stdout) = &mut self.report.stdout_preview {
                stdout.truncate(max_total_bytes.min(stdout.len()));
            }
            if max_total_bytes == 0 {
                self.report.stdout_preview = Some(String::new());
            }
            self.report.truncated = true;
        }

        self
    }
}
