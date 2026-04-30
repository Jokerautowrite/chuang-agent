use crate::control_plane::{
    audit_record_for_control, proposed_action_for_control, ControlError, ControlPlane,
    ControlReceipt, ControlRequest, ManagedUnit,
};
use crate::governance::{Governance, GovernanceError, RiskDecision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlWorkflowRequest {
    pub control: ControlRequest,
    pub approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlWorkflowResult {
    pub decision: RiskDecision,
    pub receipt: Option<ControlReceipt>,
    pub audit_recorded: bool,
    pub view: ControlWorkflowView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlWorkflowView {
    pub unit_id: String,
    pub display_name: String,
    pub decision: String,
    pub action: String,
    pub previous_status: Option<String>,
    pub next_status: Option<String>,
    pub model_name: Option<String>,
    pub audit_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlWorkflowError {
    Control(ControlError),
    Governance(GovernanceError),
    ApprovalRequired(RiskDecision),
    NotAllowed(RiskDecision),
}

pub fn run_control_workflow<P, G>(
    control_plane: &mut P,
    governance: &mut G,
    request: ControlWorkflowRequest,
) -> Result<ControlWorkflowResult, ControlWorkflowError>
where
    P: ControlPlane,
    G: Governance,
{
    let units = control_plane.list_units();
    let unit = units
        .iter()
        .find(|unit| unit.unit_id == request.control.unit_id)
        .ok_or_else(|| {
            ControlWorkflowError::Control(ControlError::UnknownUnit(
                request.control.unit_id.clone(),
            ))
        })?;
    let proposed = proposed_action_for_control(unit, &request.control)
        .map_err(ControlWorkflowError::Control)?;
    let decision = governance
        .classify(&proposed)
        .map_err(ControlWorkflowError::Governance)?;

    if matches!(decision, RiskDecision::NeedsApproval { .. }) && !request.approved {
        return Err(ControlWorkflowError::ApprovalRequired(decision));
    }
    if matches!(
        decision,
        RiskDecision::Blocked { .. } | RiskDecision::DraftOnly { .. }
    ) {
        return Err(ControlWorkflowError::NotAllowed(decision));
    }

    let audit_record = audit_record_for_control(unit, &request.control, request.approved)
        .map_err(ControlWorkflowError::Control)?;
    governance
        .audit(audit_record)
        .map_err(ControlWorkflowError::Governance)?;

    let receipt = control_plane
        .apply(request.control)
        .map_err(ControlWorkflowError::Control)?;
    let view = build_workflow_view(unit, &decision, Some(&receipt), true);

    Ok(ControlWorkflowResult {
        decision,
        receipt: Some(receipt),
        audit_recorded: true,
        view,
    })
}

pub fn build_decision_view(unit: &ManagedUnit, decision: &RiskDecision) -> ControlWorkflowView {
    build_workflow_view(unit, decision, None, false)
}

fn build_workflow_view(
    unit: &ManagedUnit,
    decision: &RiskDecision,
    receipt: Option<&ControlReceipt>,
    audit_recorded: bool,
) -> ControlWorkflowView {
    ControlWorkflowView {
        unit_id: unit.unit_id.clone(),
        display_name: unit.display_name.clone(),
        decision: decision_label(decision),
        action: receipt
            .map(|receipt| receipt.action.as_str().to_string())
            .unwrap_or_else(|| "pending".to_string()),
        previous_status: receipt.map(|receipt| format!("{:?}", receipt.previous_status)),
        next_status: receipt.map(|receipt| format!("{:?}", receipt.next_status)),
        model_name: receipt.and_then(|receipt| receipt.model_name.clone()),
        audit_recorded,
    }
}

fn decision_label(decision: &RiskDecision) -> String {
    match decision {
        RiskDecision::Allowed { reason } => format!("allowed:{reason}"),
        RiskDecision::DraftOnly { reason } => format!("draft_only:{reason}"),
        RiskDecision::NeedsApproval { reason } => format!("needs_approval:{reason}"),
        RiskDecision::Blocked { reason } => format!("blocked:{reason}"),
    }
}
