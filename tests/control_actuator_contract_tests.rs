use std::collections::BTreeMap;

use chuang_agent::actuator::{
    validate_actuator_command_contract, ActuatorCommandContract, ActuatorCommandKind,
};
use chuang_agent::control_plane::{
    contract_for_control_unit, validate_control_contract, validate_control_receipt, ControlAction,
    ControlActionKind, ControlError, ControlReceipt, ControlRequest, ManagedUnit, ManagedUnitKind,
    ManagedUnitStatus,
};
use chuang_agent::genesis_actuator::{
    AutoCliGenesisActuator, GenesisChannel, GenesisCommandSpec, GenesisConfig,
};

fn unit_with_actions(actions: &str) -> ManagedUnit {
    ManagedUnit {
        unit_id: "command-agent".to_string(),
        display_name: "Command Agent".to_string(),
        kind: ManagedUnitKind::Agent,
        status: ManagedUnitStatus::Running,
        model_name: Some("gpt-5.5".to_string()),
        metadata: BTreeMap::from([("allowed_actions".to_string(), actions.to_string())]),
    }
}

#[test]
fn control_command_contract_requires_allowlisted_action() {
    let unit = unit_with_actions("restart, change_model");
    let allowed = validate_control_contract(
        &unit,
        &ControlRequest {
            unit_id: "command-agent".to_string(),
            action: ControlAction::Restart,
            reason: "approved operator restart".to_string(),
        },
    )
    .expect("restart should be allowlisted");

    assert_eq!(allowed.unit_id, "command-agent");
    assert_eq!(allowed.audit_label, "agent:command-agent");
    assert_eq!(
        allowed.allowed_actions,
        vec![ControlActionKind::Restart, ControlActionKind::ChangeModel]
    );

    let rejected = validate_control_contract(
        &unit,
        &ControlRequest {
            unit_id: "command-agent".to_string(),
            action: ControlAction::Stop,
            reason: "not in allowlist".to_string(),
        },
    )
    .expect_err("stop should not be allowlisted");

    assert!(matches!(rejected, ControlError::UnsupportedAction(_)));
    assert!(format!("{rejected:?}").contains("not allowlisted"));
}

#[test]
fn control_command_contract_rejects_missing_or_invalid_allowlist() {
    let mut unit = unit_with_actions("");
    unit.metadata.clear();

    let no_allowlist = validate_control_contract(
        &unit,
        &ControlRequest {
            unit_id: "command-agent".to_string(),
            action: ControlAction::Restart,
            reason: "missing allowlist".to_string(),
        },
    )
    .expect_err("command units must declare explicit allowlist");
    assert!(matches!(no_allowlist, ControlError::UnsupportedAction(_)));

    unit.metadata
        .insert("allowed_actions".to_string(), "restart,delete".to_string());
    let invalid = contract_for_control_unit(&unit).expect_err("invalid action should fail");
    assert!(matches!(invalid, ControlError::InvalidRequest(_)));
    assert!(format!("{invalid:?}").contains("invalid allowlisted control action"));
}

#[test]
fn control_receipt_validation_is_reusable_for_real_command_adapters() {
    let request = ControlRequest {
        unit_id: "command-agent".to_string(),
        action: ControlAction::ChangeModel {
            model_name: "gpt-5.5".to_string(),
        },
        reason: "operator approved model switch".to_string(),
    };
    let ok = ControlReceipt {
        unit_id: "command-agent".to_string(),
        action: ControlAction::ChangeModel {
            model_name: "gpt-5.5".to_string(),
        },
        previous_status: ManagedUnitStatus::Running,
        next_status: ManagedUnitStatus::Running,
        model_name: Some("gpt-5.5".to_string()),
        message: "adapter applied".to_string(),
    };
    validate_control_receipt(&request, &ok).expect("matching receipt should pass");

    let wrong_model = ControlReceipt {
        model_name: Some("gpt-4.1".to_string()),
        action: ControlAction::ChangeModel {
            model_name: "gpt-4.1".to_string(),
        },
        ..ok
    };
    let err = validate_control_receipt(&request, &wrong_model)
        .expect_err("wrong model receipt should fail");
    assert!(matches!(err, ControlError::InvalidRequest(_)));
    assert!(format!("{err:?}").contains("receipt model_name mismatch"));
}

#[test]
fn actuator_command_contract_requires_allowlisted_action_and_audit_label() {
    let contract = ActuatorCommandContract {
        allowed_actions: vec![
            ActuatorCommandKind::Observe,
            ActuatorCommandKind::Screenshot,
        ],
        audit_label: "desktop.readonly".to_string(),
        real_execution: false,
    };

    validate_actuator_command_contract(&contract, ActuatorCommandKind::Observe)
        .expect("observe should be allowlisted");
    let rejected = validate_actuator_command_contract(&contract, ActuatorCommandKind::Click)
        .expect_err("click should not be allowlisted");
    assert!(rejected.message.contains("not allowlisted"));

    let missing_label = ActuatorCommandContract {
        audit_label: String::new(),
        ..contract
    };
    let err = validate_actuator_command_contract(&missing_label, ActuatorCommandKind::Observe)
        .expect_err("audit label is required");
    assert!(err.message.contains("audit_label"));
}

#[test]
fn genesis_autocli_specs_expose_stable_audit_labels_without_browser_worker() {
    let actuator = AutoCliGenesisActuator::with_runner(
        GenesisConfig::new("/tmp/chuang-genesis-contract-profile"),
        (),
    );

    let primary = actuator.primary_spec("query");
    let fallback = actuator.fallback_spec("query");

    assert_eq!(primary.audit_label(), "genesis.autocli.user_data_dir");
    assert_eq!(fallback.audit_label(), "genesis.autocli.cdp");
    assert_eq!(primary.channel, GenesisChannel::UserDataDir);
    assert_eq!(fallback.channel, GenesisChannel::Cdp);
}

#[test]
fn genesis_command_spec_has_audit_label_for_each_channel() {
    let primary = GenesisCommandSpec {
        program: "autocli".to_string(),
        args: vec!["deepseek".to_string()],
        channel: GenesisChannel::UserDataDir,
        timeout_ms: 30_000,
    };
    let fallback = GenesisCommandSpec {
        channel: GenesisChannel::Cdp,
        ..primary.clone()
    };

    assert_eq!(primary.audit_label(), "genesis.autocli.user_data_dir");
    assert_eq!(fallback.audit_label(), "genesis.autocli.cdp");
}
