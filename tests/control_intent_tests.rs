use chuang_agent::control_intent::{
    parse_control_intent, resolve_control_unit_id, ControlIntentError, ControlIntentInput,
};
use chuang_agent::control_plane::{ControlAction, ControlPlane, FakeControlPlane};

#[test]
fn control_intent_parses_cli_style_actions() {
    let request = parse_control_intent(ControlIntentInput {
        unit_id: Some("codex-xiaoce".to_string()),
        action: Some("restart".to_string()),
        reason: Some("test restart".to_string()),
        model_name: None,
    })
    .expect("restart intent should parse");

    assert_eq!(request.unit_id, "codex-xiaoce");
    assert!(matches!(request.action, ControlAction::Restart));
    assert_eq!(request.reason, "test restart");
}

#[test]
fn control_intent_parses_feishu_friendly_action_aliases() {
    let request = parse_control_intent(ControlIntentInput {
        unit_id: Some("codex-xiaoce".to_string()),
        action: Some("换模型".to_string()),
        reason: Some("test model switch".to_string()),
        model_name: Some("gpt-5.5".to_string()),
    })
    .expect("friendly model switch intent should parse");

    assert!(matches!(
        request.action,
        ControlAction::ChangeModel { ref model_name } if model_name == "gpt-5.5"
    ));
}

#[test]
fn control_intent_rejects_missing_required_fields() {
    let err = parse_control_intent(ControlIntentInput {
        unit_id: Some("codex-xiaoce".to_string()),
        action: None,
        reason: Some("test missing".to_string()),
        model_name: None,
    })
    .expect_err("missing action should fail");

    assert_eq!(err, ControlIntentError::MissingAction);
}

#[test]
fn control_intent_rejects_unsupported_actions_without_fallback() {
    let err = parse_control_intent(ControlIntentInput {
        unit_id: Some("codex-xiaoce".to_string()),
        action: Some("reload".to_string()),
        reason: Some("test unsupported".to_string()),
        model_name: None,
    })
    .expect_err("unsupported action should fail");

    assert_eq!(
        err,
        ControlIntentError::UnsupportedAction("reload".to_string())
    );
}

#[test]
fn control_intent_requires_model_for_model_switch() {
    let err = parse_control_intent(ControlIntentInput {
        unit_id: Some("codex-xiaoce".to_string()),
        action: Some("change-model".to_string()),
        reason: Some("test missing model".to_string()),
        model_name: None,
    })
    .expect_err("missing model should fail");

    assert_eq!(err, ControlIntentError::MissingModel);
}

#[test]
fn control_intent_resolves_display_name_for_human_surfaces() {
    let control_plane = FakeControlPlane::default_local_agents();
    let units = control_plane.list_units();

    let unit_id = resolve_control_unit_id(&units, "小策").expect("display name should resolve");

    assert_eq!(unit_id, "codex-xiaoce");
}

#[test]
fn control_intent_resolves_unit_id_without_display_lookup() {
    let control_plane = FakeControlPlane::default_local_agents();
    let units = control_plane.list_units();

    let unit_id = resolve_control_unit_id(&units, "codex-feishu-bot.service")
        .expect("unit id should resolve");

    assert_eq!(unit_id, "codex-feishu-bot.service");
}

#[test]
fn control_intent_rejects_unknown_unit_names() {
    let control_plane = FakeControlPlane::default_local_agents();
    let units = control_plane.list_units();

    let err = resolve_control_unit_id(&units, "小不存在").expect_err("unknown unit should fail");

    assert_eq!(err, ControlIntentError::UnknownUnit("小不存在".to_string()));
}
