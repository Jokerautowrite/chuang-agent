//! `governance` 模块。公开接口：trait Governance；struct ProposedAction, GovernanceError, OperatorApprovalEvidence；enum ActionKind, RiskDecision；fn risk_decision_label, risk_decision_reason, risk_decision_parts；use rules_markdown, static_rule。

use crate::common::AuditRecord;

mod rules_markdown;
mod static_rule;

pub use rules_markdown::{MarkdownRuleSet, RuleCheck};
pub use static_rule::StaticRuleGovernance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    Observe,
    Draft,
    LocalDesktopInteraction,
    LocalFileWrite,
    ShellCommand,
    SubagentDispatch,
    ExternalSend,
    PublicPost,
    Payment,
    VerificationCodeInput,
    DeleteOrCleanup,
    SecretAccess,
    PrivilegeEscalation,
    ServiceChange,
    NetworkChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedAction {
    pub action_id: String,
    pub kind: ActionKind,
    pub target: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskDecision {
    Allowed { reason: String },
    DraftOnly { reason: String },
    NeedsApproval { reason: String },
    Blocked { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OperatorApprovalEvidence {
    pub approval_id: String,
    pub operator_ref: String,
    pub evidence_ref: String,
}

pub trait Governance {
    fn classify(&self, action: &ProposedAction) -> Result<RiskDecision, GovernanceError>;
    fn audit(&mut self, record: AuditRecord) -> Result<(), GovernanceError>;

    /// 返回本会话累积的审计记录（供持久化快照使用）。
    /// 默认返回空；实现方如 StaticRuleGovernance 会返回真实记录。
    fn audit_records(&self) -> &[AuditRecord] {
        &[]
    }

    fn verify_operator_approval(
        &self,
        _evidence: &OperatorApprovalEvidence,
    ) -> Result<(), GovernanceError> {
        Err(GovernanceError {
            message: "operator approval authority unavailable".to_string(),
        })
    }
}

pub fn risk_decision_label(decision: &RiskDecision) -> String {
    let (kind, reason) = risk_decision_parts(decision);
    format!("{kind}:{reason}")
}

pub fn risk_decision_reason(decision: &RiskDecision) -> String {
    risk_decision_parts(decision).1.to_string()
}

pub fn risk_decision_parts(decision: &RiskDecision) -> (&'static str, &str) {
    match decision {
        RiskDecision::Allowed { reason } => ("allowed", reason),
        RiskDecision::DraftOnly { reason } => ("draft_only", reason),
        RiskDecision::NeedsApproval { reason } => ("needs_approval", reason),
        RiskDecision::Blocked { reason } => ("blocked", reason),
    }
}
