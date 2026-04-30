use crate::control_plane::{ControlAction, ControlRequest, ManagedUnit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlIntentInput {
    pub unit_id: Option<String>,
    pub action: Option<String>,
    pub reason: Option<String>,
    pub model_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlIntentError {
    MissingUnit,
    MissingAction,
    MissingReason,
    MissingModel,
    UnknownUnit(String),
    AmbiguousUnit(String),
    UnsupportedAction(String),
}

pub fn resolve_control_unit_id(
    units: &[ManagedUnit],
    unit_key: &str,
) -> Result<String, ControlIntentError> {
    let key = unit_key.trim();
    if key.is_empty() {
        return Err(ControlIntentError::MissingUnit);
    }

    let matches = units
        .iter()
        .filter(|unit| unit.unit_id == key || unit.display_name == key)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [unit] => Ok(unit.unit_id.clone()),
        [] => Err(ControlIntentError::UnknownUnit(key.to_string())),
        _ => Err(ControlIntentError::AmbiguousUnit(key.to_string())),
    }
}

pub fn parse_control_intent(
    input: ControlIntentInput,
) -> Result<ControlRequest, ControlIntentError> {
    let unit_id = required_field(input.unit_id, ControlIntentError::MissingUnit)?;
    let action_text = required_field(input.action, ControlIntentError::MissingAction)?;
    let reason = required_field(input.reason, ControlIntentError::MissingReason)?;
    let action = parse_action(&action_text, input.model_name)?;

    Ok(ControlRequest {
        unit_id,
        action,
        reason,
    })
}

fn parse_action(
    raw: &str,
    model_name: Option<String>,
) -> Result<ControlAction, ControlIntentError> {
    match normalize_action(raw).as_str() {
        "start" | "启动" => Ok(ControlAction::Start),
        "stop" | "关闭" | "停止" => Ok(ControlAction::Stop),
        "restart" | "重启" => Ok(ControlAction::Restart),
        "change-model" | "change_model" | "model" | "换模型" | "切模型" => {
            Ok(ControlAction::ChangeModel {
                model_name: required_field(model_name, ControlIntentError::MissingModel)?,
            })
        }
        _ => Err(ControlIntentError::UnsupportedAction(raw.to_string())),
    }
}

fn required_field(
    value: Option<String>,
    error: ControlIntentError,
) -> Result<String, ControlIntentError> {
    let Some(value) = value else {
        return Err(error);
    };
    if value.trim().is_empty() {
        return Err(error);
    }
    Ok(value)
}

fn normalize_action(raw: &str) -> String {
    raw.trim().to_lowercase()
}
