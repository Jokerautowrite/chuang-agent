use chuang_agent::live_adapter_gate::{
    evaluate_live_adapter_gate_with_lookup, require_live_adapter_enabled_with_lookup,
    LiveAdapterSlot,
};

#[test]
fn live_adapter_gates_are_disabled_by_default_with_explicit_env_names() {
    let gate = evaluate_live_adapter_gate_with_lookup(LiveAdapterSlot::ControlApply, |_| None);

    assert_eq!(gate.name, "control_apply");
    assert_eq!(gate.required_env, "CHUANG_REAL_CONTROL_ENABLE");
    assert_eq!(gate.audit_label, "control.apply.live");
    assert!(!gate.enabled);
    assert!(!gate.default_enabled);
    assert!(gate.reason.contains("disabled by default"));

    let err = require_live_adapter_enabled_with_lookup(LiveAdapterSlot::ControlApply, |_| None)
        .expect_err("disabled live adapter should be rejected");
    assert_eq!(err.required_env, "CHUANG_REAL_CONTROL_ENABLE");
    assert_eq!(err.audit_label, "control.apply.live");
}

#[test]
fn live_adapter_gate_requires_exact_one_value() {
    let disabled =
        evaluate_live_adapter_gate_with_lookup(LiveAdapterSlot::ActuatorOperation, |_| {
            Some("true".to_string())
        });
    assert!(!disabled.enabled);

    let enabled =
        require_live_adapter_enabled_with_lookup(LiveAdapterSlot::ActuatorOperation, |name| {
            assert_eq!(name, "CHUANG_REAL_ACTUATOR_ENABLE");
            Some("1".to_string())
        })
        .expect("exact 1 should enable live adapter");

    assert!(enabled.enabled);
    assert_eq!(enabled.audit_label, "actuator.operation.live");
}

#[test]
fn subagent_runner_gate_uses_codex_runner_env() {
    let gate = evaluate_live_adapter_gate_with_lookup(LiveAdapterSlot::SubagentRunner, |name| {
        assert_eq!(name, "CHUANG_CODEX_RUNNER_ENABLE");
        Some("".to_string())
    });

    assert_eq!(gate.name, "subagent_runner");
    assert_eq!(gate.audit_label, "subagent.runner.live");
    assert!(!gate.enabled);
}
