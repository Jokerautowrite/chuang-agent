use chuang_agent::control_plane::{ControlPlane, ManagedUnit};
use chuang_agent::control_surface::{
    list_control_surface_units, run_control_surface_intent, ControlSurfaceError,
    ControlSurfaceRequest,
};
use chuang_agent::control_workflow::{build_decision_view, ControlWorkflowError};
use chuang_agent::runtime_config::RuntimeConfig;
use chuang_agent::slot_registry::build_runtime_slots;

use crate::cli_args::{control_intent_error_to_cli, parse_control_apply, parse_control_output};
use crate::cli_output::{
    print_control_unit_view, print_control_view_with_format, print_json, usage, ControlOutputFormat,
};
use crate::cli_runtime::default_db_path;
use crate::cli_types::CliOptions;

pub(crate) fn control_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => control_list_command(&args[1..]),
        Some("apply") => control_apply_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn control_list_command(args: &[String]) -> Result<(), String> {
    let output = parse_control_output(args)?;

    let options = CliOptions {
        runtime: RuntimeConfig::new(default_db_path()),
    };
    let slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    let views = list_control_surface_units(&slots.control_plane);
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
    let options = CliOptions {
        runtime: RuntimeConfig::new(default_db_path()),
    };
    let mut slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let unit_key = request
        .intent
        .unit_id
        .as_deref()
        .ok_or_else(|| "control apply requires --unit".to_string())?;
    let unit = find_control_unit(&slots.control_plane, unit_key)?;
    let result = match run_control_surface_intent(
        &mut slots.control_plane,
        &mut slots.governance,
        ControlSurfaceRequest {
            intent: request.intent,
            approved: request.approve,
        },
    ) {
        Ok(result) => result,
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::ApprovalRequired(decision))) => {
            print_control_view_with_format(&build_decision_view(&unit, &decision), request.output)?;
            return Err("control action requires --approve".to_string());
        }
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::NotAllowed(decision))) => {
            print_control_view_with_format(&build_decision_view(&unit, &decision), request.output)?;
            return Err("control action was not allowed by governance".to_string());
        }
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::Control(err))) => {
            return Err(format!("control_failed: {err:?}"))
        }
        Err(ControlSurfaceError::Workflow(ControlWorkflowError::Governance(err))) => {
            return Err(format!("governance_failed: {}", err.message))
        }
        Err(ControlSurfaceError::Intent(err)) => return Err(control_intent_error_to_cli(err)),
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

fn find_control_unit<P: ControlPlane>(
    control_plane: &P,
    unit_id: &str,
) -> Result<ManagedUnit, String> {
    control_plane
        .list_units()
        .into_iter()
        .find(|unit| unit.unit_id == unit_id || unit.display_name == unit_id)
        .ok_or_else(|| format!("unknown control unit: {unit_id}"))
}
