use std::collections::BTreeMap;

use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionError, EvolutionScope, NoopEvolver, RuntimeEvent,
    RuntimeEventKind, SkillEvolver, SkillProposal, SkillProposalProvenance,
};

fn event() -> RuntimeEvent {
    RuntimeEvent {
        event_id: "event-1".to_string(),
        task_id: "task-1".to_string(),
        kind: RuntimeEventKind::TurnCompleted,
        summary: "用户确认当前 runtime 配置闭环可用".to_string(),
        metadata: BTreeMap::from([
            ("source".to_string(), "test".to_string()),
            ("task_kind".to_string(), "runtime".to_string()),
        ]),
    }
}

fn proposal() -> SkillProposal {
    SkillProposal {
        proposal_id: "proposal-1".to_string(),
        title: "配置闭环检查".to_string(),
        trigger: "runtime config changed".to_string(),
        procedure: vec![
            "run cargo fmt".to_string(),
            "run cargo test".to_string(),
            "record progress log".to_string(),
        ],
        evidence_event_ids: vec!["event-1".to_string()],
        dry_run: true,
        writes_skills: false,
        requires_approval: true,
        provenance: vec![SkillProposalProvenance {
            source_event_id: "event-1".to_string(),
            source_task_id: "task-1".to_string(),
            source_kind: RuntimeEventKind::TurnCompleted,
            source_summary: "用户确认当前 runtime 配置闭环可用".to_string(),
            source_metadata: BTreeMap::from([
                ("source".to_string(), "test".to_string()),
                ("task_kind".to_string(), "runtime".to_string()),
            ]),
        }],
    }
}

#[test]
fn noop_evolver_records_observed_events_without_generating_skills() {
    let mut evolver = NoopEvolver::new();

    let receipt = evolver
        .observe(event())
        .expect("valid event should be accepted");

    assert!(receipt.accepted);
    assert_eq!(evolver.observed_events().len(), 1);
    assert!(receipt.message.contains("noop evolver"));
}

#[test]
fn noop_evolver_returns_no_proposals_for_valid_scope() {
    let evolver = NoopEvolver::new();

    let proposals = evolver
        .propose(EvolutionScope {
            agent_id: "xiaoce".to_string(),
            task_kind: Some("runtime".to_string()),
            max_proposals: 3,
        })
        .expect("valid scope should pass");

    assert!(proposals.is_empty());
}

#[test]
fn noop_evolver_rejects_invalid_scope() {
    let evolver = NoopEvolver::new();

    let err = evolver
        .propose(EvolutionScope {
            agent_id: "xiaoce".to_string(),
            task_kind: None,
            max_proposals: 0,
        })
        .expect_err("zero max proposals should fail");

    assert!(matches!(err, EvolutionError::InvalidScope(_)));
}

#[test]
fn noop_evolver_validates_shape_but_does_not_accept_skill() {
    let evolver = NoopEvolver::new();

    let report = evolver
        .validate(&proposal())
        .expect("well-shaped proposal should produce report");

    assert_eq!(report.proposal_id, "proposal-1");
    assert!(!report.accepted);
    assert!(report.reasons[0].contains("noop evolver"));
}

#[test]
fn noop_evolver_never_solidifies_skill() {
    let mut evolver = NoopEvolver::new();

    let err = evolver
        .solidify(proposal())
        .expect_err("noop evolver should not write skills");

    assert!(matches!(err, EvolutionError::ValidationRejected(_)));
}

#[test]
fn noop_evolver_rejects_empty_event_identity() {
    let mut evolver = NoopEvolver::new();
    let mut event = event();
    event.event_id.clear();

    let err = evolver
        .observe(event)
        .expect_err("empty event id should fail");

    assert!(matches!(err, EvolutionError::InvalidEvent(_)));
}

#[test]
fn dry_run_evolver_converts_observations_to_safe_proposals() {
    let mut evolver = DryRunProposalEvolver::new();

    evolver
        .observe(event())
        .expect("valid event should be accepted");

    let proposals = evolver
        .propose(EvolutionScope {
            agent_id: "xiaoce".to_string(),
            task_kind: Some("runtime".to_string()),
            max_proposals: 3,
        })
        .expect("valid scope should produce proposals");

    assert_eq!(proposals.len(), 1);
    let proposal = &proposals[0];
    assert!(proposal.dry_run);
    assert!(!proposal.writes_skills);
    assert!(proposal.requires_approval);
    assert_eq!(proposal.evidence_event_ids, vec!["event-1"]);
    assert_eq!(proposal.provenance.len(), 1);
    assert_eq!(proposal.provenance[0].source_event_id, "event-1");
    assert_eq!(
        proposal.provenance[0].source_metadata.get("source"),
        Some(&"test".to_string())
    );
}

#[test]
fn dry_run_evolver_validates_boundaries_without_solidifying() {
    let mut evolver = DryRunProposalEvolver::new();
    let proposal = proposal();

    let report = evolver
        .validate(&proposal)
        .expect("well-shaped dry-run proposal should validate");

    assert!(report.accepted);
    assert_eq!(report.proposal_id, "proposal-1");
    assert!(report.reasons[0].contains("approval is still required"));

    let err = evolver
        .solidify(proposal)
        .expect_err("dry-run evolver should not write skills");

    assert!(matches!(err, EvolutionError::ValidationRejected(_)));
}

#[test]
fn dry_run_evolver_rejects_unsafe_proposal_markers() {
    let evolver = DryRunProposalEvolver::new();
    let mut proposal = proposal();
    proposal.dry_run = false;
    proposal.writes_skills = true;
    proposal.requires_approval = false;
    proposal.provenance.clear();

    let report = evolver
        .validate(&proposal)
        .expect("shape is valid enough to report boundary failures");

    assert!(!report.accepted);
    assert!(report
        .reasons
        .iter()
        .any(|reason| reason.contains("dry_run=true")));
    assert!(report
        .reasons
        .iter()
        .any(|reason| reason.contains("writes_skills=false")));
    assert!(report
        .reasons
        .iter()
        .any(|reason| reason.contains("requires_approval=true")));
    assert!(report
        .reasons
        .iter()
        .any(|reason| reason.contains("preserve provenance")));
}
