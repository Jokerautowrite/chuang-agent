use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::skill_evolver::{
    CanonicalSkillEvolver, DryRunProposalEvolver, EvolutionError, EvolutionScope, NoopEvolver,
    RuntimeEvent, RuntimeEventKind, SkillApprovalReceipt, SkillApprovalState, SkillEvolver,
    SkillLifecycleStatus, SkillProposal, SkillProposalProvenance, SkillRetirementRequest,
    SkillSolidifyTicket, SkillUpsertKind, ValidationReport,
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

fn temp_skill_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "chuang-skill-evolver-test-{}-{}-{}",
        name,
        std::process::id(),
        nanos
    ))
}

fn canonical_proposal(title: &str, proposal_id: &str) -> SkillProposal {
    SkillProposal {
        proposal_id: proposal_id.to_string(),
        title: title.to_string(),
        trigger: "runtime skill workflow repeats across tasks".to_string(),
        procedure: vec![
            "Review source provenance before changing the canonical skill file.".to_string(),
            "Apply the repeatable workflow and keep governance, risk, and approval boundaries visible.".to_string(),
            "Run a verification check or test, then record the result for future maintenance.".to_string(),
        ],
        evidence_event_ids: vec!["event-1".to_string()],
        dry_run: false,
        writes_skills: true,
        requires_approval: false,
        provenance: vec![SkillProposalProvenance {
            source_event_id: "event-1".to_string(),
            source_task_id: "task-1".to_string(),
            source_kind: RuntimeEventKind::TurnCompleted,
            source_summary: "workflow completed and verified in a real runtime turn".to_string(),
            source_metadata: BTreeMap::from([("source".to_string(), "test".to_string())]),
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

#[test]
fn canonical_evolver_self_approves_and_solidifies_new_skill() {
    let root = temp_skill_root("solidify-new");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    let proposal = canonical_proposal("Runtime Skill Workflow", "proposal-runtime-workflow");

    let validation = evolver
        .validate(&proposal)
        .expect("canonical proposal should be scored");

    assert!(validation.accepted);
    assert!(validation
        .reasons
        .iter()
        .any(|reason| reason.contains("total_score=")));

    let receipt = evolver
        .solidify_with_receipt(proposal)
        .expect("approved proposal should write a canonical skill");

    assert_eq!(receipt.kind, SkillUpsertKind::Created);
    assert_eq!(receipt.status, SkillLifecycleStatus::Active);
    assert!(receipt.writes_skills);
    assert!(!receipt.deletes_skill);
    assert_eq!(receipt.skill_id, "runtime-skill-workflow");
    assert_eq!(receipt.version, 1);
    assert_eq!(
        evolver
            .last_solidify_receipt()
            .map(|receipt| &receipt.skill_id),
        Some(&receipt.skill_id)
    );

    let written = fs::read_to_string(&receipt.path).expect("skill file should be readable");
    assert!(written.contains("skill_id: runtime-skill-workflow"));
    assert!(written.contains("status: active"));
    assert!(written.contains("approval_source: self_policy:darwin_rubric"));
    assert!(written.contains("retirement policy: deprecate or retire in place"));
}

#[test]
fn canonical_evolver_upserts_duplicate_instead_of_creating_copy() {
    let root = temp_skill_root("duplicate-upsert");
    let mut evolver = CanonicalSkillEvolver::new(&root);

    let first = evolver
        .solidify_with_receipt(canonical_proposal(
            "Runtime Skill Workflow",
            "proposal-runtime-workflow-1",
        ))
        .expect("first proposal should create skill");
    let second = evolver
        .solidify_with_receipt(canonical_proposal(
            "workflow runtime skill",
            "proposal-runtime-workflow-2",
        ))
        .expect("duplicate proposal should update skill");

    assert_eq!(second.kind, SkillUpsertKind::Updated);
    assert!(second.duplicate_decision.duplicate_found);
    assert_eq!(second.duplicate_decision.reason, "canonical_id_match");
    assert_eq!(first.path, second.path);
    assert_eq!(second.version, 2);

    let md_count = fs::read_dir(&root)
        .expect("skill root should be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .count();
    assert_eq!(md_count, 1);
}

#[test]
fn canonical_evolver_uses_heading_fallback_for_existing_repo_skill_without_frontmatter() {
    let root = temp_skill_root("repo-skill-fallback");
    fs::create_dir_all(&root).expect("test skill root should be creatable");
    let existing_path = root.join("external_agent_dispatch_sop.md");
    let existing = fs::read_to_string("data/skills/external_agent_dispatch_sop.md")
        .expect("repo skill fixture should be readable");
    let markdown_only = if let Some(rest) = existing.strip_prefix("---\n") {
        let end = rest
            .find("\n---\n")
            .expect("repo skill frontmatter should be closed");
        let body = rest[end + "\n---\n".len()..].trim_start();
        let heading = body
            .find("# External Agent Dispatch SOP")
            .expect("repo skill body should retain the canonical heading");
        body[heading..].to_string()
    } else {
        existing
    };
    assert!(
        markdown_only.starts_with("# External Agent Dispatch SOP"),
        "fixture intentionally covers markdown-only heading fallback"
    );
    fs::write(&existing_path, markdown_only).expect("repo skill fixture should be copied");

    let mut evolver = CanonicalSkillEvolver::new(&root);
    let receipt = evolver
        .solidify_with_receipt(canonical_proposal(
            "External Agent Dispatch SOP",
            "proposal-existing-dispatch-sop",
        ))
        .expect("heading fallback should let the existing skill be upserted");

    assert_eq!(receipt.kind, SkillUpsertKind::Updated);
    assert!(receipt.duplicate_decision.duplicate_found);
    assert_eq!(receipt.duplicate_decision.reason, "normalized_title_match");
    assert_eq!(receipt.skill_id, "external_agent_dispatch_sop");
    assert_eq!(receipt.path, existing_path);
    assert_eq!(receipt.version, 2);

    let md_count = fs::read_dir(&root)
        .expect("skill root should be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .count();
    assert_eq!(md_count, 1);
}

#[test]
fn canonical_evolver_rejects_low_scoring_proposal_without_writing() {
    let root = temp_skill_root("reject-low-score");
    let mut evolver = CanonicalSkillEvolver::new(&root).with_approval_threshold(95);
    let mut weak = canonical_proposal("Weak Workflow", "proposal-weak-workflow");
    weak.procedure = vec!["Do it.".to_string()];
    weak.provenance.clear();

    let err = evolver
        .solidify_with_receipt(weak)
        .expect_err("proposal below threshold should not write");

    assert!(matches!(err, EvolutionError::ValidationRejected(_)));
    assert!(!root.exists());
}

#[test]
fn canonical_evolver_retires_skill_in_place_without_deleting_history() {
    let root = temp_skill_root("retire");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    let receipt = evolver
        .solidify_with_receipt(canonical_proposal(
            "Runtime Skill Workflow",
            "proposal-runtime-workflow-retire",
        ))
        .expect("skill should be written before retirement");

    let retirement = evolver
        .retire(SkillRetirementRequest {
            skill_id: receipt.skill_id.clone(),
            target_status: SkillLifecycleStatus::Deprecated,
            reason: "superseded by stronger canonical workflow".to_string(),
            score: Some(42),
        })
        .expect("retirement should update the skill file in place");

    assert_eq!(retirement.previous_status, SkillLifecycleStatus::Active);
    assert_eq!(retirement.status, SkillLifecycleStatus::Deprecated);
    assert!(retirement.writes_skills);
    assert!(!retirement.deletes_skill);
    assert_eq!(retirement.path, receipt.path);
    assert!(retirement.path.exists());

    let updated =
        fs::read_to_string(retirement.path).expect("retired skill should remain readable");
    assert!(updated.contains("status: deprecated"));
    assert!(updated.contains("retirement_reason: \"superseded by stronger canonical workflow\""));
    assert!(updated.contains("retirement_score: 42"));
}

#[test]
fn canonical_evolver_retires_seeded_canonical_frontmatter_file_in_place() {
    let root = temp_skill_root("retire-seeded-frontmatter");
    fs::create_dir_all(&root).expect("test skill root should be creatable");
    let path = root.join("canonical_existing_skill.md");
    fs::write(
        &path,
        r#"---
skill_id: canonical_existing_skill
title: "Canonical Existing Skill"
trigger: "when an older canonical skill needs lifecycle maintenance"
version: 4
status: active
---

# Canonical Existing Skill

## Procedure

- Keep this body for audit.
"#,
    )
    .expect("seeded canonical skill should be writable");

    let evolver = CanonicalSkillEvolver::new(&root);
    let retirement = evolver
        .retire(SkillRetirementRequest {
            skill_id: "canonical_existing_skill".to_string(),
            target_status: SkillLifecycleStatus::Retired,
            reason: "replaced by a better maintained canonical skill".to_string(),
            score: Some(31),
        })
        .expect("seeded canonical file should retire in place");

    assert_eq!(retirement.previous_status, SkillLifecycleStatus::Active);
    assert_eq!(retirement.status, SkillLifecycleStatus::Retired);
    assert_eq!(retirement.path, path);
    assert!(retirement.path.exists());
    assert!(!retirement.deletes_skill);

    let updated = fs::read_to_string(&retirement.path).expect("retired skill should be readable");
    assert!(updated.contains("skill_id: canonical_existing_skill"));
    assert!(updated.contains("version: 5"));
    assert!(updated.contains("status: retired"));
    assert!(
        updated.contains("retirement_reason: \"replaced by a better maintained canonical skill\"")
    );
    assert!(updated.contains("retirement_score: 31"));
    assert!(updated.contains("Keep this body for audit."));
}

#[test]
fn canonical_evolver_upserts_repo_skill_seed_using_frontmatter_metadata() {
    let root = temp_skill_root("repo-skill-seed");
    fs::create_dir_all(&root).expect("test skill root should be creatable");
    let seed_path = root.join("external_agent_dispatch_sop.md");
    fs::copy("data/skills/external_agent_dispatch_sop.md", &seed_path)
        .expect("repo skill seed should be copyable");

    let mut evolver = CanonicalSkillEvolver::new(&root);
    let receipt = evolver
        .solidify_with_receipt(canonical_proposal(
            "External Agent Dispatch SOP",
            "proposal-external-agent-dispatch",
        ))
        .expect("repo skill seed should upsert in place");

    assert_eq!(receipt.kind, SkillUpsertKind::Updated);
    assert_eq!(receipt.path, seed_path);
    assert_eq!(receipt.duplicate_decision.reason, "normalized_title_match");
    assert_eq!(
        receipt.duplicate_decision.canonical_skill_id,
        "external_agent_dispatch_sop"
    );
    assert_eq!(receipt.version, 2);

    let updated = fs::read_to_string(&receipt.path).expect("updated seed should remain readable");
    assert!(updated.contains("skill_id: external_agent_dispatch_sop"));
    assert!(updated.contains("title: \"External Agent Dispatch SOP\""));
    assert!(updated.contains("status: active"));
    assert!(updated
        .contains("duplicate policy: update the canonical skill instead of creating copies."));
}
