use std::collections::BTreeMap;

use chuang_agent::app_server_service::{
    app_server_service_runtime_snapshot_from_evidence, effective_gate_value,
    AppServerServiceEvidence,
};

fn evidence(
    active_state: Option<&str>,
    main_pid: Option<u32>,
    process_environment: Option<BTreeMap<String, String>>,
) -> AppServerServiceEvidence {
    AppServerServiceEvidence {
        service_name: "chuang-agent-app-server.service".to_string(),
        load_state: Some("loaded".to_string()),
        active_state: active_state.map(str::to_string),
        sub_state: Some("running".to_string()),
        unit_file_state: Some("enabled".to_string()),
        main_pid,
        restart_count: Some(3),
        fragment_path: Some("/home/test/.config/systemd/user/chuang-agent-app-server.service".to_string()),
        exec_start: Some(
            "{ path=/home/test/.local/bin/chuang-agent ; argv[]=/home/test/.local/bin/chuang-agent app-server ; }"
                .to_string(),
        ),
        process_environment,
        observation_error: None,
    }
}

#[test]
fn active_service_environment_becomes_effective_without_exposing_values() {
    let caller = BTreeMap::new();
    let service_environment = BTreeMap::from([
        ("CHUANG_CODEX_RUNNER_ENABLE".to_string(), "1".to_string()),
        ("CHUANG_REAL_CONTROL_ENABLE".to_string(), "1".to_string()),
        ("CHUANG_REAL_ACTUATOR_ENABLE".to_string(), "1".to_string()),
        (
            "CHUANG_REAL_CONTROL_STATUS_ENABLE".to_string(),
            "1".to_string(),
        ),
        (
            "UNRELATED_SECRET".to_string(),
            "must-not-appear".to_string(),
        ),
    ]);

    let snapshot = app_server_service_runtime_snapshot_from_evidence(
        evidence(Some("active"), Some(4242), Some(service_environment)),
        caller,
    );

    assert_eq!(
        snapshot.observation_state,
        "active_process_environment_observed"
    );
    assert_eq!(snapshot.effective_environment, "service_environment");
    assert_eq!(snapshot.loaded.as_deref(), Some("loaded"));
    assert_eq!(snapshot.active.as_deref(), Some("active"));
    assert_eq!(snapshot.enabled.as_deref(), Some("enabled"));
    assert_eq!(snapshot.main_pid, Some(4242));
    assert_eq!(snapshot.restart_count, Some(3));
    assert_eq!(
        snapshot.binary_summary.as_deref(),
        Some("/home/test/.local/bin/chuang-agent")
    );
    assert_eq!(
        effective_gate_value(&snapshot, "CHUANG_REAL_ACTUATOR_ENABLE").as_deref(),
        Some("1")
    );
    assert!(snapshot
        .service_environment
        .as_ref()
        .expect("active service environment should be present")
        .selected_gate_states
        .iter()
        .all(|gate| gate.name.starts_with("CHUANG_")));
    assert!(!serde_json::to_string(&snapshot)
        .expect("snapshot should serialize")
        .contains("must-not-appear"));
}

#[test]
fn inactive_service_keeps_caller_environment_effective() {
    let caller = BTreeMap::from([("CHUANG_REAL_CONTROL_ENABLE".to_string(), "1".to_string())]);
    let snapshot = app_server_service_runtime_snapshot_from_evidence(
        evidence(Some("inactive"), None, None),
        caller,
    );

    assert_eq!(snapshot.observation_state, "service_not_active");
    assert_eq!(snapshot.effective_environment, "caller_environment");
    assert!(snapshot.service_environment.is_none());
    assert_eq!(
        effective_gate_value(&snapshot, "CHUANG_REAL_CONTROL_ENABLE").as_deref(),
        Some("1")
    );
    assert_eq!(
        effective_gate_value(&snapshot, "CHUANG_REAL_ACTUATOR_ENABLE"),
        None
    );
}

#[test]
fn active_service_without_readable_process_environment_falls_back_to_caller() {
    let mut service = evidence(Some("active"), Some(4242), None);
    service.observation_error = Some("service_process_environment_unreadable".to_string());
    let caller = BTreeMap::from([(
        "CHUANG_REAL_ACTUATOR_ENABLE".to_string(),
        "configured".to_string(),
    )]);

    let snapshot = app_server_service_runtime_snapshot_from_evidence(service, caller);

    assert_eq!(
        snapshot.observation_state,
        "active_process_environment_unavailable"
    );
    assert_eq!(snapshot.effective_environment, "caller_environment");
    assert_eq!(
        effective_gate_value(&snapshot, "CHUANG_REAL_ACTUATOR_ENABLE").as_deref(),
        Some("configured")
    );
    assert_eq!(
        snapshot.observation_error.as_deref(),
        Some("service_process_environment_unreadable")
    );
}
