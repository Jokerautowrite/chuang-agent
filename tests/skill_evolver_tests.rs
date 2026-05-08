use std::collections::BTreeMap;

use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionError, EvolutionScope, NoopEvolver, RuntimeEvent,
    RuntimeEventKind, SkillApprovalReceipt, SkillApprovalState, SkillEvolver, SkillProposal,
    SkillProposalProvenance, SkillSolidifyTicket, ValidationReport,
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

fn validation_report(accepted: bool) -> ValidationReport {
    ValidationReport {
        proposal_id: "proposal-1".to_string(),
        accepted,
        reasons: if accepted {
            vec!["approved by operator".to_string()]
        } else {
            vec!["approval still required".to_string()]
        },
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

#[test]
fn skill_approval_receipt_helpers_validate_state_and_serialize() {
    let pending =
        SkillApprovalReceipt::pending_receipt("proposal-1".to_string(), validation_report(false));

    assert!(pending.is_pending());
    assert_eq!(pending.approval_state(), SkillApprovalState::Pending);
    assert!(pending.validate_consistency().is_ok());

    let pending_json = serde_json::to_value(&pending).expect("pending receipt should serialize");
    assert_eq!(pending_json["approved"], false);
    assert_eq!(pending_json["approval_source"], "pending_operator_approval");
    assert_eq!(
        pending_json["validation_report"]["proposal_id"],
        "proposal-1"
    );

    let approved = SkillApprovalReceipt::approved_receipt(
        "proposal-1".to_string(),
        validation_report(true),
        "manual_review".to_string(),
        Some("2026-05-08T10:00:00+08:00".to_string()),
        Some("approval granted".to_string()),
    );

    assert!(approved.is_approved());
    assert_eq!(approved.approval_state(), SkillApprovalState::Approved);
    assert!(approved.validate_consistency().is_ok());

    let approved_json = serde_json::to_value(&approved).expect("approved receipt should serialize");
    assert_eq!(approved_json["approved"], true);
    assert_eq!(approved_json["approval_source"], "manual_review");
    assert_eq!(approved_json["approved_at"], "2026-05-08T10:00:00+08:00");
}

#[test]
fn skill_approval_helpers_reject_inconsistent_state() {
    let mut approved = SkillApprovalReceipt::approved_receipt(
        "proposal-1".to_string(),
        validation_report(false),
        "manual_review".to_string(),
        None,
        Some("approval granted".to_string()),
    );

    assert!(approved.validate_consistency().is_err());

    approved.validation_report = validation_report(true);
    assert!(approved.validate_consistency().is_ok());

    let mut ticket = SkillSolidifyTicket::approved_ticket(
        &proposal(),
        validation_report(true),
        "manual_review".to_string(),
        None,
        Some("approval granted".to_string()),
    );

    assert!(ticket.is_approved_review());
    assert_eq!(ticket.approval_state(), SkillApprovalState::Approved);
    assert!(ticket.validate_consistency().is_ok());

    ticket.local_only = false;

    let err = ticket
        .validate_consistency()
        .expect_err("mutated ticket should fail validation");

    assert!(err.contains("local_only=true"));
    assert_eq!(
        serde_json::to_value(&ticket).unwrap()["ticket_id"],
        "approved-solidify-proposal-1"
    );
}

#[test]
fn skill_solidify_ticket_helpers_validate_pending_and_approved_forms() {
    let proposal = proposal();
    let pending = SkillSolidifyTicket::pending_ticket(&proposal, validation_report(false));

    assert!(pending.is_pending_review());
    assert_eq!(pending.approval_state(), SkillApprovalState::Pending);
    assert!(pending.validate_consistency().is_ok());

    let pending_json = serde_json::to_value(&pending).expect("pending ticket should serialize");
    assert_eq!(pending_json["dry_run"], true);
    assert_eq!(pending_json["writes_skills"], false);
    assert_eq!(pending_json["solidifies_skill"], false);
    assert_eq!(pending_json["local_only"], true);
    assert_eq!(pending_json["approval_receipt"]["approved"], false);

    let approved = SkillSolidifyTicket::approved_ticket(
        &proposal,
        validation_report(true),
        "manual_review".to_string(),
        Some("2026-05-08T10:00:00+08:00".to_string()),
        Some("approval granted".to_string()),
    );

    assert!(approved.is_approved_review());
    assert_eq!(approved.approval_state(), SkillApprovalState::Approved);
    assert!(approved.validate_consistency().is_ok());

    let approved_json = serde_json::to_value(&approved).expect("approved ticket should serialize");
    assert_eq!(approved_json["ticket_id"], "approved-solidify-proposal-1");
    assert_eq!(approved_json["approval_receipt"]["approved"], true);
    assert_eq!(
        approved_json["approval_receipt"]["approval_source"],
        "manual_review"
    );
}

#[test]
fn skill_solidify_ticket_receipt_aliases_remain_local_only() {
    let proposal = proposal();
    let approval_receipt = SkillSolidifyTicket::approval_receipt(
        &proposal,
        validation_report(true),
        "manual_review".to_string(),
        None,
        Some("approval granted".to_string()),
    );
    let refusal_receipt = SkillSolidifyTicket::solidify_refusal_receipt(
        &proposal,
        validation_report(true),
        "manual_review".to_string(),
        None,
        Some("approval granted".to_string()),
    );

    assert!(approval_receipt.is_approved_review());
    assert!(refusal_receipt.is_approved_review());
    assert!(approval_receipt.validate_consistency().is_ok());
    assert!(refusal_receipt.validate_consistency().is_ok());
    assert!(approval_receipt.local_only);
    assert!(refusal_receipt.local_only);
    assert_eq!(approval_receipt.ticket_id, "approved-solidify-proposal-1");
    assert_eq!(refusal_receipt.ticket_id, "approved-solidify-proposal-1");
}
