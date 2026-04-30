use chuang_agent::control_plane::{ControlAction, ControlPlane, ControlRequest, FakeControlPlane};
use chuang_agent::control_workflow::{
    build_unit_views, run_control_workflow, ControlWorkflowError, ControlWorkflowRequest,
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
    assert_eq!(result.view.unit_id, "codex-xiaoce");
    assert_eq!(result.view.display_name, "小策");
    assert!(result.view.decision.starts_with("needs_approval:"));
    assert_eq!(result.view.action, "change_model");
    assert_eq!(result.view.next_status, Some("Running".to_string()));
    assert_eq!(result.view.model_name, Some("gpt-5.5".to_string()));
    assert!(result.view.audit_recorded);
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

#[test]
fn control_unit_views_render_default_local_units_for_control_surfaces() {
    let control_plane = FakeControlPlane::default_local_agents();

    let views = build_unit_views(control_plane.list_units());

    let xiaoce = views
        .iter()
        .find(|view| view.unit_id == "codex-xiaoce")
        .expect("xiaoce should exist");
    let bridge = views
        .iter()
        .find(|view| view.unit_id == "codex-feishu-bot.service")
        .expect("bridge should exist");

    assert_eq!(xiaoce.display_name, "小策");
    assert_eq!(xiaoce.kind, "agent");
    assert_eq!(xiaoce.status, "Running");
    assert_eq!(xiaoce.channel, "feishu");
    assert_eq!(bridge.kind, "service");
    assert_eq!(bridge.channel, "systemd");
}
