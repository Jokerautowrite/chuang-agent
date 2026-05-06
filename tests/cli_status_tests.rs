use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_identity_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-cli-{name}-{nanos}"))
}

fn write_fake_status_config(root: &std::path::Path) -> PathBuf {
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nidentity_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity").display(),
            root.join("identity-bootstrap").display()
        ),
    )
    .expect("config should be written");
    config_path
}

fn write_openai_status_config(root: &std::path::Path, env_name: &str) -> PathBuf {
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nidentity_root = \"{}\"\nprovider = \"openai_compatible\"\nprovider_id = \"openai-status\"\nmodel = \"gpt-status\"\nbase_url = \"https://api.example.com/v1\"\napi_key_env = \"{}\"\n",
            root.join("memory.db").display(),
            root.join("identity").display(),
            root.join("identity-bootstrap").display(),
            env_name
        ),
    )
    .expect("config should be written");
    config_path
}

#[test]
fn cli_status_prints_mvp_health_summary() {
    let root = temp_identity_root("status-text");
    let config_path = write_fake_status_config(&root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("kernel_agent_id: chuang-cli"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains("provider_slot: fake"));
    assert!(stdout.contains("model: stub-responder"));
    assert!(stdout.contains("identity_memory: hermes_dual_file"));
    assert!(stdout.contains("identity_experiences_path: "));
    assert!(stdout.contains("identity_memory_limits: user=1375 memory=2200"));
    assert!(stdout.contains("identity_root: "));
    assert!(stdout.contains("rules_root: ./rules"));
    assert!(stdout.contains(
        "atomic_tools: source=GenericAgent ok=true total=9 mapped=3 interface_only=6 manifest_schema_version=1 action_schema_version=1 report_schema_version=6"
    ));
    assert!(stdout.contains("atomic_tools_mapped: file_read,file_write,code_execute"));
    assert!(stdout.contains(
        "atomic_tools_interface_only: mouse,keyboard,screenshot,locate,wait,human_suspend"
    ));
    assert!(stdout.contains("atomic_tool name=file_read status=mapped"));
    assert!(stdout.contains("atomic_tool name=mouse status=interface_only"));
    assert!(stdout.contains("identity_snapshot_chars: user=0 memory=0"));
    assert!(stdout.contains("identity_bootstrap_chars: soul=0 story=0 first_wake=0 agents=0"));
    assert!(stdout.contains(
        "identity_bootstrap_present: soul=false story=false first_wake=false agents=false"
    ));
    assert!(stdout.contains("governance: static_rule"));
    assert!(stdout.contains(
        "governance_readiness: ok=true kind=static_rule rules_loaded=true tool_surface_governed=true goal_run_executes=false"
    ));
    assert!(stdout.contains("governance_rules: path=./rules/core.md rule_count="));
    assert!(stdout.contains(
        "governance_decisions: read_only=allowed dangerous_write=needs_approval dangerous_shell=needs_approval secret_shell=draft_only"
    ));
    assert!(stdout.contains("execution: generic_agent_mvp"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
    assert!(stdout.contains("subagent_queue_root: ./data/subagent-queue"));
    assert!(stdout.contains(
        "context_budget: max=512 reserve_system=32 min_working=1 max_tool_results=5 max_memory_segments=5"
    ));
    assert!(stdout.contains("control_plane: fake_local"));
    assert!(stdout.contains("goal_run: ok=true"));
    assert!(stdout.contains("goal_id=mainline-mvp"));
    assert!(stdout.contains("checkpoints="));
    assert!(stdout.contains("plugin_registry: available=true ok=true"));
    assert!(stdout.contains("project_readiness: ok=true state=mvp_ready_with_partial_modules"));
    assert!(stdout.contains("project_module name=main_chain state=ready"));
    assert!(stdout.contains("project_module name=external_ai state=ready"));
    assert!(stdout.contains(
        "release_readiness: ok=true name=second_test_version state=second_test_version_ready_with_partial_modules"
    ));
    assert!(stdout.contains("release_acceptance: count=7 ready="));
    assert!(stdout.contains(
        "connects_real_external_services=false verifies_real_external_services=false uses_stub_or_local_fixtures=true writes_repo_files=false"
    ));
    assert!(stdout.contains("release_acceptance_item name=real_external_services state=deferred"));
    assert!(stdout.contains("memory_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("memory_layer name=internal_identity state=ready"));
    assert!(stdout.contains("memory_layer name=external_knowledge state=ready storage=docs/external-knowledge-adapter.md"));
    assert!(stdout.contains(
        "memory_layer name=maintenance_loop state=ready storage=docs/memory-maintenance-loop.md"
    ));
    assert!(stdout.contains("channel_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("channel_layer name=app_server state=ready"));
    assert!(stdout.contains("channel_layer name=dedicated_feishu_bridge state=ready"));
    assert!(stdout.contains(
        "subagent_readiness: ok=true state=queued_protocol_partial mode=fake local_contract_ready=true live_adapter_ready=false"
    ));
    assert!(stdout.contains(
        "subagent_layer name=command_runner state=ready local_contract_ready=true live_adapter_ready=false"
    ));
    assert!(stdout.contains("subagent_layer name=multi_worker state=ready"));
    assert!(stdout.contains("external_ai_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("external_ai_layer name=genesis_actuator state=ready"));
    assert!(stdout.contains("external_ai_layer name=dispatch_sop state=ready"));
    assert!(stdout.contains("goal_run_checkpoint_log_complete:"));
    assert!(stdout.contains("goal_run_last_checkpoint_summary:"));
    assert!(stdout.contains("goal_run_incomplete_reasons:"));
    assert!(stdout.contains("placeholder_warning: provider=fake"));
    assert!(stdout.contains("placeholder_warning: actuator=fake"));
    assert!(stdout.contains("placeholder_warning: subagent=fake"));
    assert!(stdout.contains("placeholder_warning: control_plane=fake_local"));
}

#[test]
fn cli_status_can_render_json_without_secret_leak() {
    let config_root = temp_identity_root("status-provider-json");
    let config_path = write_fake_status_config(&config_root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-secret-key",
            "--provider-model",
            "gpt-5.5",
            "--provider-id",
            "custom-openai",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["kernel"]["agent_id"], "chuang-cli");
    assert_eq!(parsed["kernel"]["identity_soul_exists"], false);
    assert_eq!(parsed["kernel"]["identity_story_exists"], false);
    assert_eq!(parsed["kernel"]["identity_first_wake_exists"], false);
    assert_eq!(parsed["kernel"]["identity_agents_registry_exists"], false);
    assert_eq!(parsed["config"]["provider_kind"], "openai_compatible");
    assert_eq!(parsed["config"]["provider_id"], "custom-openai");
    assert_eq!(parsed["config"]["provider_request_timeout_ms"], Value::Null);
    assert_eq!(parsed["config"]["identity_memory_kind"], "hermes_dual_file");
    assert_eq!(
        parsed["config"]["identity_experiences_path"],
        config_root
            .join("identity")
            .join("experiences.md")
            .display()
            .to_string()
    );
    assert_eq!(
        parsed["config"]["identity_root"],
        config_root.join("identity-bootstrap").display().to_string()
    );
    assert_eq!(parsed["config"]["api_key_state"], "<set>");
    assert_eq!(parsed["plugin_registry"]["available"], true);
    assert_eq!(parsed["plugin_registry"]["ok"], true);
    assert_eq!(parsed["plugin_registry"]["plugin_count"], 5);
    assert_eq!(parsed["atomic_tools"]["source"], "GenericAgent");
    assert_eq!(parsed["atomic_tools"]["ok"], true);
    assert_eq!(parsed["atomic_tools"]["total_count"], 9);
    assert_eq!(parsed["atomic_tools"]["mapped_count"], 3);
    assert_eq!(parsed["atomic_tools"]["interface_only_count"], 6);
    assert_eq!(
        parsed["atomic_tools"]["mapped_atomic_tool_names"],
        serde_json::json!(["file_read", "file_write", "code_execute"])
    );
    assert_eq!(
        parsed["atomic_tools"]["interface_only_atomic_tool_names"],
        serde_json::json!([
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "wait",
            "human_suspend"
        ])
    );
    assert_eq!(parsed["atomic_tools"]["manifest_schema_version"], 1);
    assert_eq!(
        parsed["atomic_tools"]["manifest_schema_fields"],
        serde_json::json!([
            "kind",
            "name",
            "source",
            "status",
            "implementation",
            "description"
        ])
    );
    assert_eq!(parsed["atomic_tools"]["tool_action_schema_version"], 1);
    assert!(parsed["atomic_tools"]["tool_action_call_schema_fields"]
        .as_array()
        .expect("tool action call schema fields")
        .iter()
        .any(|field| field == "tool"));
    assert_eq!(parsed["atomic_tools"]["tool_report_schema_version"], 6);
    assert!(parsed["atomic_tools"]["tool_call_schema_fields"]
        .as_array()
        .expect("tool call schema fields")
        .iter()
        .any(|field| field == "atomic_tool_name"));
    assert_eq!(parsed["atomic_tools"]["manifests"][0]["name"], "mouse");
    assert_eq!(parsed["governance"]["ok"], true);
    assert_eq!(parsed["governance"]["kind"], "static_rule");
    assert_eq!(parsed["governance"]["rules_loaded"], true);
    assert!(
        parsed["governance"]["rule_count"]
            .as_u64()
            .expect("rule count should be numeric")
            > 0
    );
    assert_eq!(parsed["governance"]["tool_surface_governed"], true);
    assert_eq!(parsed["governance"]["read_only_decision"], "allowed");
    assert_eq!(
        parsed["governance"]["dangerous_write_decision"],
        "needs_approval"
    );
    assert_eq!(
        parsed["governance"]["dangerous_shell_decision"],
        "needs_approval"
    );
    assert_eq!(parsed["governance"]["secret_shell_decision"], "draft_only");
    assert_eq!(parsed["governance"]["goal_run_executes"], false);
    assert_eq!(parsed["goal_run"]["ok"], true);
    assert_eq!(parsed["goal_run"]["goal_id"], "mainline-mvp");
    assert!(parsed["goal_run"]["plan_exists"].is_boolean());
    assert!(parsed["goal_run"]["checkpoint_count"].is_number());
    assert!(parsed["goal_run"]["checkpoint_log_complete"].is_boolean());
    assert!(parsed["goal_run"]["last_checkpoint_summary"].is_string());
    assert!(parsed["goal_run"]["incomplete_reasons"]
        .as_array()
        .expect("goal run incomplete reasons should be an array")
        .iter()
        .all(|reason| reason.is_string()));
    assert_eq!(parsed["project_readiness"]["ok"], true);
    assert_eq!(
        parsed["project_readiness"]["overall_state"],
        "mvp_ready_with_partial_modules"
    );
    assert_eq!(parsed["release_readiness"]["ok"], true);
    assert_eq!(
        parsed["release_readiness"]["release_name"],
        "second_test_version"
    );
    assert_eq!(
        parsed["release_readiness"]["overall_state"],
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        parsed["release_readiness"]["readiness_scope"],
        "readiness_and_smoke_acceptance_only_no_live_external_service_connection"
    );
    assert_eq!(parsed["release_readiness"]["acceptance_count"], 7);
    assert_eq!(
        parsed["release_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["release_readiness"]["verifies_real_external_services"],
        false
    );
    assert_eq!(
        parsed["release_readiness"]["uses_stub_or_local_fixtures"],
        true
    );
    assert_eq!(parsed["release_readiness"]["writes_repo_files"], false);
    assert!(parsed["release_readiness"]["acceptance"]
        .as_array()
        .expect("release acceptance should be an array")
        .iter()
        .any(|item| item["name"] == "real_external_services"
            && item["state"] == "deferred"
            && item["connects_real_service"] == false));
    assert!(parsed["release_readiness"]["acceptance"]
        .as_array()
        .expect("release acceptance should be an array")
        .iter()
        .any(|item| item["name"] == "channel_preflight_only"
            && item["state"] == "partial"
            && item["read_only"] == true
            && item["connects_real_service"] == false));
    assert!(
        parsed["project_readiness"]["ready_count"]
            .as_u64()
            .expect("ready count should be numeric")
            >= 7
    );
    assert!(parsed["project_readiness"]["modules"]
        .as_array()
        .expect("modules should be an array")
        .iter()
        .any(|module| module["name"] == "main_chain" && module["state"] == "ready"));
    assert!(parsed["project_readiness"]["modules"]
        .as_array()
        .expect("modules should be an array")
        .iter()
        .any(|module| module["name"] == "external_ai" && module["state"] == "ready"));
    assert_eq!(parsed["memory_readiness"]["ok"], true);
    assert_eq!(parsed["memory_readiness"]["overall_state"], "ready");
    assert_eq!(parsed["memory_readiness"]["layer_count"], 5);
    assert!(parsed["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "lim_long_term" && layer["state"] == "ready"));
    assert!(parsed["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "external_knowledge"
            && layer["state"] == "ready"
            && layer["storage"] == "docs/external-knowledge-adapter.md"));
    assert!(parsed["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "maintenance_loop"
            && layer["state"] == "ready"
            && layer["storage"] == "docs/memory-maintenance-loop.md"));
    assert_eq!(parsed["channel_readiness"]["ok"], true);
    assert_eq!(parsed["channel_readiness"]["overall_state"], "ready");
    assert_eq!(parsed["channel_readiness"]["layer_count"], 5);
    assert!(parsed["channel_readiness"]["layers"]
        .as_array()
        .expect("channel layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "rich_messages" && layer["state"] == "ready"));
    assert_eq!(parsed["subagent_readiness"]["ok"], true);
    assert_eq!(
        parsed["subagent_readiness"]["overall_state"],
        "queued_protocol_partial"
    );
    assert_eq!(parsed["subagent_readiness"]["local_contract_ready"], true);
    assert_eq!(parsed["subagent_readiness"]["live_adapter_ready"], false);
    assert_eq!(parsed["subagent_readiness"]["layer_count"], 5);
    assert!(parsed["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "external_ai_downstream"
            && layer["state"] == "ready"
            && layer["local_contract_ready"] == true
            && layer["live_adapter_ready"] == false));
    assert_eq!(parsed["external_ai_readiness"]["ok"], true);
    assert_eq!(parsed["external_ai_readiness"]["overall_state"], "ready");
    assert_eq!(parsed["external_ai_readiness"]["layer_count"], 5);
    assert!(parsed["external_ai_readiness"]["layers"]
        .as_array()
        .expect("external ai layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "genesis_actuator" && layer["state"] == "ready"));
    assert!(parsed["external_ai_readiness"]["layers"]
        .as_array()
        .expect("external ai layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "unified_identity_engine" && layer["state"] == "ready"));
    assert!(!stdout.contains("test-secret-key"));
}

#[test]
fn cli_status_reports_missing_provider_env_in_diagnostic_mode() {
    let root = temp_identity_root("status-missing-env");
    let env_name = "CHUANG_AGENT_STATUS_MISSING_KEY";
    std::env::remove_var(env_name);
    let config_path = write_openai_status_config(&root, env_name);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["config"]["provider_kind"], "openai_compatible");
    assert_eq!(
        parsed["config"]["api_key_state"],
        format!("<missing:{env_name}>")
    );
    assert!(parsed["config"]["placeholder_warnings"]
        .as_array()
        .expect("placeholder warnings array")
        .iter()
        .any(|warning| warning.as_str().unwrap_or_default().contains(env_name)));
}

#[test]
fn cli_status_can_use_custom_identity_memory_root() {
    let root = temp_identity_root("identity-root");
    let config_root = temp_identity_root("identity-root-config");
    let config_path = write_fake_status_config(&config_root);
    fs::create_dir_all(&root).expect("root should be created");
    fs::write(root.join("USER.md"), "老爸偏好简洁中文汇报").expect("user memory should be seeded");
    fs::write(root.join("MEMORY.md"), "## mem-1\n创项目聚焦核心 MVP")
        .expect("hot memory should be seeded");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
            "--identity-memory-root",
            root.to_str().expect("temp path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(
        parsed["config"]["identity_memory_root"],
        root.display().to_string()
    );
    assert_eq!(
        parsed["config"]["identity_experiences_path"],
        root.join("experiences.md").display().to_string()
    );
    assert_eq!(
        parsed["kernel"]["identity_user_chars"].as_u64(),
        Some("老爸偏好简洁中文汇报".chars().count() as u64)
    );
    assert_eq!(
        parsed["kernel"]["identity_memory_chars"].as_u64(),
        Some("## mem-1\n创项目聚焦核心 MVP".chars().count() as u64)
    );
}

#[test]
fn cli_status_can_select_queued_external_subagent_slot() {
    let queue_root = temp_identity_root("subagent-queue");
    let config_root = temp_identity_root("subagent-queue-config");
    let config_path = write_fake_status_config(&config_root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
            "--subagent",
            "queued_external",
            "--subagent-queue-root",
            queue_root.to_str().expect("temp path should be utf8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["config"]["subagent_kind"], "queued_external");
    assert_eq!(
        parsed["config"]["subagent_queue_root"],
        queue_root.display().to_string()
    );
    assert_eq!(parsed["slots"]["subagent"], "queued_external");
}

#[test]
fn cli_status_exposes_provider_request_timeout_from_cli_override() {
    let config_root = temp_identity_root("provider-timeout-config");
    let config_path = write_fake_status_config(&config_root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-secret-key",
            "--provider-model",
            "gpt-5.5",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "native",
            "--provider-request-timeout-ms",
            "12345",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["config"]["provider_request_timeout_ms"], 12_345);
    assert!(!stdout.contains("test-secret-key"));
}

#[test]
fn cli_status_prints_provider_request_timeout_in_text() {
    let config_root = temp_identity_root("provider-timeout-text");
    let config_path = write_fake_status_config(&config_root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "test-secret-key",
            "--provider-model",
            "gpt-5.5",
            "--provider-id",
            "custom-openai",
            "--provider-transport",
            "native",
            "--provider-request-timeout-ms",
            "12345",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("provider_request_timeout_ms: 12345"));
    assert!(!stdout.contains("test-secret-key"));
}

#[test]
fn cli_status_can_override_context_budget_fields() {
    let config_root = temp_identity_root("context-budget-config");
    let config_path = write_fake_status_config(&config_root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
            "--context-max-tokens",
            "256",
            "--context-reserve-system-tokens",
            "64",
            "--context-min-working-tokens",
            "8",
            "--context-max-tool-results",
            "2",
            "--context-max-memory-segments",
            "3",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["config"]["context_max_tokens"], 256);
    assert_eq!(
        parsed["config"]["context_engine_kind"],
        "deterministic_budget"
    );
    assert_eq!(parsed["config"]["context_reserve_system_tokens"], 64);
    assert_eq!(parsed["config"]["context_min_working_tokens"], 8);
    assert_eq!(parsed["config"]["context_max_tool_results"], 2);
    assert_eq!(parsed["config"]["context_max_memory_segments"], 3);
    assert_eq!(parsed["kernel"]["context_budget_max_tokens"], 256);
}

#[test]
fn cli_status_can_select_summary_compression_context_engine() {
    let config_root = temp_identity_root("context-engine-config");
    let config_path = write_fake_status_config(&config_root);
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--context-engine",
            "summary_compression",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(
        parsed["config"]["context_engine_kind"],
        "summary_compression"
    );
}

#[test]
fn cli_status_exposes_command_control_timeout_from_config() {
    let root = temp_identity_root("control-timeout-status");
    let config_path = root.join("config.toml");
    fs::create_dir_all(&root).expect("config root should be created");
    fs::write(
        &config_path,
        r#"
control = "command"
program = "printf"
list_args = "[]"
apply_args = "{}"
control_timeout_ms = 4321
"#,
    )
    .expect("config should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");

    assert_eq!(parsed["config"]["control_plane_kind"], "command");
    assert_eq!(parsed["config"]["control_command_timeout_ms"], 4321);
}

#[test]
fn cli_status_can_load_simple_config_file_and_accept_cli_overrides() {
    let root = temp_identity_root("config");
    let identity_root = root.join("identity");
    let queue_root = root.join("queue");
    let config_path = root.join("config.toml");
    fs::create_dir_all(&identity_root).expect("identity root should be created");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{db}"
recall_limit = 9
identity_memory_root = "{identity}"
subagent = "queued_external"
subagent_queue_root = "{queue}"

[provider]
kind = "fake"
id = "config-fake"
model = "config-stub"

[context]
max_tokens = 384
reserve_system_tokens = 48
min_working_tokens = 3
max_tool_results = 4
max_memory_segments = 6
"#,
            db = root.join("chuang.db").display(),
            identity = identity_root.display(),
            queue = queue_root.display()
        ),
    )
    .expect("config should write");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--context-max-tokens",
            "256",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["config"]["provider_id"], "config-fake");
    assert_eq!(parsed["config"]["model_name"], "config-stub");
    assert_eq!(parsed["config"]["recall_limit"], 9);
    assert_eq!(
        parsed["config"]["subagent_queue_root"],
        queue_root.display().to_string()
    );
    assert_eq!(parsed["config"]["subagent_kind"], "queued_external");
    assert_eq!(parsed["config"]["context_max_tokens"], 256);
    assert_eq!(parsed["config"]["context_reserve_system_tokens"], 48);
}
