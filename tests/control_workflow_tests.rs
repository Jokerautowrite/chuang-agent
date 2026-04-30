use chuang_agent::control_plane::{ControlAction, ControlRequest, FakeControlPlane};
use chuang_agent::control_workflow::{
    run_control_workflow, ControlWorkflowError, ControlWorkflowRequest,
};
use chuang_agent::governance::{RiskDecision, StaticRuleGovernance};

#[test]
fn control_workflow_requires_approval_before_service_change() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let err = run_control_workflow(
        &mut control_plane,
        &mut governance,
        ControlWorkflowRequest {
            control: ControlRequest {
                unit_id: "codex-xiaoce".to_string(),
                action: ControlAction::Restart,
                reason: "test restart".to_string(),
            },
            approved: false,
        },
    )
    .expect_err("restart should require approval");

    assert!(matches!(
        err,
        ControlWorkflowError::ApprovalRequired(RiskDecision::NeedsApproval { .. })
    ));
    assert!(governance.audit_records().is_empty());
}

#[test]
fn control_workflow_applies_and_audits_after_approval() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let result = run_control_workflow(
        &mut control_plane,
        &mut governance,
        ControlWorkflowRequest {
            control: ControlRequest {
                unit_id: "codex-xiaoce".to_string(),
                action: ControlAction::ChangeModel {
                    model_name: "gpt-5.5".to_string(),
                },
                reason: "test model switch".to_string(),
            },
            approved: true,
        },
    )
    .expect("approved model switch should apply");

    assert!(matches!(
        result.decision,
        RiskDecision::NeedsApproval { .. }
    ));
    assert!(result.audit_recorded);
    assert_eq!(governance.audit_records().len(), 1);
    assert_eq!(
        result.receipt.expect("receipt should exist").model_name,
        Some("gpt-5.5".to_string())
    );
}

#[test]
fn control_workflow_reports_unknown_unit_before_governance() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let err = run_control_workflow(
        &mut control_plane,
        &mut governance,
        ControlWorkflowRequest {
            control: ControlRequest {
                unit_id: "missing-unit".to_string(),
                action: ControlAction::Start,
                reason: "test missing".to_string(),
            },
            approved: true,
        },
    )
    .expect_err("unknown unit should fail");

    assert!(matches!(err, ControlWorkflowError::Control(_)));
    assert!(governance.audit_records().is_empty());
}
