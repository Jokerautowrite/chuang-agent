use chuang_agent::control_intent::ControlIntentInput;
use chuang_agent::control_plane::{ControlPlane, FakeControlPlane};
use chuang_agent::control_surface::{
    list_control_surface_units, run_control_surface_intent, run_control_surface_outcome,
    ControlSurfaceError, ControlSurfaceRequest,
};
use chuang_agent::control_workflow::ControlWorkflowError;
use chuang_agent::governance::{RiskDecision, StaticRuleGovernance};

#[test]
fn control_surface_lists_units_as_ui_ready_views() {
    let control_plane = FakeControlPlane::default_local_agents();

    let views = list_control_surface_units(&control_plane);

    let xiaoce = views
        .iter()
        .find(|view| view.display_name == "小策")
        .expect("xiaoce should exist");
    assert_eq!(xiaoce.unit_id, "codex-xiaoce");
    assert_eq!(xiaoce.kind, "agent");
    assert_eq!(xiaoce.channel, "feishu");
}

#[test]
fn control_surface_runs_display_name_intent_through_governance() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let err = run_control_surface_intent(
        &mut control_plane,
        &mut governance,
        ControlSurfaceRequest {
            intent: ControlIntentInput {
                unit_id: Some("小策".to_string()),
                action: Some("重启".to_string()),
                reason: Some("test display name restart".to_string()),
                model_name: None,
            },
            approved: false,
        },
    )
    .expect_err("restart should still require approval");

    assert!(matches!(
        err,
        ControlSurfaceError::Workflow(ControlWorkflowError::ApprovalRequired(
            RiskDecision::NeedsApproval { .. }
        ))
    ));
    assert!(governance.audit_records().is_empty());
}

#[test]
fn control_surface_applies_approved_friendly_model_switch() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let result = run_control_surface_intent(
        &mut control_plane,
        &mut governance,
        ControlSurfaceRequest {
            intent: ControlIntentInput {
                unit_id: Some("小策".to_string()),
                action: Some("换模型".to_string()),
                reason: Some("test approved friendly model switch".to_string()),
                model_name: Some("gpt-5.5".to_string()),
            },
            approved: true,
        },
    )
    .expect("approved friendly model switch should apply");

    assert_eq!(result.view.unit_id, "codex-xiaoce");
    assert_eq!(result.view.display_name, "小策");
    assert_eq!(result.view.action, "change_model");
    assert_eq!(result.view.model_name, Some("gpt-5.5".to_string()));
    assert!(result.view.audit_recorded);
    assert_eq!(governance.audit_records().len(), 1);

    let xiaoce = control_plane
        .list_units()
        .into_iter()
        .find(|unit| unit.unit_id == "codex-xiaoce")
        .expect("xiaoce should exist");
    assert_eq!(xiaoce.model_name, Some("gpt-5.5".to_string()));
}

#[test]
fn control_surface_rejects_unknown_display_name_before_workflow() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let err = run_control_surface_intent(
        &mut control_plane,
        &mut governance,
        ControlSurfaceRequest {
            intent: ControlIntentInput {
                unit_id: Some("小不存在".to_string()),
                action: Some("重启".to_string()),
                reason: Some("test unknown".to_string()),
                model_name: None,
            },
            approved: true,
        },
    )
    .expect_err("unknown display name should fail");

    assert!(matches!(err, ControlSurfaceError::Intent(_)));
    assert!(governance.audit_records().is_empty());
}

#[test]
fn control_surface_outcome_returns_needs_approval_view_without_error() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let outcome = run_control_surface_outcome(
        &mut control_plane,
        &mut governance,
        ControlSurfaceRequest {
            intent: ControlIntentInput {
                unit_id: Some("小策".to_string()),
                action: Some("重启".to_string()),
                reason: Some("test outcome approval".to_string()),
                model_name: None,
            },
            approved: false,
        },
    )
    .expect("approval-needed outcome should be renderable");

    assert_eq!(outcome.status, "needs_approval");
    assert_eq!(outcome.view.unit_id, "codex-xiaoce");
    assert_eq!(outcome.view.display_name, "小策");
    assert!(outcome.view.decision.starts_with("needs_approval:"));
    assert!(!outcome.view.audit_recorded);
    assert!(governance.audit_records().is_empty());
}

#[test]
fn control_surface_outcome_returns_applied_view_after_approval() {
    let mut control_plane = FakeControlPlane::default_local_agents();
    let mut governance = StaticRuleGovernance::new();

    let outcome = run_control_surface_outcome(
        &mut control_plane,
        &mut governance,
        ControlSurfaceRequest {
            intent: ControlIntentInput {
                unit_id: Some("小策".to_string()),
                action: Some("换模型".to_string()),
                reason: Some("test outcome applied".to_string()),
                model_name: Some("gpt-5.5".to_string()),
            },
            approved: true,
        },
    )
    .expect("approved outcome should apply");

    assert_eq!(outcome.status, "applied");
    assert_eq!(outcome.view.unit_id, "codex-xiaoce");
    assert_eq!(outcome.view.action, "change_model");
    assert!(outcome.view.audit_recorded);
    assert_eq!(governance.audit_records().len(), 1);
}
