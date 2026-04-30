use crate::control_plane::{
    audit_record_for_control, proposed_action_for_control, ControlError, ControlPlane,
    ControlReceipt, ControlRequest,
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

    Ok(ControlWorkflowResult {
        decision,
        receipt: Some(receipt),
        audit_recorded: true,
    })
}
