use chuang_agent::common::{AgentId, AuditRecord, TaskId, Timestamp};
use chuang_agent::governance::{
    ActionKind, Governance, MarkdownRuleSet, ProposedAction, RiskDecision, StaticRuleGovernance,
};
use chuang_agent::permission_profile_slot::safe_default_profile;

fn action(kind: ActionKind, target: &str) -> ProposedAction {
    ProposedAction {
        action_id: "act-1".to_string(),
        kind,
        target: target.to_string(),
        summary: "test action".to_string(),
    }
}

#[test]
fn static_governance_allows_observe_and_draft_actions() {
    let governance = StaticRuleGovernance::new();

    assert!(matches!(
        governance
            .classify(&action(ActionKind::Observe, "screen"))
            .unwrap(),
        RiskDecision::Allowed { .. }
    ));
    assert!(matches!(
        governance
            .classify(&action(ActionKind::Draft, "message"))
            .unwrap(),
        RiskDecision::Allowed { .. }
    ));
}

#[test]
fn static_governance_requires_approval_for_external_or_destructive_actions() {
    let governance = StaticRuleGovernance::new();

    for kind in [
        ActionKind::ExternalSend,
        ActionKind::PublicPost,
        ActionKind::Payment,
        ActionKind::VerificationCodeInput,
        ActionKind::DeleteOrCleanup,
        ActionKind::PrivilegeEscalation,
        ActionKind::ServiceChange,
        ActionKind::NetworkChange,
        ActionKind::SecretAccess,
    ] {
        assert!(matches!(
            governance
                .classify(&action(kind, "explicit-target"))
                .unwrap(),
            RiskDecision::NeedsApproval { .. }
        ));
    }
}

#[test]
fn static_governance_treats_local_desktop_interaction_as_local_action() {
    let governance = StaticRuleGovernance::new();

    let decision = governance
        .classify(&action(
            ActionKind::LocalDesktopInteraction,
            "actuator::mouse x=10 y=20",
        ))
        .expect("governance should classify");

    assert!(matches!(
        decision,
        RiskDecision::Allowed { ref reason }
            if reason.contains("profile=full_local_workspace")
    ));
}

#[test]
fn governance_runtime_decision_changes_with_permission_profile() {
    let governance = StaticRuleGovernance::with_profile(safe_default_profile());

    let decision = governance
        .classify(&action(
            ActionKind::LocalDesktopInteraction,
            "actuator::keyboard",
        ))
        .expect("governance should classify");

    assert!(matches!(decision, RiskDecision::NeedsApproval { .. }));
}

#[test]
fn static_governance_blocks_empty_targets() {
    let governance = StaticRuleGovernance::new();

    assert!(matches!(
        governance
            .classify(&action(ActionKind::ShellCommand, " "))
            .unwrap(),
        RiskDecision::Blocked { .. }
    ));
}

#[test]
fn static_governance_records_audit_entries() {
    let mut governance = StaticRuleGovernance::new();
    let record = AuditRecord {
        operation: "classify".to_string(),
        agent_id: AgentId("xiaoce".to_string()),
        task_id: TaskId("task-1".to_string()),
        delta_bytes: 0,
        reason: "contract-test".to_string(),
        timestamp: Timestamp("2026-05-01T00:00:00Z".to_string()),
    };

    governance.audit(record.clone()).unwrap();

    assert_eq!(governance.audit_records(), &[record]);
}

#[test]
fn markdown_ruleset_rejects_empty_or_ruleless_content() {
    assert!(MarkdownRuleSet::from_content("rules.md", String::new()).is_err());
    assert!(MarkdownRuleSet::from_content("rules.md", "# Only a heading".to_string()).is_err());
}

#[test]
fn static_governance_attaches_rules_fingerprint_to_decision_reason() {
    let rules = MarkdownRuleSet::from_content(
        "rules/core.md",
        "1. Clarify before irreversible work\n2. Keep changes minimal\n".to_string(),
    )
    .expect("rules should parse");
    let governance = StaticRuleGovernance::with_rules(rules);

    let decision = governance
        .classify(&action(ActionKind::Draft, "project-plan"))
        .expect("governance should classify");

    match decision {
        RiskDecision::Allowed { reason } => {
            assert!(reason.contains("action=read-only or draft"));
            assert!(reason.contains("rules="));
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}
