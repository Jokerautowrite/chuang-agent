use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo_command() -> Command {
    let mut command = Command::new("cargo");
    command.env("CODEX_PPTOKEN_API_KEY", "test-key");
    command
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-{name}-{nanos}"))
}

fn write_fake_status_config(root: &Path) -> PathBuf {
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

#[test]
fn live_runner_readiness_view_rejects_missing_allow_runner_command() {
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim(),
        "subagent live-preflight requires --allow-runner-command"
    );
}

#[test]
fn live_runner_readiness_view_outputs_readonly_preflight_json_without_secrets() {
    let output = cargo_command()
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "rehearsal",
            "--capability",
            "rehearsal",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("test-key"));

    let parsed: Value = serde_json::from_str(&stdout).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    let mut rehearsal_keys = rehearsal
        .as_object()
        .expect("rehearsal should be an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    rehearsal_keys.sort();
    let mut expected_rehearsal_keys = vec![
        "adapter_entrypoint".to_string(),
        "approval_audit_prerequisites".to_string(),
        "approval_audit_prerequisites_ok".to_string(),
        "capability_routing".to_string(),
        "capability_routing_ok".to_string(),
        "forbidden_capabilities".to_string(),
        "forbidden_capabilities_ok".to_string(),
        "gate".to_string(),
        "gate_enabled".to_string(),
        "live_worker_available".to_string(),
        "next_action".to_string(),
        "ok".to_string(),
        "readonly".to_string(),
        "ready_for_live".to_string(),
        "report_admission".to_string(),
        "report_admission_ok".to_string(),
        "runner_allowlist".to_string(),
        "runner_allowlist_ok".to_string(),
        "starts_external_worker".to_string(),
        "worker_runtime_reason".to_string(),
        "worker_runtime_state".to_string(),
    ];
    expected_rehearsal_keys.sort();
    assert_eq!(rehearsal_keys, expected_rehearsal_keys);
    assert_eq!(rehearsal["ok"], true);
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["readonly"], true);
    assert_eq!(rehearsal["starts_external_worker"], false);
    assert_eq!(rehearsal["live_worker_available"], false);
    assert_eq!(
        rehearsal["worker_runtime_state"],
        "configured_but_gate_disabled"
    );
    assert_eq!(
        rehearsal["worker_runtime_reason"],
        "runner command and capability route are configured, but CHUANG_CODEX_RUNNER_ENABLE is not enabled; live_worker_available remains false"
    );
    assert_eq!(rehearsal["gate_enabled"], false);
    assert_eq!(rehearsal["runner_allowlist_ok"], true);
    assert_eq!(rehearsal["capability_routing_ok"], true);
    assert_eq!(rehearsal["report_admission_ok"], true);
    assert_eq!(rehearsal["forbidden_capabilities_ok"], true);
    assert_eq!(rehearsal["approval_audit_prerequisites_ok"], true);
    assert_eq!(rehearsal["next_action"], "keep rehearsal read-only; set CHUANG_CODEX_RUNNER_ENABLE=1 only after operator approval of exact runner command, capabilities, and report admission evidence");
    assert_eq!(rehearsal["gate"]["enabled"], false);
    assert_eq!(
        rehearsal["gate"]["required_env"],
        "CHUANG_CODEX_RUNNER_ENABLE"
    );
    assert_eq!(rehearsal["gate"]["audit_label"], "subagent.runner.live");
    assert_eq!(rehearsal["runner_allowlist"]["ok"], true);
    assert_eq!(
        rehearsal["runner_allowlist"]["matched_runner_command"],
        "scripts/chuang-codex-runner.py"
    );
    assert_eq!(rehearsal["runner_allowlist"]["exact_match_required"], true);
    assert_eq!(rehearsal["capability_routing"]["ok"], true);
    assert_eq!(
        rehearsal["capability_routing"]["matched_capabilities"],
        serde_json::json!(["rehearsal"])
    );
    assert_eq!(rehearsal["report_admission"]["ok"], true);
    assert_eq!(
        rehearsal["report_admission"]["covered_commands"],
        serde_json::json!(["run-once", "run-loop", "report", "collect"])
    );
    assert_eq!(rehearsal["forbidden_capabilities"]["ok"], true);
    assert_eq!(rehearsal["approval_audit_prerequisites"]["ok"], true);
    assert_eq!(
        rehearsal["approval_audit_prerequisites"]["audit_label"],
        "subagent.runner.live"
    );
}

#[test]
fn live_runner_readiness_view_status_json_exposes_blocked_reason_and_next_action() {
    let root = temp_root("live-runner-readiness-view");
    let config_path = write_fake_status_config(&root);
    let output = cargo_command()
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("test-key"));

    let parsed: Value = serde_json::from_str(&stdout).expect("stdout json");
    let subagent = &parsed["subagent_readiness"];
    let mut subagent_keys = subagent
        .as_object()
        .expect("subagent_readiness should be an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    subagent_keys.sort();
    let mut expected_subagent_keys = vec![
        "blocked_count".to_string(),
        "capability_mismatch_blocks_live".to_string(),
        "capability_mismatch_reason".to_string(),
        "capability_route_state".to_string(),
        "deferred_count".to_string(),
        "layer_count".to_string(),
        "layers".to_string(),
        "live_adapter_ready".to_string(),
        "live_adapter_reason".to_string(),
        "live_adapter_state".to_string(),
        "live_worker_available".to_string(),
        "local_contract_ready".to_string(),
        "local_contract_reason".to_string(),
        "local_contract_state".to_string(),
        "mode".to_string(),
        "model_tool_worker_available".to_string(),
        "model_tool_worker_reason".to_string(),
        "model_tool_worker_state".to_string(),
        "ok".to_string(),
        "overall_state".to_string(),
        "partial_count".to_string(),
        "ready_count".to_string(),
        "worker_runtime_blocked_reason".to_string(),
        "worker_runtime_reason".to_string(),
        "worker_runtime_state".to_string(),
    ];
    expected_subagent_keys.sort();
    assert_eq!(subagent_keys, expected_subagent_keys);
    assert_eq!(subagent["ok"], true);
    assert_eq!(subagent["mode"], "fake");
    assert_eq!(subagent["live_worker_available"], false);
    assert_eq!(subagent["worker_runtime_state"], "local_contract_only");
    assert_eq!(
        subagent["worker_runtime_blocked_reason"],
        "live_worker_unavailable: subagent slot is fake; local contracts are visible but no live worker can run"
    );
    assert_eq!(subagent["capability_mismatch_blocks_live"], true);
    assert_eq!(
        subagent["capability_mismatch_reason"],
        "live subagent preflight must reject missing or mismatched dispatch required_capabilities before any worker starts"
    );

    let layers = subagent["layers"]
        .as_array()
        .expect("subagent layers should be an array");
    let live_runner = layers
        .iter()
        .find(|layer| layer["name"] == "live_runner_rehearsal")
        .expect("live_runner_rehearsal layer should exist");
    assert_eq!(live_runner["state"], "ready");
    assert_eq!(live_runner["live_worker_available"], false);
    assert_eq!(live_runner["worker_runtime_state"], "local_contract_only");
    assert_eq!(live_runner["blocked_reason"], "live_runner_rehearsal is read-only; missing or mismatched dispatch required_capabilities keep ready_for_live=false");
    assert_eq!(
        live_runner["capability_route_state"],
        "requires_dispatch_required_capabilities"
    );
    assert_eq!(live_runner["capability_mismatch_blocks_live"], true);
    assert_eq!(
        live_runner["capability_mismatch_reason"],
        "capability mismatch or missing dispatch required_capabilities must block live runner readiness"
    );
    assert_eq!(live_runner["local_contract_ready"], true);
    assert_eq!(live_runner["local_contract_state"], "ready");
    assert_eq!(
        live_runner["local_contract_reason"],
        "live_runner_rehearsal local contract is protocol-ready"
    );
    assert_eq!(live_runner["live_adapter_ready"], false);
    assert_eq!(live_runner["live_adapter_state"], "deferred");
    assert_eq!(
        live_runner["live_adapter_reason"],
        "read-only live runner rehearsal is ready; real worker execution remains gated and deferred"
    );
    assert_eq!(
        live_runner["current"],
        "subagent live-preflight rehearses live runner gate, command allowlist, capability routing, ReportAdmission, forbidden capabilities, and audit prerequisites without starting a worker"
    );
    assert_eq!(
        live_runner["next_action"],
        "run one approved live runner rehearsal only after operator enables CHUANG_CODEX_RUNNER_ENABLE=1 for an exact allowlisted command"
    );
    assert_eq!(live_runner["boundary"], "read_only_preflight");
}

#[test]
fn live_runner_readiness_view_script_outputs_aggregated_json_view() {
    let empty_headless = std::env::temp_dir().join(format!(
        "chuang-live-runner-no-headless-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&empty_headless);
    let output = Command::new("bash")
        .arg("scripts/chuang-live-runner-readiness-view.sh")
        .arg("--json")
        .env("CHUANG_AGENT_ROOT", manifest_dir())
        .env_remove("CHUANG_CDP_PORT")
        .env("CHUANG_HEADLESS_STATE_DIR", &empty_headless)
        .current_dir(manifest_dir())
        .output()
        .expect("script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("test-key"));

    let parsed: Value = serde_json::from_str(&stdout).expect("stdout json");
    let mut keys = parsed
        .as_object()
        .expect("aggregated view should be an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "binary_blocked_reason".to_string(),
            "binary_path".to_string(),
            "config_path".to_string(),
            "connects_hermes".to_string(),
            "connects_real_feishu".to_string(),
            "connects_real_provider".to_string(),
            "deletes_files".to_string(),
            "live_readiness".to_string(),
            "live_runner_rehearsal".to_string(),
            "modifies_repo".to_string(),
            "policy_tool_status".to_string(),
            "prints_secret_values".to_string(),
            "readonly".to_string(),
            "reads_secret_values".to_string(),
            "runtime_report_surface".to_string(),
            "schema_version".to_string(),
            "source_evidence_refs".to_string(),
            "sources".to_string(),
            "starts_external_worker".to_string(),
            "starts_worker".to_string(),
            "workspace_root".to_string(),
            "writes_core_memory".to_string(),
        ]
    );
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["readonly"], true);
    assert_eq!(parsed["binary_blocked_reason"], Value::Null);
    assert!(parsed["binary_path"]
        .as_str()
        .expect("binary path")
        .contains("target/debug/chuang-agent"));
    assert_eq!(parsed["connects_hermes"], false);
    assert_eq!(parsed["connects_real_provider"], false);
    assert_eq!(parsed["connects_real_feishu"], false);
    assert_eq!(parsed["starts_external_worker"], false);
    assert_eq!(parsed["starts_worker"], false);
    assert_eq!(parsed["reads_secret_values"], false);
    assert_eq!(parsed["prints_secret_values"], false);
    assert_eq!(parsed["modifies_repo"], false);
    assert_eq!(parsed["deletes_files"], false);
    assert_eq!(parsed["writes_core_memory"], false);
    assert_eq!(
        parsed["workspace_root"],
        manifest_dir().display().to_string()
    );
    assert_eq!(
        parsed["config_path"],
        manifest_dir().join("config.toml").display().to_string()
    );

    let policy_tool_status = &parsed["policy_tool_status"];
    assert_eq!(
        policy_tool_status["active_permission_profile"],
        "full_local_workspace"
    );
    assert_eq!(policy_tool_status["ga_tool_descriptor_mapped_count"], 9);
    assert_eq!(policy_tool_status["tool_descriptor_count"], 15);
    let tool_descriptors = policy_tool_status["ga_tool_descriptors"]
        .as_array()
        .expect("policy tool descriptors should be array");
    let file_write = tool_descriptors
        .iter()
        .find(|descriptor| descriptor["name"] == "file_write")
        .expect("file_write descriptor should be surfaced");
    assert_eq!(file_write["external_commit"], false);
    assert_eq!(file_write["requires_approval"], false);
    assert!(file_write["risk_tags"]
        .as_array()
        .expect("file_write risk tags should be array")
        .iter()
        .any(|tag| tag == "write"));

    let live_readiness = &parsed["live_readiness"];
    assert_eq!(live_readiness["ok"], true);
    assert_eq!(live_readiness["overall_state"], "local_ready_live_pending");
    assert_eq!(live_readiness["ga_local_mapped_only"], true);
    assert_eq!(live_readiness["desktop_browser_live_gated"], true);
    assert_eq!(live_readiness["browser_worker_frozen"], true);
    assert_eq!(live_readiness["live_worker_available"], false);
    assert_eq!(live_readiness["real_external_acceptance_pending"], true);
    assert_eq!(
        live_readiness["provider_live_request_verified_by_status"],
        false
    );
    assert_eq!(live_readiness["ready_does_not_mean_live"], true);

    let runtime_surface = &parsed["runtime_report_surface"];
    assert_eq!(runtime_surface["ok"], true);
    assert_eq!(runtime_surface["artifact_count"], 11);
    assert_eq!(runtime_surface["observability_field_count"], 26);
    let artifact_locators = runtime_surface["artifact_locators"]
        .as_array()
        .expect("artifact locators");
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_meta.tool_protocol_errors_json"));
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_meta.runtime_event_ledger_json"));
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_response.trace"));
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_meta.context_compaction_events"));
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_meta.goal_handoff_query_summary_json"));
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_meta.subagent_children_summary_json"));
    assert!(artifact_locators
        .iter()
        .any(|locator| locator == "runtime_meta.context_compaction_summary_json"));

    let observability_fields = runtime_surface["observability_fields"]
        .as_array()
        .expect("observability fields");
    assert!(observability_fields
        .iter()
        .any(|field| field == "runtime_event_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "runtime_event_approval_resolved_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "tool_protocol_error_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "runtime_response_trace_chars"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "tool_unified_execution_status"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "tool_unified_execution_failure_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "tool_unified_execution_failure_classes"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "goal_handoff_query_summary_json"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "goal_handoff_parent_context_handoff_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "goal_handoff_report_admission_ref_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "goal_handoff_report_admission_refs"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "goal_handoff_report_admission_reason_codes"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_summary_json"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_child_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_accepted_report_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_report_admission_ref_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_report_admission_refs"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_missing_report_count"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "subagent_children_report_reason_codes"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "context_pack_trace"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "context_compaction_events"));
    assert!(observability_fields
        .iter()
        .any(|field| field == "context_compaction_summary_json"));

    let rehearsal = &parsed["live_runner_rehearsal"];
    // overall_state is the local contract; state may be blocked when operator env
    // (e.g. CHUANG_PROXY_API_KEY) is missing on this machine.
    assert_eq!(rehearsal["overall_state"], "ready");
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["starts_external_worker"], false);
    assert_eq!(rehearsal["capability_mismatch_blocks_live"], true);
    let blocked_reason = rehearsal["blocked_reason"]
        .as_str()
        .expect("blocked reason");
    assert!(
        blocked_reason.contains("required_capabilities")
            || blocked_reason.contains("config_missing_env")
            || blocked_reason.contains("CHUANG_PROXY_API_KEY"),
        "unexpected blocked_reason={blocked_reason}"
    );
    assert!(!rehearsal["next_action"]
        .as_str()
        .expect("next action")
        .is_empty());
    assert!(rehearsal["source_evidence_refs"]["subagent_live_preflight"]
        .as_str()
        .expect("preflight ref")
        .contains("subagent live-preflight"));
    assert!(rehearsal["source_evidence_refs"]["status_json"]
        .as_str()
        .expect("status ref")
        .contains("status --config"));
    assert!(rehearsal["source_evidence_refs"]["doctor_json"]
        .as_str()
        .expect("doctor ref")
        .contains("doctor --config"));
    assert!(rehearsal["source_evidence_refs"]["app_server_health"]
        .as_str()
        .expect("app server ref")
        .contains("app-server health"));
    assert_eq!(
        rehearsal["layers"]["status"]["name"],
        "live_runner_rehearsal"
    );
    // doctor layer can be null when doctor CLI fails on this machine; app_server should still be present.
    if rehearsal["layers"]["doctor"].get("name").and_then(|v| v.as_str()).is_some() {
        assert_eq!(
            rehearsal["layers"]["doctor"]["name"],
            "live_runner_rehearsal"
        );
    }
    assert_eq!(
        rehearsal["layers"]["app_server_health"]["name"],
        "live_runner_rehearsal"
    );
    assert_eq!(
        rehearsal["subagent_readiness"]["status"]["mode"],
        "queued_external"
    );
    if rehearsal["subagent_readiness"]["doctor"]
        .get("mode")
        .and_then(|v| v.as_str())
        .is_some()
    {
        assert_eq!(
            rehearsal["subagent_readiness"]["doctor"]["mode"],
            "queued_external"
        );
    }
    assert_eq!(
        rehearsal["subagent_readiness"]["app_server_health"]["mode"],
        "queued_external"
    );
}

#[test]
fn live_runner_readiness_view_script_text_output_lists_runtime_surface_fields() {
    let output = Command::new("bash")
        .arg("scripts/chuang-live-runner-readiness-view.sh")
        .env("CHUANG_AGENT_ROOT", manifest_dir())
        .current_dir(manifest_dir())
        .output()
        .expect("script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("runtime_report_surface.ok=true"));
    assert!(stdout.contains("runtime_report_surface.artifact_count=11"));
    assert!(stdout.contains("runtime_report_surface.observability_field_count=26"));
    assert!(stdout.contains("runtime_report_surface.artifact_locators="));
    assert!(stdout.contains("runtime_report_surface.observability_fields="));
    assert!(stdout.contains("policy_tool_status.active_permission_profile=full_local_workspace"));
    assert!(stdout.contains("policy_tool_status.ga_tool_descriptors=9/15"));
    assert!(stdout.contains("policy_tool_status.missing=none"));
    assert!(stdout.contains("live_readiness.ok=true"));
    assert!(stdout.contains("live_readiness.state=local_ready_live_pending"));
    assert!(stdout.contains("live_readiness.ga_local_mapped_only=true"));
    assert!(stdout.contains("live_readiness.desktop_browser_live_gated=true"));
    assert!(stdout.contains("live_readiness.browser_worker_frozen=true"));
    assert!(stdout.contains("live_readiness.live_worker_available=false"));
    assert!(stdout.contains("live_readiness.real_external_acceptance_pending=true"));
    assert!(stdout.contains("live_readiness.provider_live_request_verified_by_status=false"));
    assert!(stdout.contains("live_readiness.ready_does_not_mean_live=true"));
    assert!(stdout.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(stdout.contains("tool_protocol_error_count"));
    assert!(stdout.contains("tool_unified_execution_status"));
    assert!(stdout.contains("tool_unified_execution_failure_count"));
    assert!(stdout.contains("tool_unified_execution_failure_classes"));
    assert!(stdout.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(stdout.contains("runtime_meta.subagent_children_summary_json"));
    assert!(stdout.contains("runtime_meta.context_compaction_summary_json"));
    assert!(stdout.contains("goal_handoff_report_admission_reason_codes"));
    assert!(stdout.contains("goal_handoff_report_admission_refs"));
    assert!(stdout.contains("subagent_children_report_admission_ref_count"));
    assert!(stdout.contains("subagent_children_report_admission_refs"));
    assert!(stdout.contains("subagent_children_report_reason_codes"));
    assert!(stdout.contains("context_compaction_summary_json"));
}
