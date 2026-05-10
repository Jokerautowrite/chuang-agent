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
    assert!(stdout.contains(
        "provider_readiness: ok=true state=ready kind=fake transport=fake fallback_configured=false timeout_ms=none api_key_state=none"
    ));
    assert!(stdout.contains("identity_memory: hermes_dual_file"));
    assert!(stdout.contains("identity_experiences_path: "));
    assert!(stdout.contains("identity_memory_limits: user=1375 memory=2200"));
    assert!(stdout.contains("identity_root: "));
    assert!(stdout.contains("rules_root: ./rules"));
    assert!(stdout.contains(
        "atomic_tools: source=GenericAgent ok=true total=9 mapped=9 interface_only=0 manifest_schema_version=1 action_schema_version=1 report_schema_version=6"
    ));
    assert!(stdout.contains("runtime_capability_primer: 普通对话默认注入同一份能力 primer"));
    assert!(stdout.contains("file_read/file_write/code_execute/list_dir=受治理工作区读写/执行"));
    assert!(stdout.contains("goal/subagent 派活"));
    assert!(stdout.contains("locate/screenshot=只读观察"));
    assert!(stdout.contains("桌面/浏览器真实动作仍需治理/live gate/allowlist/receipt"));
    assert!(stdout.contains("atomic_tools_mapped: mouse,keyboard,screenshot,locate,file_read,file_write,code_execute,wait,human_suspend"));
    assert!(stdout.contains("atomic_tools_executable: mouse,keyboard,screenshot,locate,file_read,file_write,code_execute,wait,human_suspend"));
    assert!(stdout.contains("atomic_tools_interface_only: none"));
    assert!(stdout.contains(
        "atomic_tools_desktop_browser_interface_only: none reason=all GA atoms are mapped to governed runtime ports; real desktop/browser execution still requires an audited actuator adapter, live gate, allowlist, and receipt"
    ));
    assert!(stdout.contains(
        "atomic_tools_desktop_browser_live_gated: mouse,keyboard,screenshot,locate required=adapter,live_gate,allowlist,audit_receipt"
    ));
    assert!(stdout.contains(
        "atomic_tools_self_check_entrypoints: status --json,doctor --json,app-server health --diagnostic --json"
    ));
    assert!(stdout.contains("atomic_tool name=file_read status=mapped"));
    assert!(stdout.contains("atomic_tool name=mouse status=mapped"));
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
        "context_budget: max=272000 reserve_system=32 min_working=1 max_tool_results=5 max_memory_segments=5"
    ));
    assert!(stdout.contains("control_plane: fake_local"));
    assert!(stdout.contains(
        "goal_mode: ok=true kind=lightweight_runtime_context cli_entrypoint=run --goal TEXT context_source=goal default_goal_id=mainline-mvp allowed_slots=context,governance,execution,report,memory checkpoint_policy=progress_log:true handoff:true commit:true final_report_policy=validation:true next_steps:true bypasses_governance=false adds_core_slot=false"
    ));
    assert!(stdout.contains("goal_run: ok=true"));
    assert!(stdout.contains("goal_id=mainline-mvp"));
    assert!(stdout.contains("checkpoints="));
    assert!(stdout.contains("plugin_registry: available=true ok=true"));
    assert!(stdout.contains(
        "local_contract_readiness: ok=true state=ready contracts=6 ready=6 partial=0 deferred=0 blocked=0 connects_real_external_services=false writes_core_memory=false executes_plugins=false"
    ));
    assert!(stdout.contains(
        "local_contract name=knowledge_context_preview state=ready boundary=local_markdown_text_preview_only read_only=true dry_run=false connects_real_service=false writes_core_memory=false writes_repo_files=false executes_plugins=false"
    ));
    assert!(stdout.contains(
        "local_contract name=skill_proposal_review state=ready boundary=self_scored_review_and_dedup read_only=false dry_run=false connects_real_service=false writes_core_memory=false writes_repo_files=false executes_plugins=false"
    ));
    assert!(stdout.contains(
        "local_contract name=skill_lifecycle_write_retire state=ready boundary=self_maintained_upsert_and_retire read_only=false dry_run=false connects_real_service=false writes_core_memory=false writes_repo_files=true executes_plugins=false"
    ));
    assert!(stdout.contains(
        "local_contract name=plugin_registry_evidence state=ready boundary=manifest_check_only"
    ));
    assert!(stdout.contains(
        "local_contract name=external_knowledge_source_contracts state=ready boundary=adapter_contract_only"
    ));
    assert!(stdout.contains(
        "local_contract_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
    ));
    assert!(stdout.contains(
        "local_contract name=goal_mode_smoke_gate state=ready boundary=local_cli_smoke_only"
    ));
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
    assert!(stdout.contains(
        "third_test_candidate: ok=true state=local_gate_ready_requires_manual_live_check local_gate_ready=true smoke_script=scripts/chuang-third-test-smoke.sh marker=third_test_candidate_smoke_ok requires_manual_live_check=true connects_real_external_services=false operator_env_blocks_100_percent=true real_live_ready=false"
    ));
    assert!(stdout.contains("memory_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("memory_layer name=internal_identity state=ready"));
    assert!(stdout.contains("memory_layer name=external_knowledge state=ready storage=docs/external-knowledge-adapter.md"));
    assert!(stdout.contains(
        "memory_layer_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
    ));
    assert!(stdout.contains(
        "memory_layer name=maintenance_loop state=ready storage=docs/memory-maintenance-loop.md"
    ));
    assert!(stdout.contains(
        "memory_maintenance_receipt: available=true readable=true state=missing receipts=0 latest_entry_id=none latest_source_record_id=none latest_approval_source=none latest_approved_at=none latest_provenance_preserved=false"
    ));
    assert!(stdout.contains("channel_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("channel_layer name=app_server state=ready"));
    assert!(stdout.contains("channel_layer name=dedicated_feishu_bridge state=ready"));
    assert!(stdout.contains(
        "subagent_readiness: ok=true state=queued_protocol_partial mode=fake local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=partial"
    ));
    assert!(stdout.contains(
        "capability_mismatch_reason=capability mismatch or missing dispatch required_capabilities must block live runner readiness"
    ));
    assert!(stdout.contains("live_worker_available=false worker_runtime_state=local_contract_only"));
    assert!(stdout
        .contains("worker_runtime_blocked_reason=live_worker_unavailable: subagent slot is fake"));
    assert!(stdout.contains("capability_route_state=requires_dispatch_required_capabilities"));
    assert!(stdout.contains("capability_mismatch_blocks_live=true"));
    assert!(stdout.contains(
        "capability_mismatch_reason=capability mismatch or missing dispatch required_capabilities must block live runner readiness"
    ));
    assert!(stdout.contains("subagent_worker_runtime_reason: subagent slot is fake"));
    assert!(stdout.contains(
        "subagent_capability_mismatch_reason: live subagent preflight must reject missing or mismatched dispatch required_capabilities"
    ));
    assert!(stdout.contains("subagent_readiness_local_contract_reason:"));
    assert!(stdout.contains("subagent_readiness_live_adapter_reason:"));
    assert!(stdout.contains(
        "subagent_layer name=command_runner state=ready local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=deferred"
    ));
    assert!(stdout.contains("live_adapter_reason=local command-runner contract is ready"));
    assert!(stdout.contains(
        "subagent_layer name=live_runner_rehearsal state=ready local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=deferred live_worker_available=false worker_runtime_state=local_contract_only blocked_reason=live_runner_rehearsal is read-only; missing or mismatched dispatch required_capabilities keep ready_for_live=false capability_route_state=requires_dispatch_required_capabilities capability_mismatch_blocks_live=true boundary=read_only_preflight"
    ));
    assert!(stdout.contains(
        "capability_mismatch_reason=capability mismatch or missing dispatch required_capabilities must block live runner readiness"
    ));
    assert!(stdout.contains("live_adapter_reason=read-only live runner rehearsal is ready"));
    assert!(stdout.contains("subagent_layer name=multi_worker state=ready"));
    assert!(stdout.contains(
        "live_adapter_gates: ok=true state=disabled_by_default gates=3 enabled=0 disabled=3"
    ));
    assert!(stdout.contains(
        "live_adapter_gate name=control_apply state=disabled enabled=false default_enabled=false env_value_state=unset required_env=CHUANG_REAL_CONTROL_ENABLE audit_label=control.apply.live"
    ));
    assert!(stdout.contains("preflight=confirm CHUANG_REAL_CONTROL_ENABLE=1"));
    assert!(stdout.contains("must_reject=arbitrary systemd unit or process control"));
    assert!(stdout.contains("next=keep disabled until the operator approves exact live adapter targets and preflight evidence"));
    assert!(stdout.contains("external_ai_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains("external_ai_layer name=genesis_actuator state=ready"));
    assert!(stdout.contains("external_ai_layer name=dispatch_sop state=ready"));
    assert!(stdout.contains("goal_run_checkpoint_log_complete:"));
    assert!(stdout.contains("goal_run_last_checkpoint_summary:"));
    assert!(stdout.contains("goal_run_last_checkpoint_created_at:"));
    assert!(stdout.contains("goal_run_last_checkpoint_completed_worker_ids:"));
    assert!(stdout.contains("goal_run_last_checkpoint_validation_notes:"));
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
    assert_eq!(parsed["config"]["subagent_live_worker"]["enabled"], false);
    assert_eq!(
        parsed["config"]["subagent_live_worker"]["adapter_kind"],
        "none"
    );
    assert_eq!(
        parsed["config"]["subagent_live_worker"]["status"],
        "disabled"
    );
    assert_eq!(
        parsed["config"]["subagent_live_worker"]["starts_worker"],
        false
    );
    assert_eq!(parsed["config"]["subagent_live_worker"]["available"], false);
    assert!(parsed["config"]["subagent_live_worker"]["reason"]
        .as_str()
        .expect("subagent live worker reason")
        .contains("no live worker is started"));
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
    assert_eq!(parsed["plugin_registry"]["evidence_available"], true);
    assert_eq!(parsed["plugin_registry"]["check_only"], true);
    assert_eq!(parsed["plugin_registry"]["executes_plugins"], false);
    assert_eq!(parsed["plugin_registry"]["reads_secret"], false);
    assert!(
        parsed["plugin_registry"]["capability_count"]
            .as_u64()
            .expect("capability count")
            >= 5
    );
    assert_eq!(parsed["local_contract_readiness"]["ok"], true);
    assert_eq!(parsed["local_contract_readiness"]["overall_state"], "ready");
    assert_eq!(parsed["local_contract_readiness"]["contract_count"], 6);
    assert_eq!(parsed["local_contract_readiness"]["ready_count"], 6);
    assert_eq!(
        parsed["local_contract_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["local_contract_readiness"]["writes_core_memory"],
        false
    );
    assert_eq!(
        parsed["local_contract_readiness"]["executes_plugins"],
        false
    );
    assert!(parsed["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts should be an array")
        .iter()
        .any(|contract| contract["name"] == "knowledge_context_preview"
            && contract["read_only"] == true
            && contract["connects_real_service"] == false));
    assert!(parsed["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts should be an array")
        .iter()
        .any(|contract| contract["name"] == "skill_proposal_review"
            && contract["evidence"]
                .as_str()
                .expect("skill proposal review evidence")
                .contains("writable lifecycle")
            && contract["dry_run"] == false
            && contract["writes_core_memory"] == false));
    assert!(parsed["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts should be an array")
        .iter()
        .any(
            |contract| contract["name"] == "skill_lifecycle_write_retire"
                && contract["evidence"]
                    .as_str()
                    .expect("skill lifecycle write/retire evidence")
                    .contains("skill lifecycle write/retire")
                && contract["dry_run"] == false
                && contract["writes_repo_files"] == true
                && contract["boundary"] == "self_maintained_upsert_and_retire"
        ));
    assert!(parsed["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts should be an array")
        .iter()
        .any(|contract| contract["name"] == "plugin_registry_evidence"
            && contract["executes_plugins"] == false));
    assert!(parsed["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts should be an array")
        .iter()
        .any(
            |contract| contract["name"] == "external_knowledge_source_contracts"
                && contract["boundary"] == "adapter_contract_only"
        ));
    assert!(parsed["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts should be an array")
        .iter()
        .any(|contract| contract["name"] == "goal_mode_smoke_gate"
            && contract["boundary"] == "local_cli_smoke_only"
            && contract["read_only"] == true
            && contract["connects_real_service"] == false));
    assert_eq!(parsed["atomic_tools"]["source"], "GenericAgent");
    let runtime_capability_primer = parsed["runtime_capability_primer"]
        .as_str()
        .expect("runtime capability primer should be text");
    assert!(runtime_capability_primer
        .contains("file_read/file_write/code_execute/list_dir=受治理工作区读写/执行"));
    assert!(runtime_capability_primer.contains("普通对话默认注入同一份能力 primer"));
    assert!(runtime_capability_primer.contains("memory/session=回溯补充"));
    assert!(runtime_capability_primer.contains("goal/subagent 派活"));
    assert!(runtime_capability_primer.contains("locate/screenshot=只读观察"));
    assert!(runtime_capability_primer
        .contains("桌面/浏览器真实动作仍需治理/live gate/allowlist/receipt"));
    assert_eq!(parsed["atomic_tools"]["ok"], true);
    assert_eq!(parsed["atomic_tools"]["total_count"], 9);
    assert_eq!(parsed["atomic_tools"]["mapped_count"], 9);
    assert_eq!(parsed["atomic_tools"]["interface_only_count"], 0);
    assert_eq!(
        parsed["atomic_tools"]["mapped_atomic_tool_names"],
        serde_json::json!([
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "file_read",
            "file_write",
            "code_execute",
            "wait",
            "human_suspend"
        ])
    );
    assert_eq!(
        parsed["atomic_tools"]["governed_executable_atomic_tool_names"],
        serde_json::json!([
            "mouse",
            "keyboard",
            "screenshot",
            "locate",
            "file_read",
            "file_write",
            "code_execute",
            "wait",
            "human_suspend"
        ])
    );
    assert_eq!(
        parsed["atomic_tools"]["interface_only_atomic_tool_names"],
        serde_json::json!([])
    );
    assert_eq!(
        parsed["atomic_tools"]["desktop_browser_interface_only_atomic_tool_names"],
        serde_json::json!([])
    );
    assert_eq!(
        parsed["atomic_tools"]["desktop_browser_live_gated_atomic_tool_names"],
        serde_json::json!(["mouse", "keyboard", "screenshot", "locate"])
    );
    assert!(parsed["atomic_tools"]["interface_only_reason"]
        .as_str()
        .expect("interface only reason")
        .contains("all GA atoms are mapped to governed runtime ports"));
    assert_eq!(
        parsed["atomic_tools"]["local_cli_self_check_entrypoints"],
        serde_json::json!([
            "status --json",
            "doctor --json",
            "app-server health --diagnostic --json"
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
    assert!(parsed["atomic_tools"]["tool_action_call_schema_fields"]
        .as_array()
        .expect("tool action call schema fields")
        .iter()
        .any(|field| field == "reason"));
    assert!(parsed["atomic_tools"]["tool_action_call_schema_fields"]
        .as_array()
        .expect("tool action call schema fields")
        .iter()
        .any(|field| field == "prompt"));
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
    assert_eq!(parsed["goal_mode"]["ok"], true);
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["update_progress_log"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["update_handoff"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["checkpoint_policy"]["commit_checkpoint"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["final_report_policy"]["include_validation"],
        true
    );
    assert_eq!(
        parsed["goal_mode"]["final_report_policy"]["include_next_steps"],
        true
    );
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
    assert_eq!(parsed["third_test_candidate"]["ok"], true);
    assert_eq!(
        parsed["third_test_candidate"]["overall_state"],
        "local_gate_ready_requires_manual_live_check"
    );
    assert_eq!(parsed["third_test_candidate"]["local_gate_ready"], true);
    assert_eq!(
        parsed["third_test_candidate"]["smoke_script"],
        "scripts/chuang-third-test-smoke.sh"
    );
    assert_eq!(
        parsed["third_test_candidate"]["marker"],
        "third_test_candidate_smoke_ok"
    );
    assert_eq!(
        parsed["third_test_candidate"]["requires_manual_live_check"],
        true
    );
    assert_eq!(
        parsed["third_test_candidate"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["third_test_candidate"]["operator_env_blocks_100_percent"],
        true
    );
    assert_eq!(parsed["third_test_candidate"]["real_live_ready"], false);
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
    assert_eq!(parsed["memory_maintenance_receipt"]["available"], true);
    assert_eq!(parsed["memory_maintenance_receipt"]["readable"], true);
    assert_eq!(parsed["memory_maintenance_receipt"]["state"], "missing");
    assert_eq!(parsed["memory_maintenance_receipt"]["receipt_count"], 0);
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_entry_id"],
        Value::Null
    );
    assert_eq!(parsed["memory_maintenance_receipt"]["error"], Value::Null);
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
    assert!(parsed["subagent_readiness"]["local_contract_reason"]
        .as_str()
        .expect("subagent local contract reason should be a string")
        .contains("protocol-ready"));
    assert_eq!(parsed["subagent_readiness"]["live_adapter_ready"], false);
    assert_eq!(parsed["subagent_readiness"]["live_worker_available"], false);
    assert_eq!(
        parsed["subagent_readiness"]["worker_runtime_state"],
        "local_contract_only"
    );
    assert!(parsed["subagent_readiness"]["worker_runtime_reason"]
        .as_str()
        .expect("subagent worker runtime reason should be a string")
        .contains("subagent slot is fake"));
    assert!(
        parsed["subagent_readiness"]["worker_runtime_blocked_reason"]
            .as_str()
            .expect("subagent worker runtime blocked reason should be a string")
            .contains("subagent slot is fake")
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_route_state"],
        "requires_dispatch_required_capabilities"
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_mismatch_blocks_live"],
        true
    );
    assert!(parsed["subagent_readiness"]["capability_mismatch_reason"]
        .as_str()
        .expect("subagent capability mismatch reason should be a string")
        .contains("missing or mismatched dispatch required_capabilities"));
    assert!(parsed["subagent_readiness"]["live_adapter_reason"]
        .as_str()
        .expect("subagent live adapter reason should be a string")
        .contains("read-only live runner rehearsal is ready"));
    assert_eq!(parsed["subagent_readiness"]["layer_count"], 6);
    assert!(parsed["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "external_ai_downstream"
            && layer["state"] == "ready"
            && layer["local_contract_ready"] == true
            && layer["local_contract_reason"]
                .as_str()
                .expect("layer local contract reason should be a string")
                .contains("protocol-ready")
            && layer["live_adapter_ready"] == false));
    assert!(parsed["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "command_runner"
            && layer["live_adapter_reason"]
                .as_str()
                .expect("layer live adapter reason should be a string")
                .contains("live runner adapters remain deferred")));
    assert!(parsed["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers should be an array")
        .iter()
        .any(|layer| layer["name"] == "live_runner_rehearsal"
            && layer["state"] == "ready"
            && layer["boundary"] == "read_only_preflight"
            && layer["live_worker_available"] == false
            && layer["worker_runtime_state"] == "local_contract_only"
            && layer["blocked_reason"]
                .as_str()
                .expect("layer blocked reason should be a string")
                .contains("required_capabilities")
            && layer["capability_route_state"] == "requires_dispatch_required_capabilities"
            && layer["capability_mismatch_blocks_live"] == true
            && layer["capability_mismatch_reason"]
                .as_str()
                .expect("layer capability mismatch reason should be a string")
                .contains("required_capabilities")
            && layer["live_adapter_reason"]
                .as_str()
                .expect("layer live adapter reason should be a string")
                .contains("read-only live runner rehearsal is ready")));
    assert_eq!(parsed["live_adapter_gates"]["ok"], true);
    assert_eq!(
        parsed["live_adapter_gates"]["overall_state"],
        "disabled_by_default"
    );
    assert_eq!(parsed["live_adapter_gates"]["gate_count"], 3);
    assert_eq!(parsed["live_adapter_gates"]["enabled_count"], 0);
    assert!(parsed["live_adapter_gates"]["gates"]
        .as_array()
        .expect("live adapter gates should be an array")
        .iter()
        .any(|gate| gate["name"] == "actuator_operation"
            && gate["required_env"] == "CHUANG_REAL_ACTUATOR_ENABLE"
            && gate["audit_label"] == "actuator.operation.live"
            && gate["enabled"] == false
            && gate["default_enabled"] == false
            && gate["env_value_state"] == "unset"
            && gate["preflight_checks"]
                .as_array()
                .expect("preflight checks")
                .iter()
                .any(|check| check
                    .as_str()
                    .expect("check")
                    .contains("CHUANG_REAL_ACTUATOR_ENABLE=1"))
            && gate["must_reject_capabilities"]
                .as_array()
                .expect("must reject capabilities")
                .iter()
                .any(|capability| capability
                    .as_str()
                    .expect("capability")
                    .contains("verification-code entry"))));
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
    assert_eq!(parsed["browser_readiness"]["ok"], true);
    assert_eq!(
        parsed["browser_readiness"]["overall_state"],
        "desktop_read_ready_browser_read_unavailable"
    );
    assert_eq!(
        parsed["browser_readiness"]["desktop_read_observation_ready"],
        true
    );
    assert_eq!(
        parsed["browser_readiness"]["desktop_read_tools"],
        serde_json::json!(["screenshot", "locate"])
    );
    assert_eq!(
        parsed["browser_readiness"]["browser_read_adapter_available"],
        false
    );
    assert_eq!(
        parsed["browser_readiness"]["browser_read_reason_code"],
        "real_adapter_missing"
    );
    assert_eq!(
        parsed["browser_readiness"]["browser_read_capabilities"],
        serde_json::json!(["url", "title", "dom_text"])
    );
    assert_eq!(
        parsed["browser_readiness"]["browser_read_does_not_use_desktop_read"],
        true
    );
    assert!(parsed["browser_readiness"]["browser_read_reason"]
        .as_str()
        .expect("browser read reason")
        .contains("must not infer URL, title, or DOM"));
    assert_eq!(parsed["knowledge_readiness"]["ok"], true);
    assert_eq!(
        parsed["knowledge_readiness"]["overall_state"],
        "local_preview_ready_knowledge_read_unavailable"
    );
    assert_eq!(parsed["knowledge_readiness"]["local_preview_ready"], true);
    assert_eq!(
        parsed["knowledge_readiness"]["live_adapter_available"],
        false
    );
    assert_eq!(
        parsed["knowledge_readiness"]["live_sources"],
        serde_json::json!(["wiki", "gbrain"])
    );
    assert_eq!(
        parsed["knowledge_readiness"]["live_reason_code"],
        "endpoint_missing"
    );
    assert_eq!(
        parsed["knowledge_readiness"]["local_preview_is_separate"],
        true
    );
    assert_eq!(
        parsed["knowledge_readiness"]["connects_real_service"],
        false
    );
    assert!(parsed["knowledge_readiness"]["live_reason"]
        .as_str()
        .expect("knowledge read reason")
        .contains("endpoint is missing"));
    assert_eq!(parsed["live_readiness"]["ok"], true);
    assert_eq!(
        parsed["live_readiness"]["overall_state"],
        "local_ready_live_pending"
    );
    assert_eq!(parsed["live_readiness"]["ga_local_mapped_only"], true);
    assert_eq!(parsed["live_readiness"]["desktop_browser_live_gated"], true);
    assert_eq!(parsed["live_readiness"]["browser_worker_frozen"], true);
    assert_eq!(parsed["live_readiness"]["live_worker_available"], false);
    assert_eq!(
        parsed["live_readiness"]["real_external_acceptance_pending"],
        true
    );
    assert_eq!(
        parsed["live_readiness"]["provider_live_request_verified_by_status"],
        false
    );
    assert_eq!(parsed["live_readiness"]["mapped_does_not_mean_live"], true);
    assert_eq!(parsed["live_readiness"]["gated_does_not_mean_ready"], true);
    assert_eq!(parsed["live_readiness"]["frozen_does_not_mean_ready"], true);
    assert_eq!(parsed["live_readiness"]["ready_does_not_mean_live"], true);
    let live_terms = parsed["live_readiness"]["terms"]
        .as_array()
        .expect("live readiness terms should be an array");
    assert!(live_terms
        .iter()
        .any(|term| term["term"] == "ga_local_mapped_only"
            && term["current_value"] == "true"
            && term["does_not_mean"]
                .as_str()
                .expect("does_not_mean")
                .contains("desktop/browser live execution")));
    assert!(live_terms
        .iter()
        .any(|term| term["term"] == "desktop_browser_live_gated"
            && term["current_value"] == "true"
            && term["does_not_mean"] == "actuator live action ready"));
    assert!(live_terms
        .iter()
        .any(|term| term["term"] == "browser_worker_frozen"
            && term["current_value"] == "true"
            && term["does_not_mean"]
                .as_str()
                .expect("does_not_mean")
                .contains("browser automation ready")));
    assert!(live_terms
        .iter()
        .any(|term| term["term"] == "browser_read_unavailable"
            && term["current_value"] == "true"
            && term["does_not_mean"]
                .as_str()
                .expect("does_not_mean")
                .contains("desktop_read observe/screenshot evidence")));
    assert!(live_terms
        .iter()
        .any(|term| term["term"] == "knowledge_read_unavailable"
            && term["current_value"] == "true"
            && term["does_not_mean"]
                .as_str()
                .expect("does_not_mean")
                .contains("local external knowledge preview")));
    assert!(live_terms
        .iter()
        .any(|term| term["term"] == "live_worker_available"
            && term["current_value"] == "false"
            && term["does_not_mean"]
                .as_str()
                .expect("does_not_mean")
                .contains("read-only preflight")));
    assert!(live_terms.iter().any(|term| {
        term["term"] == "real_external_acceptance_pending"
            && term["current_value"] == "true"
            && term["does_not_mean"]
                .as_str()
                .expect("does_not_mean")
                .contains("local-ready, mapped, gated, frozen")
    }));
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
fn cli_status_exposes_memory_maintenance_receipt_summary() {
    let root = temp_identity_root("memory-receipt");
    let config_path = write_fake_status_config(&root);
    fs::write(
        root.join("identity").join("experiences.md"),
        r#"## lim-candidate-42
writeback=memory_maintenance_apply
approved_writeback=true
approval_source=cli --approve-writeback
approved_at=2026-05-07T12:34:56Z
approval_note=老爸批准写入 LIM 候选
provenance_preserved=true
source=lim_dry_run
source_record_id=turn-42
created_at=2026-05-07T12:00:00Z
lesson=先把回执面露出来
"#,
    )
    .expect("receipt experiences should be written");

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
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["memory_maintenance_receipt"]["available"], true);
    assert_eq!(parsed["memory_maintenance_receipt"]["readable"], true);
    assert_eq!(parsed["memory_maintenance_receipt"]["state"], "ready");
    assert_eq!(parsed["memory_maintenance_receipt"]["receipt_count"], 1);
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_entry_id"],
        "lim-candidate-42"
    );
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_source_record_id"],
        "turn-42"
    );
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_approval_source"],
        "cli --approve-writeback"
    );
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_approved_at"],
        "2026-05-07T12:34:56Z"
    );
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_approval_note"],
        "老爸批准写入 LIM 候选"
    );
    assert_eq!(
        parsed["memory_maintenance_receipt"]["latest_provenance_preserved"],
        true
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
    assert_eq!(parsed["subagent_readiness"]["live_worker_available"], false);
    assert_eq!(
        parsed["subagent_readiness"]["worker_runtime_state"],
        "local_contract_only"
    );
    assert!(parsed["subagent_readiness"]["worker_runtime_reason"]
        .as_str()
        .expect("queued_external worker runtime reason")
        .contains("no live worker adapter is available yet"));
    assert!(
        parsed["subagent_readiness"]["worker_runtime_blocked_reason"]
            .as_str()
            .expect("queued_external worker runtime blocked reason")
            .contains("queued_external")
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_route_state"],
        "requires_dispatch_required_capabilities"
    );
    assert_eq!(
        parsed["subagent_readiness"]["capability_mismatch_blocks_live"],
        true
    );
    assert!(parsed["subagent_readiness"]["capability_mismatch_reason"]
        .as_str()
        .expect("queued_external capability mismatch reason")
        .contains("missing or mismatched dispatch required_capabilities"));
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
    assert_eq!(parsed["provider_readiness"]["transport"], "native");
    assert_eq!(parsed["provider_readiness"]["request_timeout_ms"], 12_345);
    assert_eq!(parsed["provider_readiness"]["fallback_configured"], false);
    assert_eq!(parsed["provider_readiness"]["api_key_state"], "<set>");
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
    assert!(stdout.contains(
        "provider_readiness: ok=true state=ready kind=openai_compatible transport=native fallback_configured=false timeout_ms=12345 api_key_state=<set>"
    ));
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
