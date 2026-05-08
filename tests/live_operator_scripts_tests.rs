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
    assert!(script.contains("\"connects_real_feishu\": False"));
    assert!(script.contains("\"reads_secret_values\": False"));
    assert!(script.contains("\"starts_services\": False"));
    assert!(script.contains("\"stops_services\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"deletes_files\": False"));
    assert!(script.contains("\"reuses_codex_or_hermes_credentials\": False"));
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
    assert_eq!(data["operator"], "operator-x");
    assert_eq!(data["env_file"], env_file.display().to_string());
    assert_eq!(data["workspace_root"], manifest_dir.display().to_string());
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
    assert_eq!(data["boundaries"]["connects_real_feishu"], false);
    assert_eq!(data["boundaries"]["reads_secret_values"], false);
    assert_eq!(data["boundaries"]["starts_services"], false);
    assert_eq!(data["boundaries"]["stops_services"], false);
    assert_eq!(data["boundaries"]["modifies_repo"], false);
    assert_eq!(data["boundaries"]["deletes_files"], false);
    assert_eq!(
        data["boundaries"]["reuses_codex_or_hermes_credentials"],
        false
    );
    assert!(!stdout.contains("CHUANG_LIVE_PLACEHOLDER=1"));
}

#[test]
fn provider_readiness_check_reports_local_only_block_without_secret_values() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-provider-readiness-check.sh");
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
    assert_eq!(data["source_command"], "cargo run --quiet -- status --json");
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
    assert!(live_runner_rehearsal < provider_readiness);
    assert!(provider_readiness < provider_block);
    assert!(provider_block < provider_error);
    assert!(provider_error < marker);
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
