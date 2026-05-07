use super::{
    validate_event, validate_proposal, validate_scope, EvolutionError, EvolutionReceipt,
    EvolutionScope, RuntimeEvent, SkillEvolver, SkillId, SkillProposal, ValidationReport,
};

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
        validate_scope(&scope)?;
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
