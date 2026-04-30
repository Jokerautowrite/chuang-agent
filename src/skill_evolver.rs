use std::collections::BTreeMap;

mod noop;

pub use noop::NoopEvolver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvent {
    pub event_id: String,
    pub task_id: String,
    pub kind: RuntimeEventKind,
    pub summary: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillProposal {
    pub proposal_id: String,
    pub title: String,
    pub trigger: String,
    pub procedure: Vec<String>,
    pub evidence_event_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    pub proposal_id: String,
    pub accepted: bool,
    pub reasons: Vec<String>,
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
    ValidationRejected(Vec<String>),
}

pub trait SkillEvolver {
    fn observe(&mut self, event: RuntimeEvent) -> Result<EvolutionReceipt, EvolutionError>;
    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError>;
    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError>;
    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError>;
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

    Ok(())
}
