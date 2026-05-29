use super::schema::{
    ContextDebugSummary, ContextDropReasonSummary, ExecutionStatus, ReportAdmission,
    ReportAdmissionStatus, SubagentReport,
};
use super::size_limit::DEFAULT_REPORT_SIZE_LIMIT_BYTES;
use crate::common::{AgentId, ReportId, TaskId, Timestamp};
use crate::skill_evolver::SkillProposal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportRejectReason {
    InvalidUtf8,
    InvalidJson,
    UnsupportedSchemaVersion {
        required_major: u64,
        current: String,
    },
    MissingRequiredField {
        field: &'static str,
    },
    EmptyRequiredField {
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
    InvalidTimestampOrder {
        started_at: String,
        finished_at: String,
    },
    InvalidFieldFormat {
        field: &'static str,
        reason: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeReportInput {
    pub report_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub summary: String,
    pub response_body: String,
    pub response_trace: String,
    pub dropped_segment_ids: Vec<String>,
    pub drop_reasons: Vec<(String, String)>,
    pub budget_exceeded: bool,
    pub budget_exceeded_reasons: Vec<String>,
    pub working_reservation: Option<super::schema::WorkingReservationDebug>,
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

    fn extract_string(
        value: &serde_json::Value,
        field: &'static str,
    ) -> Result<String, ReportRejectReason> {
        value
            .get(field)
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .ok_or(ReportRejectReason::MissingRequiredField { field })
    }

    fn extract_non_empty_string(
        value: &serde_json::Value,
        field: &'static str,
    ) -> Result<String, ReportRejectReason> {
        let extracted = Self::extract_string(value, field)?;
        if extracted.trim().is_empty() {
            Err(ReportRejectReason::EmptyRequiredField { field })
        } else {
            Ok(extracted)
        }
    }

    fn contains_bool(
        value: &serde_json::Value,
        field: &'static str,
    ) -> Result<(), ReportRejectReason> {
        if value.get(field).and_then(|value| value.as_bool()).is_some() {
            Ok(())
        } else {
            Err(ReportRejectReason::MissingRequiredField { field })
        }
    }

    fn contains_object(
        value: &serde_json::Value,
        field: &'static str,
    ) -> Result<(), ReportRejectReason> {
        if value
            .get(field)
            .and_then(|value| value.as_object())
            .is_some()
        {
            Ok(())
        } else {
            Err(ReportRejectReason::MissingRequiredField { field })
        }
    }

    fn contains_array(
        value: &serde_json::Value,
        field: &'static str,
    ) -> Result<(), ReportRejectReason> {
        if value
            .get(field)
            .and_then(|value| value.as_array())
            .is_some()
        {
            Ok(())
        } else {
            Err(ReportRejectReason::MissingRequiredField { field })
        }
    }

    fn parse_timestamp(
        field: &'static str,
        value: &str,
    ) -> Result<chrono::DateTime<chrono::FixedOffset>, ReportRejectReason> {
        chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
            ReportRejectReason::InvalidTimestampFormat {
                field,
                found: value.to_string(),
            }
        })
    }

    fn validate_skill_proposals(value: &serde_json::Value) -> Result<(), ReportRejectReason> {
        let Some(skill_proposals_value) = value.get("skill_proposals") else {
            return Ok(());
        };

        if !skill_proposals_value.is_array() {
            return Err(ReportRejectReason::InvalidFieldFormat {
                field: "skill_proposals",
                reason: "must be an array when present".to_string(),
            });
        }

        let proposals: Vec<SkillProposal> = serde_json::from_value(skill_proposals_value.clone())
            .map_err(|error| ReportRejectReason::InvalidFieldFormat {
                field: "skill_proposals",
                reason: format!("failed to decode SkillProposal entries: {error}"),
            })?;

        for proposal in &proposals {
            if proposal.proposal_id.trim().is_empty() {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].proposal_id",
                    reason: "must not be empty".to_string(),
                });
            }
            if proposal.title.trim().is_empty() {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].title",
                    reason: "must not be empty".to_string(),
                });
            }
            if proposal.trigger.trim().is_empty() {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].trigger",
                    reason: "must not be empty".to_string(),
                });
            }
            if proposal.procedure.is_empty() {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].procedure",
                    reason: "must not be empty".to_string(),
                });
            }
            if proposal.procedure.iter().any(|step| step.trim().is_empty()) {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].procedure[]",
                    reason: "steps must not be empty".to_string(),
                });
            }
            if proposal.evidence_event_ids.is_empty() {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].evidence_event_ids",
                    reason: "must not be empty".to_string(),
                });
            }
            if proposal
                .evidence_event_ids
                .iter()
                .any(|event_id| event_id.trim().is_empty())
            {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].evidence_event_ids[]",
                    reason: "ids must not be empty".to_string(),
                });
            }
            if proposal.provenance.is_empty() {
                return Err(ReportRejectReason::InvalidFieldFormat {
                    field: "skill_proposals[].provenance",
                    reason: "must not be empty".to_string(),
                });
            }
            for provenance in &proposal.provenance {
                if provenance.source_event_id.trim().is_empty() {
                    return Err(ReportRejectReason::InvalidFieldFormat {
                        field: "skill_proposals[].provenance[].source_event_id",
                        reason: "must not be empty".to_string(),
                    });
                }
                if provenance.source_task_id.trim().is_empty() {
                    return Err(ReportRejectReason::InvalidFieldFormat {
                        field: "skill_proposals[].provenance[].source_task_id",
                        reason: "must not be empty".to_string(),
                    });
                }
                if provenance.source_summary.trim().is_empty() {
                    return Err(ReportRejectReason::InvalidFieldFormat {
                        field: "skill_proposals[].provenance[].source_summary",
                        reason: "must not be empty".to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn admit_raw(
        &self,
        raw: &[u8],
        controller_agent_id: AgentId,
        decided_at: Timestamp,
    ) -> ReportAdmission {
        match self.validate(raw) {
            Ok(()) => {
                let value = serde_json::from_slice::<serde_json::Value>(raw).ok();
                ReportAdmission {
                    schema_version: "1.0.0".to_string(),
                    report_id: optional_id(&value, "report_id").map(ReportId),
                    task_id: optional_id(&value, "task_id").map(TaskId),
                    agent_id: optional_id(&value, "agent_id").map(AgentId),
                    controller_agent_id,
                    status: ReportAdmissionStatus::Accepted,
                    reason_code: "report_validated".to_string(),
                    upstream_reason_code: None,
                    reason: "report_validated".to_string(),
                    decided_at,
                }
            }
            Err(reason) => {
                let reason_code = reject_reason_code(&reason).to_string();
                let value = serde_json::from_slice::<serde_json::Value>(raw).ok();
                ReportAdmission {
                    schema_version: "1.0.0".to_string(),
                    report_id: optional_id(&value, "report_id").map(ReportId),
                    task_id: optional_id(&value, "task_id").map(TaskId),
                    agent_id: optional_id(&value, "agent_id").map(AgentId),
                    controller_agent_id,
                    status: ReportAdmissionStatus::Rejected,
                    reason_code,
                    upstream_reason_code: None,
                    reason: format!("{reason:?}"),
                    decided_at,
                }
            }
        }
    }
}

fn reject_reason_code(reason: &ReportRejectReason) -> &'static str {
    match reason {
        ReportRejectReason::InvalidUtf8 => "invalid_utf8",
        ReportRejectReason::InvalidJson => "invalid_json",
        ReportRejectReason::UnsupportedSchemaVersion { .. } => "unsupported_schema_version",
        ReportRejectReason::MissingRequiredField { .. } => "missing_required_field",
        ReportRejectReason::EmptyRequiredField { .. } => "empty_required_field",
        ReportRejectReason::InvalidEnumFormat { .. } => "invalid_enum_format",
        ReportRejectReason::InvalidTimestampFormat { .. } => "invalid_timestamp_format",
        ReportRejectReason::InvalidTimestampOrder { .. } => "invalid_timestamp_order",
        ReportRejectReason::InvalidFieldFormat { .. } => "invalid_field_format",
        ReportRejectReason::SizeLimitExceeded { .. } => "size_limit_exceeded",
        ReportRejectReason::TruncationFailed { .. } => "truncation_failed",
    }
}

fn optional_id(value: &Option<serde_json::Value>, field: &str) -> Option<String> {
    value
        .as_ref()
        .and_then(|value| value.get(field))
        .and_then(|value| value.as_str())
        .map(str::to_string)
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

        let text = std::str::from_utf8(raw).map_err(|_| ReportRejectReason::InvalidUtf8)?;

        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| ReportRejectReason::InvalidJson)?;

        let schema_version = Self::extract_string(&value, "schema_version")?;
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

        Self::extract_non_empty_string(&value, "report_id")?;
        Self::extract_non_empty_string(&value, "task_id")?;
        Self::extract_non_empty_string(&value, "agent_id")?;
        let status = Self::extract_string(&value, "status")?;
        if ExecutionStatus::from_str(&status).is_none() {
            return Err(ReportRejectReason::InvalidEnumFormat {
                field: "status",
                found: status,
            });
        }

        let started_at_raw = Self::extract_string(&value, "started_at")?;
        let started_at = Self::parse_timestamp("started_at", &started_at_raw)?;
        let finished_at_raw = Self::extract_string(&value, "finished_at")?;
        let finished_at = Self::parse_timestamp("finished_at", &finished_at_raw)?;
        if finished_at < started_at {
            return Err(ReportRejectReason::InvalidTimestampOrder {
                started_at: started_at_raw,
                finished_at: finished_at_raw,
            });
        }
        Self::extract_non_empty_string(&value, "summary")?;
        Self::contains_object(&value, "resource_usage")?;
        Self::contains_array(&value, "artifacts")?;
        Self::contains_bool(&value, "truncated")?;
        Self::validate_skill_proposals(&value)?;

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

    pub fn from_runtime(input: RuntimeReportInput) -> Self {
        Self {
            report: SubagentReport {
                schema_version: "1.0.0".to_string(),
                report_id: crate::common::ReportId(input.report_id),
                task_id: crate::common::TaskId(input.task_id),
                agent_id: crate::common::AgentId(input.agent_id),
                parent_agent_id: input.parent_agent_id.map(crate::common::AgentId),
                status: ExecutionStatus::Success,
                started_at: crate::common::Timestamp("2026-04-30T00:00:00.000Z".to_string()),
                finished_at: crate::common::Timestamp("2026-04-30T00:00:01.000Z".to_string()),
                summary: input.summary,
                exit_code: Some(0),
                stdout_preview: Some(input.response_body),
                stderr_preview: Some(String::new()),
                resource_usage: Default::default(),
                artifacts: vec![],
                replay_ref: None,
                context_debug: Some(ContextDebugSummary {
                    dropped_segment_ids: input.dropped_segment_ids,
                    drop_reasons: input
                        .drop_reasons
                        .into_iter()
                        .map(|(segment_id, reason)| ContextDropReasonSummary { segment_id, reason })
                        .collect(),
                    budget_exceeded: input.budget_exceeded,
                    budget_exceeded_reasons: input.budget_exceeded_reasons,
                    working_reservation: input.working_reservation,
                }),
                governance_decision: None,
                truncated: false,
                skill_proposals: vec![],
            },
        }
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
