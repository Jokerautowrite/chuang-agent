use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_live_readiness_config(manifest_dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("chuang-live-operator-scripts-{nanos}"));
    fs::create_dir_all(&root).expect("test root should be created");

    let identity_memory_root = root.join("hermes-memory");
    let subagent_queue_root = root.join("subagent-queue");
    fs::create_dir_all(&identity_memory_root).expect("identity memory root should be created");
    fs::create_dir_all(&subagent_queue_root).expect("subagent queue root should be created");

    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nrecall_limit = 5\nidentity_memory_root = \"{}\"\nidentity_root = \"{root_dir}/identity\"\nsoul_path = \"{root_dir}/identity/SOUL.md\"\nstory_path = \"{root_dir}/identity/STORY.md\"\nfirst_wake_path = \"{root_dir}/identity/FIRST_WAKE.md\"\nagents_registry_path = \"{root_dir}/identity/agents.toml\"\nrules_root = \"{root_dir}/rules\"\nrules_core_path = \"{root_dir}/rules/core.md\"\nprovider = \"openai_compatible\"\nprovider_id = \"live-readiness-preflight-openai\"\nbase_url = \"https://api.example.com/v1\"\nmodel = \"gpt-live-readiness-preflight\"\napi_key_env = \"CHUANG_AGENT_LIVE_READINESS_PREFLIGHT_API_KEY\"\ntransport = \"stub\"\nsubagent = \"queued_external\"\nsubagent_queue_root = \"{}\"\nactuator = \"command\"\nactuator_program = \"sh\"\nactuator_args = \"{root_dir}/scripts/chuang-actuator-adapter-example.sh --json\"\nactuator_timeout_ms = 30000\ncontrol = \"command\"\nprogram = \"sh\"\nlist_args = \"{root_dir}/scripts/chuang-control-adapter-example.sh list --json\"\napply_args = \"{root_dir}/scripts/chuang-control-adapter-example.sh apply --json\"\ncontrol_timeout_ms = 30000\n",
            root.join("chuang-agent.db").display(),
            identity_memory_root.display(),
            subagent_queue_root.display(),
            root_dir = manifest_dir.display(),
        ),
    )
    .expect("live readiness config should be written");

    (root, config_path)
}

#[test]
fn live_operator_receipt_script_is_readonly_and_template_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-receipt.sh");
    let script = fs::read_to_string(&script_path).expect("receipt script should be readable");

    assert!(script.contains("Readonly receipt template for a manual Chuang live test."));
    assert!(script.contains("CHUANG_LIVE_OPERATOR"));
    assert!(script.contains("CHUANG_LIVE_ENV_FILE"));
    assert!(script.contains("CHUANG_LIVE_OPERATOR_ENV_FILE"));
    assert!(script.contains("CHUANG_FEISHU_ENV_FILE"));
    assert!(script.contains("CHUANG_LIVE_REQUEST_ID"));
    assert!(script.contains("\"connects_real_feishu\": False"));
    assert!(script.contains("\"sends_feishu_messages\": False"));
    assert!(script.contains("\"connects_real_provider\": False"));
    assert!(script.contains("\"starts_workers\": False"));
    assert!(script.contains("\"dispatches_tasks\": False"));
    assert!(script.contains("\"performs_desktop_actions\": False"));
    assert!(script.contains("\"performs_browser_actions\": False"));
    assert!(script.contains("\"connects_real_wiki\": False"));
    assert!(script.contains("\"connects_real_gbrain\": False"));
    assert!(script.contains("\"reads_secret_values\": False"));
    assert!(script.contains("\"prints_secret_values\": False"));
    assert!(script.contains("\"starts_services\": False"));
    assert!(script.contains("\"stops_services\": False"));
    assert!(script.contains("\"touches_services\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"deletes_files\": False"));
    assert!(script.contains("\"reuses_codex_or_hermes_credentials\": False"));
    assert!(script.contains("\"provider_live_request_receipt_ref\""));
    assert!(script.contains("\"capability_routing_ref\""));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
}

#[test]
fn live_operator_receipt_script_outputs_redacted_json_template() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-receipt.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-live-operator-receipt-smoke-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let env_file = temp_dir.join("live.env");
    fs::write(&env_file, "CHUANG_LIVE_PLACEHOLDER=1\n").expect("env file should write");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .env("CHUANG_LIVE_OPERATOR", "operator-x")
        .env("CHUANG_LIVE_REQUEST_ID", "live-request-123")
        .env("CHUANG_LIVE_ENV_FILE", &env_file)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("receipt script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("receipt output should be json");

    assert_eq!(data["schema_version"], 1);
    assert!(data["tested_at"].as_str().is_some());
    assert_eq!(data["request_id"], "live-request-123");
    assert_eq!(data["operator"], "operator-x");
    assert_eq!(data["env_file"], env_file.display().to_string());
    assert_eq!(data["workspace_root"], manifest_dir.display().to_string());
    assert_eq!(data["approval_scope"], "<fill_exact_live_scope>");
    assert_eq!(
        data["rollback_condition"],
        "<fill_abort_or_rollback_condition>"
    );
    assert_eq!(data["acceptance_status"], "not_verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["cannot_mark_complete_without_operator_evidence"], true);
    assert_eq!(data["preflight_status"], "<fill_after_test>");
    assert_eq!(data["health_status"], "<fill_after_test>");
    assert_eq!(data["new_thread_status"], "<fill_after_test>");
    assert_eq!(data["session_status"], "<fill_after_test>");
    assert_eq!(data["runtime_report_id"], "<fill_after_test>");
    assert_eq!(data["provider_status"], "<fill_after_test>");
    assert_eq!(
        data["codex_hermes_isolation"],
        "<keep_codex_and_hermes_separate>"
    );
    assert_eq!(data["notes"], serde_json::json!([]));
    assert_eq!(data["blockers"], serde_json::json!([]));
    assert_eq!(data["boundaries"]["readonly"], true);
    assert_eq!(data["readonly_boundaries"], data["boundaries"]);
    assert_eq!(data["boundaries"]["connects_real_feishu"], false);
    assert_eq!(data["boundaries"]["sends_feishu_messages"], false);
    assert_eq!(data["boundaries"]["connects_real_provider"], false);
    assert_eq!(data["boundaries"]["starts_workers"], false);
    assert_eq!(data["boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["boundaries"]["performs_desktop_actions"], false);
    assert_eq!(data["boundaries"]["performs_browser_actions"], false);
    assert_eq!(data["boundaries"]["connects_real_wiki"], false);
    assert_eq!(data["boundaries"]["connects_real_gbrain"], false);
    assert_eq!(data["boundaries"]["reads_secret_values"], false);
    assert_eq!(data["boundaries"]["prints_secret_values"], false);
    assert_eq!(data["boundaries"]["starts_services"], false);
    assert_eq!(data["boundaries"]["stops_services"], false);
    assert_eq!(data["boundaries"]["touches_services"], false);
    assert_eq!(data["boundaries"]["modifies_repo"], false);
    assert_eq!(data["boundaries"]["deletes_files"], false);
    assert_eq!(
        data["boundaries"]["reuses_codex_or_hermes_credentials"],
        false
    );
    let service_ids: Vec<&str> = data["service_receipts"]
        .as_array()
        .expect("service receipts should be an array")
        .iter()
        .map(|item| {
            item["id"]
                .as_str()
                .expect("service receipt id should be a string")
        })
        .collect();
    assert_eq!(
        service_ids,
        vec![
            "feishu",
            "provider",
            "subagent_live_rehearsal",
            "desktop",
            "browser",
            "wiki",
            "gbrain"
        ]
    );
    for service in data["service_receipts"]
        .as_array()
        .expect("service receipts should be an array")
    {
        assert_eq!(service["status"], "<not_verified|verified|blocked>");
    }
    assert_eq!(
        data["service_evidence"]["feishu"]["runtime_report_id"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_evidence"]["provider"]["api_key_state"],
        "<set|missing>"
    );
    assert_eq!(
        data["service_evidence"]["provider"]["provider_live_request_receipt_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["allowlist_receipt_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_receipts"][2]["evidence"]["gate_receipt_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_receipts"][2]["evidence"]["allowlist_receipt_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_receipts"][2]["evidence"]["capability_routing_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_receipts"][2]["evidence"]["report_admission_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["gate_receipt_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["capability_routing_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["report_admission_ref"],
        "<fill_after_test>"
    );
    assert_eq!(
        data["service_evidence"]["wiki"]["writes_core_memory"],
        false
    );
    assert_eq!(
        data["service_evidence"]["gbrain"]["writes_core_memory"],
        false
    );
    assert_eq!(data["real_live_acceptance"]["complete"], false);
    assert_eq!(data["real_live_acceptance"]["status"], "not_verified");
    assert_eq!(data["real_live_acceptance"]["gap_count"], 7);
    assert_eq!(
        data["real_live_acceptance"]["cannot_mark_complete_from_template"],
        true
    );
    assert_eq!(
        data["real_live_acceptance"]["requires_operator_evidence"],
        true
    );
    assert!(!stdout.contains("CHUANG_LIVE_PLACEHOLDER=1"));
}

#[test]
fn provider_readiness_check_reports_local_only_block_without_secret_values() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-provider-readiness-check.sh");
    let script = fs::read_to_string(&script_path).expect("provider readiness script should read");
    assert!(script.contains("source_status_surface"));
    let (_root, config_path) = write_live_readiness_config(&manifest_dir);

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .arg("--config")
        .arg(&config_path)
        .env(
            "CHUANG_AGENT_LIVE_READINESS_PREFLIGHT_API_KEY",
            "super-secret-value",
        )
        .current_dir(&manifest_dir)
        .output()
        .expect("provider readiness check should execute");

    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("provider readiness output should be json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["readonly"], true);
    assert_eq!(
        data["source_status_surface"],
        "cargo run --quiet -- status --json"
    );
    assert_eq!(data["connects_real_provider"], false);
    assert_eq!(data["prints_secret_values"], false);
    assert_eq!(data["ok"], false);
    assert_eq!(data["provider_kind"], "openai_compatible");
    assert_eq!(data["transport"], "stub");
    assert_eq!(data["api_key_state"], "<set>");
    assert_eq!(
        data["blocked_reason"],
        "provider_placeholder_warnings_present"
    );
    assert_eq!(
        data["current"],
        "provider transport=stub is local-only and ready for smoke coverage"
    );
    assert_eq!(
        data["next_action"],
        "switch to a real provider transport only after live secrets and transport diagnostics are confirmed"
    );
    assert_eq!(data["placeholder_warning_count"], 1);
    assert!(!stdout.contains("super-secret-value"));
}

#[test]
fn provider_readiness_check_uses_provider_env_file_when_available() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-provider-readiness-check.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-provider-readiness-check-{nanos}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let provider_env = temp_dir.join("provider.env");
    fs::write(
        &provider_env,
        "CODEX_PPTOKEN_API_KEY=provider-secret-value\n",
    )
    .expect("provider env should write");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .arg("--config")
        .arg(manifest_dir.join("config.toml"))
        .env("CHUANG_PROVIDER_ENV_FILE", &provider_env)
        .current_dir(&manifest_dir)
        .output()
        .expect("provider readiness check should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("provider readiness output should be json");

    assert_eq!(data["ok"], true);
    assert_eq!(data["api_key_state"], "<set>");
    assert_eq!(data["connects_real_provider"], false);
    assert_eq!(data["prints_secret_values"], false);
    assert!(!stdout.contains("provider-secret-value"));
}

#[test]
fn live_gaps_check_uses_provider_env_file_when_available() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-gaps-check.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-live-gaps-check-{nanos}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let provider_env = temp_dir.join("provider.env");
    fs::write(
        &provider_env,
        "CODEX_PPTOKEN_API_KEY=provider-secret-value\n",
    )
    .expect("provider env should write");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .env("CHUANG_PROVIDER_ENV_FILE", &provider_env)
        .current_dir(&manifest_dir)
        .output()
        .expect("live gaps check should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("live gaps output should be json");

    assert_eq!(data["ok"], true);
    assert_eq!(data["provider_readiness"]["overall_state"], "ready");
    assert_eq!(data["provider_readiness"]["api_key_state"], "<set>");
    assert_eq!(data["matrix"][2]["state"], "pending");
    assert!(!stdout.contains("provider-secret-value"));
}

#[test]
fn candidate_verify_wrapper_only_treats_expected_provider_blocks_as_non_fatal() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-candidate-verify.sh"))
        .expect("candidate verify wrapper should be readable");

    let complete_local_smoke = wrapper
        .find("sh scripts/chuang-complete-local-smoke.sh")
        .expect("candidate verify should run complete-local smoke");
    let live_runner_rehearsal = wrapper
        .find("bash scripts/chuang-live-runner-rehearsal-smoke.sh")
        .expect("candidate verify should run live runner rehearsal smoke");
    let live_gaps_check = wrapper
        .find("bash scripts/chuang-live-gaps-check.sh")
        .expect("candidate verify should run live gaps check");
    let live_runner_readiness_view = wrapper
        .find("[candidate-verify] live runner readiness view")
        .expect("candidate verify should run live runner readiness view");
    let operator_checklist = wrapper
        .find("[candidate-verify] live operator checklist readonly summary")
        .expect("candidate verify should run live operator checklist readonly summary");
    let goal_run_status = wrapper
        .find("[candidate-verify] goal run status readonly summary")
        .expect("candidate verify should run goal run status readonly summary");
    let provider_readiness = wrapper
        .find("provider readiness check")
        .expect("candidate verify should include provider readiness check");
    let provider_block = wrapper
        .find("provider readiness check reported a non-live block")
        .expect("candidate verify should keep expected provider blocks non-fatal");
    let provider_error = wrapper
        .find("provider readiness check failed unexpectedly")
        .expect("candidate verify should surface unexpected provider check failures");
    let marker = wrapper
        .find("chuang_candidate_verify_ok")
        .expect("candidate verify should print a stable success marker");

    assert!(complete_local_smoke < live_runner_rehearsal);
    assert!(live_runner_rehearsal < live_gaps_check);
    assert!(live_gaps_check < live_runner_readiness_view);
    assert!(live_runner_readiness_view < operator_checklist);
    assert!(operator_checklist < goal_run_status);
    assert!(goal_run_status < provider_readiness);
    assert!(provider_readiness < provider_block);
    assert!(provider_block < provider_error);
    assert!(provider_error < marker);
    assert!(wrapper.contains("[candidate-verify] live gaps check"));
    assert!(wrapper.contains("[candidate-verify] live runner readiness view"));
    assert!(wrapper.contains("scripts/chuang-live-runner-readiness-view.sh --json"));
    assert!(wrapper.contains("scripts/chuang-live-operator-checklist.sh --json"));
    assert!(wrapper.contains("scripts/chuang-goal-run-status.sh --json"));
    assert!(wrapper.contains("provider_status=$?"));
    assert!(wrapper.contains("exit \"$provider_status\""));
    assert!(wrapper.contains("unset CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(wrapper.contains("unset CHUANG_REAL_CONTROL_ENABLE"));
    assert!(wrapper.contains("unset CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(!wrapper.contains("git status --short"));
    assert!(!wrapper.contains("working tree must be clean"));
    assert!(!wrapper.contains("systemctl"));
    assert!(!wrapper.contains("\nrm "));
    assert!(!wrapper.contains(" rm -"));
    assert!(!wrapper.contains(".codex-im/.env"));
    assert!(!wrapper.contains("hermes-gateway"));
    assert!(!wrapper.contains("FEISHU_"));
}

#[test]
fn third_test_smoke_wrapper_sequences_local_gates_and_readonly_summaries() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-third-test-smoke.sh"))
        .expect("third test smoke wrapper should be readable");

    let clean_tree_check = wrapper
        .find("git status --short")
        .expect("third test smoke should check for a clean tree first");
    let final_verify = wrapper
        .find("sh scripts/chuang-final-verify.sh")
        .expect("third test smoke should run final verify");
    let live_preflight = wrapper
        .find("sh scripts/chuang-live-readonly-preflight.sh")
        .expect("third test smoke should run live readonly preflight");
    let live_gaps_check = wrapper
        .find("bash scripts/chuang-live-gaps-check.sh --json")
        .expect("third test smoke should run live gaps matrix");
    let live_runner_readiness_view = wrapper
        .find("[third-test] live runner readiness view")
        .expect("third test smoke should run live runner readiness view");
    let operator_checklist = wrapper
        .find("bash scripts/chuang-live-operator-checklist.sh --json")
        .expect("third test smoke should run live operator checklist readonly summary");
    let goal_status = wrapper
        .find("bash scripts/chuang-goal-run-status.sh --json")
        .expect("third test smoke should run goal run status readonly summary");
    let marker = wrapper
        .find("third_test_candidate_smoke_ok")
        .expect("third test smoke should print a stable success marker");

    assert!(clean_tree_check < final_verify);
    assert!(final_verify < live_preflight);
    assert!(live_preflight < live_gaps_check);
    assert!(live_gaps_check < live_runner_readiness_view);
    assert!(live_runner_readiness_view < operator_checklist);
    assert!(operator_checklist < goal_status);
    assert!(goal_status < marker);
    assert!(wrapper.contains("working tree must be clean before third test smoke"));
    assert!(wrapper.contains("[third-test] live runner readiness view"));
    assert!(wrapper.contains("scripts/chuang-live-runner-readiness-view.sh --json"));
    assert!(wrapper.contains("live_runner_readiness_view_state="));
    assert!(wrapper.contains("live_runner_readiness_view_ready_for_live="));
    assert!(wrapper.contains("live_runner_readiness_view_blocked_reason="));
    assert!(wrapper.contains("goal_run_status_interactive_state="));
    assert!(wrapper.contains("goal_run_status_activity_hint="));
    assert!(wrapper.contains("live_gaps_summary="));
    assert!(wrapper.contains("live_operator_checklist_status="));
    assert!(wrapper.contains("goal_run_status_overall="));
    assert!(wrapper.contains("third_test_candidate_smoke_ok"));
    assert!(!wrapper.contains("systemctl"));
    assert!(!wrapper.contains("tmux new"));
    assert!(!wrapper.contains("codex exec"));
    assert!(!wrapper.contains("git reset"));
    assert!(!wrapper.contains("git checkout"));
    assert!(!wrapper.contains("\nrm "));
    assert!(!wrapper.contains(" rm -"));
    assert!(!wrapper.contains(".codex-im/.env"));
    assert!(!wrapper.contains("hermes-gateway"));
}

#[test]
fn live_readonly_preflight_wrapper_includes_provider_readiness_evidence_and_stays_readonly() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper =
        fs::read_to_string(manifest_dir.join("scripts/chuang-live-readonly-preflight.sh"))
            .expect("live readonly preflight wrapper should be readable");

    let watchdog = wrapper
        .find("scripts/chuang-goal-watchdog.sh")
        .expect("preflight should run watchdog");
    let status = wrapper
        .find("status diagnostic")
        .expect("preflight should run status diagnostic");
    let provider_readiness = wrapper
        .find("scripts/chuang-provider-readiness-check.sh")
        .expect("preflight should include provider readiness check");
    let provider_block = wrapper
        .find("provider readiness check reported a local-only block")
        .expect("preflight should treat expected provider block as local-only evidence");
    let complete_local = wrapper
        .find("complete local smoke")
        .expect("preflight should still finish with complete-local smoke");
    let marker = wrapper
        .find("live_readiness_preflight_ok")
        .expect("preflight should print a stable success marker");

    assert!(watchdog < status);
    assert!(status < provider_readiness);
    assert!(provider_readiness < provider_block);
    assert!(provider_block < complete_local);
    assert!(complete_local < marker);
    assert!(wrapper.contains("provider_status=$?"));
    assert!(wrapper.contains("exit \"$provider_status\""));
    assert!(wrapper.contains("unset CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(wrapper.contains("unset CHUANG_REAL_CONTROL_ENABLE"));
    assert!(wrapper.contains("unset CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(!wrapper.contains("git reset"));
    assert!(!wrapper.contains("git checkout"));
    assert!(!wrapper.contains("systemctl"));
    assert!(!wrapper.contains("\nrm "));
    assert!(!wrapper.contains(" rm -"));
    assert!(!wrapper.contains(".codex-im/.env"));
    assert!(!wrapper.contains("hermes-gateway"));
}

#[test]
fn live_operator_checklist_exposes_external_live_acceptance_matrix_without_claiming_live() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-checklist.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-live-operator-checklist-matrix-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let provider_env = temp_dir.join("provider.env");
    fs::write(
        &provider_env,
        "CODEX_PPTOKEN_API_KEY=provider-secret-value\n",
    )
    .expect("provider env should write");
    let env_file = temp_dir.join("chuang-feishu.env");
    fs::write(
        &env_file,
        format!(
            "CHUANG_FEISHU_APP_ID=app-secret-value\nCHUANG_FEISHU_APP_SECRET=feishu-secret-value\nCHUANG_AGENT_WORKSPACE_ROOT={}\nCHUANG_PROVIDER_ENV_FILE={}\n",
            manifest_dir.display(),
            provider_env.display()
        ),
    )
    .expect("operator env should write");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .env("CHUANG_LIVE_OPERATOR_ENV_FILE", &env_file)
        .current_dir(&manifest_dir)
        .output()
        .expect("checklist should execute");
    assert!(
        output.status.success(),
        "stderr: {} stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("checklist output should be json");
    assert_eq!(
        data["checks"]["feishu_env_file"]["inherited_forbidden_credential_env_states"]
            ["HERMES_FEISHU_ENCRYPT_KEY"],
        "<unset>"
    );
    assert_eq!(
        data["checks"]["feishu_env_file"]["inherited_forbidden_credential_env_states"]
            ["CODEX_FEISHU_BOT_ID"],
        "<unset>"
    );
    let matrix = data["external_live_acceptance_matrix"]
        .as_array()
        .expect("matrix should be an array");
    let surfaces = matrix
        .iter()
        .map(|item| item["id"].as_str().expect("id should be string"))
        .collect::<Vec<_>>();
    assert_eq!(
        surfaces,
        vec![
            "feishu",
            "provider",
            "subagent_live_rehearsal",
            "desktop",
            "browser",
            "wiki",
            "gbrain"
        ]
    );
    assert_eq!(data["real_live_acceptance"]["complete"], false);
    assert_eq!(data["real_live_acceptance"]["status"], "not_verified");
    assert_eq!(data["real_live_acceptance"]["gap_count"], 7);
    assert_eq!(data["real_live_acceptance"]["checklist_is_readonly"], true);
    assert_eq!(
        data["real_live_acceptance"]["cannot_mark_complete_from_readonly_checklist"],
        true
    );
    assert_eq!(
        matrix
            .iter()
            .find(|item| item["id"] == "subagent_live_rehearsal")
            .expect("subagent rehearsal entry")["evidence_refs"]["gate"],
        "<fill_after_test>"
    );
    assert_eq!(
        matrix
            .iter()
            .find(|item| item["id"] == "subagent_live_rehearsal")
            .expect("subagent rehearsal entry")["evidence_refs"]["allowlist"],
        "<fill_after_test>"
    );
    assert_eq!(
        matrix
            .iter()
            .find(|item| item["id"] == "subagent_live_rehearsal")
            .expect("subagent rehearsal entry")["evidence_refs"]["capability_routing"],
        "<fill_after_test>"
    );
    assert_eq!(
        matrix
            .iter()
            .find(|item| item["id"] == "subagent_live_rehearsal")
            .expect("subagent rehearsal entry")["evidence_refs"]["report_admission"],
        "<fill_after_test>"
    );
    for item in matrix {
        assert_eq!(item["manual_live_required"], true);
        assert_eq!(item["connects_real_service_in_checklist"], false);
        assert_eq!(item["completion_state"], "not_verified");
        assert_eq!(item["must_not_count_as_complete"], true);
        assert_eq!(item["prints_secret_values"], false);
        assert!(!item["readonly_probe"].as_str().unwrap_or("").is_empty());
        if item["id"] == "subagent_live_rehearsal" {
            assert_eq!(
                item["required_evidence"],
                serde_json::json!([
                    "single worker only",
                    "gate receipt is explicit",
                    "allowlist receipt is explicit",
                    "capability routing receipt is explicit",
                    "report admission receipt or blocked reason is explicit",
                ])
            );
        } else {
            assert!(
                item["required_evidence"]
                    .as_array()
                    .expect("required evidence should be listed")
                    .len()
                    >= 2
            );
        }
    }
    assert!(!stdout.contains("app-secret-value"));
    assert!(!stdout.contains("feishu-secret-value"));
    assert!(!stdout.contains("provider-secret-value"));
}

#[test]
fn live_operator_checklist_blocks_codex_and_hermes_feishu_secret_names() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-checklist.sh");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-live-operator-checklist-forbidden-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let provider_env = temp_dir.join("provider.env");
    fs::write(
        &provider_env,
        "CODEX_PPTOKEN_API_KEY=provider-secret-value\n",
    )
    .expect("provider env should write");
    let env_file = temp_dir.join("chuang-feishu.env");
    fs::write(
        &env_file,
        format!(
            "CHUANG_FEISHU_APP_ID=app-secret-value\nCHUANG_FEISHU_APP_SECRET=feishu-secret-value\nCHUANG_AGENT_WORKSPACE_ROOT={}\nCHUANG_PROVIDER_ENV_FILE={}\nHERMES_FEISHU_ENCRYPT_KEY=legacy-hermes-secret\nCODEX_FEISHU_BOT_ID=legacy-codex-bot\n",
            manifest_dir.display(),
            provider_env.display()
        ),
    )
    .expect("operator env should write");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .env("CHUANG_LIVE_OPERATOR_ENV_FILE", &env_file)
        .current_dir(&manifest_dir)
        .output()
        .expect("checklist should execute");
    assert!(
        !output.status.success(),
        "forbidden Feishu credential names should block checklist"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).expect("checklist output should be json");
    assert_eq!(data["ok"], false);
    assert_eq!(data["status"], "blocked");
    assert_eq!(
        data["checks"]["feishu_env_file"]["forbidden_credential_env_names_in_file"],
        serde_json::json!(["HERMES_FEISHU_ENCRYPT_KEY", "CODEX_FEISHU_BOT_ID"])
    );
    assert!(data["blockers"]
        .as_array()
        .expect("blockers should be array")
        .iter()
        .any(|blocker| blocker == "forbidden_codex_or_hermes_feishu_names_in_env_file"));
    assert!(!stdout.contains("legacy-hermes-secret"));
    assert!(!stdout.contains("legacy-codex-bot"));
    assert!(!stdout.contains("feishu-secret-value"));
    assert!(!stdout.contains("provider-secret-value"));
}

#[test]
fn candidate_verify_wrapper_includes_live_runner_readiness_view_before_operator_summary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-candidate-verify.sh"))
        .expect("candidate verify wrapper should be readable");

    let live_gaps_check = wrapper
        .find("[candidate-verify] live gaps check")
        .expect("candidate verify should keep live gaps check");
    let live_runner_readiness_view = wrapper
        .find("[candidate-verify] live runner readiness view")
        .expect("candidate verify should run live runner readiness view");
    let operator_checklist = wrapper
        .find("[candidate-verify] live operator checklist readonly summary")
        .expect("candidate verify should run live operator checklist readonly summary");
    let goal_run_status = wrapper
        .find("[candidate-verify] goal run status readonly summary")
        .expect("candidate verify should run goal run status readonly summary");

    assert!(live_gaps_check < live_runner_readiness_view);
    assert!(live_runner_readiness_view < operator_checklist);
    assert!(operator_checklist < goal_run_status);
    assert!(wrapper.contains("scripts/chuang-live-runner-readiness-view.sh --json"));
    assert!(wrapper.contains("scripts/chuang-live-operator-checklist.sh --json"));
    assert!(wrapper.contains("scripts/chuang-goal-run-status.sh --json"));
    assert!(wrapper.contains("runtime_surface = data[\"runtime_report_surface\"]"));
    assert!(wrapper.contains("policy_tool_status = data[\"policy_tool_status\"]"));
    assert!(wrapper.contains("runtime_surface[\"artifact_count\"] == 11"));
    assert!(wrapper.contains("runtime_surface[\"observability_field_count\"] == 26"));
    assert!(wrapper.contains("policy_tool_status[\"active_permission_profile\"] == \"local_ga\""));
    assert!(wrapper.contains("policy_tool_status[\"ga_tool_descriptor_mapped_count\"] == 9"));
    assert!(wrapper.contains("file_write[\"external_commit\"] is False"));
    assert!(wrapper.contains("file_write[\"requires_approval\"] is False"));
    assert!(wrapper.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(wrapper.contains("runtime_meta.subagent_children_summary_json"));
    assert!(wrapper.contains("runtime_event_approval_requested_count"));
    assert!(wrapper.contains("runtime_event_elicitation_requested_count"));
    assert!(wrapper.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(wrapper.contains("tool_protocol_error_count"));
    assert!(wrapper.contains("runtime_response.trace"));
    assert!(wrapper.contains("runtime_response_trace_chars"));
    assert!(wrapper.contains("goal_handoff_report_admission_reason_codes"));
    assert!(wrapper.contains("goal_handoff_report_admission_refs"));
    assert!(wrapper.contains("subagent_children_report_admission_ref_count"));
    assert!(wrapper.contains("subagent_children_report_admission_refs"));
    assert!(wrapper.contains("subagent_children_report_reason_codes"));
    assert!(wrapper.contains("candidate_runtime_report_surface_artifacts="));
    assert!(wrapper.contains("candidate_runtime_report_surface_observability_fields="));
    assert!(wrapper.contains("candidate_policy_tool_status_ga_tool_descriptors="));
    assert!(wrapper.contains("candidate_live_runner_readiness_view_state="));
    assert!(wrapper.contains("candidate_live_runner_readiness_view_ready_for_live="));
    assert!(wrapper.contains("candidate_live_runner_readiness_view_blocked_reason="));
    assert!(wrapper.contains("candidate_goal_run_status_interactive_state="));
    assert!(wrapper.contains("candidate_goal_run_status_activity_hint="));
}

#[test]
fn third_test_smoke_wrapper_includes_live_runner_readiness_view_before_operator_summary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-third-test-smoke.sh"))
        .expect("third test smoke wrapper should be readable");

    let live_gaps_check = wrapper
        .find("[third-test] live gaps matrix")
        .expect("third test smoke should keep live gaps matrix");
    let live_runner_readiness_view = wrapper
        .find("[third-test] live runner readiness view")
        .expect("third test smoke should run live runner readiness view");
    let operator_checklist = wrapper
        .find("[third-test] live operator checklist readonly summary")
        .expect("third test smoke should run live operator checklist readonly summary");
    let goal_run_status = wrapper
        .find("[third-test] goal run status readonly summary")
        .expect("third test smoke should run goal run status readonly summary");

    assert!(live_gaps_check < live_runner_readiness_view);
    assert!(live_runner_readiness_view < operator_checklist);
    assert!(operator_checklist < goal_run_status);
    assert!(wrapper.contains("scripts/chuang-live-runner-readiness-view.sh --json"));
    assert!(wrapper.contains("scripts/chuang-live-operator-checklist.sh --json"));
    assert!(wrapper.contains("scripts/chuang-goal-run-status.sh --json"));
    assert!(wrapper.contains("runtime_surface = data[\"runtime_report_surface\"]"));
    assert!(wrapper.contains("policy_tool_status = data[\"policy_tool_status\"]"));
    assert!(wrapper.contains("runtime_surface[\"artifact_count\"] == 11"));
    assert!(wrapper.contains("runtime_surface[\"observability_field_count\"] == 26"));
    assert!(wrapper.contains("policy_tool_status[\"active_permission_profile\"] == \"local_ga\""));
    assert!(wrapper.contains("policy_tool_status[\"ga_tool_descriptor_mapped_count\"] == 9"));
    assert!(wrapper.contains("file_write[\"external_commit\"] is False"));
    assert!(wrapper.contains("file_write[\"requires_approval\"] is False"));
    assert!(wrapper.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(wrapper.contains("runtime_meta.subagent_children_summary_json"));
    assert!(wrapper.contains("runtime_event_approval_requested_count"));
    assert!(wrapper.contains("runtime_event_elicitation_requested_count"));
    assert!(wrapper.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(wrapper.contains("tool_protocol_error_count"));
    assert!(wrapper.contains("runtime_response.trace"));
    assert!(wrapper.contains("runtime_response_trace_chars"));
    assert!(wrapper.contains("goal_handoff_report_admission_reason_codes"));
    assert!(wrapper.contains("goal_handoff_report_admission_refs"));
    assert!(wrapper.contains("subagent_children_report_admission_ref_count"));
    assert!(wrapper.contains("subagent_children_report_admission_refs"));
    assert!(wrapper.contains("subagent_children_report_reason_codes"));
    assert!(wrapper.contains("live_runner_readiness_view_runtime_report_surface_artifacts="));
    assert!(
        wrapper.contains("live_runner_readiness_view_runtime_report_surface_observability_fields=")
    );
    assert!(wrapper.contains("live_runner_readiness_view_policy_tool_status_ga_tool_descriptors="));
    assert!(wrapper.contains("live_runner_readiness_view_state="));
    assert!(wrapper.contains("live_runner_readiness_view_ready_for_live="));
    assert!(wrapper.contains("live_runner_readiness_view_blocked_reason="));
}
