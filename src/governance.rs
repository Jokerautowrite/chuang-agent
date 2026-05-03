use crate::common::AuditRecord;

mod rules_markdown;
mod static_rule;

pub use rules_markdown::{MarkdownRuleSet, RuleCheck};
pub use static_rule::StaticRuleGovernance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    Observe,
    Draft,
    LocalFileWrite,
    ShellCommand,
    ExternalSend,
    PublicPost,
    Payment,
    VerificationCodeInput,
    DeleteOrCleanup,
    SecretAccess,
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

pub trait Governance {
    fn classify(&self, action: &ProposedAction) -> Result<RiskDecision, GovernanceError>;
    fn audit(&mut self, record: AuditRecord) -> Result<(), GovernanceError>;
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
