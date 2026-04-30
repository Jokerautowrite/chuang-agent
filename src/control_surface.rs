use crate::control_intent::{
    parse_control_intent, resolve_control_unit_id, ControlIntentError, ControlIntentInput,
};
use crate::control_plane::ControlPlane;
use crate::control_workflow::{
    build_unit_views, run_control_workflow, ControlUnitView, ControlWorkflowError,
    ControlWorkflowRequest, ControlWorkflowResult,
};
use crate::governance::Governance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlSurfaceRequest {
    pub intent: ControlIntentInput,
    pub approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlSurfaceError {
    Intent(ControlIntentError),
    Workflow(ControlWorkflowError),
}

pub fn list_control_surface_units<P>(control_plane: &P) -> Vec<ControlUnitView>
where
    P: ControlPlane,
{
    build_unit_views(control_plane.list_units())
}

pub fn run_control_surface_intent<P, G>(
    control_plane: &mut P,
    governance: &mut G,
    request: ControlSurfaceRequest,
) -> Result<ControlWorkflowResult, ControlSurfaceError>
where
    P: ControlPlane,
    G: Governance,
{
    let units = control_plane.list_units();
    let unit_key = request
        .intent
        .unit_id
        .as_deref()
        .ok_or(ControlSurfaceError::Intent(ControlIntentError::MissingUnit))?;
    let unit_id = resolve_control_unit_id(&units, unit_key).map_err(ControlSurfaceError::Intent)?;
    let control = parse_control_intent(ControlIntentInput {
        unit_id: Some(unit_id),
        action: request.intent.action,
        reason: request.intent.reason,
        model_name: request.intent.model_name,
    })
    .map_err(ControlSurfaceError::Intent)?;

    run_control_workflow(
        control_plane,
        governance,
        ControlWorkflowRequest {
            control,
            approved: request.approved,
        },
    )
    .map_err(ControlSurfaceError::Workflow)
}
