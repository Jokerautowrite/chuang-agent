use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Default)]
pub struct NoopEvolver {
    observed_events: Vec<RuntimeEvent>,
}

impl NoopEvolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observed_events(&self) -> &[RuntimeEvent] {
        &self.observed_events
    }
}

impl SkillEvolver for NoopEvolver {
    fn observe(&mut self, event: RuntimeEvent) -> Result<EvolutionReceipt, EvolutionError> {
        validate_event(&event)?;
        self.observed_events.push(event);

        Ok(EvolutionReceipt {
            accepted: true,
            message: "event recorded; noop evolver does not propose skills".to_string(),
        })
    }

    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError> {
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

        Ok(Vec::new())
    }

    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError> {
        validate_proposal(proposal)?;

        Ok(ValidationReport {
            proposal_id: proposal.proposal_id.clone(),
            accepted: false,
            reasons: vec!["noop evolver never validates new skills".to_string()],
        })
    }

    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError> {
        validate_proposal(&proposal)?;
        Err(EvolutionError::ValidationRejected(vec![
            "noop evolver cannot solidify skills".to_string(),
        ]))
    }
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
