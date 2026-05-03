use chuang_agent::control_intent::{parse_control_intent, ControlIntentInput};
use chuang_agent::control_plane::ManagedUnit;
use chuang_agent::control_workflow::{
    build_decision_view, build_unit_views, run_control_workflow_for_unit, ControlWorkflowError,
    ControlWorkflowRequest,
};
use chuang_agent::slot_registry::{build_runtime_slots, ControlPlaneSlot};

use crate::cli_args::{
    control_intent_error_to_cli, parse_control_apply, parse_control_output,
    parse_control_runtime_options,
};
use crate::cli_output::{
    print_control_unit_view, print_control_view_with_format, print_json, usage, ControlOutputFormat,
};

pub(crate) fn control_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => control_list_command(&args[1..]),
        Some("apply") => control_apply_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn control_list_command(args: &[String]) -> Result<(), String> {
    let output = parse_control_output(args)?;
    let options = parse_control_runtime_options(args)?;
    let slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    let views = build_unit_views(
        slots
            .control_plane
            .try_list_units()
            .map_err(|err| format!("control_failed: {err:?}"))?,
    );
    match output {
        ControlOutputFormat::Text => {
            for unit in views {
                print_control_unit_view(&unit);
            }
        }
        ControlOutputFormat::Json => print_json(&views)?,
    }

    Ok(())
}

fn control_apply_command(args: &[String]) -> Result<(), String> {
    let request = parse_control_apply(args)?;
    let options = parse_control_runtime_options(args)?;
    let mut slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let unit_key = request
        .intent
        .unit_id
        .as_deref()
        .ok_or_else(|| "control apply requires --unit".to_string())?;
    let unit = find_control_unit(&slots.control_plane, unit_key)?;
    let control = parse_control_intent(ControlIntentInput {
        unit_id: Some(unit.unit_id.clone()),
        action: request.intent.action,
        reason: request.intent.reason,
        model_name: request.intent.model_name,
    })
    .map_err(control_intent_error_to_cli)?;
    let result = match run_control_workflow_for_unit(
        &mut slots.control_plane,
        &mut slots.governance,
        ControlWorkflowRequest {
            control,
            approved: request.approve,
        },
        &unit,
    ) {
        Ok(result) => result,
        Err(ControlWorkflowError::ApprovalRequired(decision)) => {
            print_control_view_with_format(&build_decision_view(&unit, &decision), request.output)?;
            return Err("control action requires --approve".to_string());
        }
        Err(ControlWorkflowError::NotAllowed(decision)) => {
            print_control_view_with_format(&build_decision_view(&unit, &decision), request.output)?;
            return Err("control action was not allowed by governance".to_string());
        }
        Err(ControlWorkflowError::Control(err)) => return Err(format!("control_failed: {err:?}")),
        Err(ControlWorkflowError::Governance(err)) => {
            return Err(format!("governance_failed: {}", err.message))
        }
    };

    print_control_view_with_format(&result.view, request.output)?;
    let receipt = result
        .receipt
        .ok_or_else(|| "control workflow returned no receipt".to_string())?;
    if request.output == ControlOutputFormat::Text {
        println!(
            "control_applied unit_id={} action={} previous={:?} next={:?} model={}",
            receipt.unit_id,
            receipt.action.as_str(),
            receipt.previous_status,
            receipt.next_status,
            receipt.model_name.as_deref().unwrap_or("none")
        );
    }

    Ok(())
}

fn find_control_unit(
    control_plane: &ControlPlaneSlot,
    unit_id: &str,
) -> Result<ManagedUnit, String> {
    control_plane
        .try_list_units()
        .map_err(|err| format!("control_failed: {err:?}"))?
        .into_iter()
        .find(|unit| unit.unit_id == unit_id || unit.display_name == unit_id)
        .ok_or_else(|| format!("unknown control unit: {unit_id}"))
}
