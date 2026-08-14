#![cfg(unix)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::app_server_service::{
    app_server_persistence_runtime_snapshot_from_socket,
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

#[test]
fn persistence_snapshot_reads_only_aggregate_fields_from_canonical_socket() {
    let socket = std::env::temp_dir().join(format!(
        "chuang-agent-service-status-{}.sock",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos()
    ));
    let listener = UnixListener::bind(&socket).expect("status socket should bind");
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("status client should connect");
        let mut reader = BufReader::new(stream.try_clone().expect("stream should clone"));
        let mut request = String::new();
        reader
            .read_line(&mut request)
            .expect("status request should read");
        assert!(request.contains("\"server/status\""));
        let mut stream = stream;
        writeln!(
            stream,
            "{}",
            serde_json::json!({
                "id": 1,
                "result": {
                    "persistence": {
                        "enabled": true,
                        "schema": 1,
                        "lock_held": true,
                        "thread_count": 2,
                        "turn_count": 5,
                        "active_count": 1,
                        "interrupted_count": 2,
                        "snapshot_updated_at": 123456789,
                    }
                }
            })
        )
        .expect("status response should write");
    });

    let snapshot = app_server_persistence_runtime_snapshot_from_socket(&socket);
    server.join().expect("status server should finish");

    assert_eq!(snapshot.observation_state, "available");
    assert_eq!(snapshot.persistence_enabled, Some(true));
    assert_eq!(snapshot.schema, Some(1));
    assert_eq!(snapshot.lock_held, Some(true));
    assert_eq!(snapshot.thread_count, Some(2));
    assert_eq!(snapshot.turn_count, Some(5));
    assert_eq!(snapshot.active_count, Some(1));
    assert_eq!(snapshot.interrupted_count, Some(2));
    assert_eq!(snapshot.snapshot_updated_at, Some(123456789));
    let rendered = serde_json::to_string(&snapshot).expect("snapshot should serialize");
    assert!(!rendered.contains("provider"));
    assert!(!rendered.contains("tool"));
    assert!(!rendered.contains("secret"));
}

#[test]
fn persistence_snapshot_failure_is_unavailable_without_breaking_service_snapshot() {
    let socket = PathBuf::from(format!(
        "/tmp/chuang-agent-missing-status-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos()
    ));
    let persistence = app_server_persistence_runtime_snapshot_from_socket(&socket);
    assert_eq!(persistence.observation_state, "unavailable");
    assert!(persistence.persistence_enabled.is_none());

    let snapshot = app_server_service_runtime_snapshot_from_evidence(
        evidence(Some("inactive"), None, None),
        BTreeMap::new(),
    );
    assert_eq!(snapshot.observation_state, "service_not_active");
    assert_eq!(snapshot.persistence.observation_state, "unavailable");
}
