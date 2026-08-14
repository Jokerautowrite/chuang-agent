use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::benchmark::{
    BenchmarkCase, BenchmarkDef, BenchmarkRunRequest, BenchmarkStore, CaseScore,
};
use chuang_agent::skill_evolver::{
    BenchmarkScoreGate, CanonicalSkillEvolver, DryRunProposalEvolver, EvolutionError,
    EvolutionScope, FailureDetectorConfig, FailureEvidence, FailurePattern, FixedScoreScorer,
    GovernanceContext, GovernanceDecision, NoopEvolver, NoopRuleChangeGovernance,
    PolicyRuleChangeGovernance, RepeatedFailureDetector, RuleChangeGovernance, RuleChangeKind,
    RuleChangeProposal, RuntimeEvent, RuntimeEventKind, SkillApprovalReceipt, SkillApprovalState,
    SkillEvolver, SkillLifecycleStatus, SkillProposal, SkillProposalProvenance,
    SkillRetirementRequest, SkillScoringGateConfig, SkillSolidifyTicket, SkillUpsertKind,
    ValidationReport,
};

const EXTERNAL_AGENT_DISPATCH_SKILL_SEED: &str = r#"---
skill_id: external_agent_dispatch_sop
title: "External Agent Dispatch SOP"
version: 1
status: active
---
# External Agent Dispatch SOP

Keep one canonical workflow and apply the duplicate policy: update the canonical skill instead of creating copies.
"#;

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
    let existing = EXTERNAL_AGENT_DISPATCH_SKILL_SEED.to_string();
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
    fs::write(&seed_path, EXTERNAL_AGENT_DISPATCH_SKILL_SEED)
        .expect("repo skill seed fixture should be writable");

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

// ======================= repeated-failure detection =======================

fn failure_event(id: &str, task: &str, tool: &str) -> RuntimeEvent {
    RuntimeEvent {
        event_id: id.to_string(),
        task_id: task.to_string(),
        kind: RuntimeEventKind::ToolFailed,
        summary: format!("tool {tool} failed"),
        metadata: BTreeMap::from([("tool".to_string(), tool.to_string())]),
    }
}

fn success_event(id: &str, tool: &str) -> RuntimeEvent {
    RuntimeEvent {
        event_id: id.to_string(),
        task_id: "task-success".to_string(),
        kind: RuntimeEventKind::ToolSucceeded,
        summary: format!("tool {tool} succeeded"),
        metadata: BTreeMap::from([("tool".to_string(), tool.to_string())]),
    }
}

#[test]
fn failure_detector_emits_pattern_when_repeats_meet_threshold() {
    let detector = RepeatedFailureDetector::new(FailureDetectorConfig::default());
    let events = vec![
        failure_event("f1", "t1", "build"),
        failure_event("f2", "t1", "build"),
        failure_event("f3", "t2", "build"),
    ];

    let patterns = detector.detect(&events);

    assert_eq!(patterns.len(), 1);
    let pattern = &patterns[0];
    assert_eq!(pattern.signature, "tool=build");
    assert_eq!(pattern.kind, RuntimeEventKind::ToolFailed);
    assert_eq!(pattern.count, 3);
    assert_eq!(pattern.event_ids, vec!["f1", "f2", "f3"]);
    assert_eq!(pattern.task_ids, vec!["t1", "t2"]);
    assert_eq!(pattern.first_seen_event_id, "f1");
    assert_eq!(pattern.last_seen_event_id, "f3");
    assert_eq!(pattern.window_size, 3);
    assert!(pattern.summary.contains("tool=build"));
    assert!(pattern.summary.contains("3 times"));
}

#[test]
fn failure_detector_ignores_below_threshold_and_non_failure_events() {
    let detector = RepeatedFailureDetector::new(FailureDetectorConfig::default());

    let below_threshold = vec![failure_event("f1", "t1", "build")];
    assert!(detector.detect(&below_threshold).is_empty());

    let mixed = vec![
        failure_event("f1", "t1", "build"),
        success_event("s1", "build"),
    ];
    assert!(detector.detect(&mixed).is_empty());
}

#[test]
fn failure_detector_honors_window() {
    let config = FailureDetectorConfig::default().window(2);
    let detector = RepeatedFailureDetector::new(config);
    let events = vec![
        failure_event("old", "t1", "build"),
        failure_event("new1", "t2", "build"),
        failure_event("new2", "t3", "build"),
    ];

    let patterns = detector.detect(&events);

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].count, 2);
    assert_eq!(patterns[0].event_ids, vec!["new1", "new2"]);
    assert_eq!(patterns[0].window_size, 2);
    assert_eq!(patterns[0].first_seen_event_id, "new1");
}

#[test]
fn failure_detector_skips_events_without_classifiable_signature() {
    let mut event = failure_event("f1", "t1", "build");
    event.metadata.clear();
    let detector = RepeatedFailureDetector::new(FailureDetectorConfig::default().min_repeats(1));

    assert!(detector.detect(&[event]).is_empty());
}

#[test]
fn failure_detector_groups_distinct_signatures_separately() {
    let detector = RepeatedFailureDetector::new(FailureDetectorConfig::default());
    let events = vec![
        failure_event("f1", "t1", "build"),
        failure_event("f2", "t1", "deploy"),
        failure_event("f3", "t2", "build"),
    ];

    let patterns = detector.detect(&events);

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].signature, "tool=build");
    assert_eq!(patterns[0].count, 2);
}

#[test]
fn canonical_evolver_detect_rejects_invalid_config() {
    let root = temp_skill_root("detect-invalid-config");
    let evolver = CanonicalSkillEvolver::new(&root);

    let err = evolver
        .detect_repeated_failures(&FailureDetectorConfig {
            min_repeats: 0,
            window: None,
            failure_kinds: vec![RuntimeEventKind::ToolFailed],
        })
        .expect_err("zero min_repeats should fail");

    assert!(matches!(err, EvolutionError::InvalidScope(_)));

    let err = evolver
        .detect_repeated_failures(&FailureDetectorConfig {
            min_repeats: 2,
            window: None,
            failure_kinds: Vec::new(),
        })
        .expect_err("empty failure_kinds should fail");

    assert!(matches!(err, EvolutionError::InvalidScope(_)));
}

// ======================= rule-change proposal =======================

#[test]
fn canonical_evolver_proposes_structured_rule_change_from_pattern() {
    let root = temp_skill_root("propose-rule-change");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    assert_eq!(patterns.len(), 1);

    let proposal = evolver
        .propose_rule_change(&patterns[0])
        .expect("pattern from observed events should produce a proposal");

    assert_eq!(proposal.change_kind, RuleChangeKind::CreateRule);
    assert_eq!(proposal.rule_id, "build-for-rule-tool");
    assert!(proposal.writes_rules);
    assert!(proposal.requires_governance);
    assert_eq!(proposal.evidence.len(), 1);
    assert_eq!(proposal.evidence[0].pattern_signature, "tool=build");
    assert_eq!(proposal.evidence[0].event_ids, vec!["f1", "f2"]);
    assert_eq!(proposal.evidence[0].task_ids, vec!["t1", "t2"]);
    assert_eq!(proposal.provenance.len(), 2);
    assert_eq!(proposal.provenance[0].source_event_id, "f1");
    assert!(proposal.old_procedure.is_empty());
    assert_eq!(proposal.new_procedure.len(), 3);
    assert!(proposal.rationale.contains("repeated failure tool=build"));
}

#[test]
fn canonical_evolver_propose_rejects_pattern_with_unknown_events() {
    let root = temp_skill_root("propose-invalid-pattern");
    let evolver = CanonicalSkillEvolver::new(&root);
    let pattern = FailurePattern {
        signature: "tool=build".to_string(),
        kind: RuntimeEventKind::ToolFailed,
        count: 1,
        window_size: 1,
        event_ids: vec!["ghost-event".to_string()],
        task_ids: vec!["t1".to_string()],
        first_seen_event_id: "ghost-event".to_string(),
        last_seen_event_id: "ghost-event".to_string(),
        summary: "not grounded".to_string(),
    };

    let err = evolver
        .propose_rule_change(&pattern)
        .expect_err("unverifiable evidence must not become a proposal");

    assert!(matches!(err, EvolutionError::InvalidProposal(_)));
}

// ======================= governance =======================

fn governance_context(events: Vec<RuntimeEvent>) -> GovernanceContext {
    GovernanceContext {
        observed_events: events,
        detector_config: FailureDetectorConfig::default(),
    }
}

fn base_rule_change_proposal() -> RuleChangeProposal {
    RuleChangeProposal {
        proposal_id: "proposal-rule-1".to_string(),
        rule_id: "build-for-rule-tool".to_string(),
        change_kind: RuleChangeKind::CreateRule,
        title: "Rule for tool=build".to_string(),
        trigger: "repeated failure tool=build observed 2 times".to_string(),
        old_procedure: Vec::new(),
        new_procedure: vec![
            "Review the repeated failure evidence and the existing rule before changing it."
                .to_string(),
            "Apply the corrective procedure for tool=build and capture the outcome.".to_string(),
            "Verify the fix with a check or test and record the governance boundary.".to_string(),
        ],
        rationale: "repeated failure needs a rule update".to_string(),
        evidence: vec![FailureEvidence {
            pattern_signature: "tool=build".to_string(),
            count: 2,
            event_ids: vec!["f1".to_string(), "f2".to_string()],
            task_ids: vec!["t1".to_string(), "t2".to_string()],
            summary: "repeated failure tool=build observed 2 times".to_string(),
        }],
        writes_rules: true,
        requires_governance: true,
        provenance: vec![SkillProposalProvenance {
            source_event_id: "f1".to_string(),
            source_task_id: "t1".to_string(),
            source_kind: RuntimeEventKind::ToolFailed,
            source_summary: "tool build failed".to_string(),
            source_metadata: BTreeMap::from([("tool".to_string(), "build".to_string())]),
        }],
    }
}

#[test]
fn policy_governance_approves_verifiable_evidence() {
    let root = temp_skill_root("governance-approve");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let proposal = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    let context = governance_context(evolver.observed_events().to_vec());

    let decision = PolicyRuleChangeGovernance::default()
        .evaluate(&proposal, &context)
        .expect("well-formed proposal should be evaluable");

    assert!(decision.approved);
    assert_eq!(decision.proposal_id, proposal.proposal_id);
    assert_eq!(
        decision.approval_source,
        "policy:repeated_failure_evidence_v1"
    );
    assert_eq!(decision.decided_by, "governance.policy");
    assert!(decision.decided_at.is_some());
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("verified 2 evidence event(s)")));
}

#[test]
fn policy_governance_rejects_missing_evidence_events() {
    let proposal = base_rule_change_proposal();
    let context = governance_context(Vec::new());

    let decision = PolicyRuleChangeGovernance::default()
        .evaluate(&proposal, &context)
        .expect("proposal should be evaluable");

    assert!(!decision.approved);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("not found in observed runtime events")));
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("nothing is written to disk")));
}

#[test]
fn policy_governance_rejects_weak_evidence_below_min_repeats() {
    let mut proposal = base_rule_change_proposal();
    proposal.evidence[0].count = 1;
    let context = governance_context(vec![failure_event("f1", "t1", "build")]);

    let decision = PolicyRuleChangeGovernance::default()
        .evaluate(&proposal, &context)
        .expect("proposal should be evaluable");

    assert!(!decision.approved);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("below min_repeats")));
}

#[test]
fn policy_governance_rejects_non_failure_kind_evidence() {
    let mut proposal = base_rule_change_proposal();
    proposal.evidence[0].event_ids = vec!["s1".to_string()];
    let context = governance_context(vec![success_event("s1", "build")]);

    let decision = PolicyRuleChangeGovernance::default()
        .evaluate(&proposal, &context)
        .expect("proposal should be evaluable");

    assert!(!decision.approved);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("not a configured failure kind")));
}

#[test]
fn noop_governance_never_approves_rule_changes() {
    let proposal = base_rule_change_proposal();
    let context = governance_context(vec![failure_event("f1", "t1", "build")]);

    let decision = NoopRuleChangeGovernance
        .evaluate(&proposal, &context)
        .expect("noop governance should be evaluable");

    assert!(!decision.approved);
    assert_eq!(decision.approval_source, "noop_governance");
    assert!(decision.reasons[0].contains("never approves"));
}

struct MismatchedGovernance;

impl RuleChangeGovernance for MismatchedGovernance {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
        _context: &GovernanceContext,
    ) -> Result<GovernanceDecision, EvolutionError> {
        Ok(GovernanceDecision {
            proposal_id: format!("other-{}", proposal.proposal_id),
            approved: true,
            reasons: vec!["test approval".to_string()],
            approval_source: "test".to_string(),
            decided_by: "test".to_string(),
            decided_at: None,
        })
    }
}

struct ApproveAllGovernance;

impl RuleChangeGovernance for ApproveAllGovernance {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
        _context: &GovernanceContext,
    ) -> Result<GovernanceDecision, EvolutionError> {
        Ok(GovernanceDecision {
            proposal_id: proposal.proposal_id.clone(),
            approved: true,
            reasons: vec!["test approval".to_string()],
            approval_source: "test".to_string(),
            decided_by: "test".to_string(),
            decided_at: Some("2026-08-10T00:00:00Z".to_string()),
        })
    }
}

// ======================= outer loop: apply / persist / rollback =======================

#[test]
fn outer_loop_approves_and_persists_new_rule_in_existing_format() {
    let root = temp_skill_root("outer-loop-create");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let proposal = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    let context = governance_context(evolver.observed_events().to_vec());

    let receipt = evolver
        .apply_rule_change(
            proposal.clone(),
            &PolicyRuleChangeGovernance::default(),
            &context,
        )
        .expect("governance-approved rule change should persist");

    assert_eq!(receipt.change_kind, RuleChangeKind::CreateRule);
    assert_eq!(receipt.rule_id, "build-for-rule-tool");
    assert_eq!(receipt.version, 1);
    assert_eq!(receipt.previous_version, None);
    assert!(receipt.writes_rules);
    assert!(!receipt.deletes_rules);
    assert_eq!(receipt.path, root.join("build-for-rule-tool.md"));
    assert!(receipt.path.exists());

    let written = fs::read_to_string(&receipt.path).expect("rule file should be readable");
    assert!(written.contains("skill_id: build-for-rule-tool"));
    assert!(written.contains("status: active"));
    assert!(written.contains("version: 1"));
    assert!(written.contains("approval_source: self_policy:darwin_rubric"));
    assert!(written.contains("evidence_event_ids:"));
    assert!(written.contains("  - f1"));
    assert!(written.contains("  - f2"));

    let history = evolver
        .rule_change_history()
        .expect("journal should be observable");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].entry_id, format!("rc-{}", proposal.proposal_id));
    assert_eq!(history[0].rule_id, "build-for-rule-tool");
    assert_eq!(history[0].version, 1);
    assert_eq!(history[0].previous_version, None);
    assert!(history[0].before.is_none());
    assert!(history[0].after.contains("skill_id: build-for-rule-tool"));
    assert_eq!(history[0].decision.proposal_id, proposal.proposal_id);
    assert!(history[0].decision.approved);
    assert!(evolver
        .last_rule_change_receipt()
        .is_some_and(|receipt| receipt.rule_id == "build-for-rule-tool"));

    assert!(evolver
        .rule_change_journal_path()
        .ends_with(".evolver/rule_changes.jsonl"));
    assert!(evolver.rule_change_journal_path().exists());
}

#[test]
fn outer_loop_updates_existing_rule_and_records_before_content() {
    let root = temp_skill_root("outer-loop-update");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let first = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    let context = governance_context(evolver.observed_events().to_vec());
    let first_receipt = evolver
        .apply_rule_change(first, &PolicyRuleChangeGovernance::default(), &context)
        .expect("first rule change should persist");
    assert_eq!(first_receipt.version, 1);

    evolver
        .observe(failure_event("f3", "t3", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f4", "t4", "build"))
        .expect("failure event should be observed");
    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let second = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    let context = governance_context(evolver.observed_events().to_vec());

    assert_eq!(second.change_kind, RuleChangeKind::UpdateRule);
    assert_eq!(second.rule_id, "build-for-rule-tool");
    assert_eq!(second.old_procedure.len(), 3);
    assert_eq!(
        second.old_procedure[0],
        "Review the repeated failure evidence and the existing rule before changing it."
    );

    let second_receipt = evolver
        .apply_rule_change(second, &PolicyRuleChangeGovernance::default(), &context)
        .expect("governance-approved update should persist");

    assert_eq!(second_receipt.change_kind, RuleChangeKind::UpdateRule);
    assert_eq!(second_receipt.version, 2);
    assert_eq!(second_receipt.previous_version, Some(1));
    assert_eq!(first_receipt.path, second_receipt.path);

    let history = evolver
        .rule_change_history()
        .expect("journal should be observable");
    assert_eq!(history.len(), 2);
    let update_entry = &history[1];
    assert_eq!(update_entry.previous_version, Some(1));
    let before = update_entry
        .before
        .as_deref()
        .expect("update entry must keep before content for rollback");
    assert!(before.contains("version: 1"));
    assert!(update_entry.after.contains("version: 2"));

    let md_count = fs::read_dir(&root)
        .expect("skill root should be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .count();
    assert_eq!(md_count, 1);
}

#[test]
fn outer_loop_rejected_by_governance_writes_nothing() {
    let root = temp_skill_root("outer-loop-rejected");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let proposal = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    let context = governance_context(evolver.observed_events().to_vec());

    let err = evolver
        .apply_rule_change(proposal, &NoopRuleChangeGovernance, &context)
        .expect_err("noop governance must reject before any write");

    assert!(matches!(err, EvolutionError::ValidationRejected(_)));
    // 观察流持久化会创建 .evolver/ 目录；拒绝路径的语义是「无规则落盘」。
    assert!(!root.join("build-for-rule-tool.md").exists());
    assert!(evolver
        .rule_change_history()
        .expect("empty journal should be readable")
        .is_empty());
    assert!(evolver.last_rule_change_receipt().is_none());
}

#[test]
fn apply_rule_change_rejects_inconsistent_governance_decision() {
    let root = temp_skill_root("outer-loop-mismatched-decision");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let proposal = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    let context = governance_context(evolver.observed_events().to_vec());

    let err = evolver
        .apply_rule_change(proposal, &MismatchedGovernance, &context)
        .expect_err("governance decision must reference the evaluated proposal");

    assert!(matches!(err, EvolutionError::InvalidRuleChange(_)));
    // 治理决策不一致时不得落盘规则（观察流 .evolver/ 目录属有意副作用）。
    assert!(!root.join("build-for-rule-tool.md").exists());
}

#[test]
fn apply_rule_change_rejects_invalid_proposal_before_governance() {
    let root = temp_skill_root("outer-loop-invalid-proposal");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    let mut proposal = base_rule_change_proposal();
    proposal.title.clear();
    let context = governance_context(Vec::new());

    let err = evolver
        .apply_rule_change(proposal, &ApproveAllGovernance, &context)
        .expect_err("invalid proposal must fail before governance");

    assert!(matches!(err, EvolutionError::InvalidRuleChange(_)));
    assert!(!root.exists());
}

#[test]
fn apply_rule_change_surfaces_storage_errors() {
    let root = temp_skill_root("outer-loop-storage-error");
    fs::write(&root, "this path is a file, not a directory")
        .expect("blocking file should be writable");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    let proposal = base_rule_change_proposal();
    let context = governance_context(Vec::new());

    let err = evolver
        .apply_rule_change(proposal, &ApproveAllGovernance, &context)
        .expect_err("write path must surface storage failure instead of falling back");

    assert!(matches!(err, EvolutionError::StorageError(_)));
}

#[test]
fn outer_loop_rollback_restores_previous_procedure() {
    let root = temp_skill_root("outer-loop-rollback");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");
    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let context = governance_context(evolver.observed_events().to_vec());
    let governance = PolicyRuleChangeGovernance::default();

    let first = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    evolver
        .apply_rule_change(first, &governance, &context)
        .expect("first rule change should persist");

    let mut update = base_rule_change_proposal();
    update.proposal_id = "proposal-rule-update".to_string();
    update.change_kind = RuleChangeKind::UpdateRule;
    update.old_procedure = vec![
        "Review the repeated failure evidence and the existing rule before changing it."
            .to_string(),
        "Apply the corrective procedure for tool=build and capture the outcome.".to_string(),
        "Verify the fix with a check or test and record the governance boundary.".to_string(),
    ];
    update.new_procedure = vec![
        "Run the new fallback path for tool=build and capture the outcome.".to_string(),
        "Check the result against the expected contract and record it.".to_string(),
        "Keep the governance and approval boundaries visible after the change.".to_string(),
    ];
    update.evidence[0].event_ids = vec!["f1".to_string(), "f2".to_string()];
    let update_receipt = evolver
        .apply_rule_change(update.clone(), &governance, &context)
        .expect("governance-approved update should persist");
    assert_eq!(update_receipt.version, 2);

    let rollback_receipt = evolver
        .rollback_rule_change(&format!("rc-{}", update.proposal_id), &governance, &context)
        .expect("rollback of an update should restore the old procedure");

    assert_eq!(rollback_receipt.version, 3);
    assert_eq!(rollback_receipt.change_kind, RuleChangeKind::UpdateRule);
    assert_eq!(rollback_receipt.previous_version, Some(2));

    let written = fs::read_to_string(&rollback_receipt.path)
        .expect("rolled-back rule file should be readable");
    assert!(written.contains("Review the repeated failure evidence and the existing rule"));
    assert!(!written.contains("Run the new fallback path"));

    let history = evolver
        .rule_change_history()
        .expect("journal should be observable");
    assert_eq!(history.len(), 3);
    assert!(history[2]
        .proposal
        .rationale
        .contains("rollback of proposal-rule-update"));
    let md_count = fs::read_dir(&root)
        .expect("skill root should be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .count();
    assert_eq!(md_count, 1);
}

#[test]
fn outer_loop_rollback_refuses_rule_creation_deletion() {
    let root = temp_skill_root("outer-loop-rollback-create");
    let mut evolver = CanonicalSkillEvolver::new(&root);
    evolver
        .observe(failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    evolver
        .observe(failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");
    let patterns = evolver
        .detect_repeated_failures(&FailureDetectorConfig::default())
        .expect("valid config should detect");
    let context = governance_context(evolver.observed_events().to_vec());
    let governance = PolicyRuleChangeGovernance::default();

    let proposal = evolver
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    evolver
        .apply_rule_change(proposal.clone(), &governance, &context)
        .expect("rule creation should persist");

    let err = evolver
        .rollback_rule_change(
            &format!("rc-{}", proposal.proposal_id),
            &governance,
            &context,
        )
        .expect_err("rollback of a creation must refuse deletion");

    assert!(matches!(err, EvolutionError::InvalidRuleChange(_)));
    assert!(root.join("build-for-rule-tool.md").exists());
}

#[test]
fn observed_events_persist_across_instances_for_cross_turn_accumulation() {
    // 跨 turn 累积失败证据：turn1 的观察流落盘后，新实例（turn2）能恢复，
    // 使 min_repeats>=2 在真实多 turn 场景可触发。
    let root = temp_skill_root("observed-persist");

    let mut turn1 = CanonicalSkillEvolver::new(&root);
    let failed = RuntimeEvent {
        event_id: "ledger:t1:0:cli".to_string(),
        task_id: "turn-1".to_string(),
        kind: RuntimeEventKind::ToolFailed,
        summary: "tool code_execute failed".to_string(),
        metadata: BTreeMap::from([
            ("tool".to_string(), "code_execute".to_string()),
            ("error_code".to_string(), "needs_approval".to_string()),
            (
                "error".to_string(),
                "profile=full_local_workspace action=delete or cleanup".to_string(),
            ),
        ]),
    };
    turn1
        .observe(failed)
        .expect("turn1 failure event should be observed and persisted");

    let events_path = turn1.observed_events_path();
    assert!(
        events_path.exists(),
        "observed events jsonl should be persisted"
    );

    // 模拟新 CLI 进程：全新 evolver 实例从磁盘恢复观察流。
    let turn2 = CanonicalSkillEvolver::new(&root);
    assert_eq!(turn2.observed_events().len(), 1);
    assert_eq!(
        turn2.observed_events()[0]
            .metadata
            .get("tool")
            .map(String::as_str),
        Some("code_execute")
    );

    // min_repeats=2 时单条失败不触发；再补一条同签名失败应命中 pattern。
    let config = FailureDetectorConfig::default().min_repeats(2);
    let patterns = turn2
        .detect_repeated_failures(&config)
        .expect("detect should run over restored stream");
    assert!(
        patterns.is_empty(),
        "single restored failure must not trigger min_repeats=2"
    );

    let mut turn2 = turn2;
    let second = RuntimeEvent {
        event_id: "ledger:t2:0:cli".to_string(),
        task_id: "turn-2".to_string(),
        kind: RuntimeEventKind::ToolFailed,
        summary: "tool code_execute failed".to_string(),
        metadata: BTreeMap::from([
            ("tool".to_string(), "code_execute".to_string()),
            ("error_code".to_string(), "needs_approval".to_string()),
            (
                "error".to_string(),
                "profile=full_local_workspace action=delete or cleanup".to_string(),
            ),
        ]),
    };
    turn2
        .observe(second)
        .expect("turn2 failure event should be observed and persisted");

    let patterns = turn2
        .detect_repeated_failures(&config)
        .expect("detect should run over accumulated stream");
    assert_eq!(
        patterns.len(),
        1,
        "two same-signature failures should form a pattern"
    );
    assert_eq!(patterns[0].count, 2);
}

#[test]
fn observed_events_truncate_oldest_over_max_and_skip_corrupt_lines() {
    let root = temp_skill_root("observed-bounded");
    let mut evolver = CanonicalSkillEvolver::new(&root);

    // 直接写入超过上限的事件（通过反复 observe 太慢；上限是 4096，用循环补到
    // 上限 + 3，验证只保留最近 OBSERVED_EVENTS_MAX 条）。
    for index in 0..(4096 + 3) {
        evolver
            .observe(RuntimeEvent {
                event_id: format!("ev-{index}"),
                task_id: "task-bounded".to_string(),
                kind: RuntimeEventKind::TurnCompleted,
                summary: "bounded stream event".to_string(),
                metadata: BTreeMap::new(),
            })
            .expect("observe should succeed");
    }
    assert_eq!(evolver.observed_events().len(), 4096);
    assert_eq!(evolver.observed_events()[0].event_id, "ev-3");

    // 新实例加载恢复（上限截断后的内容已持久化）。
    let restored = CanonicalSkillEvolver::new(&root);
    assert_eq!(restored.observed_events().len(), 4096);
    assert_eq!(restored.observed_events()[0].event_id, "ev-3");

    // 损坏行应被跳过且不 panic。
    let path = restored.observed_events_path();
    let mut content = fs::read_to_string(&path).expect("jsonl should exist");
    content.push_str("{this-is-not-json}\n");
    fs::write(&path, content).expect("append corrupt line");
    let tolerant = CanonicalSkillEvolver::new(&root);
    assert_eq!(tolerant.observed_events().len(), 4096);
}

// ======================= scoring gate =======================

fn write_gate_benchmark(root: &std::path::Path, id: &str) -> BenchmarkStore {
    let store = BenchmarkStore::new(root);
    let def = BenchmarkDef {
        id: id.to_string(),
        capability: "skill_evolution".to_string(),
        version: 1,
        title: "skill evolution scoring gate eval".to_string(),
        cases: vec![BenchmarkCase {
            id: "case-1".to_string(),
            title: "rule change quality".to_string(),
            max_score: 10,
            statement: "评估规则修改提案：触发条件是否清晰、新流程是否可执行。".to_string(),
            rubric: "满分10分：触发条件明确3分；新流程可执行4分；理由充分3分。".to_string(),
        }],
    };
    store
        .write_def(&def)
        .expect("benchmark def should be writable");
    store
}

fn gate_update_proposal() -> RuleChangeProposal {
    let mut proposal = base_rule_change_proposal();
    proposal.change_kind = RuleChangeKind::UpdateRule;
    proposal.old_procedure = vec!["旧流程步骤".to_string()];
    proposal
}

fn gate_for(
    benchmark_root: &std::path::Path,
    benchmark_id: &str,
    after_score: u16,
    max_score: u16,
) -> BenchmarkScoreGate {
    BenchmarkScoreGate::new(
        SkillScoringGateConfig::new(benchmark_id, benchmark_root),
        Box::new(FixedScoreScorer::new(benchmark_id, after_score, max_score)),
    )
}

#[test]
fn scoring_gate_no_baseline_create_rule_is_admitted_and_persisted() {
    let skill_root = temp_skill_root("gate-create-nobaseline");
    let bench_root = temp_skill_root("gate-create-nobaseline-bench");
    write_gate_benchmark(&bench_root, "skill-gate");
    let mut evolver = CanonicalSkillEvolver::new(&skill_root);
    let proposal = base_rule_change_proposal();
    let gate = gate_for(&bench_root, "skill-gate", 8, 10);

    let receipt = evolver
        .apply_rule_change_gated(
            proposal,
            &ApproveAllGovernance,
            &governance_context(vec![]),
            &gate,
        )
        .expect("no baseline first registration (CreateRule) should be admitted");

    assert_eq!(receipt.change_kind, RuleChangeKind::CreateRule);
    assert_eq!(receipt.rule_id, "build-for-rule-tool");
    assert!(skill_root.join("build-for-rule-tool.md").exists());
    // 首次登记不产生候选池条目。
    assert!(evolver
        .candidate_pool_history()
        .expect("candidate history should load")
        .is_empty());
}

#[test]
fn scoring_gate_no_baseline_update_rule_is_rejected_into_candidate_pool() {
    let skill_root = temp_skill_root("gate-update-nobaseline");
    let bench_root = temp_skill_root("gate-update-nobaseline-bench");
    write_gate_benchmark(&bench_root, "skill-gate");
    let mut evolver = CanonicalSkillEvolver::new(&skill_root);
    let proposal = gate_update_proposal();
    let gate = gate_for(&bench_root, "skill-gate", 8, 10);

    let err = evolver
        .apply_rule_change_gated(
            proposal,
            &ApproveAllGovernance,
            &governance_context(vec![]),
            &gate,
        )
        .expect_err("UpdateRule without baseline must be rejected");

    assert!(matches!(err, EvolutionError::ValidationRejected(_)));
    // 未达标提案记录进候选池（append-only JSONL）。
    let history = evolver
        .candidate_pool_history()
        .expect("candidate history should load");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].proposal.proposal_id, "proposal-rule-1");
    assert_eq!(history[0].decision.change_kind, RuleChangeKind::UpdateRule);
    assert!(!history[0].decision.admitted);
    assert_eq!(history[0].decision.baseline_score, None);
    // 绝不落盘正式规则。
    assert!(!skill_root.join("build-for-rule-tool.md").exists());
    assert!(evolver.candidate_pool_path().exists());
}

#[test]
fn scoring_gate_baseline_strict_improvement_upserts_rule() {
    let skill_root = temp_skill_root("gate-strict-improve");
    let bench_root = temp_skill_root("gate-strict-improve-bench");
    let store = write_gate_benchmark(&bench_root, "skill-gate");
    store
        .record_run(&BenchmarkRunRequest {
            benchmark_id: "skill-gate".to_string(),
            case_scores: vec![CaseScore {
                case_id: "case-1".to_string(),
                score: 6,
                max_score: 10,
                reason: "baseline run".to_string(),
            }],
        })
        .expect("baseline should be recorded");
    let mut evolver = CanonicalSkillEvolver::new(&skill_root);
    let proposal = base_rule_change_proposal();
    let gate = gate_for(&bench_root, "skill-gate", 8, 10);

    let receipt = evolver
        .apply_rule_change_gated(
            proposal,
            &ApproveAllGovernance,
            &governance_context(vec![]),
            &gate,
        )
        .expect("score 8 strictly above baseline 6 should be admitted");

    assert_eq!(receipt.change_kind, RuleChangeKind::CreateRule);
    assert!(skill_root.join("build-for-rule-tool.md").exists());
    assert!(evolver
        .candidate_pool_history()
        .expect("candidate history should load")
        .is_empty());
}

#[test]
fn scoring_gate_baseline_no_improvement_is_rejected_into_candidate_pool() {
    let skill_root = temp_skill_root("gate-no-improve");
    let bench_root = temp_skill_root("gate-no-improve-bench");
    let store = write_gate_benchmark(&bench_root, "skill-gate");
    store
        .record_run(&BenchmarkRunRequest {
            benchmark_id: "skill-gate".to_string(),
            case_scores: vec![CaseScore {
                case_id: "case-1".to_string(),
                score: 6,
                max_score: 10,
                reason: "baseline run".to_string(),
            }],
        })
        .expect("baseline should be recorded");
    let mut evolver = CanonicalSkillEvolver::new(&skill_root);
    let proposal = base_rule_change_proposal();
    // 与基线持平：不算严格提升。
    let gate = gate_for(&bench_root, "skill-gate", 6, 10);

    let err = evolver
        .apply_rule_change_gated(
            proposal,
            &ApproveAllGovernance,
            &governance_context(vec![]),
            &gate,
        )
        .expect_err("score equal to baseline must be rejected");

    assert!(matches!(err, EvolutionError::ValidationRejected(_)));
    let history = evolver
        .candidate_pool_history()
        .expect("candidate history should load");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].decision.baseline_score, Some(6));
    assert_eq!(history[0].decision.after_score, Some(6));
    assert!(!history[0].decision.admitted);
    assert!(!skill_root.join("build-for-rule-tool.md").exists());
}

#[test]
fn scoring_gate_proposal_embedding_rubric_fails_closed() {
    let skill_root = temp_skill_root("gate-isolation");
    let bench_root = temp_skill_root("gate-isolation-bench");
    write_gate_benchmark(&bench_root, "skill-gate");
    let mut evolver = CanonicalSkillEvolver::new(&skill_root);
    // 提案 rationale 内嵌 rubric 原文 = 作弊；必须 fail-closed，连候选池都不进。
    let mut proposal = base_rule_change_proposal();
    proposal.rationale = "满分10分：触发条件明确3分；新流程可执行4分；理由充分3分。".to_string();
    let gate = gate_for(&bench_root, "skill-gate", 8, 10);

    let err = evolver
        .apply_rule_change_gated(
            proposal,
            &ApproveAllGovernance,
            &governance_context(vec![]),
            &gate,
        )
        .expect_err("rubric leak must fail closed");

    assert!(matches!(err, EvolutionError::InvalidRuleChange(_)));
    assert!(!skill_root.join("build-for-rule-tool.md").exists());
    // 分数本身已不可信：不记录候选池。
    assert!(evolver
        .candidate_pool_history()
        .expect("candidate history should load")
        .is_empty());
}

#[test]
fn scoring_gate_snapshot_before_change_captures_existing_content() {
    let skill_root = temp_skill_root("gate-snapshot");
    let mut evolver = CanonicalSkillEvolver::new(&skill_root);
    // 先正常落一条规则。
    let proposal = base_rule_change_proposal();
    evolver
        .apply_rule_change(proposal, &ApproveAllGovernance, &governance_context(vec![]))
        .expect("create rule should persist");

    let snapshot = evolver
        .snapshot_before_change("build-for-rule-tool")
        .expect("snapshot should capture existing rule");
    assert_eq!(snapshot.rule_id, "build-for-rule-tool");
    assert!(snapshot.path.is_some());
    let before = snapshot.before.expect("before content should exist");
    assert!(before.contains("skill_id: build-for-rule-tool"));
    assert!(!snapshot.captured_at.is_empty());

    // 不存在的规则：before=None（CreateRule 首次登记语义）。
    let missing = evolver
        .snapshot_before_change("no-such-rule")
        .expect("missing rule snapshot should be benign");
    assert_eq!(missing.before, None);
    assert_eq!(missing.path, None);
}
