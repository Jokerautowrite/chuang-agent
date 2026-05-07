use super::{
    validate_event, validate_proposal, validate_scope, EvolutionError, EvolutionReceipt,
    EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver, SkillId, SkillProposal,
    SkillProposalProvenance, ValidationReport,
};

#[derive(Debug, Clone, Default)]
pub struct DryRunProposalEvolver {
    observed_events: Vec<RuntimeEvent>,
}

impl DryRunProposalEvolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observed_events(&self) -> &[RuntimeEvent] {
        &self.observed_events
    }

    fn proposal_from_event(scope: &EvolutionScope, event: &RuntimeEvent) -> SkillProposal {
        SkillProposal {
            proposal_id: format!(
                "dry-run-{}-{}",
                stable_id_part(&scope.agent_id),
                stable_id_part(&event.event_id)
            ),
            title: format!("Dry-run skill candidate for {}", scope.agent_id),
            trigger: format!(
                "Observed {} during task {}",
                event_kind_label(&event.kind),
                event.task_id
            ),
            procedure: vec![
                "Review the preserved event provenance.".to_string(),
                format!(
                    "Consider whether this observation should become a reusable skill: {}",
                    event.summary
                ),
                "Request explicit approval before writing or solidifying any skill.".to_string(),
            ],
            evidence_event_ids: vec![event.event_id.clone()],
            dry_run: true,
            writes_skills: false,
            requires_approval: true,
            provenance: vec![SkillProposalProvenance {
                source_event_id: event.event_id.clone(),
                source_task_id: event.task_id.clone(),
                source_kind: event.kind.clone(),
                source_summary: event.summary.clone(),
                source_metadata: event.metadata.clone(),
            }],
        }
    }

    fn event_matches_scope(scope: &EvolutionScope, event: &RuntimeEvent) -> bool {
        match &scope.task_kind {
            Some(task_kind) => match event.metadata.get("task_kind") {
                Some(event_task_kind) => event_task_kind == task_kind,
                None => true,
            },
            None => true,
        }
    }
}

impl SkillEvolver for DryRunProposalEvolver {
    fn observe(&mut self, event: RuntimeEvent) -> Result<EvolutionReceipt, EvolutionError> {
        validate_event(&event)?;
        self.observed_events.push(event);

        Ok(EvolutionReceipt {
            accepted: true,
            message:
                "event recorded; dry-run evolver can propose candidates but cannot write skills"
                    .to_string(),
        })
    }

    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError> {
        validate_scope(&scope)?;

        Ok(self
            .observed_events
            .iter()
            .filter(|event| Self::event_matches_scope(&scope, event))
            .take(scope.max_proposals)
            .map(|event| Self::proposal_from_event(&scope, event))
            .collect())
    }

    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError> {
        validate_proposal(proposal)?;

        let mut reasons = Vec::new();
        if !proposal.dry_run {
            reasons.push("proposal must be marked dry_run=true".to_string());
        }
        if proposal.writes_skills {
            reasons.push("dry-run proposal must be marked writes_skills=false".to_string());
        }
        if !proposal.requires_approval {
            reasons.push("dry-run proposal must be marked requires_approval=true".to_string());
        }
        if proposal.provenance.is_empty() {
            reasons.push("dry-run proposal must preserve provenance".to_string());
        }

        let accepted = reasons.is_empty();
        if accepted {
            reasons.push(
                "dry-run proposal is structurally valid; approval is still required before any skill write"
                    .to_string(),
            );
        }

        Ok(ValidationReport {
            proposal_id: proposal.proposal_id.clone(),
            accepted,
            reasons,
        })
    }

    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError> {
        validate_proposal(&proposal)?;
        Err(EvolutionError::ValidationRejected(vec![
            "dry-run evolver cannot solidify or write skills".to_string(),
        ]))
    }
}

fn stable_id_part(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();

    let trimmed = normalized.trim_matches('-');
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed.to_string()
    }
}

fn event_kind_label(kind: &RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::TurnCompleted => "turn_completed",
        RuntimeEventKind::ToolSucceeded => "tool_succeeded",
        RuntimeEventKind::ToolFailed => "tool_failed",
        RuntimeEventKind::UserCorrection => "user_correction",
        RuntimeEventKind::ManualObservation => "manual_observation",
    }
}
