use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod canonical;
mod dry_run;
mod failure;
mod noop;
mod rule_change;

pub use canonical::{
    CanonicalSkillEvolver, DuplicateDecision, SkillLifecycleStatus, SkillRetirementReceipt,
    SkillRetirementRequest, SkillScoreCard, SkillScoreDimension, SkillSelfApprovalDecision,
    SkillUpsertKind, SkillUpsertReceipt,
};
pub use dry_run::DryRunProposalEvolver;
pub use failure::{FailureDetectorConfig, FailurePattern, RepeatedFailureDetector};
pub use noop::NoopEvolver;
pub use rule_change::{
    FailureEvidence, GovernanceContext, GovernanceDecision, NoopRuleChangeGovernance,
    PolicyRuleChangeGovernance, RuleChangeGovernance, RuleChangeJournal, RuleChangeJournalEntry,
    RuleChangeKind, RuleChangeProposal, RuleChangeReceipt,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub event_id: String,
    pub task_id: String,
    pub kind: RuntimeEventKind,
    pub summary: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventKind {
    TurnCompleted,
    ToolSucceeded,
    ToolFailed,
    UserCorrection,
    ManualObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionScope {
    pub agent_id: String,
    pub task_kind: Option<String>,
    pub max_proposals: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProposal {
    pub proposal_id: String,
    pub title: String,
    pub trigger: String,
    pub procedure: Vec<String>,
    pub evidence_event_ids: Vec<String>,
    pub dry_run: bool,
    pub writes_skills: bool,
    pub requires_approval: bool,
    pub provenance: Vec<SkillProposalProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProposalProvenance {
    pub source_event_id: String,
    pub source_task_id: String,
    pub source_kind: RuntimeEventKind,
    pub source_summary: String,
    pub source_metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub proposal_id: String,
    pub accepted: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillApprovalState {
    Pending,
    Approved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillApprovalReceipt {
    pub proposal_id: String,
    pub validation_report: ValidationReport,
    pub approved: bool,
    pub approval_source: String,
    pub approved_at: Option<String>,
    pub approval_note: Option<String>,
}

impl SkillApprovalReceipt {
    pub fn pending_receipt(proposal_id: String, validation_report: ValidationReport) -> Self {
        Self {
            proposal_id,
            validation_report,
            approved: false,
            approval_source: "pending_operator_approval".to_string(),
            approved_at: None,
            approval_note: Some(
                "skill propose only emits a local review ticket; canonical solidify is handled by the write path"
                    .to_string(),
            ),
        }
    }

    pub fn approved_receipt(
        proposal_id: String,
        validation_report: ValidationReport,
        approval_source: String,
        approved_at: Option<String>,
        approval_note: Option<String>,
    ) -> Self {
        Self {
            proposal_id,
            validation_report,
            approved: true,
            approval_source,
            approved_at,
            approval_note,
        }
    }

    pub fn pending(proposal_id: String, validation_report: ValidationReport) -> Self {
        Self::pending_receipt(proposal_id, validation_report)
    }

    pub fn approved(
        proposal_id: String,
        validation_report: ValidationReport,
        approval_source: String,
        approved_at: Option<String>,
        approval_note: Option<String>,
    ) -> Self {
        Self::approved_receipt(
            proposal_id,
            validation_report,
            approval_source,
            approved_at,
            approval_note,
        )
    }

    pub fn approval_state(&self) -> SkillApprovalState {
        if self.approved {
            SkillApprovalState::Approved
        } else {
            SkillApprovalState::Pending
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.approval_state(), SkillApprovalState::Pending)
    }

    pub fn is_approved(&self) -> bool {
        matches!(self.approval_state(), SkillApprovalState::Approved)
    }

    pub fn validate_consistency(&self) -> Result<(), String> {
        if self.proposal_id.trim().is_empty() {
            return Err("proposal_id must not be empty".to_string());
        }

        if self.validation_report.proposal_id != self.proposal_id {
            return Err("validation_report.proposal_id must match proposal_id".to_string());
        }

        if self.approval_source.trim().is_empty() {
            return Err("approval_source must not be empty".to_string());
        }

        if self.approved && !self.validation_report.accepted {
            return Err("approved receipts require an accepted validation report".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillSolidifyTicket {
    pub ticket_id: String,
    pub proposal_id: String,
    pub approval_receipt: SkillApprovalReceipt,
    pub dry_run: bool,
    pub writes_skills: bool,
    pub solidifies_skill: bool,
    pub local_only: bool,
}

impl SkillSolidifyTicket {
    pub fn pending_ticket(proposal: &SkillProposal, validation_report: ValidationReport) -> Self {
        Self {
            ticket_id: format!("pending-solidify-{}", proposal.proposal_id),
            proposal_id: proposal.proposal_id.clone(),
            approval_receipt: SkillApprovalReceipt::pending_receipt(
                proposal.proposal_id.clone(),
                validation_report,
            ),
            dry_run: true,
            writes_skills: false,
            solidifies_skill: false,
            local_only: true,
        }
    }

    pub fn approved_ticket(
        proposal: &SkillProposal,
        validation_report: ValidationReport,
        approval_source: String,
        approved_at: Option<String>,
        approval_note: Option<String>,
    ) -> Self {
        Self {
            ticket_id: format!("approved-solidify-{}", proposal.proposal_id),
            proposal_id: proposal.proposal_id.clone(),
            approval_receipt: SkillApprovalReceipt::approved_receipt(
                proposal.proposal_id.clone(),
                validation_report,
                approval_source,
                approved_at,
                approval_note,
            ),
            dry_run: true,
            writes_skills: false,
            solidifies_skill: false,
            local_only: true,
        }
    }

    pub fn approval_receipt(
        proposal: &SkillProposal,
        validation_report: ValidationReport,
        approval_source: String,
        approved_at: Option<String>,
        approval_note: Option<String>,
    ) -> Self {
        Self::approved_ticket(
            proposal,
            validation_report,
            approval_source,
            approved_at,
            approval_note,
        )
    }

    pub fn solidify_refusal_receipt(
        proposal: &SkillProposal,
        validation_report: ValidationReport,
        approval_source: String,
        approved_at: Option<String>,
        approval_note: Option<String>,
    ) -> Self {
        Self::approved_ticket(
            proposal,
            validation_report,
            approval_source,
            approved_at,
            approval_note,
        )
    }

    pub fn pending_review(proposal: &SkillProposal, validation_report: ValidationReport) -> Self {
        Self::pending_ticket(proposal, validation_report)
    }

    pub fn approved_review(
        proposal: &SkillProposal,
        validation_report: ValidationReport,
        approval_source: String,
        approved_at: Option<String>,
        approval_note: Option<String>,
    ) -> Self {
        Self::approved_ticket(
            proposal,
            validation_report,
            approval_source,
            approved_at,
            approval_note,
        )
    }

    pub fn approval_state(&self) -> SkillApprovalState {
        self.approval_receipt.approval_state()
    }

    pub fn is_pending_review(&self) -> bool {
        self.approval_receipt.is_pending()
    }

    pub fn is_approved_review(&self) -> bool {
        self.approval_receipt.is_approved()
    }

    pub fn validate_consistency(&self) -> Result<(), String> {
        self.approval_receipt.validate_consistency()?;

        if self.proposal_id.trim().is_empty() {
            return Err("proposal_id must not be empty".to_string());
        }

        if self.approval_receipt.proposal_id != self.proposal_id {
            return Err("approval_receipt.proposal_id must match proposal_id".to_string());
        }

        let expected_ticket_id = match self.approval_state() {
            SkillApprovalState::Pending => format!("pending-solidify-{}", self.proposal_id),
            SkillApprovalState::Approved => format!("approved-solidify-{}", self.proposal_id),
        };

        if self.ticket_id != expected_ticket_id {
            return Err("ticket_id must match approval state and proposal_id".to_string());
        }

        if !self.dry_run {
            return Err("solidify tickets must remain dry_run=true".to_string());
        }

        if self.writes_skills {
            return Err("solidify tickets must remain writes_skills=false".to_string());
        }

        if self.solidifies_skill {
            return Err("solidify tickets must remain solidifies_skill=false".to_string());
        }

        if !self.local_only {
            return Err("solidify tickets must remain local_only=true".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionReceipt {
    pub accepted: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionError {
    InvalidEvent(String),
    InvalidScope(String),
    InvalidProposal(String),
    InvalidRuleChange(String),
    ValidationRejected(Vec<String>),
    StorageError(String),
}

pub trait SkillEvolver {
    fn observe(&mut self, event: RuntimeEvent) -> Result<EvolutionReceipt, EvolutionError>;
    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError>;
    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError>;
    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError>;
}

fn validate_scope(scope: &EvolutionScope) -> Result<(), EvolutionError> {
    if scope.max_proposals == 0 {
        return Err(EvolutionError::InvalidScope(
            "max_proposals must be greater than zero".to_string(),
        ));
    }

    if scope.agent_id.trim().is_empty() {
        return Err(EvolutionError::InvalidScope(
            "agent_id must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_event(event: &RuntimeEvent) -> Result<(), EvolutionError> {
    if event.event_id.trim().is_empty() {
        return Err(EvolutionError::InvalidEvent(
            "event_id must not be empty".to_string(),
        ));
    }

    if event.task_id.trim().is_empty() {
        return Err(EvolutionError::InvalidEvent(
            "task_id must not be empty".to_string(),
        ));
    }

    if event.summary.trim().is_empty() {
        return Err(EvolutionError::InvalidEvent(
            "summary must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_proposal(proposal: &SkillProposal) -> Result<(), EvolutionError> {
    if proposal.proposal_id.trim().is_empty() {
        return Err(EvolutionError::InvalidProposal(
            "proposal_id must not be empty".to_string(),
        ));
    }

    if proposal.title.trim().is_empty() {
        return Err(EvolutionError::InvalidProposal(
            "title must not be empty".to_string(),
        ));
    }

    if proposal.trigger.trim().is_empty() {
        return Err(EvolutionError::InvalidProposal(
            "trigger must not be empty".to_string(),
        ));
    }

    if proposal.procedure.is_empty() {
        return Err(EvolutionError::InvalidProposal(
            "procedure must not be empty".to_string(),
        ));
    }

    if proposal.evidence_event_ids.is_empty() {
        return Err(EvolutionError::InvalidProposal(
            "evidence_event_ids must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_rule_change_proposal(
    proposal: &crate::skill_evolver::rule_change::RuleChangeProposal,
) -> Result<(), EvolutionError> {
    if proposal.proposal_id.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "proposal_id must not be empty".to_string(),
        ));
    }

    if proposal.rule_id.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "rule_id must not be empty".to_string(),
        ));
    }

    if proposal.title.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "title must not be empty".to_string(),
        ));
    }

    if proposal.trigger.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "trigger must not be empty".to_string(),
        ));
    }

    if proposal.new_procedure.is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "new_procedure must not be empty".to_string(),
        ));
    }

    if proposal.rationale.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "rationale must not be empty".to_string(),
        ));
    }

    if proposal.evidence.is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "evidence must not be empty".to_string(),
        ));
    }

    if !proposal.writes_rules {
        return Err(EvolutionError::InvalidRuleChange(
            "applying rule changes requires writes_rules=true".to_string(),
        ));
    }

    if !proposal.requires_governance {
        return Err(EvolutionError::InvalidRuleChange(
            "applying rule changes requires requires_governance=true".to_string(),
        ));
    }

    if proposal.provenance.is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "provenance must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_governance_decision(
    decision: &crate::skill_evolver::rule_change::GovernanceDecision,
    expected_proposal_id: &str,
) -> Result<(), EvolutionError> {
    if decision.proposal_id.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "governance decision proposal_id must not be empty".to_string(),
        ));
    }

    if decision.proposal_id != expected_proposal_id {
        return Err(EvolutionError::InvalidRuleChange(
            "governance decision proposal_id must match the evaluated proposal".to_string(),
        ));
    }

    if decision.approval_source.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "approval_source must not be empty".to_string(),
        ));
    }

    if decision.decided_by.trim().is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "decided_by must not be empty".to_string(),
        ));
    }

    if decision.reasons.is_empty() {
        return Err(EvolutionError::InvalidRuleChange(
            "governance decision must record audit reasons".to_string(),
        ));
    }

    Ok(())
}
