use std::collections::BTreeMap;

use chuang_agent::control_plane::{
    ControlAction, ControlError, ControlPlane, ControlRequest, FakeControlPlane, ManagedUnit,
    ManagedUnitKind, ManagedUnitStatus,
};

fn unit(unit_id: &str, kind: ManagedUnitKind) -> ManagedUnit {
    ManagedUnit {
        unit_id: unit_id.to_string(),
        display_name: unit_id.to_string(),
        kind,
        status: ManagedUnitStatus::Stopped,
        model_name: None,
        metadata: BTreeMap::new(),
    }
}

#[test]
fn fake_control_plane_lists_default_local_agents_without_merging_channels() {
    let plane = FakeControlPlane::default_local_agents();
    let units = plane.list_units();

    assert!(units.iter().any(|unit| unit.display_name == "小创"));
    assert!(units.iter().any(|unit| unit.display_name == "小承"));
    assert!(units.iter().any(|unit| unit.display_name == "小云"));
    assert!(units.iter().any(|unit| unit.display_name == "小策"));
    assert!(units
        .iter()
        .any(|unit| unit.unit_id == "codex-feishu-bot.service"));
}

#[test]
fn fake_control_plane_starts_stops_and_restarts_units() {
    let mut plane = FakeControlPlane::new(vec![unit("service-1", ManagedUnitKind::Service)])
        .expect("unit should be valid");

    let start = plane
        .apply(ControlRequest {
            unit_id: "service-1".to_string(),
            action: ControlAction::Start,
            reason: "test start".to_string(),
        })
        .expect("start should succeed");
    let stop = plane
        .apply(ControlRequest {
            unit_id: "service-1".to_string(),
            action: ControlAction::Stop,
            reason: "test stop".to_string(),
        })
        .expect("stop should succeed");
    let restart = plane
        .apply(ControlRequest {
            unit_id: "service-1".to_string(),
            action: ControlAction::Restart,
            reason: "test restart".to_string(),
        })
        .expect("restart should succeed");

    assert_eq!(start.next_status, ManagedUnitStatus::Running);
    assert_eq!(stop.next_status, ManagedUnitStatus::Stopped);
    assert_eq!(restart.next_status, ManagedUnitStatus::Running);
}

#[test]
fn fake_control_plane_changes_model_only_for_agents() {
    let mut plane = FakeControlPlane::new(vec![
        unit("agent-1", ManagedUnitKind::Agent),
        unit("service-1", ManagedUnitKind::Service),
    ])
    .expect("units should be valid");

    let receipt = plane
        .apply(ControlRequest {
            unit_id: "agent-1".to_string(),
            action: ControlAction::ChangeModel {
                model_name: "gpt-5.5".to_string(),
            },
            reason: "test model switch".to_string(),
        })
        .expect("agent model change should succeed");
    let service_err = plane
        .apply(ControlRequest {
            unit_id: "service-1".to_string(),
            action: ControlAction::ChangeModel {
                model_name: "gpt-5.5".to_string(),
            },
            reason: "test invalid model switch".to_string(),
        })
        .expect_err("service model change should fail");

    assert_eq!(receipt.model_name, Some("gpt-5.5".to_string()));
    assert!(matches!(service_err, ControlError::UnsupportedAction(_)));
}

#[test]
fn fake_control_plane_rejects_unknown_unit_and_empty_reason() {
    let mut plane = FakeControlPlane::new(vec![unit("agent-1", ManagedUnitKind::Agent)])
        .expect("unit should be valid");

    let unknown = plane
        .apply(ControlRequest {
            unit_id: "missing".to_string(),
            action: ControlAction::Start,
            reason: "test missing".to_string(),
        })
        .expect_err("unknown unit should fail");
    let empty_reason = plane
        .apply(ControlRequest {
            unit_id: "agent-1".to_string(),
            action: ControlAction::Start,
            reason: String::new(),
        })
        .expect_err("empty reason should fail");

    assert!(matches!(unknown, ControlError::UnknownUnit(_)));
    assert!(matches!(empty_reason, ControlError::InvalidRequest(_)));
}
