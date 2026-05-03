use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chuang_agent::control_plane::{
    audit_record_for_control, proposed_action_for_control, CommandControlPlane, ControlAction,
    ControlError, ControlPlane, ControlRequest, FakeControlPlane, ManagedUnit, ManagedUnitKind,
    ManagedUnitStatus,
};
use chuang_agent::governance::{Governance, RiskDecision, StaticRuleGovernance};
use chuang_agent::runtime_config::ControlPlaneCommandConfig;

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

fn temp_script_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-control-{name}-{nanos}.sh"))
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
fn command_control_plane_lists_and_applies_external_command_json() {
    let mut plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "printf".to_string(),
        list_args: r#"[{"unit_id":"command-agent","display_name":"CommandAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{"channel":"command"}}]"#
            .to_string(),
        apply_args: r#"{"unit_id":"command-agent","action":"change_model","previous_status":"Running","next_status":"Running","model_name":"gpt-5.4","message":"command_control_applied"}"#
            .to_string(),
        timeout_ms: 30_000,
    });

    let units = plane
        .try_list_units()
        .expect("script list should return units");
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].unit_id, "command-agent");
    assert_eq!(
        units[0].metadata.get("channel").map(String::as_str),
        Some("command")
    );

    let receipt = plane
        .apply(ControlRequest {
            unit_id: "command-agent".to_string(),
            action: ControlAction::ChangeModel {
                model_name: "gpt-5.4".to_string(),
            },
            reason: "test command control".to_string(),
        })
        .expect("command control apply should parse receipt");

    assert_eq!(receipt.unit_id, "command-agent");
    assert_eq!(receipt.model_name, Some("gpt-5.4".to_string()));
    assert_eq!(receipt.message, "command_control_applied");
}

#[test]
fn command_control_plane_passes_apply_request_to_external_command_stdin() {
    let script_path = temp_script_path("stdin");
    fs::write(
        &script_path,
        r#"#!/bin/sh
if [ "$1" = "list" ]; then
  printf '[{"unit_id":"script-agent","display_name":"ScriptAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{"channel":"script"}}]'
  exit 0
fi
if [ "$1" = "apply" ]; then
  input="$(cat)"
  model="$(printf '%s' "$input" | sed -n 's/.*"model_name":"\([^"]*\)".*/\1/p')"
  printf '{"unit_id":"script-agent","action":"change_model","previous_status":"Running","next_status":"Running","model_name":"%s","message":"stdin_model_received"}' "$model"
  exit 0
fi
exit 2
"#,
    )
    .expect("script should be writable");
    let mut perms = fs::metadata(&script_path)
        .expect("script metadata should exist")
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&script_path, perms).expect("script should be executable");

    let mut plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "sh".to_string(),
        list_args: format!("{} list", script_path.display()),
        apply_args: format!("{} apply", script_path.display()),
        timeout_ms: 30_000,
    });

    let units = plane
        .try_list_units()
        .expect("script list should return units");
    assert_eq!(units.len(), 1);
    assert_eq!(
        units[0].metadata.get("channel").map(String::as_str),
        Some("script")
    );

    let receipt = plane
        .apply(ControlRequest {
            unit_id: "script-agent".to_string(),
            action: ControlAction::ChangeModel {
                model_name: "gpt-5.4".to_string(),
            },
            reason: "test stdin bridge".to_string(),
        })
        .expect("script apply should return receipt");

    assert_eq!(receipt.model_name, Some("gpt-5.4".to_string()));
    assert_eq!(receipt.message, "stdin_model_received");
}

#[test]
fn command_control_plane_preserves_quoted_arguments_without_shell() {
    let script_path = temp_script_path("quoted");
    fs::write(
        &script_path,
        r#"#!/bin/sh
if [ "$1" = "list" ] && [ "$2" = "agent with space" ]; then
  printf '[{"unit_id":"quoted-agent","display_name":"QuotedAgent","kind":"agent","status":"Running","model_name":null,"metadata":{"label":"%s"}}]' "$2"
  exit 0
fi
exit 2
"#,
    )
    .expect("script should be writable");
    let mut perms = fs::metadata(&script_path)
        .expect("script metadata should exist")
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&script_path, perms).expect("script should be executable");

    let plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "sh".to_string(),
        list_args: format!(r#"{} list "agent with space""#, script_path.display()),
        apply_args: "apply".to_string(),
        timeout_ms: 30_000,
    });

    let units = plane
        .try_list_units()
        .expect("quoted list args should be passed as one argv value");

    assert_eq!(units.len(), 1);
    assert_eq!(
        units[0].metadata.get("label").map(String::as_str),
        Some("agent with space")
    );
}

#[test]
fn command_control_plane_reports_list_command_failure() {
    let plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "false".to_string(),
        list_args: String::new(),
        apply_args: String::new(),
        timeout_ms: 30_000,
    });

    let err = plane
        .try_list_units()
        .expect_err("failing list command should be reported");

    assert!(matches!(err, ControlError::InvalidRequest(_)));
    assert!(format!("{err:?}").contains("status=Some(1)"));
}

#[test]
fn command_control_plane_reports_malformed_list_json() {
    let plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "printf".to_string(),
        list_args: "not-json".to_string(),
        apply_args: String::new(),
        timeout_ms: 30_000,
    });

    let err = plane
        .try_list_units()
        .expect_err("malformed list output should be reported");

    assert!(matches!(err, ControlError::InvalidRequest(_)));
    assert!(format!("{err:?}").contains("control list output parse failed"));
}

#[test]
fn command_control_plane_times_out_stuck_list_command() {
    let plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "sleep".to_string(),
        list_args: "1".to_string(),
        apply_args: String::new(),
        timeout_ms: 20,
    });
    let started = Instant::now();

    let err = plane
        .try_list_units()
        .expect_err("stuck list command should time out");

    assert!(matches!(err, ControlError::InvalidRequest(_)));
    assert!(format!("{err:?}").contains("timed out after 20ms"));
    assert!(started.elapsed().as_millis() < 500);
}

#[test]
fn command_control_plane_rejects_apply_receipt_for_wrong_unit() {
    let mut plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "printf".to_string(),
        list_args: r#"[{"unit_id":"command-agent","display_name":"CommandAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{}}]"#
            .to_string(),
        apply_args: r#"{"unit_id":"other-agent","action":"restart","previous_status":"Running","next_status":"Running","model_name":null,"message":"wrong"}"#
            .to_string(),
        timeout_ms: 30_000,
    });

    let err = plane
        .apply(ControlRequest {
            unit_id: "command-agent".to_string(),
            action: ControlAction::Restart,
            reason: "test mismatched unit".to_string(),
        })
        .expect_err("mismatched receipt unit should fail");

    assert!(matches!(err, ControlError::InvalidRequest(_)));
    assert!(format!("{err:?}").contains("receipt unit_id mismatch"));
}

#[test]
fn command_control_plane_rejects_apply_receipt_for_wrong_action() {
    let mut plane = CommandControlPlane::new(ControlPlaneCommandConfig {
        program: "printf".to_string(),
        list_args: r#"[{"unit_id":"command-agent","display_name":"CommandAgent","kind":"agent","status":"Running","model_name":"gpt-5.5","metadata":{}}]"#
            .to_string(),
        apply_args: r#"{"unit_id":"command-agent","action":"stop","previous_status":"Running","next_status":"Stopped","model_name":null,"message":"wrong"}"#
            .to_string(),
        timeout_ms: 30_000,
    });

    let err = plane
        .apply(ControlRequest {
            unit_id: "command-agent".to_string(),
            action: ControlAction::Restart,
            reason: "test mismatched action".to_string(),
        })
        .expect_err("mismatched receipt action should fail");

    assert!(matches!(err, ControlError::InvalidRequest(_)));
    assert!(format!("{err:?}").contains("receipt action mismatch"));
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

#[test]
fn control_requests_can_be_classified_by_governance_before_apply() {
    let unit = unit("agent-1", ManagedUnitKind::Agent);
    let request = ControlRequest {
        unit_id: "agent-1".to_string(),
        action: ControlAction::Restart,
        reason: "用户点击重启".to_string(),
    };

    let proposed = proposed_action_for_control(&unit, &request)
        .expect("control request should become proposed action");
    let decision = StaticRuleGovernance::new()
        .classify(&proposed)
        .expect("governance should classify control action");

    assert_eq!(proposed.target, "agent:agent-1");
    assert!(proposed.summary.contains("restart"));
    assert!(matches!(decision, RiskDecision::NeedsApproval { .. }));
}

#[test]
fn control_requests_can_build_audit_record_after_approval() {
    let unit = unit("agent-1", ManagedUnitKind::Agent);
    let request = ControlRequest {
        unit_id: "agent-1".to_string(),
        action: ControlAction::ChangeModel {
            model_name: "gpt-5.5".to_string(),
        },
        reason: "用户确认换模型".to_string(),
    };

    let audit = audit_record_for_control(&unit, &request, true)
        .expect("control request should produce audit record");

    assert_eq!(audit.operation, "control.change_model");
    assert_eq!(audit.agent_id.0, "control-plane");
    assert_eq!(audit.task_id.0, "control:agent-1");
    assert!(audit.reason.contains("approved=true"));
    assert!(audit.reason.contains("target=agent:agent-1"));
}
