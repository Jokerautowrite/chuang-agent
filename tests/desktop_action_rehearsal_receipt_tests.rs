use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::governance::{RiskDecision, StaticRuleGovernance};
use chuang_agent::runtime_config::{ActuatorCommandConfig, ActuatorConfig};
use chuang_agent::tool_runtime::{
    execute_tool_call_with_governance_and_config, ToolCall, ToolExecutionConfig,
};
use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn receipt_script_path() -> PathBuf {
    manifest_dir().join("scripts/chuang-desktop-action-rehearsal-receipt.sh")
}

fn temp_workspace(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should move forward")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-desktop-action-{name}-{nanos}"))
}

fn run_receipt_script() -> Value {
    let output = Command::new("bash")
        .arg(receipt_script_path())
        .arg("--json")
        .output()
        .expect("desktop action rehearsal receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
        .expect("receipt output should be json")
}

#[test]
fn desktop_action_rehearsal_receipt_script_static_safety_guards() {
    let script =
        fs::read_to_string(receipt_script_path()).expect("receipt script should be readable");

    assert!(script.contains("CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(script.contains("chuang-real-actuator-adapter.py"));
    assert!(script.contains("config/actuator-allowlist.example.json"));
    assert!(script.contains("actuator.operation.live"));
    assert!(script.contains("LocalDesktopInteraction"));
    assert!(script.contains("env -u CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(script.contains("performs_desktop_action"));
    assert!(script.contains("global_real_live_ready"));

    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
}

#[test]
fn desktop_action_rehearsal_receipt_script_outputs_dry_run_receipt() {
    let data = run_receipt_script();

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "desktop_action_rehearsal_receipt");
    assert!(data["tested_at"].as_str().is_some());
    assert_eq!(data["action"], "open_app");
    assert_eq!(data["app_name"], "Chrome");
    assert_eq!(data["uses_actuator_adapter"], true);
    assert_eq!(data["uses_allowlist"], true);
    assert_eq!(
        data["adapter_path"],
        "scripts/chuang-real-actuator-adapter.py"
    );
    assert_eq!(
        data["allowlist_path"],
        "config/actuator-allowlist.example.json"
    );
    assert_eq!(data["audit_label"], "actuator.operation.live");
    assert_eq!(data["required_env"], "CHUANG_REAL_ACTUATOR_ENABLE");
    assert_eq!(data["live_gate_env_state"], "<missing>");
    assert_eq!(data["dry_run"], true);
    assert_eq!(data["real_execution"], false);
    assert_eq!(data["performs_desktop_action"], false);
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["global_real_live_ready"], false);

    let adapter = &data["adapter_response"];
    assert_eq!(adapter["allowed"], true);
    assert_eq!(adapter["action"], "open_app");
    assert_eq!(adapter["app_handle_uri"], "chuang-actuator://app/Chrome");
    let message = adapter["message"].as_str().expect("message should exist");
    assert!(message.contains("allowed=true"));
    assert!(message.contains("dry_run=true"));
    assert!(message.contains("real_execution=false"));
    assert!(message.contains("audit_label=actuator.operation.live"));
    assert!(message.contains("required_env=CHUANG_REAL_ACTUATOR_ENABLE"));

    let governance = &data["governance"];
    assert_eq!(governance["action_kind"], "LocalDesktopInteraction");
    assert_eq!(governance["decision"], "allowed");
    assert_eq!(
        governance["reason"],
        "local action allowed by static policy"
    );

    let boundaries = &data["boundaries"];
    assert_eq!(boundaries["uses_actuator_adapter"], true);
    assert_eq!(boundaries["uses_allowlist"], true);
    assert_eq!(boundaries["requires_live_gate_for_real_execution"], true);
    assert_eq!(boundaries["live_gate_closed_for_rehearsal"], true);
    assert_eq!(boundaries["performs_desktop_action"], false);
    assert_eq!(boundaries["connects_real_provider"], false);
    assert_eq!(boundaries["connects_real_feishu"], false);
    assert_eq!(boundaries["modifies_repo"], false);
    assert_eq!(boundaries["deletes_files"], false);
}

#[test]
fn desktop_action_open_app_goes_through_tool_governance_and_command_actuator() {
    let root = temp_workspace("tool-runtime");
    fs::create_dir_all(&root).expect("workspace root should be created");

    let adapter = manifest_dir().join("scripts/chuang-real-actuator-adapter.py");
    let allowlist = manifest_dir().join("config/actuator-allowlist.example.json");
    let mut governance = StaticRuleGovernance::new();
    let config = ToolExecutionConfig {
        actuator: Some(ActuatorConfig::Command(ActuatorCommandConfig {
            program: "env".to_string(),
            args: format!(
                "-u CHUANG_REAL_ACTUATOR_ENABLE {} --json --allowlist {}",
                adapter.display(),
                allowlist.display()
            ),
            timeout_ms: 30_000,
        })),
        ..ToolExecutionConfig::default()
    };

    let outcome = execute_tool_call_with_governance_and_config(
        &root,
        &mut governance,
        &ToolCall::OpenApp {
            app_name: "Chrome".to_string(),
        },
        "desktop-rehearsal-test",
        "turn-1:open-app",
        &config,
    )
    .expect("governed open_app should succeed");

    assert!(matches!(outcome.decision, RiskDecision::Allowed { .. }));
    assert!(outcome.record.ok);
    assert_eq!(outcome.record.tool_name, "open_app");
    assert_eq!(
        outcome.record.decision.as_deref(),
        Some("allowed:local action allowed by static policy")
    );
    assert!(outcome
        .record
        .output
        .as_deref()
        .expect("open_app output should exist")
        .contains("chuang-actuator://app/Chrome"));

    assert_eq!(governance.audit_records().len(), 1);
    assert_eq!(governance.audit_records()[0].operation, "tool.open_app");
    assert!(governance.audit_records()[0]
        .reason
        .contains("decision=allowed"));
    assert!(governance.audit_records()[0].reason.contains("ok=true"));
}
