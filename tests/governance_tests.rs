use chuang_agent::common::{AgentId, AuditRecord, TaskId, Timestamp};
use chuang_agent::governance::{
    ActionKind, Governance, ProposedAction, RiskDecision, StaticRuleGovernance,
};

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
        ActionKind::ServiceChange,
        ActionKind::NetworkChange,
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
