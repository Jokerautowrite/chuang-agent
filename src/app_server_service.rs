use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::process::Command;

pub const APP_SERVER_SERVICE_NAME: &str = "chuang-agent-app-server.service";
const SELECTED_GATE_NAMES: [&str; 4] = [
    "CHUANG_CODEX_RUNNER_ENABLE",
    "CHUANG_REAL_CONTROL_ENABLE",
    "CHUANG_REAL_ACTUATOR_ENABLE",
    "CHUANG_REAL_CONTROL_STATUS_ENABLE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServerServiceEvidence {
    pub service_name: String,
    pub load_state: Option<String>,
    pub active_state: Option<String>,
    pub sub_state: Option<String>,
    pub unit_file_state: Option<String>,
    pub main_pid: Option<u32>,
    pub restart_count: Option<u64>,
    pub fragment_path: Option<String>,
    pub exec_start: Option<String>,
    pub process_environment: Option<BTreeMap<String, String>>,
    pub observation_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppServerServiceRuntimeSnapshot {
    pub service_name: String,
    pub observation_state: String,
    pub loaded: Option<String>,
    pub active: Option<String>,
    pub substate: Option<String>,
    pub enabled: Option<String>,
    pub main_pid: Option<u32>,
    pub restart_count: Option<u64>,
    pub fragment_path: Option<String>,
    pub binary_summary: Option<String>,
    pub caller_environment: AppServerGateEnvironment,
    pub service_environment: Option<AppServerGateEnvironment>,
    pub effective_environment: String,
    pub observation_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppServerGateEnvironment {
    pub source: String,
    pub selected_gate_states: Vec<AppServerGateState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppServerGateState {
    pub name: String,
    pub state: String,
    pub enabled: bool,
}

pub fn collect_app_server_service_runtime_snapshot() -> AppServerServiceRuntimeSnapshot {
    let caller_environment = selected_environment_from_process();
    let mut evidence = collect_service_evidence();

    if service_is_active(&evidence) {
        evidence.process_environment = match evidence.main_pid {
            Some(pid) => match read_selected_process_environment(pid) {
                Ok(environment) => Some(environment),
                Err(error) => {
                    evidence.observation_error = Some(error);
                    None
                }
            },
            None => None,
        };
    }

    app_server_service_runtime_snapshot_from_evidence(evidence, caller_environment)
}

pub fn app_server_service_runtime_snapshot_from_evidence(
    evidence: AppServerServiceEvidence,
    caller_environment: BTreeMap<String, String>,
) -> AppServerServiceRuntimeSnapshot {
    let caller_environment =
        app_server_gate_environment_from_values("caller_environment", &caller_environment);
    let service_environment = service_is_active(&evidence)
        .then(|| evidence.process_environment.as_ref())
        .flatten()
        .map(|values| app_server_gate_environment_from_values("service_environment", values));
    let service_is_active = service_is_active(&evidence);
    let effective_environment = if service_environment.is_some() {
        "service_environment"
    } else {
        "caller_environment"
    }
    .to_string();

    AppServerServiceRuntimeSnapshot {
        service_name: evidence.service_name,
        observation_state: if service_environment.is_some() {
            "active_process_environment_observed".to_string()
        } else if service_is_active {
            "active_process_environment_unavailable".to_string()
        } else if evidence.observation_error.is_some() {
            "unavailable".to_string()
        } else {
            "service_not_active".to_string()
        },
        loaded: evidence.load_state,
        active: evidence.active_state,
        substate: evidence.sub_state,
        enabled: evidence.unit_file_state,
        main_pid: evidence.main_pid,
        restart_count: evidence.restart_count,
        fragment_path: evidence.fragment_path,
        binary_summary: safe_binary_summary(evidence.exec_start.as_deref()),
        caller_environment,
        service_environment,
        effective_environment,
        observation_error: evidence.observation_error,
    }
}

pub fn effective_gate_value(
    snapshot: &AppServerServiceRuntimeSnapshot,
    gate_name: &str,
) -> Option<String> {
    let environment = if snapshot.effective_environment == "service_environment" {
        snapshot
            .service_environment
            .as_ref()
            .unwrap_or(&snapshot.caller_environment)
    } else {
        &snapshot.caller_environment
    };
    environment
        .selected_gate_states
        .iter()
        .find(|gate| gate.name == gate_name)
        .and_then(|gate| match gate.state.as_str() {
            "enabled" => Some("1".to_string()),
            "set_non_enabling" => Some("configured".to_string()),
            _ => None,
        })
}

fn collect_service_evidence() -> AppServerServiceEvidence {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            APP_SERVER_SERVICE_NAME,
            "--property=LoadState",
            "--property=ActiveState",
            "--property=SubState",
            "--property=UnitFileState",
            "--property=MainPID",
            "--property=NRestarts",
            "--property=FragmentPath",
            "--property=ExecStart",
            "--no-pager",
        ])
        .output();

    let Ok(output) = output else {
        return unavailable_evidence("systemctl_user_show_unavailable");
    };
    if !output.status.success() {
        return unavailable_evidence("systemctl_user_show_failed");
    }

    let values = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<BTreeMap<_, _>>();
    AppServerServiceEvidence {
        service_name: APP_SERVER_SERVICE_NAME.to_string(),
        load_state: values.get("LoadState").cloned(),
        active_state: values.get("ActiveState").cloned(),
        sub_state: values.get("SubState").cloned(),
        unit_file_state: values.get("UnitFileState").cloned(),
        main_pid: values
            .get("MainPID")
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|pid| *pid > 0),
        restart_count: values
            .get("NRestarts")
            .and_then(|value| value.parse::<u64>().ok()),
        fragment_path: values.get("FragmentPath").cloned(),
        exec_start: values.get("ExecStart").cloned(),
        process_environment: None,
        observation_error: None,
    }
}

fn unavailable_evidence(error: &str) -> AppServerServiceEvidence {
    AppServerServiceEvidence {
        service_name: APP_SERVER_SERVICE_NAME.to_string(),
        load_state: None,
        active_state: None,
        sub_state: None,
        unit_file_state: None,
        main_pid: None,
        restart_count: None,
        fragment_path: None,
        exec_start: None,
        process_environment: None,
        observation_error: Some(error.to_string()),
    }
}

fn selected_environment_from_process() -> BTreeMap<String, String> {
    SELECTED_GATE_NAMES
        .iter()
        .filter_map(|name| {
            env::var(name)
                .ok()
                .map(|value| ((*name).to_string(), value))
        })
        .collect()
}

fn read_selected_process_environment(pid: u32) -> Result<BTreeMap<String, String>, String> {
    let bytes = fs::read(format!("/proc/{pid}/environ"))
        .map_err(|_| "service_process_environment_unreadable".to_string())?;
    let mut values = BTreeMap::new();
    for entry in bytes.split(|byte| *byte == 0) {
        let Ok(entry) = std::str::from_utf8(entry) else {
            continue;
        };
        let Some((name, value)) = entry.split_once('=') else {
            continue;
        };
        if SELECTED_GATE_NAMES.contains(&name) {
            values.insert(name.to_string(), value.to_string());
        }
    }
    Ok(values)
}

fn app_server_gate_environment_from_values(
    source: &str,
    values: &BTreeMap<String, String>,
) -> AppServerGateEnvironment {
    AppServerGateEnvironment {
        source: source.to_string(),
        selected_gate_states: SELECTED_GATE_NAMES
            .iter()
            .map(|name| {
                let value = values.get(*name);
                let enabled = value.is_some_and(|value| value == "1");
                AppServerGateState {
                    name: (*name).to_string(),
                    state: if enabled {
                        "enabled"
                    } else if value.is_some() {
                        "set_non_enabling"
                    } else {
                        "unset"
                    }
                    .to_string(),
                    enabled,
                }
            })
            .collect(),
    }
}

fn service_is_active(evidence: &AppServerServiceEvidence) -> bool {
    evidence.active_state.as_deref() == Some("active") && evidence.main_pid.is_some()
}

fn safe_binary_summary(exec_start: Option<&str>) -> Option<String> {
    let exec_start = exec_start?;
    let path = exec_start.split("path=").nth(1)?.split(';').next()?.trim();
    if path.is_empty()
        || path.len() > 256
        || path.chars().any(char::is_control)
        || ["token", "secret", "password", "api_key"]
            .iter()
            .any(|needle| path.to_ascii_lowercase().contains(needle))
    {
        return None;
    }
    Some(path.to_string())
}
