use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-doctor-test-{name}-{nanos}"))
}

/// doctor 的 goal_run 就绪度按进程 CWD 的 ./context/goal-runs 读取；
/// 干净 clone 没有运行期 goal-run 数据，因此测试自建最小合法 fixture，
/// 并把子进程 CWD 指到临时 root，保证断言不依赖仓库内未提交文件。
fn write_goal_run_fixture(root: &Path) {
    let mut goal_run = chuang_agent::goal_run::GoalRun::new(
        chuang_agent::goal_mode::GoalSpec::mainline_mvp("doctor fixture"),
        vec![chuang_agent::goal_run::GoalWorkerPlan {
            worker_id: "w1".to_string(),
            objective: "fixture worker".to_string(),
            write_scope_ids: vec!["scope1".to_string()],
            validation_checks: vec!["true".to_string()],
        }],
        vec![chuang_agent::goal_run::GoalWriteScope {
            scope_id: "scope1".to_string(),
            paths: vec!["evidence.txt".to_string()],
        }],
        chuang_agent::goal_run::GoalValidationPlan {
            commands: vec!["true".to_string()],
        },
        chuang_agent::goal_run::GoalIntegrationPolicy {
            main_process_owns_integration: true,
            workers_may_commit: false,
            workers_may_touch_secrets: false,
            require_worker_reports: false,
        },
    )
    .expect("goal run fixture should build");
    goal_run
        .record_checkpoint(chuang_agent::goal_run::GoalCheckpoint::new(
            "ck-1",
            "fixture checkpoint summary",
            vec!["w1".to_string()],
            vec!["fixture validation note".to_string()],
        ))
        .expect("goal run checkpoint should record");
    fs::create_dir_all(root.join("context/goal-runs")).expect("goal-runs dir should create");
    fs::write(
        root.join("context/goal-runs/mainline-mvp.json"),
        serde_json::to_string_pretty(&goal_run).expect("goal run fixture should serialize"),
    )
    .expect("goal run fixture should write");
}

#[test]
fn cli_doctor_reports_mvp_health_in_text() {
    let root = temp_root("text");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    fs::create_dir_all(root.join("rules")).expect("rules root should be created");
    fs::write(
        root.join("rules/core.md"),
        "- Keep responses minimal and testable.\n",
    )
    .expect("core rule should write");
    write_goal_run_fixture(&root);
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nrules_root = \"{}\"\nrules_core_path = \"{}\"\n",
            root.join("memory.db").display(),
            root.join("identity").display(),
            root.join("rules").display(),
            root.join("rules/core.md").display()
        ),
    )
    .expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .current_dir(&root)
        .args([
            "doctor",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
        ])
        .output()
        .expect("doctor should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("doctor_ok: true"));
    assert!(stdout.contains("doctor_check name=config ok=true"));
    assert!(stdout.contains("doctor_check name=identity_memory ok=true"));
    assert!(stdout.contains("doctor_check name=identity_experiences ok=true"));
    assert!(stdout.contains("doctor_check name=memory_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=provider_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=channel_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=subagent_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=live_adapter_preflight ok=true"));
    assert!(stdout.contains("doctor_check name=app_server_service_runtime ok=true"));
    assert!(stdout.contains("doctor_check name=external_ai_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=local_contract_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=slots ok=true"));
    assert!(stdout.contains("doctor_check name=atomic_tools ok=true"));
    assert!(stdout.contains("doctor_check name=governance_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=goal_mode ok=true"));
    assert!(stdout.contains("doctor_check name=goal_run_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=project_readiness ok=true"));
    assert!(stdout.contains("doctor_check name=actuator_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=control_plane_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=runtime_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=subagent_queue_smoke ok=true"));
    assert!(stdout.contains("doctor_check name=plugin_registry ok=true"));
    assert!(stdout.contains("provider: fake"));
    assert!(stdout.contains(
        "provider_readiness: ok=true state=ready kind=fake transport=fake fallback_configured=false timeout_ms=none api_key_state=none"
    ));
    assert!(stdout.contains("execution: generic_agent_mvp"));
    assert!(stdout.contains(
        "governance_readiness: ok=true kind=static_rule rules_loaded=true tool_surface_governed=true goal_run_executes=false"
    ));
    assert!(stdout.contains(
        "governance_decisions: read_only=allowed dangerous_write=needs_approval dangerous_shell=needs_approval secret_shell=needs_approval"
    ));
    assert!(stdout.contains(
        "policy_tool_status: active_profile=full_local_workspace normal_local_action_default=file_write/code_execute/open_app/click/input=allow_with_audit"
    ));
    assert!(stdout.contains("high_risk_boundary=external_send=require_approval"));
    assert!(stdout.contains("ga_tool_descriptors=9/15 missing=0"));
    assert!(stdout.contains("runtime_report_surface: ok=true artifacts=11 observability_fields=26"));
    assert!(stdout.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(stdout.contains("tool_protocol_error_count"));
    assert!(stdout.contains("runtime_meta.runtime_event_ledger_json"));
    assert!(stdout.contains("runtime_meta.context_compaction_events"));
    assert!(stdout.contains("runtime_meta.context_compaction_summary_json"));
    assert!(stdout.contains("runtime_event_tool_started_count"));
    assert!(stdout.contains("runtime_event_tool_finished_count"));
    assert!(stdout.contains("runtime_event_approval_requested_count"));
    assert!(stdout.contains("runtime_event_approval_resolved_count"));
    assert!(stdout.contains("runtime_event_elicitation_requested_count"));
    assert!(stdout.contains("tool_unified_execution_status"));
    assert!(stdout.contains("tool_unified_execution_failure_count"));
    assert!(stdout.contains("tool_unified_execution_failure_classes"));
    assert!(stdout.contains("context_pack_trace"));
    assert!(stdout.contains("context_compaction_summary_json"));
    assert!(stdout.contains("runtime_response.trace"));
    assert!(stdout.contains("runtime_response_trace_chars"));
    assert!(stdout.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(stdout.contains("runtime_meta.subagent_children_summary_json"));
    assert!(stdout.contains("goal_handoff_parent_context_handoff_count"));
    assert!(stdout.contains("goal_handoff_report_admission_ref_count"));
    assert!(stdout.contains("goal_handoff_report_admission_refs"));
    assert!(stdout.contains("goal_handoff_report_admission_reason_codes"));
    assert!(stdout.contains("subagent_children_child_count"));
    assert!(stdout.contains("subagent_children_accepted_report_count"));
    assert!(stdout.contains("subagent_children_report_admission_ref_count"));
    assert!(stdout.contains("subagent_children_report_admission_refs"));
    assert!(stdout.contains("subagent_children_missing_report_count"));
    assert!(stdout.contains("subagent_children_report_reason_codes"));
    assert!(stdout.contains(
        "atomic_tools_ok: true manifest_schema_version=1 action_schema_version=1 report_schema_version=6"
    ));
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
    assert!(stdout.contains(
        "subagent_live_worker: enabled=false adapter_kind=none status=disabled starts_worker=false available=false reason=subagent_live_worker disabled by default; no live worker is started"
    ));
    assert!(stdout.contains(
        "goal_mode: ok=true entrypoint=run --goal TEXT kind=lightweight_runtime_context context_source=goal default_goal_id=mainline-mvp allowed_slots=context,governance,execution,report,memory checkpoint_policy=progress_log:true handoff:true commit:true final_report_policy=validation:true next_steps:true bypasses_governance=false adds_core_slot=false"
    ));
    assert!(stdout.contains("goal_run_ok: true plan_exists="));
    assert!(stdout.contains("goal_id=mainline-mvp checkpoints="));
    assert!(stdout.contains("workers="));
    assert!(stdout.contains("validation_commands="));
    assert!(stdout.contains("goal_id=mainline-mvp"));
    assert!(stdout.contains("goal_run_checkpoint_log_complete:"));
    assert!(stdout.contains("goal_run_last_checkpoint:"));
    assert!(stdout.contains("goal_run_last_checkpoint_summary:"));
    assert!(stdout.contains("goal_run_last_checkpoint_created_at:"));
    assert!(stdout.contains("goal_run_last_checkpoint_completed_worker_ids:"));
    assert!(stdout.contains("goal_run_last_checkpoint_validation_notes:"));
    assert!(stdout.contains("goal_run_incomplete_reasons:"));
    assert!(stdout.contains("project_readiness: ok=true state=mvp_ready_with_partial_modules"));
    assert!(stdout.contains(
        "local_contract_readiness: ok=true state=ready contracts=6 ready=6 partial=0 deferred=0 blocked=0 connects_real_external_services=false writes_core_memory=false executes_plugins=false"
    ));
    assert!(stdout.contains(
        "local_contract_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
    ));
    assert!(stdout.contains(
        "release_readiness: ok=true name=second_test_version state=second_test_version_ready"
    ));
    assert!(stdout.contains("release_acceptance: count=7"));
    assert!(stdout.contains(
        "connects_real_external_services=false verifies_real_external_services=false uses_stub_or_local_fixtures=true writes_repo_files=false"
    ));
    assert!(stdout.contains(
        "third_test_candidate: ok=true state=local_gate_ready_requires_manual_live_check local_gate_ready=true smoke_script=scripts/chuang-third-test-smoke.sh marker=third_test_candidate_smoke_ok requires_manual_live_check=true connects_real_external_services=false operator_env_blocks_100_percent=true real_live_ready=false"
    ));
    assert!(stdout.contains("memory_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains(
        "memory_layer_boundary: local_read_only_preview_source_contract live_retrieval_pending_gated"
    ));
    assert!(stdout.contains("channel_readiness: ok=true state=ready layers=5"));
    assert!(stdout.contains(
        "subagent_readiness: ok=true state=queued_protocol_partial mode=fake local_contract_ready=true local_contract_state=ready live_adapter_ready=false live_adapter_state=partial"
    ));
    assert!(stdout.contains("live_worker_available=false worker_runtime_state=local_contract_only"));
    assert!(stdout
        .contains("worker_runtime_blocked_reason=live_worker_unavailable: subagent slot is fake"));
    assert!(stdout.contains("capability_route_state=requires_dispatch_required_capabilities"));
    assert!(stdout.contains("capability_mismatch_blocks_live=true"));
    assert!(stdout.contains(
        "capability_mismatch_reason=live subagent preflight must reject missing or mismatched dispatch required_capabilities"
    ));
    assert!(stdout.contains("subagent_worker_runtime_reason: subagent slot is fake"));
    assert!(stdout.contains(
        "subagent_capability_mismatch_reason: live subagent preflight must reject missing or mismatched dispatch required_capabilities"
    ));
    assert!(stdout.contains("subagent_readiness_local_contract_reason:"));
    assert!(stdout.contains("subagent_readiness_live_adapter_reason:"));
    assert!(stdout.contains(
        "live_adapter_gates: ok=true state=disabled_by_default gates=3 enabled=0 disabled=3"
    ));
    assert!(stdout.contains("live_readiness: ok=true state=local_ready_live_pending"));
    assert!(stdout.contains("local_ready_scope=ready/local-ready only covers local contracts"));
    assert!(stdout.contains("ga_local_mapped_only=true"));
    assert!(stdout.contains("desktop_browser_live_gated=true"));
    assert!(stdout.contains("browser_worker_frozen=true"));
    assert!(stdout.contains("real_external_acceptance_pending=true"));
    assert!(stdout.contains("provider_live_request_verified_by_status=false"));
    assert!(stdout.contains("ready_does_not_mean_live=true"));
    assert!(stdout.contains(
        "live_adapter_gate name=subagent_runner state=disabled enabled=false default_enabled=false env_value_state=unset required_env=CHUANG_CODEX_RUNNER_ENABLE audit_label=subagent.runner.live"
    ));
    assert!(stdout.contains("preflight=confirm CHUANG_CODEX_RUNNER_ENABLE=1"));
    assert!(stdout.contains("must_reject=unscoped external worker pool"));
    assert!(stdout.contains("next=keep disabled until the operator approves exact live adapter targets and preflight evidence"));
    #[cfg(unix)]
    assert!(stdout.contains("external_ai_readiness: ok=true state=ready"));
    #[cfg(windows)]
    assert!(stdout.contains("external_ai_readiness: ok=true state=external_ai_adapter_partial"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
    assert!(stdout.contains("placeholder_warning: provider=fake"));
    assert!(stdout.contains("placeholder_warning: control_plane=fake_local"));
}

#[test]
fn cli_doctor_can_render_json_without_secret_leak() {
    let root = temp_root("json");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    fs::create_dir_all(root.join("rules")).expect("rules root should be created");
    fs::write(
        root.join("rules/core.md"),
        "- Keep responses minimal and testable.\n",
    )
    .expect("core rule should write");
    write_goal_run_fixture(&root);
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nrules_root = \"{}\"\nrules_core_path = \"{}\"\n",
            root.join("memory.db").display(),
            root.join("identity").display(),
            root.join("rules").display(),
            root.join("rules/core.md").display()
        ),
    )
    .expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .current_dir(&root)
        .args([
            "doctor",
            "--json",
            "--config",
            config_path.to_str().expect("config path should be utf8"),
            "--provider-base-url",
            "https://api.example.com/v1",
            "--provider-api-key",
            "doctor-secret-key",
            "--provider-model",
            "gpt-5.5",
        ])
        .output()
        .expect("doctor should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be json");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["checks"].as_array().expect("checks array").len(), 27);
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "atomic_tools"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "provider_readiness"
            && check["detail"]
                .as_str()
                .expect("provider readiness detail")
                .contains("transport=stub")));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "goal_mode"
            && check["detail"]
                .as_str()
                .expect("goal mode detail")
                .contains("default_goal_id=mainline-mvp")
            && check["detail"]
                .as_str()
                .expect("goal mode detail")
                .contains("allowed_slots=context,governance,execution,report,memory")));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "governance_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "goal_run_readiness"
            && check["detail"]
                .as_str()
                .expect("goal run detail")
                .contains("checkpoint_log_complete=")
            && check["detail"]
                .as_str()
                .expect("goal run detail")
                .contains("last_checkpoint=")
            && check["detail"]
                .as_str()
                .expect("goal run detail")
                .contains("incomplete_reasons=")));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "project_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "release_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "third_test_candidate"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "identity_experiences"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "memory_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "channel_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "subagent_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "live_adapter_preflight"
            && check["detail"]
                .as_str()
                .expect("live adapter preflight detail")
                .contains("next_actions=")));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "app_server_service_runtime"
            && check["detail"]
                .as_str()
                .expect("app server service runtime detail")
                .contains("effective_environment=")));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "live_readiness"
            && check["detail"]
                .as_str()
                .expect("live readiness detail")
                .contains("ready_does_not_mean_live=true")));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "external_ai_readiness"));
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "local_contract_readiness"
            && check["detail"]
                .as_str()
                .expect("local contract readiness detail")
                .contains("contracts=6")));
    assert_eq!(
        parsed["status"]["config"]["identity_experiences_path"],
        root.join("identity")
            .join("experiences.md")
            .display()
            .to_string()
    );
    assert_eq!(parsed["status"]["atomic_tools"]["ok"], true);
    assert_eq!(parsed["status"]["governance"]["ok"], true);
    assert_eq!(parsed["status"]["governance"]["rules_loaded"], true);
    assert_eq!(
        parsed["status"]["governance"]["tool_surface_governed"],
        true
    );
    assert_eq!(
        parsed["status"]["governance"]["dangerous_shell_decision"],
        "needs_approval"
    );
    assert_eq!(
        parsed["status"]["governance"]["dangerous_write_decision"],
        "needs_approval"
    );
    assert_eq!(parsed["status"]["governance"]["goal_run_executes"], false);
    assert_eq!(parsed["status"]["goal_mode"]["ok"], true);
    assert_eq!(
        parsed["status"]["goal_mode"]["cli_entrypoint"],
        "run --goal TEXT"
    );
    assert_eq!(
        parsed["status"]["goal_mode"]["checkpoint_policy"]["update_progress_log"],
        true
    );
    assert_eq!(
        parsed["status"]["goal_mode"]["checkpoint_policy"]["update_handoff"],
        true
    );
    assert_eq!(
        parsed["status"]["goal_mode"]["checkpoint_policy"]["commit_checkpoint"],
        true
    );
    assert_eq!(
        parsed["status"]["goal_mode"]["final_report_policy"]["include_validation"],
        true
    );
    assert_eq!(
        parsed["status"]["goal_mode"]["final_report_policy"]["include_next_steps"],
        true
    );
    assert_eq!(parsed["status"]["goal_run"]["ok"], true);
    assert_eq!(parsed["status"]["goal_run"]["goal_id"], "mainline-mvp");
    assert!(parsed["status"]["goal_run"]["plan_exists"].is_boolean());
    assert!(parsed["status"]["goal_run"]["checkpoint_count"].is_number());
    assert!(parsed["status"]["goal_run"]["checkpoint_log_complete"].is_boolean());
    assert!(
        parsed["status"]["goal_run"]["last_checkpoint_summary"].is_string()
            || parsed["status"]["goal_run"]["last_checkpoint_summary"].is_null()
    );
    assert!(parsed["status"]["goal_run"]["incomplete_reasons"]
        .as_array()
        .expect("goal run incomplete reasons should be an array")
        .iter()
        .all(|reason| reason.is_string()));
    assert_eq!(parsed["status"]["project_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["project_readiness"]["overall_state"],
        "mvp_ready_with_partial_modules"
    );
    assert_eq!(parsed["status"]["local_contract_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["overall_state"],
        "ready"
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["contract_count"],
        6
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["writes_core_memory"],
        false
    );
    assert_eq!(
        parsed["status"]["local_contract_readiness"]["executes_plugins"],
        false
    );
    assert!(parsed["status"]["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts array")
        .iter()
        .any(
            |contract| contract["name"] == "external_knowledge_source_contracts"
                && contract["boundary"] == "adapter_contract_only"
        ));
    assert!(parsed["status"]["local_contract_readiness"]["contracts"]
        .as_array()
        .expect("local contracts array")
        .iter()
        .any(|contract| contract["name"] == "goal_mode_smoke_gate"
            && contract["boundary"] == "local_cli_smoke_only"
            && contract["read_only"] == true));
    assert_eq!(parsed["status"]["release_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["release_readiness"]["release_name"],
        "second_test_version"
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["overall_state"],
        "second_test_version_ready_with_partial_modules"
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["readiness_scope"],
        "readiness_and_smoke_acceptance_only_no_live_external_service_connection"
    );
    assert_eq!(parsed["status"]["release_readiness"]["acceptance_count"], 7);
    assert_eq!(
        parsed["status"]["release_readiness"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["verifies_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["uses_stub_or_local_fixtures"],
        true
    );
    assert_eq!(
        parsed["status"]["release_readiness"]["writes_repo_files"],
        false
    );
    assert_eq!(
        parsed["status"]["config"]["subagent_live_worker"]["enabled"],
        false
    );
    assert_eq!(
        parsed["status"]["config"]["subagent_live_worker"]["adapter_kind"],
        "none"
    );
    assert_eq!(
        parsed["status"]["config"]["subagent_live_worker"]["starts_worker"],
        false
    );
    assert_eq!(
        parsed["status"]["config"]["subagent_live_worker"]["available"],
        false
    );
    assert!(parsed["status"]["config"]["subagent_live_worker"]["reason"]
        .as_str()
        .expect("doctor subagent live worker reason")
        .contains("no live worker is started"));
    assert!(parsed["status"]["release_readiness"]["acceptance"]
        .as_array()
        .expect("release acceptance array")
        .iter()
        .any(|item| item["name"] == "real_external_services"
            && item["state"] == "deferred"
            && item["connects_real_service"] == false));
    assert_eq!(parsed["status"]["third_test_candidate"]["ok"], true);
    assert_eq!(
        parsed["status"]["third_test_candidate"]["overall_state"],
        "local_gate_ready_requires_manual_live_check"
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["local_gate_ready"],
        true
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["smoke_script"],
        "scripts/chuang-third-test-smoke.sh"
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["marker"],
        "third_test_candidate_smoke_ok"
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["requires_manual_live_check"],
        true
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["connects_real_external_services"],
        false
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["operator_env_blocks_100_percent"],
        true
    );
    assert_eq!(
        parsed["status"]["third_test_candidate"]["real_live_ready"],
        false
    );
    assert_eq!(parsed["status"]["memory_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["memory_readiness"]["overall_state"],
        "ready"
    );
    assert_eq!(parsed["status"]["memory_readiness"]["layer_count"], 5);
    assert!(parsed["status"]["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers array")
        .iter()
        .any(|layer| layer["name"] == "external_knowledge"
            && layer["state"] == "ready"
            && layer["storage"] == "docs/external-knowledge-adapter.md"));
    assert!(parsed["status"]["memory_readiness"]["layers"]
        .as_array()
        .expect("memory layers array")
        .iter()
        .any(|layer| layer["name"] == "maintenance_loop"
            && layer["state"] == "ready"
            && layer["storage"] == "docs/memory-maintenance-loop.md"));
    assert_eq!(parsed["status"]["channel_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["channel_readiness"]["overall_state"],
        "ready"
    );
    assert_eq!(parsed["status"]["subagent_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["subagent_readiness"]["overall_state"],
        "queued_protocol_partial"
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["local_contract_ready"],
        true
    );
    assert!(
        parsed["status"]["subagent_readiness"]["local_contract_reason"]
            .as_str()
            .expect("subagent local contract reason")
            .contains("protocol-ready")
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["live_adapter_ready"],
        false
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["live_worker_available"],
        false
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["worker_runtime_state"],
        "local_contract_only"
    );
    assert!(
        parsed["status"]["subagent_readiness"]["worker_runtime_reason"]
            .as_str()
            .expect("subagent worker runtime reason")
            .contains("subagent slot is fake")
    );
    assert!(
        parsed["status"]["subagent_readiness"]["worker_runtime_blocked_reason"]
            .as_str()
            .expect("subagent worker runtime blocked reason")
            .contains("subagent slot is fake")
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["capability_route_state"],
        "requires_dispatch_required_capabilities"
    );
    assert_eq!(
        parsed["status"]["subagent_readiness"]["capability_mismatch_blocks_live"],
        true
    );
    assert!(
        parsed["status"]["subagent_readiness"]["capability_mismatch_reason"]
            .as_str()
            .expect("subagent capability mismatch reason")
            .contains("missing or mismatched dispatch required_capabilities")
    );
    assert!(
        parsed["status"]["subagent_readiness"]["live_adapter_reason"]
            .as_str()
            .expect("subagent live adapter reason")
            .contains("read-only live runner rehearsal is ready")
    );
    assert_eq!(parsed["status"]["subagent_readiness"]["layer_count"], 6);
    assert!(parsed["status"]["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "multi_worker"
            && layer["state"] == "ready"
            && layer["blocked_reason"]
                .as_str()
                .expect("multi-worker blocked reason")
                .contains("no live worker adapter")
            && layer["capability_route_state"] == "not_live_routed"
            && layer["capability_mismatch_blocks_live"] == true
            && layer["live_adapter_reason"]
                .as_str()
                .expect("multi-worker live adapter reason")
                .contains("external worker pools remain deferred")));
    assert!(parsed["status"]["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "live_runner_rehearsal"
            && layer["blocked_reason"]
                .as_str()
                .expect("live runner blocked reason")
                .contains("required_capabilities")
            && layer["capability_route_state"] == "requires_dispatch_required_capabilities"
            && layer["capability_mismatch_reason"]
                .as_str()
                .expect("live runner capability mismatch reason")
                .contains("required_capabilities")));
    assert_eq!(parsed["status"]["live_readiness"]["ok"], true);
    assert_eq!(
        parsed["status"]["live_readiness"]["overall_state"],
        "local_ready_live_pending"
    );
    assert_eq!(
        parsed["status"]["live_readiness"]["ready_does_not_mean_live"],
        true
    );
    assert_eq!(parsed["status"]["live_adapter_gates"]["ok"], true);
    assert_eq!(
        parsed["status"]["live_adapter_gates"]["overall_state"],
        "disabled_by_default"
    );
    assert_eq!(parsed["status"]["live_adapter_gates"]["gate_count"], 3);
    assert!(parsed["status"]["live_adapter_gates"]["gates"]
        .as_array()
        .expect("live adapter gates array")
        .iter()
        .any(|gate| gate["name"] == "control_apply"
            && gate["required_env"] == "CHUANG_REAL_CONTROL_ENABLE"
            && gate["audit_label"] == "control.apply.live"
            && gate["enabled"] == false
            && gate["env_value_state"] == "unset"
            && gate["preflight_checks"]
                .as_array()
                .expect("preflight checks")
                .iter()
                .any(|check| check
                    .as_str()
                    .expect("check")
                    .contains("Chuang-only unit allowlist"))
            && gate["must_reject_capabilities"]
                .as_array()
                .expect("must reject capabilities")
                .iter()
                .any(|capability| capability
                    .as_str()
                    .expect("capability")
                    .contains("Codex or Hermes service control"))));
    assert!(parsed["status"]["subagent_readiness"]["layers"]
        .as_array()
        .expect("subagent layers array")
        .iter()
        .any(|layer| layer["name"] == "external_ai_downstream" && layer["state"] == "ready"));
    assert_eq!(parsed["status"]["external_ai_readiness"]["ok"], true);
    #[cfg(unix)]
    assert_eq!(
        parsed["status"]["external_ai_readiness"]["overall_state"],
        "ready"
    );
    #[cfg(windows)]
    assert_eq!(
        parsed["status"]["external_ai_readiness"]["overall_state"],
        "external_ai_adapter_partial"
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["manifest_schema_version"],
        1
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_report_schema_version"],
        6
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["tool_action_schema_version"],
        1
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["mapped_atomic_tool_names"],
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
        parsed["status"]["atomic_tools"]["governed_executable_atomic_tool_names"],
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
        parsed["status"]["atomic_tools"]["interface_only_atomic_tool_names"],
        serde_json::json!([])
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["desktop_browser_interface_only_atomic_tool_names"],
        serde_json::json!([])
    );
    assert_eq!(
        parsed["status"]["atomic_tools"]["desktop_browser_live_gated_atomic_tool_names"],
        serde_json::json!(["mouse", "keyboard", "screenshot", "locate"])
    );
    assert!(parsed["status"]["atomic_tools"]["interface_only_reason"]
        .as_str()
        .expect("interface only reason should be text")
        .contains("all GA atoms are mapped to governed runtime ports"));
    assert_eq!(
        parsed["status"]["atomic_tools"]["local_cli_self_check_entrypoints"],
        serde_json::json!([
            "status --json",
            "doctor --json",
            "app-server health --diagnostic --json"
        ])
    );
    assert_eq!(
        parsed["status"]["config"]["provider_kind"],
        "openai_compatible"
    );
    assert_eq!(parsed["status"]["config"]["api_key_state"], "<set>");
    assert!(parsed["status"]["config"]["placeholder_warnings"]
        .as_array()
        .expect("placeholder warnings should be an array")
        .iter()
        .any(|warning| warning
            .as_str()
            .expect("warning should be string")
            .contains("actuator=fake")));
    assert!(!stdout.contains("doctor-secret-key"));
}

#[test]
#[cfg(unix)]
fn cli_doctor_reports_command_control_list_failure() {
    let root = temp_root("command-control-fail");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{}"
identity_memory_root = "{}"
control = "command"
program = "false"
list_args = "--version"
apply_args = "apply --json"
"#,
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("doctor_control_plane_list_failed"),
        "stderr={stderr}"
    );
    assert!(stderr.contains("status=Some(1)"), "stderr={stderr}");
}

#[test]
#[cfg(unix)]
fn cli_doctor_command_control_smoke_does_not_call_apply() {
    let root = temp_root("command-control-readonly");
    fs::create_dir_all(root.join("identity")).expect("identity root should be created");
    let marker_path = root.join("apply-called");
    let script_path = root.join("control-adapter.sh");
    fs::write(
        &script_path,
        format!(
            r#"#!/bin/sh
if [ "$1" = "list" ]; then
  printf '[{{"unit_id":"chuang-readonly","display_name":"Chuang Readonly","kind":"service","status":"Running","model_name":null,"metadata":{{"adapter":"doctor-test"}}}}]'
  exit 0
fi
if [ "$1" = "apply" ]; then
  printf applied > "{}"
  printf '{{"unit_id":"chuang-readonly","action":"restart","previous_status":"Running","next_status":"Running","model_name":null,"message":"apply should not run"}}'
  exit 0
fi
exit 2
"#,
            marker_path.display()
        ),
    )
    .expect("script should be writable");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
db_path = "{}"
identity_memory_root = "{}"
control = "command"
program = "sh"
list_args = "{} list"
apply_args = "{} apply"
"#,
            root.join("memory.db").display(),
            root.join("identity").display(),
            script_path.display(),
            script_path.display()
        ),
    )
    .expect("config file should be writable");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "doctor",
            "--json",
            "--config",
            config_path
                .to_str()
                .expect("config path should be valid utf-8"),
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !marker_path.exists(),
        "doctor control smoke must remain list-only and must not call apply"
    );
    let parsed: Value =
        serde_json::from_slice(&output.stdout).expect("doctor stdout should be json");
    assert!(parsed["checks"]
        .as_array()
        .expect("checks array")
        .iter()
        .any(|check| check["name"] == "control_plane_smoke" && check["ok"] == true));
}
