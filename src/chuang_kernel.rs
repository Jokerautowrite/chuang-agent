use std::collections::BTreeMap;

use crate::agent_runtime::{AgentRuntime, AgentRuntimeError, RuntimeRequest, RuntimeResult};
use crate::common::{AgentId, AuditRecord, TaskId, Timestamp};
use crate::context_engine::{ContextBudget, ContextEngineKind, ContextSegment, SegmentSource};
use crate::governance::{ActionKind, Governance, GovernanceError, ProposedAction, RiskDecision};
use crate::hermes_memory::DualFileMemorySnapshot;
use crate::memory_admission::{
    preview_chars, MemoryEntryView, TextMemoryAdmission, TextMemoryAdmissionDecision,
};
use crate::memory_store::{MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError};
use crate::responder::Responder;
use crate::runtime_report::build_runtime_report;
use crate::subagent_report::{GovernanceDecisionSummary, SubagentReport};
use serde::Serialize;

pub use crate::memory_admission::DEFAULT_MEMORY_WRITE_MAX_CHARS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelConfig {
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: Option<ContextBudget>,
    pub context_engine_kind: Option<ContextEngineKind>,
    pub memory_write_max_chars: Option<usize>,
    pub identity_snapshot: Option<DualFileMemorySnapshot>,
}

impl ChuangKernelConfig {
    pub fn mvp_default(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            parent_agent_id: None,
            recall_limit: 5,
            metadata: BTreeMap::new(),
            context_budget: None,
            context_engine_kind: None,
            memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
            identity_snapshot: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelTurn {
    pub turn_id: String,
    pub user_input: String,
    pub result: RuntimeResult,
    pub report: SubagentReport,
    pub governance_decision: Option<RiskDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChuangKernelSnapshot {
    pub agent_id: String,
    pub turn_count: u64,
    pub recall_limit: usize,
    pub metadata_keys: Vec<String>,
    pub context_budget_max_tokens: Option<u16>,
    pub memory_write_max_chars: Option<usize>,
    pub identity_user_chars: Option<usize>,
    pub identity_memory_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChuangKernelMemoryError {
    Store(MemoryStoreError),
    HardLimitExceeded {
        limit_chars: usize,
        attempted_chars: usize,
        existing_entries: Vec<MemoryEntryView>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChuangKernelGovernanceError {
    Governance(GovernanceError),
    NotAllowed { decision: RiskDecision },
    Runtime(AgentRuntimeError),
}

pub struct ChuangKernel<S, R> {
    config: ChuangKernelConfig,
    runtime: AgentRuntime<S, R>,
    turn_count: u64,
}

impl<S, R> ChuangKernel<S, R> {
    pub fn with_responder(config: ChuangKernelConfig, store: S, responder: R) -> Self {
        Self {
            runtime: AgentRuntime::with_responder_and_context_engine(
                store,
                responder,
                config.context_engine_kind.clone().unwrap_or_default(),
            ),
            config,
            turn_count: 0,
        }
    }

    pub fn snapshot(&self) -> ChuangKernelSnapshot {
        ChuangKernelSnapshot {
            agent_id: self.config.agent_id.clone(),
            turn_count: self.turn_count,
            recall_limit: self.config.recall_limit,
            metadata_keys: self.config.metadata.keys().cloned().collect(),
            context_budget_max_tokens: self
                .config
                .context_budget
                .as_ref()
                .map(|budget| budget.max_tokens),
            memory_write_max_chars: self.config.memory_write_max_chars,
            identity_user_chars: self
                .config
                .identity_snapshot
                .as_ref()
                .map(|snapshot| snapshot.user.chars().count()),
            identity_memory_chars: self
                .config
                .identity_snapshot
                .as_ref()
                .map(|snapshot| snapshot.memory.chars().count()),
        }
    }
}

impl<S: MemoryStore, R: Responder> ChuangKernel<S, R> {
    pub fn run_governed_turn<G: Governance>(
        &mut self,
        user_input: impl Into<String>,
        governance: &mut G,
    ) -> Result<ChuangKernelTurn, ChuangKernelGovernanceError> {
        let next_turn = self.turn_count + 1;
        let turn_id = format!("turn-{next_turn}");
        let action = self.propose_runtime_turn_action(&turn_id);
        let decision = governance
            .classify(&action)
            .map_err(ChuangKernelGovernanceError::Governance)?;

        if !matches!(decision, RiskDecision::Allowed { .. }) {
            return Err(ChuangKernelGovernanceError::NotAllowed { decision });
        }

        let mut turn = self
            .run_turn_with_id(turn_id.clone(), user_input.into())
            .map_err(ChuangKernelGovernanceError::Runtime)?;
        turn.report.governance_decision = Some(governance_decision_summary(&action, &decision));
        turn.governance_decision = Some(decision.clone());

        governance
            .audit(AuditRecord {
                operation: "run_governed_turn".to_string(),
                agent_id: AgentId(self.config.agent_id.clone()),
                task_id: TaskId(turn_id),
                delta_bytes: turn.report.summary.len() as i64,
                reason: render_governance_audit_reason(&decision),
                timestamp: Timestamp("2026-05-01T00:00:00Z".to_string()),
            })
            .map_err(ChuangKernelGovernanceError::Governance)?;

        Ok(turn)
    }

    pub fn run_turn(
        &mut self,
        user_input: impl Into<String>,
    ) -> Result<ChuangKernelTurn, AgentRuntimeError> {
        let next_turn = self.turn_count + 1;
        let turn_id = format!("turn-{next_turn}");
        self.run_turn_with_id(turn_id, user_input.into())
    }

    fn run_turn_with_id(
        &mut self,
        turn_id: String,
        user_input: String,
    ) -> Result<ChuangKernelTurn, AgentRuntimeError> {
        let result = self.runtime.run(&RuntimeRequest {
            user_input: user_input.clone(),
            recall_limit: self.config.recall_limit,
            metadata: self.config.metadata.clone(),
            context_budget: self.config.context_budget.clone(),
            extra_context_segments: self.identity_context_segments(),
        })?;
        let report = build_runtime_report(
            &result,
            format!("report-{turn_id}"),
            turn_id.clone(),
            self.config.agent_id.clone(),
            self.config.parent_agent_id.clone(),
        );
        self.turn_count += 1;

        Ok(ChuangKernelTurn {
            turn_id,
            user_input,
            result,
            report,
            governance_decision: None,
        })
    }

    pub fn remember_turn(
        &mut self,
        turn: &ChuangKernelTurn,
    ) -> Result<String, ChuangKernelMemoryError> {
        let record_id = format!("turn-memory-{}", turn.turn_id);
        let content = format!(
            "user={}\nresponse={}\nsummary={}",
            turn.user_input, turn.result.response.body, turn.report.summary
        );

        if let Some(limit_chars) = self.config.memory_write_max_chars {
            match TextMemoryAdmission::new(limit_chars)
                .evaluate(&content, self.existing_turn_summary_entries()?)
            {
                TextMemoryAdmissionDecision::Accepted => {}
                TextMemoryAdmissionDecision::Rejected {
                    limit_chars,
                    attempted_chars,
                    existing_entries,
                } => {
                    return Err(ChuangKernelMemoryError::HardLimitExceeded {
                        limit_chars,
                        attempted_chars,
                        existing_entries,
                    });
                }
            }
        }

        self.runtime
            .memory_store_mut()
            .put(MemoryRecord {
                id: record_id.clone(),
                content,
                metadata: BTreeMap::from([
                    ("kind".to_string(), "turn_summary".to_string()),
                    ("agent_id".to_string(), self.config.agent_id.clone()),
                    ("turn_id".to_string(), turn.turn_id.clone()),
                ]),
                created_at: "2026-05-01T00:00:00Z".to_string(),
                expires_at: None,
            })
            .map_err(ChuangKernelMemoryError::Store)?;
        Ok(record_id)
    }

    fn existing_turn_summary_entries(
        &mut self,
    ) -> Result<Vec<MemoryEntryView>, ChuangKernelMemoryError> {
        let hits = self
            .runtime
            .memory_store_mut()
            .search(&MemoryQuery {
                text: None,
                metadata: BTreeMap::from([("kind".to_string(), "turn_summary".to_string())]),
                limit: 1000,
            })
            .map_err(ChuangKernelMemoryError::Store)?;

        Ok(hits
            .into_iter()
            .map(|hit| MemoryEntryView {
                id: hit.record.id,
                chars: hit.record.content.chars().count(),
                content_preview: preview_chars(&hit.record.content, 80),
            })
            .collect())
    }

    fn identity_context_segments(&self) -> Vec<ContextSegment> {
        let Some(snapshot) = &self.config.identity_snapshot else {
            return Vec::new();
        };

        let mut segments = Vec::new();
        if !snapshot.user.trim().is_empty() {
            segments.push(identity_segment(
                "identity-user",
                "USER.md",
                &snapshot.user,
                245,
            ));
        }
        if !snapshot.memory.trim().is_empty() {
            segments.push(identity_segment(
                "identity-memory",
                "MEMORY.md",
                &snapshot.memory,
                210,
            ));
        }
        segments
    }

    fn propose_runtime_turn_action(&self, turn_id: &str) -> ProposedAction {
        ProposedAction {
            action_id: format!("run-{turn_id}"),
            kind: ActionKind::Draft,
            target: format!("{}:{turn_id}", self.config.agent_id),
            summary: "run local runtime turn and build auditable report".to_string(),
        }
    }
}

fn render_governance_audit_reason(decision: &RiskDecision) -> String {
    match decision {
        RiskDecision::Allowed { reason } => format!("allowed:{reason}"),
        RiskDecision::DraftOnly { reason } => format!("draft_only:{reason}"),
        RiskDecision::NeedsApproval { reason } => format!("needs_approval:{reason}"),
        RiskDecision::Blocked { reason } => format!("blocked:{reason}"),
    }
}

fn governance_decision_summary(
    action: &ProposedAction,
    decision: &RiskDecision,
) -> GovernanceDecisionSummary {
    let (decision, reason) = match decision {
        RiskDecision::Allowed { reason } => ("allowed", reason),
        RiskDecision::DraftOnly { reason } => ("draft_only", reason),
        RiskDecision::NeedsApproval { reason } => ("needs_approval", reason),
        RiskDecision::Blocked { reason } => ("blocked", reason),
    };

    GovernanceDecisionSummary {
        action_id: action.action_id.clone(),
        decision: decision.to_string(),
        reason: reason.clone(),
    }
}

fn identity_segment(id: &str, source_file: &str, content: &str, priority: u8) -> ContextSegment {
    ContextSegment {
        id: id.to_string(),
        source: SegmentSource::Identity,
        content: content.to_string(),
        tokens: Some(content.chars().count().min(u16::MAX as usize) as u16),
        priority,
        created_at: default_identity_timestamp(),
        last_accessed: default_identity_timestamp(),
        metadata: [("source_file".to_string(), source_file.to_string())]
            .into_iter()
            .collect(),
    }
}

fn default_identity_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
        .expect("static identity timestamp should parse")
        .with_timezone(&chrono::Utc)
}
