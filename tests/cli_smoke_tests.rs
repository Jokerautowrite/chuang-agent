use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn write_fake_config(root: &std::path::Path) -> PathBuf {
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "db_path = \"{}\"\nidentity_memory_root = \"{}\"\nprovider = \"fake\"\nprovider_id = \"fake-runtime\"\nmodel = \"stub-responder\"\n",
            root.join("memory.db").display(),
            root.join("identity").display()
        ),
    )
    .expect("fake config should be written");
    config_path
}

fn script_contains_goal_run_status_no_tail_state(manifest_dir: &std::path::Path) -> bool {
    fs::read_to_string(manifest_dir.join("scripts/chuang-goal-run-status.sh"))
        .map(|script| {
            script.contains("session_present_no_tail")
                && script
                    .contains("tmux session and panes are present but no pane tail was captured")
        })
        .unwrap_or(false)
}

#[test]
fn second_test_smoke_wrapper_reuses_safe_mvp_smoke() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-second-test-smoke.sh"))
        .expect("second-test smoke wrapper should be readable");
    let mvp_smoke = fs::read_to_string(manifest_dir.join("scripts/chuang-mvp-smoke.sh"))
        .expect("mvp smoke should be readable");
    let feishu_turn_summary_smoke =
        fs::read_to_string(manifest_dir.join("scripts/chuang-feishu-turn-summary-smoke.js"))
            .expect("feishu turn summary smoke should be readable");

    assert!(wrapper.contains("CHUANG_SMOKE_NAME=second_test"));
    assert!(wrapper.contains("scripts/chuang-mvp-smoke.sh"));
    assert!(mvp_smoke.contains("smoke_name=\"${CHUANG_SMOKE_NAME:-mvp}\""));
    assert!(mvp_smoke.contains("printf '%s_smoke_ok work_dir=%s\\n' \"$smoke_name\" \"$work_dir\""));
    assert!(mvp_smoke.contains("assert data[\"approval_ticket_count\"] == 1"));
    assert!(mvp_smoke.contains("data[\"goal_run\"][\"plan_exists\"] is True"));
    assert!(mvp_smoke.contains("def assert_live_readiness(live_readiness):"));
    assert!(mvp_smoke.contains("assert_live_readiness(data[\"live_readiness\"])"));
    assert!(mvp_smoke.contains("assert_live_readiness(status[\"live_readiness\"])"));
    assert!(mvp_smoke.contains(
        "live_readiness[\"overall_state\"] in (\"local_ready_live_pending\", \"global_real_live_ready\")"
    ));
    assert!(mvp_smoke.contains("[smoke] channel simulate"));
    assert!(mvp_smoke.contains("live_readiness = data[\"live_readiness\"]"));
    assert!(mvp_smoke.contains(
        "live_readiness[\"real_external_acceptance_pending\"] is (not global_real_live_ready)"
    ));
    assert!(mvp_smoke.contains(
        "live_readiness[\"provider_live_request_verified_by_status\"] is global_real_live_ready"
    ));
    assert!(mvp_smoke.contains("live_readiness[\"ready_does_not_mean_live\"] is True"));
    assert!(mvp_smoke.contains("assert \"live_readiness\" not in data[\"runtime_observability\"]"));
    assert!(mvp_smoke.contains("chuang-feishu-turn-summary-smoke.js"));
    assert!(feishu_turn_summary_smoke
        .contains("live readiness local_ready_live_pending / 真实验收待完成 / ready不等于live"));
    assert!(feishu_turn_summary_smoke.contains("raw current text should stay out"));
    assert!(feishu_turn_summary_smoke.contains("!process.includes(\"raw current text\")"));
    assert!(mvp_smoke.contains("policy_tool_status = data[\"policy_tool_status\"]"));
    assert!(mvp_smoke.contains("policy_tool_status = status[\"policy_tool_status\"]"));
    assert!(mvp_smoke
        .contains("policy_tool_status[\"active_permission_profile\"] == \"full_local_workspace\""));
    assert!(mvp_smoke.contains("policy_tool_status[\"ga_tool_descriptor_mapped_count\"] == 9"));
    assert!(mvp_smoke.contains("file_write[\"external_commit\"] is False"));
    assert!(mvp_smoke.contains("file_write[\"requires_approval\"] is False"));
    assert!(mvp_smoke.contains("runtime_report_surface = data[\"runtime_report_surface\"]"));
    assert!(mvp_smoke.contains("runtime_report_surface[\"artifact_count\"] == 11"));
    assert!(mvp_smoke.contains("runtime_report_surface[\"observability_field_count\"] == 26"));
    assert!(mvp_smoke.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(mvp_smoke.contains("tool_protocol_error_count"));
    assert!(mvp_smoke.contains("runtime_response.trace"));
    assert!(mvp_smoke.contains("runtime_response_trace_chars"));
    assert!(mvp_smoke.contains("tool_unified_execution_status"));
    assert!(mvp_smoke.contains("tool_unified_execution_failure_count"));
    assert!(mvp_smoke.contains("tool_unified_execution_failure_classes"));
    assert!(mvp_smoke.contains("runtime_event_tool_started_count"));
    assert!(mvp_smoke.contains("runtime_event_tool_finished_count"));
    assert!(mvp_smoke.contains("runtime_event_approval_requested_count"));
    assert!(mvp_smoke.contains("runtime_event_approval_resolved_count"));
    assert!(mvp_smoke.contains("runtime_event_elicitation_requested_count"));
    assert!(mvp_smoke.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(mvp_smoke.contains("runtime_meta.subagent_children_summary_json"));
    assert!(mvp_smoke.contains("runtime_meta.context_compaction_summary_json"));
    assert!(mvp_smoke.contains("goal_handoff_query_summary_json"));
    assert!(mvp_smoke.contains("subagent_children_summary_json"));
    assert!(mvp_smoke.contains("goal_handoff_parent_context_handoff_count"));
    assert!(mvp_smoke.contains("goal_handoff_report_admission_ref_count"));
    assert!(mvp_smoke.contains("goal_handoff_report_admission_refs"));
    assert!(mvp_smoke.contains("goal_handoff_report_admission_reason_codes"));
    assert!(mvp_smoke.contains("subagent_children_child_count"));
    assert!(mvp_smoke.contains("subagent_children_accepted_report_count"));
    assert!(mvp_smoke.contains("subagent_children_report_admission_ref_count"));
    assert!(mvp_smoke.contains("subagent_children_report_admission_refs"));
    assert!(mvp_smoke.contains("subagent_children_report_reason_codes"));
    assert!(mvp_smoke.contains("subagent_children_missing_report_count"));
    assert!(mvp_smoke.contains("context_compaction_summary_json"));
    assert!(mvp_smoke.contains("checks_by_name[\"goal_run_readiness\"]"));
    assert!(mvp_smoke.contains("data[\"provider_readiness\"]"));
    assert!(mvp_smoke.contains("data[\"subagent_readiness\"][\"live_worker_available\"] is False"));
    assert!(mvp_smoke.contains(
        "data[\"subagent_readiness\"][\"worker_runtime_state\"] == \"local_contract_only\""
    ));
    assert!(mvp_smoke.contains("assert data[\"approval_tickets\"][0][\"local_only\"] is True"));
    assert!(mvp_smoke.contains(
        "assert data[\"approval_tickets\"][0][\"approval_receipt\"][\"approved\"] is False"
    ));
    assert!(mvp_smoke.contains(
        "assert data[\"approval_tickets\"][0][\"approval_receipt\"][\"approval_source\"] == \"pending_operator_approval\""
    ));
    assert!(!wrapper.contains("rm "));
    assert!(!wrapper.contains("systemctl"));
}

#[test]
fn complete_local_smoke_wrapper_reuses_safe_local_acceptance() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-complete-local-smoke.sh"))
        .expect("complete local smoke wrapper should be readable");

    assert!(wrapper.contains("scripts/chuang-second-test-smoke.sh"));
    assert!(wrapper.contains("scripts/chuang-goal-watchdog.sh"));
    assert!(wrapper.contains("--once"));
    assert!(wrapper.contains("scripts/chuang-goal-mode-smoke.sh"));
    assert!(wrapper.contains("scripts/chuang-goal-mode-negative-smoke.sh"));
    assert!(wrapper.contains("chuang-feishu-command-smoke.js"));
    assert!(wrapper.contains("chuang-feishu-turn-summary-smoke.js"));
    assert!(wrapper.contains("chuang-feishu-session-smoke.js"));
    assert!(wrapper.contains("chuang-feishu-rich-message-smoke.js"));
    assert!(
        wrapper.contains("app-server health --workspace-root \"$work_dir\" --diagnostic --json")
    );
    assert!(wrapper.contains("console snapshot --config \"$config_path\" --json"));
    assert!(wrapper.contains("complete_local_smoke_ok"));
    assert!(wrapper.contains("transport = \"stub\""));
    assert!(wrapper.contains("CHUANG_AGENT_COMPLETE_SMOKE_API_KEY=\"test-key\""));
    assert!(wrapper.contains("connects_real_external_services"));
    assert!(wrapper.contains("verifies_real_external_services"));
    assert!(wrapper.contains("goal_run\"][\"plan_exists\"] is True"));
    assert!(wrapper.contains("def assert_live_readiness(live_readiness):"));
    assert!(wrapper.contains("assert_live_readiness(data[\"live_readiness\"])"));
    assert!(wrapper.contains("assert_live_readiness(data[\"status\"][\"live_readiness\"])"));
    assert!(wrapper.contains("assert_live_readiness(status[\"live_readiness\"])"));
    assert!(wrapper.contains(
        "live_readiness[\"overall_state\"] in (\"local_ready_live_pending\", \"global_real_live_ready\")"
    ));
    assert!(wrapper.contains("checks_by_name[\"goal_run_readiness\"]"));
    assert!(wrapper.contains("provider_readiness"));
    assert!(wrapper.contains("provider_id\"] == \"complete-local-openai\""));
    assert!(wrapper.contains("live_worker_available"));
    assert!(wrapper.contains("worker_runtime_state\"] == \"local_contract_only\""));
    assert!(wrapper.contains("policy_tool_status = data[\"policy_tool_status\"]"));
    assert!(wrapper.contains("policy_tool_status = data[\"status\"][\"policy_tool_status\"]"));
    assert!(wrapper.contains("policy_tool_status = status[\"policy_tool_status\"]"));
    assert!(wrapper
        .contains("policy_tool_status[\"active_permission_profile\"] == \"full_local_workspace\""));
    assert!(wrapper.contains("policy_tool_status[\"ga_tool_descriptor_mapped_count\"] == 9"));
    assert!(wrapper.contains("file_write[\"external_commit\"] is False"));
    assert!(wrapper.contains("file_write[\"requires_approval\"] is False"));
    assert!(wrapper.contains("runtime_report_surface = data[\"runtime_report_surface\"]"));
    assert!(
        wrapper.contains("runtime_report_surface = data[\"status\"][\"runtime_report_surface\"]")
    );
    assert!(wrapper.contains("runtime_report_surface = status[\"runtime_report_surface\"]"));
    assert!(wrapper.contains("runtime_report_surface[\"artifact_count\"] == 11"));
    assert!(wrapper.contains("runtime_report_surface[\"observability_field_count\"] == 26"));
    assert!(wrapper.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(wrapper.contains("tool_protocol_error_count"));
    assert!(wrapper.contains("runtime_response.trace"));
    assert!(wrapper.contains("runtime_response_trace_chars"));
    assert!(wrapper.contains("tool_unified_execution_status"));
    assert!(wrapper.contains("tool_unified_execution_failure_count"));
    assert!(wrapper.contains("tool_unified_execution_failure_classes"));
    assert!(wrapper.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(wrapper.contains("runtime_meta.subagent_children_summary_json"));
    assert!(wrapper.contains("runtime_meta.context_compaction_summary_json"));
    assert!(wrapper.contains("runtime_event_approval_requested_count"));
    assert!(wrapper.contains("goal_handoff_report_admission_ref_count"));
    assert!(wrapper.contains("goal_handoff_parent_context_handoff_count"));
    assert!(wrapper.contains("goal_handoff_report_admission_refs"));
    assert!(wrapper.contains("goal_handoff_report_admission_reason_codes"));
    assert!(wrapper.contains("subagent_children_report_admission_ref_count"));
    assert!(wrapper.contains("subagent_children_child_count"));
    assert!(wrapper.contains("subagent_children_accepted_report_count"));
    assert!(wrapper.contains("subagent_children_report_admission_refs"));
    assert!(wrapper.contains("subagent_children_report_reason_codes"));
    assert!(wrapper.contains("subagent_children_missing_report_count"));
    assert!(wrapper.contains("context_compaction_summary_json"));
    assert!(wrapper.contains("for gate in data[\"live_adapter_gates\"][\"gates\"]"));
    assert!(!wrapper.contains("rm "));
    assert!(!wrapper.contains("systemctl"));
    assert!(!wrapper.contains("https://liusuapi.top"));
    assert!(!wrapper.contains(".codex-im/.env"));
    assert!(!wrapper.contains("hermes-gateway"));
}

#[test]
fn provider_readiness_check_is_status_only_and_secret_safe() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script =
        fs::read_to_string(manifest_dir.join("scripts/chuang-provider-readiness-check.sh"))
            .expect("provider readiness check should be readable");

    assert!(script.contains("cargo run --quiet -- status"));
    assert!(script.contains("--json"));
    assert!(script.contains("provider_readiness"));
    assert!(script.contains("provider_kind"));
    assert!(script.contains("transport"));
    assert!(script.contains("request_timeout_ms"));
    assert!(script.contains("api_key_state"));
    assert!(script.contains("sanitized_api_key_state"));
    assert!(script.contains("\"<set>\""));
    assert!(script.contains("\"<missing>\""));
    assert!(script.contains("provider_api_key_env_missing"));
    assert!(script.contains("next_action"));
    assert!(script.contains("connects_real_provider"));
    assert!(script.contains("\"connects_real_provider\": False"));
    assert!(script.contains("prints_secret_values"));
    assert!(script.contains("\"prints_secret_values\": False"));
    assert!(!script.contains("run --input"));
    assert!(!script.contains("app-server"));
    assert!(!script.contains("doctor"));
    assert!(!script.contains("curl "));
    assert!(!script.contains("wget "));
    assert!(!script.contains("FEISHU_"));
    assert!(!script.contains("HERMES_"));
    assert!(!script.contains(".codex-im/.env"));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("\nrm "));
    assert!(!script.contains(" rm -"));
    assert!(!script.contains("systemctl"));
}

#[test]
fn feishu_bridge_script_discovers_desktop_env_without_host_specific_display() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = fs::read_to_string(manifest_dir.join("scripts/chuang-feishu-bridge.sh"))
        .expect("feishu bridge script should be readable");
    let env_example =
        fs::read_to_string(manifest_dir.join("ops/systemd/chuang-feishu-bridge.env.example"))
            .expect("feishu bridge env example should be readable");
    let service_example =
        fs::read_to_string(manifest_dir.join("ops/systemd/chuang-feishu-bridge.service.example"))
            .expect("feishu bridge service example should be readable");

    assert!(script.contains("detect_desktop_env()"));
    assert!(script.contains("uid=\"$(id -u)\""));
    assert!(script.contains("XDG_RUNTIME_DIR"));
    assert!(script.contains("XAUTHORITY"));
    assert!(script.contains("/tmp/.X11-unix/X*"));
    assert!(script.contains("WAYLAND_DISPLAY"));
    assert!(script.contains("export DISPLAY=\":${socket##*/X}\""));
    assert!(script.contains("export XAUTHORITY=\"$candidate\""));
    assert!(script.contains("export XDG_RUNTIME_DIR=\"$runtime_dir\""));
    assert!(script.contains("ROOT=\"${CHUANG_AGENT_ROOT:-$ROOT}\""));
    assert!(
        script.contains("PROVIDER_ENV_FILE=\"${CHUANG_PROVIDER_ENV_FILE:-$PROVIDER_ENV_FILE}\"")
    );
    assert!(script
        .contains("FEISHU_SDK_MODULES=\"${CHUANG_FEISHU_SDK_NODE_MODULES:-$FEISHU_SDK_MODULES}\""));
    assert!(script.contains("CHUANG_REAL_ACTUATOR_ENABLE=\"${CHUANG_REAL_ACTUATOR_ENABLE:-1}\""));
    assert!(!script.contains("DISPLAY=:0"));
    assert!(!script.contains("/run/user/1000"));
    assert!(!script.contains("XAUTHORITY=/run/user"));
    assert!(env_example.contains("Desktop env is auto-detected"));
    assert!(env_example.contains("/absolute/path/to/chuang-agent"));
    assert!(service_example.contains("/absolute/path/to/chuang-agent"));
    assert!(!env_example.contains("/home/user"));
    assert!(!service_example.contains("/home/user"));
}

#[test]
fn feishu_bridge_script_rejects_forbidden_provider_env_on_direct_startup() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-feishu-bridge-direct-startup-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("direct startup test root should be created");

    let bridge_env = root.join("chuang-feishu-bridge.env");
    let provider_env = root.join("provider.env");
    fs::write(
        &bridge_env,
        format!(
            "CHUANG_FEISHU_APP_ID=cli_a_chuang_startup\nCHUANG_FEISHU_APP_SECRET=bridge-secret-value\nCHUANG_AGENT_WORKSPACE_ROOT={}\nCHUANG_PROVIDER_ENV_FILE={}\n",
            manifest_dir.display(),
            provider_env.display()
        ),
    )
    .expect("bridge env should write");
    fs::write(
        &provider_env,
        "OPENAI_API_KEY=provider-secret-value\nCHUANG_FEISHU_APP_ID=forbidden-feishu-app\nHERMES_FEISHU_BOT_ID=legacy-hermes-bot\n",
    )
    .expect("provider env should write");

    let output = Command::new("node")
        .arg("-e")
        .arg(
            "try { require('./scripts/chuang-feishu-bridge.js'); console.log('unexpected_bridge_startup_success'); } catch (error) { console.error(error.message); process.exit(1); }",
        )
        .env(
            "NODE_PATH",
            "/home/user/.codex/codex-feishu-bridge/node_modules",
        )
        .env("CHUANG_FEISHU_ENV_FILE", &bridge_env)
        .env("CHUANG_PROVIDER_ENV_FILE", &provider_env)
        .current_dir(&manifest_dir)
        .output()
        .expect("direct bridge startup should execute");

    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("provider-secret-value"));
    assert!(!stderr.contains("provider-secret-value"));
    assert!(!stdout.contains("bridge-secret-value"));
    assert!(!stderr.contains("bridge-secret-value"));
    assert!(!stdout.contains("forbidden-feishu-app"));
    assert!(!stderr.contains("forbidden-feishu-app"));
    assert!(!stdout.contains("legacy-hermes-bot"));
    assert!(!stderr.contains("legacy-hermes-bot"));
    assert!(stderr.contains("Provider env file contains forbidden Feishu config names"));
    assert!(stderr.contains("CHUANG_FEISHU_APP_ID"));
    assert!(stderr.contains("HERMES_FEISHU_BOT_ID"));
}

#[test]
fn live_runner_rehearsal_smoke_uses_disabled_codex_runner_and_report_admission() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-runner-rehearsal-smoke.sh");
    let script =
        fs::read_to_string(&script_path).expect("live runner rehearsal smoke should be readable");

    assert!(script.contains("subagent live-preflight"));
    assert!(script.contains("--runner-command scripts/chuang-codex-runner.py"));
    assert!(script.contains("--allow-runner-command scripts/chuang-codex-runner.py"));
    assert!(script.contains("--requires-capability rehearsal"));
    assert!(script.contains("--capability rehearsal"));
    assert!(script.contains("starts_external_worker"));
    assert!(script.contains("assert data[\"starts_external_worker\"] is False"));
    assert!(script.contains("assert data[\"readonly\"] is True"));
    assert!(script.contains("assert data[\"ready_for_live\"] is False"));
    assert!(script.contains("assert data[\"gate\"][\"enabled\"] is False"));
    assert!(script
        .contains("assert data[\"gate\"][\"required_env\"] == \"CHUANG_CODEX_RUNNER_ENABLE\""));
    assert!(script.contains("assert data[\"gate\"][\"audit_label\"] == \"subagent.runner.live\""));
    assert!(script.contains("assert data[\"runner_allowlist\"][\"exact_match_required\"] is True"));
    assert!(script.contains("assert data[\"report_admission\"][\"required\"] is True"));
    assert!(script.contains(
        "assert data[\"report_admission\"][\"evidence\"].startswith(\"run-once, run-loop, report, and collect\")"
    ));
    assert!(script.contains("assert not queue_root.exists()"));
    assert!(script.contains("subagent dispatch"));
    assert!(script.contains("--subagent-queue-root \"$queue_root\""));
    assert!(script.contains("subagent list"));
    assert!(script.contains("assert data[\"report_count\"] == 0"));
    assert!(script.contains("list_after_json"));
    assert!(script.contains("queue evidence remains visible after runner"));
    assert!(script.contains("assert listing[\"report_count\"] == 1"));
    assert!(script.contains("assert item[\"has_report\"] is True"));
    assert!(script.contains("assert item[\"is_claimed\"] is True"));
    assert!(script.contains("subagent run-once"));
    assert!(script.contains("--runner command"));
    assert!(script.contains("--approve-exec"));
    assert!(script.contains("CHUANG_CODEX_RUNNER_WORKSPACE=\"$runner_workspace\""));
    assert!(script.contains("admission = run_once[\"report_admission\"]"));
    assert!(script.contains("assert admission[\"status\"] == \"Accepted\""));
    assert!(script.contains("assert admission[\"reason_code\"] == \"report_validated\""));
    assert!(
        script.contains("assert admission[\"controller_agent_id\"] == \"cli-subagent-controller\"")
    );
    assert!(script.contains("assert admission[\"task_id\"] == dispatch[\"task_id\"]"));
    assert!(script.contains("assert admission[\"agent_id\"] == dispatch[\"agent_id\"]"));
    assert!(script.contains("assert admission[\"report_id\"].startswith(\"report-\")"));
    assert!(script.contains("assert admission[\"decided_at\"].endswith(\"Z\")"));
    assert!(script.contains("subagent report"));
    assert!(script.contains("subagent collect"));
    assert!(script.contains("data.get(\"report_available\") or data.get(\"available\")"));
    assert!(script.contains("assert admission[\"report_id\"] == report[\"report_id\"]"));
    assert!(script.contains("assert admission[\"task_id\"] == report[\"task_id\"]"));
    assert!(script.contains("assert admission[\"agent_id\"] == report[\"agent_id\"]"));
    assert!(script.contains("assert report[\"schema_version\"] == \"1.0\""));
    assert!(script.contains("assert report[\"status\"] == \"Failed\""));
    assert!(script.contains("codex runner disabled by default"));
    assert!(script.contains("approval_receipt=cli_flag:--approve-exec"));
    assert!(script.contains("live_runner_rehearsal_smoke_ok"));
    assert!(script.contains("unset CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(!script.contains("CHUANG_CODEX_RUNNER_ENABLE=1"));
    assert!(!script.contains("codex exec"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("\nrm "));
    assert!(!script.contains(" rm -"));
    assert!(!script.contains(".codex-im/.env"));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
}

#[test]
fn final_verify_wrapper_requires_clean_tree_and_candidate_verify() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = fs::read_to_string(manifest_dir.join("scripts/chuang-final-verify.sh"))
        .expect("final verify wrapper should be readable");
    let candidate_wrapper =
        fs::read_to_string(manifest_dir.join("scripts/chuang-candidate-verify.sh"))
            .expect("candidate verify wrapper should be readable");

    let clean_tree_check = wrapper
        .find("git status --short")
        .expect("final verify should check for a clean tree first");
    let candidate_verify = wrapper
        .find("sh scripts/chuang-candidate-verify.sh")
        .expect("final verify should run candidate verify");
    let final_diff_check = wrapper
        .find("git diff --check")
        .expect("final verify should run a final diff check");

    assert!(clean_tree_check < candidate_verify);
    assert!(candidate_verify < final_diff_check);
    assert!(wrapper.contains("working tree must be clean before final verify"));
    assert!(wrapper.contains("exit 2"));
    assert!(wrapper.contains("chuang_final_verify_ok"));
    assert!(candidate_wrapper.contains("runtime_surface = data[\"runtime_report_surface\"]"));
    assert!(candidate_wrapper.contains("live_readiness = data[\"live_readiness\"]"));
    assert!(candidate_wrapper.contains("runtime_surface[\"artifact_count\"] == 11"));
    assert!(candidate_wrapper.contains("runtime_surface[\"observability_field_count\"] == 26"));
    assert!(candidate_wrapper.contains("runtime_meta.tool_protocol_errors_json"));
    assert!(candidate_wrapper.contains("tool_protocol_error_count"));
    assert!(candidate_wrapper.contains("runtime_response.trace"));
    assert!(candidate_wrapper.contains("tool_unified_execution_status"));
    assert!(candidate_wrapper.contains("tool_unified_execution_failure_count"));
    assert!(candidate_wrapper.contains("tool_unified_execution_failure_classes"));
    assert!(candidate_wrapper.contains(
        "live_readiness[\"overall_state\"] in (\"local_ready_live_pending\", \"global_real_live_ready\")"
    ));
    assert!(candidate_wrapper.contains("candidate_live_readiness_state="));
    assert!(candidate_wrapper.contains("runtime_meta.goal_handoff_query_summary_json"));
    assert!(candidate_wrapper.contains("runtime_meta.subagent_children_summary_json"));
    assert!(candidate_wrapper.contains("goal_handoff_report_admission_reason_codes"));
    assert!(candidate_wrapper.contains("goal_handoff_parent_context_handoff_count"));
    assert!(candidate_wrapper.contains("goal_handoff_report_admission_ref_count"));
    assert!(candidate_wrapper.contains("goal_handoff_report_admission_refs"));
    assert!(candidate_wrapper.contains("subagent_children_report_admission_ref_count"));
    assert!(candidate_wrapper.contains("subagent_children_child_count"));
    assert!(candidate_wrapper.contains("subagent_children_accepted_report_count"));
    assert!(candidate_wrapper.contains("subagent_children_missing_report_count"));
    assert!(candidate_wrapper.contains("subagent_children_report_admission_refs"));
    assert!(candidate_wrapper.contains("subagent_children_report_reason_codes"));
    assert!(!wrapper.contains("rm "));
    assert!(!wrapper.contains("reset"));
    assert!(!wrapper.contains("git checkout"));
    assert!(!wrapper.contains("systemctl"));
}

#[test]
fn candidate_verify_wrapper_sequences_dirty_tree_friendly_candidate_gates() {
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
    let marker = wrapper
        .find("chuang_candidate_verify_ok")
        .expect("candidate verify should print a stable success marker");

    assert!(complete_local_smoke < live_runner_rehearsal);
    assert!(live_runner_rehearsal < live_gaps_check);
    assert!(live_gaps_check < live_runner_readiness_view);
    assert!(live_runner_readiness_view < operator_checklist);
    assert!(operator_checklist < goal_run_status);
    assert!(goal_run_status < provider_readiness);
    assert!(provider_readiness < marker);
    assert!(wrapper.contains("[candidate-verify] live gaps check"));
    assert!(wrapper.contains("[candidate-verify] live runner readiness view"));
    assert!(wrapper.contains("scripts/chuang-live-runner-readiness-view.sh --json"));
    assert!(wrapper.contains("scripts/chuang-live-operator-checklist.sh --json"));
    assert!(wrapper.contains("scripts/chuang-goal-run-status.sh --json"));
    assert!(wrapper.contains("cannot_mark_complete_from_readonly_checklist"));
    assert!(wrapper.contains("candidate_live_operator_real_live_acceptance"));
    assert!(wrapper.contains("candidate_goal_run_status_overall"));
    assert!(wrapper.contains("candidate_goal_run_status_interactive_state"));
    assert!(wrapper.contains("candidate_goal_run_status_activity_hint"));
    assert!(wrapper.contains(r#"assert isinstance(data["interactive_state"], str)"#));
    assert!(wrapper.contains(r#"assert isinstance(data["activity_hint"], str)"#));
    assert!(script_contains_goal_run_status_no_tail_state(&manifest_dir));
    assert!(wrapper.contains(r#"project_goal_run = data["project_goal_run"]"#));
    assert!(wrapper.contains("candidate_project_goal_run_checkpoint_count="));
    assert!(wrapper.contains("candidate_project_goal_run_checkpoint_log_complete="));
    assert!(wrapper.contains("candidate_project_goal_run_last_checkpoint="));
    assert!(wrapper.contains("candidate_project_goal_run_last_checkpoint_created_at="));
    assert!(wrapper.contains("candidate_project_goal_run_last_completed_worker_count="));
    assert!(wrapper.contains("candidate_project_goal_run_last_validation_note_count="));
    assert!(wrapper.contains("does_not_call_provider"));
    assert!(wrapper.contains("does_not_read_provider_readiness"));
    assert!(wrapper.contains("connects_real_feishu\"] is False"));
    assert!(wrapper.contains("connects_real_provider\"] is False"));
    assert!(wrapper.contains("performs_desktop_actions\"] is False"));
    assert!(wrapper.contains("performs_browser_actions\"] is False"));
    assert!(wrapper.contains("dispatches_tasks\"] is False"));
    assert!(wrapper.contains("starts_worker\"] is False"));
    assert!(wrapper.contains("touches_services\"] is False"));
    assert!(wrapper.contains("scripts/chuang-provider-readiness-check.sh"));
    assert!(wrapper.contains("if [ -f \"$provider_readiness_check\" ]; then"));
    assert!(wrapper.contains("if bash \"$provider_readiness_check\"; then"));
    assert!(wrapper.contains("provider readiness check reported a non-live block"));
    assert!(wrapper.contains("continuing candidate-only gate"));
    assert!(wrapper.contains("provider readiness remains covered by complete-local"));
    assert!(wrapper.contains("no real provider call is attempted"));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"gate_receipt_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"allowlist_receipt_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"capability_routing_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"report_admission_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains("assert real_live[\"services\"][2][\"required\"] == ["));
    assert!(wrapper.contains("assert service[\"manual_live_required\"] is True"));
    assert!(wrapper.contains("assert service[\"must_not_count_as_complete\"] is True"));
    assert!(wrapper.contains("unset CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(wrapper.contains("unset CHUANG_REAL_CONTROL_ENABLE"));
    assert!(wrapper.contains("unset CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(!wrapper.contains("git status --short"));
    assert!(!wrapper.contains("working tree must be clean"));
    assert!(!wrapper.contains("git diff --check"));
    assert!(!wrapper.contains("git reset"));
    assert!(!wrapper.contains("git checkout"));
    assert!(!wrapper.contains("systemctl"));
    assert!(!wrapper.contains("\nrm "));
    assert!(!wrapper.contains(" rm -"));
    assert!(!wrapper.contains(".codex-im/.env"));
    assert!(!wrapper.contains("hermes-gateway"));
    assert!(!wrapper.contains("FEISHU_"));
}

#[test]
fn live_gaps_check_reports_local_preflight_and_real_live_pending_without_live_side_effects() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = fs::read_to_string(manifest_dir.join("scripts/chuang-live-gaps-check.sh"))
        .expect("live gaps check should be readable");

    let status = script
        .find("run_chuang status --json")
        .expect("live gaps check should read status json");
    let preflight = script
        .find("run_chuang subagent live-preflight")
        .expect("live gaps check should run live-preflight");
    let matrix = script
        .find("\"matrix\"")
        .expect("live gaps check should output a matrix");

    assert!(status < preflight);
    assert!(preflight < matrix);
    assert!(script.contains("check_name\": \"live-gaps\""));
    assert!(script.contains("marker\": \"live_gaps_check_ok\""));
    assert!(script
        .contains("\"summary\": \"local_contract=ready preflight=ready_but_no_start real_live=\""));
    assert!(script.contains("\"name\": \"local_contract\""));
    assert!(script.contains("\"name\": \"preflight_ready_but_no_start\""));
    assert!(script.contains("\"name\": \"real_live\""));
    assert!(script.contains("\"state\": \"ready_but_no_start\""));
    assert!(script.contains("\"state\": \"ready\" if real_live_ready else \"pending\""));
    assert!(script.contains("ready_for_live\"] is False"));
    assert!(script.contains("starts_external_worker\"] is False"));
    assert!(script.contains("live_worker_available\"] is False"));
    assert!(script.contains("\"id\": \"live_worker_adapter_pending\""));
    assert!(script.contains("\"id\": \"live_runner_gate_disabled\""));
    assert!(script.contains("\"id\": \"manual_operator_live_receipt_missing\""));
    assert!(script.contains("\"id\": \"real_external_services_not_verified\""));
    assert!(script.contains("\"api_key_state\": provider_api_key_state"));
    assert!(script.contains("\"uses_redacted_state_only\": True"));
    assert!(script.contains("unset CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(script.contains("unset CHUANG_REAL_CONTROL_ENABLE"));
    assert!(script.contains("unset CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(script.contains("\"connects_real_feishu\": False"));
    assert!(script.contains("\"connects_real_provider\": False"));
    assert!(script.contains("\"starts_external_worker\": False"));
    assert!(script.contains("\"enables_live_gate\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"prints_secret_values\": False"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("tmux new"));
    assert!(!script.contains("codex exec"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("\nrm "));
    assert!(!script.contains(" rm -"));
    assert!(!script.contains(".codex-im/.env"));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
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
    assert!(wrapper.contains("operator_status=0"));
    assert!(wrapper.contains("operator_status=$?"));
    assert!(wrapper.contains("live_gaps_summary="));
    assert!(wrapper.contains("live_gaps_gap_count="));
    assert!(wrapper.contains("live_gaps_marker="));
    assert!(wrapper.contains("[third-test] live runner readiness view"));
    assert!(wrapper.contains("scripts/chuang-live-runner-readiness-view.sh --json"));
    assert!(wrapper.contains("data[\"check_name\"] == \"live-gaps\""));
    assert!(wrapper.contains("data[\"summary\"] in ("));
    assert!(
        wrapper.contains("\"local_contract=ready preflight=ready_but_no_start real_live=pending\"")
    );
    assert!(
        wrapper.contains("\"local_contract=ready preflight=ready_but_no_start real_live=ready\"")
    );
    assert!(wrapper.contains("matrix[\"local_contract\"][\"state\"] == \"ready\""));
    assert!(wrapper
        .contains("matrix[\"preflight_ready_but_no_start\"][\"state\"] == \"ready_but_no_start\""));
    assert!(wrapper.contains("matrix[\"real_live\"][\"state\"] in (\"pending\", \"ready\")"));
    assert!(wrapper.contains("\"live_worker_adapter_pending\" in gap_ids"));
    assert!(wrapper.contains("\"live_runner_gate_disabled\" in gap_ids"));
    assert!(wrapper.contains("\"manual_operator_live_receipt_missing\" in gap_ids"));
    assert!(wrapper.contains("\"real_external_services_not_verified\" in gap_ids"));
    assert!(wrapper.contains("[ \"$operator_status\" -ne 0 ] && [ \"$operator_status\" -ne 1 ]"));
    assert!(wrapper.contains("live_operator_checklist_status="));
    assert!(wrapper.contains("live_operator_checklist_blockers="));
    assert!(wrapper.contains("live_operator_real_live_acceptance="));
    assert!(wrapper.contains("live_operator_real_live_gap_count="));
    assert!(wrapper.contains("goal_run_status_overall="));
    assert!(wrapper.contains("goal_run_status_interactive_state="));
    assert!(wrapper.contains("goal_run_status_activity_hint="));
    assert!(wrapper.contains(r#"assert isinstance(data["interactive_state"], str)"#));
    assert!(wrapper.contains(r#"assert isinstance(data["activity_hint"], str)"#));
    assert!(script_contains_goal_run_status_no_tail_state(&manifest_dir));
    assert!(wrapper.contains(r#"project_goal_run = data["project_goal_run"]"#));
    assert!(wrapper.contains("project_goal_run_checkpoint_count="));
    assert!(wrapper.contains("project_goal_run_checkpoint_log_complete="));
    assert!(wrapper.contains("project_goal_run_last_checkpoint="));
    assert!(wrapper.contains("project_goal_run_last_checkpoint_created_at="));
    assert!(wrapper.contains("project_goal_run_last_completed_worker_count="));
    assert!(wrapper.contains("project_goal_run_last_validation_note_count="));
    assert!(wrapper.contains("does_not_call_provider"));
    assert!(wrapper.contains("does_not_read_provider_readiness"));
    assert!(wrapper.contains("third_test_candidate_smoke_ok"));
    assert!(wrapper.contains("boundaries[\"connects_real_feishu\"] is False"));
    assert!(wrapper.contains("boundaries[\"sends_feishu_messages\"] is False"));
    assert!(wrapper.contains("boundaries[\"connects_real_provider\"] is False"));
    assert!(wrapper.contains("boundaries[\"performs_desktop_actions\"] is False"));
    assert!(wrapper.contains("boundaries[\"performs_browser_actions\"] is False"));
    assert!(wrapper.contains("boundaries[\"connects_real_wiki\"] is False"));
    assert!(wrapper.contains("boundaries[\"connects_real_gbrain\"] is False"));
    assert!(wrapper.contains("real_live[\"complete\"] is False"));
    assert!(wrapper.contains("real_live[\"status\"] == \"not_verified\""));
    assert!(wrapper.contains("real_live[\"gap_count\"] == 7"));
    assert!(wrapper.contains(
        "assert data[\"service_receipts\"][2][\"evidence\"][\"capability_routing_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"gate_receipt_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"allowlist_receipt_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"capability_routing_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper.contains(
        "assert data[\"service_evidence\"][\"subagent_live_rehearsal\"][\"report_admission_ref\"] == \"<fill_after_test>\""
    ));
    assert!(wrapper
        .contains("assert data[\"real_live_acceptance\"][\"services\"][2][\"required\"] == ["));
    assert!(wrapper.contains("assert service[\"manual_live_required\"] is True"));
    assert!(wrapper.contains("assert service[\"must_not_count_as_complete\"] is True"));
    assert!(wrapper.contains(
        "[\"feishu\", \"provider\", \"subagent_live_rehearsal\", \"desktop\", \"browser\", \"wiki\", \"gbrain\"]"
    ));
    assert!(wrapper.contains("boundaries[\"starts_services\"] is False"));
    assert!(wrapper.contains("boundaries[\"touches_services\"] is False"));
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
fn goal_watchdog_once_writes_readonly_status_report() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let log_dir = std::env::temp_dir().join(format!(
        "chuang-goal-watchdog-smoke-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&log_dir).expect("watchdog log dir should be created");

    let output = Command::new("bash")
        .arg(manifest_dir.join("scripts/chuang-goal-watchdog.sh"))
        .arg("--once")
        .env("ROOT", &manifest_dir)
        .env("SESSION", format!("chuang-goal-missing-{nanos}"))
        .env("LOG_DIR", &log_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("watchdog should execute once");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = log_dir.join("latest-watchdog-report.json");
    let report = fs::read_to_string(&report_path).expect("watchdog report should be readable");
    let data: serde_json::Value =
        serde_json::from_str(&report).expect("watchdog report should be valid json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["readonly"], true);
    assert_eq!(data["project_root"], manifest_dir.display().to_string());
    assert_eq!(data["tmux_session_present"], false);
    assert_eq!(
        data["takeover"]["next_action"],
        "start_or_attach_worker_after_operator_review"
    );
    assert_eq!(data["boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["boundaries"]["modifies_repo"], false);
    assert_eq!(data["boundaries"]["restarts_worker"], false);
    assert_eq!(data["boundaries"]["touches_services"], false);
    assert!(data["git"]["status_short"].is_array());
    assert!(log_dir.join("watchdog.log").exists());
}

#[test]
fn overnight_runner_writes_structured_status_without_restart_or_cleanup() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runner = fs::read_to_string(manifest_dir.join("scripts/run-chuang-goal-overnight.sh"))
        .expect("overnight runner should be readable");

    assert!(runner.contains("STATUS_FILE=\"${STATUS_FILE:-$LOG_DIR/status.json}\""));
    assert!(runner.contains("write_status \"running\" \"start_first_iteration\""));
    assert!(runner.contains("write_status \"running\" \"invoke_codex_exec\""));
    assert!(runner.contains("write_status \"finished\" \"operator_review_status_and_logs\""));
    assert!(runner.contains("\"run_id\": os.environ[\"RUN_ID\"]"));
    assert!(runner.contains("\"iteration\": int(os.environ[\"ITERATION\"])"));
    assert!(runner.contains("\"deadline\": deadline_epoch"));
    assert!(runner.contains("\"last_iteration_exit_status\": last_status"));
    assert!(runner.contains("\"last_message_file\": os.environ[\"LAST_MESSAGE\"]"));
    assert!(runner.contains("\"jsonl_log\": os.environ[\"JSONL_LOG\"]"));
    assert!(runner.contains("\"plain_log\": os.environ[\"PLAIN_LOG\"]"));
    assert!(runner.contains("\"status\": os.environ[\"RUN_STATUS\"]"));
    assert!(runner.contains("\"next_action\": os.environ[\"NEXT_ACTION\"]"));
    assert!(runner.contains("CHUANG_OVERNIGHT_DRY_RUN"));
    assert!(runner.contains("CHUANG_OVERNIGHT_MAX_ITERATIONS"));
    assert!(runner.contains("\"restarts_codex\": False"));
    assert!(runner.contains("\"cleans_logs\": False"));
    assert!(runner.contains("\"touches_services\": False"));
    assert!(!runner.contains("systemctl"));
    assert!(!runner.contains("\nrm "));
    assert!(!runner.contains(" rm -"));
    assert!(!runner.contains("git reset"));
    assert!(!runner.contains("git checkout"));
}

#[test]
fn goal_run_status_script_reads_watchdog_and_overnight_status_without_actions() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-goal-run-status.sh");
    let script =
        fs::read_to_string(&script_path).expect("goal run status script should be readable");

    assert!(script.contains("Readonly status view for Chuang terminal goal workers"));
    assert!(script.contains("CHUANG_GOAL_WATCHDOG_REPORT_FILE"));
    assert!(script.contains("CHUANG_GOAL_RUN_ROOT"));
    assert!(script.contains("CHUANG_GOAL_OVERNIGHT_STATUS_FILE"));
    assert!(script.contains("CHUANG_AGENT_ROOT"));
    assert!(script.contains("CHUANG_PROJECT_GOAL_RUN_FILE"));
    assert!(script.contains("project_goal_run_last_checkpoint_created_at:"));
    assert!(script.contains("project_goal_run_last_completed_worker_count:"));
    assert!(script.contains("project_goal_run_last_validation_note_count:"));
    assert!(script.contains("session_present_no_tail"));
    assert!(script.contains("tmux session and panes are present but no pane tail was captured"));
    assert!(script.contains("\"dispatches_tasks\": False"));
    assert!(script.contains("\"starts_worker\": False"));
    assert!(script.contains("\"restarts_worker\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"deletes_logs\": False"));
    assert!(script.contains("\"touches_services\": False"));
    assert!(!script.contains("tmux new"));
    assert!(!script.contains("tmux send-keys"));
    assert!(!script.contains("codex exec"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-goal-run-status-smoke-{}-{nanos}",
        std::process::id()
    ));
    let watchdog_dir = root.join("watchdog");
    let run_root = root.join("runs");
    let latest_run = run_root.join("20260507-010203");
    let lexically_newer_stale_run = run_root.join("zzzz-stale-run");
    let project_goal_run = root.join("mainline-mvp.json");
    fs::create_dir_all(&watchdog_dir).expect("watchdog dir should be created");
    fs::create_dir_all(&latest_run).expect("latest run dir should be created");
    fs::create_dir_all(&lexically_newer_stale_run).expect("stale run dir should be created");

    let watchdog_report = watchdog_dir.join("latest-watchdog-report.json");
    fs::write(
        &watchdog_report,
        serde_json::json!({
            "schema_version": 1,
            "generated_at": "2026-05-07T01:02:03+08:00",
            "readonly": true,
            "project_root": manifest_dir.display().to_string(),
            "session": "chuang-goal",
            "tmux_session_present": true,
            "pane": {
                "bytes": 128,
                "panes": ["pane=%1 active=1 pid=123 current_command=codex"],
                "tail": ["Planning script inspection and testing", "Thinking about next patch"]
            },
            "codex_processes": {"count": 1, "processes": ["123 1 00:01 codex --no-alt-screen"]},
            "git": {"dirty": false, "status_short": []},
            "takeover": {"next_action": "monitor_or_attach_if_human_review_needed"},
            "boundaries": {
                "dispatches_tasks": false,
                "modifies_repo": false,
                "restarts_worker": false,
                "touches_services": false
            }
        })
        .to_string(),
    )
    .expect("watchdog report should write");
    fs::write(
        latest_run.join("summary.md"),
        "# Chuang Overnight Goal Run\n\n- run_id: 20260507-010203\n- status: running\n- iterations: 3\n",
    )
    .expect("summary should write");
    fs::write(
        latest_run.join("status.json"),
        r#"{"status":"running","iteration":3,"updated_at":"2026-01-01T00:00:00Z"}"#,
    )
    .expect("overnight status should write");
    fs::write(latest_run.join("run.log"), "iteration 3 still running\n")
        .expect("run log should write");
    fs::write(latest_run.join("last-message.md"), "continuing goal work\n")
        .expect("last message should write");
    fs::write(
        lexically_newer_stale_run.join("status.json"),
        r#"{"status":"finished","iteration":1,"updated_at":"2025-01-01T00:00:00Z"}"#,
    )
    .expect("stale overnight status should write");
    fs::write(
        &project_goal_run,
        serde_json::json!({
            "goal_spec": {"goal_id": "mainline-mvp"},
            "checkpoint_log": [
                {
                    "checkpoint_id": "checkpoint-goal-status-test",
                    "summary": "goal status checkpoint summary",
                    "created_at": "2026-05-12T12:00:00Z",
                    "completed_worker_ids": ["main-process"],
                    "validation_notes": ["goal status script test"]
                }
            ]
        })
        .to_string(),
    )
    .expect("project goal run should write");

    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_GOAL_WATCHDOG_REPORT_FILE", &watchdog_report)
        .env("CHUANG_GOAL_RUN_ROOT", &run_root)
        .env("CHUANG_PROJECT_GOAL_RUN_FILE", &project_goal_run)
        .current_dir(&manifest_dir)
        .output()
        .expect("goal run status script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status output should be json");
    assert_eq!(data["ok"], true);
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["readonly_boundaries"]["readonly"], true);
    assert_eq!(data["readonly_boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["readonly_boundaries"]["starts_worker"], false);
    assert_eq!(data["readonly_boundaries"]["restarts_worker"], false);
    assert_eq!(data["readonly_boundaries"]["modifies_repo"], false);
    assert_eq!(data["readonly_boundaries"]["deletes_logs"], false);
    assert_eq!(data["readonly_boundaries"]["touches_services"], false);
    assert_eq!(data["watchdog"]["available"], true);
    assert_eq!(data["watchdog"]["readonly"], true);
    assert_eq!(data["watchdog"]["freshness"]["available"], true);
    assert_eq!(data["watchdog"]["session"], "chuang-goal");
    assert!(data["watchdog"]["pane_tail"].is_array());
    assert_eq!(data["watchdog"]["tmux_session_present"], true);
    assert_eq!(data["watchdog"]["codex_process_count"], 1);
    assert_eq!(data["watchdog"]["git_dirty"], false);
    assert_eq!(
        data["watchdog"]["next_action"],
        "monitor_or_attach_if_human_review_needed"
    );
    assert!(data["tmux_observation"]["session"].is_string());
    assert!(
        data["tmux_observation"]["session_present"].is_boolean()
            || data["tmux_observation"]["session_present"].is_null()
    );
    assert_eq!(
        data["overnight"]["latest_run_dir"],
        latest_run.display().to_string()
    );
    assert_ne!(
        data["overnight"]["latest_run_dir"],
        lexically_newer_stale_run.display().to_string()
    );
    assert_eq!(data["overnight"]["status_json"]["available"], true);
    assert_eq!(
        data["overnight"]["status_json"]["data"]["status"],
        "running"
    );
    assert_eq!(data["overnight"]["freshness"]["available"], true);
    assert_eq!(data["overnight"]["freshness"]["stale"], true);
    assert_eq!(data["overnight"]["summary"]["fields"]["status"], "running");
    assert_eq!(data["overnight"]["summary"]["fields"]["iterations"], "3");
    assert_eq!(data["freshness"]["overnight"]["stale"], true);
    assert_eq!(data["project_goal_run"]["available"], true);
    assert_eq!(data["project_goal_run"]["goal_id"], "mainline-mvp");
    assert_eq!(data["project_goal_run"]["checkpoint_count"], 1);
    assert_eq!(data["project_goal_run"]["checkpoint_log_complete"], true);
    assert_eq!(
        data["project_goal_run"]["last_checkpoint_id"],
        "checkpoint-goal-status-test"
    );
    assert_eq!(
        data["project_goal_run"]["last_checkpoint_summary"],
        "goal status checkpoint summary"
    );
    assert_eq!(
        data["project_goal_run"]["last_checkpoint_created_at"],
        "2026-05-12T12:00:00Z"
    );
    assert_eq!(
        data["project_goal_run"]["last_completed_worker_ids"],
        serde_json::json!(["main-process"])
    );
    assert_eq!(
        data["project_goal_run"]["last_validation_notes"],
        serde_json::json!(["goal status script test"])
    );
    assert_eq!(data["interactive_state"], "working");
    assert!(data["activity_hint"]
        .as_str()
        .unwrap_or("")
        .contains("actively"));
    assert_eq!(data["overall_status"], "interactive_active_overnight_stale");
}

#[test]
fn live_operator_checklist_reports_redacted_manual_live_steps() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-checklist.sh");
    let script =
        fs::read_to_string(&script_path).expect("live operator checklist should be readable");

    assert!(script.contains("Readonly operator checklist for the first manual Chuang live check"));
    assert!(script.contains("CHUANG_LIVE_OPERATOR_ENV_FILE"));
    assert!(script.contains("\"connects_real_feishu\": False"));
    assert!(script.contains("\"sends_feishu_messages\": False"));
    assert!(script.contains("\"connects_real_provider\": False"));
    assert!(script.contains("\"performs_desktop_actions\": False"));
    assert!(script.contains("\"performs_browser_actions\": False"));
    assert!(script.contains("\"connects_real_wiki\": False"));
    assert!(script.contains("\"connects_real_gbrain\": False"));
    assert!(script.contains("\"starts_services\": False"));
    assert!(script.contains("\"starts_workers\": False"));
    assert!(script.contains("\"dispatches_tasks\": False"));
    assert!(script.contains("\"touches_services\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("\"prints_secret_values\": False"));
    assert!(script.contains("bridge_tools_command"));
    assert!(script.contains("bridge_capabilities_command"));
    assert!(script.contains("send /tools to the Chuang Feishu bot"));
    assert!(script.contains("send /capabilities to the Chuang Feishu bot"));
    assert!(script.contains("mounted_feishu_capabilities"));
    assert!(script.contains("provider_readiness_evidence"));
    assert!(script.contains("scripts/chuang-provider-readiness-check.sh"));
    assert!(script.contains("source_status_surface"));
    assert!(script.contains("cargo run --quiet -- status --json"));
    assert!(script.contains("local_readonly_evidence"));
    assert!(script.contains("scripts/chuang-live-readonly-preflight.sh"));
    assert!(script.contains("real_live_acceptance"));
    assert!(script.contains("completion_state"));
    assert!(script.contains("not_verified"));
    assert!(script.contains("must_not_count_as_complete"));
    assert!(script.contains("wiki_source_contract"));
    assert!(script.contains("gbrain_source_contract"));
    assert!(script.contains("external_ai_dry_run"));
    assert!(script.contains("normal text to app-server"));
    assert!(script.contains("does not reuse Codex/Hermes credentials"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("tmux new"));
    assert!(!script.contains("codex exec"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("\nrm "));
    assert!(!script.contains(" rm -"));

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-live-operator-checklist-smoke-{}-{nanos}",
        std::process::id()
    ));
    let workspace = root.join("workspace");
    let provider_env = root.join("provider.env");
    let feishu_env = root.join("chuang-feishu.env");
    fs::create_dir_all(&workspace).expect("workspace should be created");
    fs::write(
        workspace.join("config.toml"),
        "provider = \"openai_compatible\"\n",
    )
    .expect("workspace config should write");
    fs::write(
        &provider_env,
        "CODEX_PPTOKEN_API_KEY=secret-provider-value\n",
    )
    .expect("provider env should write");
    fs::write(
        &feishu_env,
        format!(
            "CHUANG_FEISHU_APP_ID=cli_a_test\nCHUANG_FEISHU_APP_SECRET=secret-feishu-value\nCHUANG_AGENT_WORKSPACE_ROOT={}\nCHUANG_PROVIDER_ENV_FILE={}\nCHUANG_FEISHU_CONNECTION_MODE=websocket\n",
            workspace.display(),
            provider_env.display()
        ),
    )
    .expect("feishu env should write");

    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_LIVE_OPERATOR_ENV_FILE", &feishu_env)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("live operator checklist should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("secret-provider-value"));
    assert!(!stdout.contains("secret-feishu-value"));

    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("checklist output should be json");
    assert_eq!(data["ok"], true);
    assert_eq!(data["status"], "ready");
    assert_eq!(data["readonly_boundaries"]["readonly"], true);
    assert_eq!(data["readonly_boundaries"]["connects_real_feishu"], false);
    assert_eq!(data["readonly_boundaries"]["sends_feishu_messages"], false);
    assert_eq!(data["readonly_boundaries"]["connects_real_provider"], false);
    assert_eq!(
        data["readonly_boundaries"]["performs_desktop_actions"],
        false
    );
    assert_eq!(
        data["readonly_boundaries"]["performs_browser_actions"],
        false
    );
    assert_eq!(data["readonly_boundaries"]["connects_real_wiki"], false);
    assert_eq!(data["readonly_boundaries"]["connects_real_gbrain"], false);
    assert_eq!(data["readonly_boundaries"]["starts_services"], false);
    assert_eq!(data["readonly_boundaries"]["starts_workers"], false);
    assert_eq!(data["readonly_boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["readonly_boundaries"]["touches_services"], false);
    assert_eq!(data["readonly_boundaries"]["modifies_repo"], false);
    assert_eq!(data["readonly_boundaries"]["prints_secret_values"], false);
    assert_eq!(
        data["checks"]["feishu_env_file"]["required"]["CHUANG_FEISHU_APP_SECRET"],
        "<set>"
    );
    assert_eq!(
        data["checks"]["provider_env_file"]["required"]["CODEX_PPTOKEN_API_KEY"],
        "<set>"
    );
    assert_eq!(
        data["commands"]["local_preflight"],
        format!(
            "node scripts/chuang-feishu-live-preflight.js --env-file {} --workspace-root {} --json",
            feishu_env.display(),
            workspace.display()
        )
    );
    assert_eq!(
        data["commands"]["provider_readiness_check"],
        format!(
            "bash scripts/chuang-provider-readiness-check.sh --config {}",
            workspace.join("config.toml").display()
        )
    );
    assert_eq!(
        data["commands"]["bridge_tools_command"],
        "send /tools to the Chuang Feishu bot"
    );
    assert_eq!(
        data["commands"]["bridge_capabilities_command"],
        "send /capabilities to the Chuang Feishu bot"
    );
    assert_eq!(
        data["commands"]["wiki_source_contract"],
        "cargo run --quiet -- memory knowledge source-contract --source wiki --json"
    );
    assert_eq!(
        data["commands"]["gbrain_source_contract"],
        "cargo run --quiet -- memory knowledge source-contract --source gbrain --json"
    );
    assert!(data["commands"]["external_ai_dry_run"]
        .as_str()
        .unwrap_or("")
        .contains("--dry-run --json"));
    let mounted = data["mounted_feishu_capabilities"]
        .as_array()
        .expect("mounted feishu capabilities should be an array");
    assert!(mounted.iter().any(|item| item["command"] == "/tools"
        && item["expected_evidence"]
            .as_array()
            .expect("expected evidence should be array")
            .iter()
            .any(|evidence| evidence == "normal text to app-server")));
    assert!(mounted.iter().any(|item| item["command"] == "/capabilities"
        && item["capability"]
            .as_str()
            .unwrap_or("")
            .contains("alias of /tools")));
    assert_eq!(data["provider_readiness_evidence"]["readonly"], true);
    assert_eq!(
        data["provider_readiness_evidence"]["source_status_surface"],
        "cargo run --quiet -- status --json"
    );
    assert_eq!(
        data["provider_readiness_evidence"]["connects_real_provider"],
        false
    );
    assert_eq!(
        data["provider_readiness_evidence"]["prints_secret_values"],
        false
    );
    let readiness_fields = data["provider_readiness_evidence"]["expected_fields"]
        .as_array()
        .expect("provider readiness expected fields should be array");
    assert!(readiness_fields
        .iter()
        .any(|field| field == "source_status_surface"));
    assert!(readiness_fields
        .iter()
        .any(|field| field == "provider_kind"));
    assert!(readiness_fields.iter().any(|field| field == "transport"));
    assert!(readiness_fields
        .iter()
        .any(|field| field == "request_timeout_ms"));
    assert!(readiness_fields
        .iter()
        .any(|field| field == "api_key_state"));
    assert!(readiness_fields.iter().any(|field| field == "current"));
    assert!(readiness_fields.iter().any(|field| field == "next_action"));
    assert_eq!(data["local_readonly_evidence"]["readonly"], true);
    assert_eq!(data["local_readonly_evidence"]["starts_workers"], false);
    assert_eq!(data["local_readonly_evidence"]["dispatches_tasks"], false);
    assert_eq!(data["local_readonly_evidence"]["touches_services"], false);
    assert_eq!(data["local_readonly_evidence"]["modifies_repo"], false);
    let readonly_steps = data["local_readonly_evidence"]["expected_steps"]
        .as_array()
        .expect("local readonly expected steps should be array");
    assert!(readonly_steps
        .iter()
        .any(|step| step == "provider readiness check"));
    assert!(readonly_steps
        .iter()
        .any(|step| step == "complete local smoke"));
    assert_eq!(data["real_live_acceptance"]["complete"], false);
    assert_eq!(data["real_live_acceptance"]["status"], "not_verified");
    assert_eq!(data["real_live_acceptance"]["gap_count"], 7);
    assert_eq!(
        data["real_live_acceptance"]["cannot_mark_complete_from_readonly_checklist"],
        true
    );
    let live_gaps = data["real_live_acceptance"]["services"]
        .as_array()
        .expect("real live acceptance services should be array");
    assert_eq!(live_gaps.len(), 7);
    for id in [
        "feishu",
        "provider",
        "subagent_live_rehearsal",
        "desktop",
        "browser",
        "wiki",
        "gbrain",
    ] {
        let gap = live_gaps
            .iter()
            .find(|item| item["id"] == id)
            .expect("expected live gap should be present");
        assert_eq!(gap["completion_state"], "not_verified");
        assert_eq!(gap["must_not_count_as_complete"], true);
        assert_eq!(gap["connects_real_service_in_checklist"], false);
        assert_eq!(gap["prints_secret_values"], false);
    }
    assert!(data["suggested_provider_env_file"].is_null());
    assert!(data["manual_steps"]
        .as_array()
        .expect("manual steps should be an array")
        .iter()
        .any(|step| step.as_str().unwrap_or("").contains("/health")));
    assert!(data["manual_steps"]
        .as_array()
        .expect("manual steps should be an array")
        .iter()
        .any(|step| step.as_str().unwrap_or("").contains("/tools")));
    assert!(data["manual_steps"]
        .as_array()
        .expect("manual steps should be an array")
        .iter()
        .any(|step| step
            .as_str()
            .unwrap_or("")
            .contains("provider_readiness_check")));
    assert!(data["manual_steps"]
        .as_array()
        .expect("manual steps should be an array")
        .iter()
        .any(|step| step
            .as_str()
            .unwrap_or("")
            .contains("real live acceptance as incomplete")));
}

#[test]
fn live_operator_checklist_uses_default_provider_env_when_missing() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-checklist.sh");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-live-operator-checklist-default-provider-{}-{nanos}",
        std::process::id()
    ));
    let workspace = root.join("workspace");
    let home_dir = root.join("home");
    let provider_env = home_dir.join(".config/chuang-agent/provider.env");
    let feishu_env = root.join("chuang-feishu.env");
    fs::create_dir_all(&workspace).expect("workspace should be created");
    fs::create_dir_all(
        provider_env
            .parent()
            .expect("provider env parent should exist"),
    )
    .expect("provider env dir should be created");
    fs::create_dir_all(&home_dir).expect("home dir should be created");
    fs::write(
        workspace.join("config.toml"),
        "provider = \"openai_compatible\"\n",
    )
    .expect("workspace config should write");
    fs::write(
        &provider_env,
        "CODEX_PPTOKEN_API_KEY=secret-provider-value\n",
    )
    .expect("provider env should write");
    fs::write(
        &feishu_env,
        format!(
            "CHUANG_FEISHU_APP_ID=cli_a_test\nCHUANG_FEISHU_APP_SECRET=secret-feishu-value\nCHUANG_AGENT_WORKSPACE_ROOT={}\nCHUANG_FEISHU_CONNECTION_MODE=websocket\n",
            workspace.display()
        ),
    )
    .expect("feishu env should write");

    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_LIVE_OPERATOR_ENV_FILE", &feishu_env)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .env("HOME", &home_dir)
        .env_remove("CHUANG_PROVIDER_ENV_FILE")
        .current_dir(&manifest_dir)
        .output()
        .expect("live operator checklist should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("secret-provider-value"));
    assert!(!stdout.contains("secret-feishu-value"));

    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("checklist output should be json");
    assert_eq!(data["ok"], true);
    assert_eq!(data["status"], "ready");
    assert_eq!(
        data["paths"]["provider_env_file"],
        provider_env.display().to_string()
    );
    assert_eq!(
        data["suggested_provider_env_file"]["path"],
        provider_env.display().to_string()
    );
    assert_eq!(data["suggested_provider_env_file"]["exists"], true);
    assert_eq!(data["suggested_provider_env_file"]["state"], "<set>");
    assert_eq!(
        data["commands"]["provider_env_next_step"],
        format!(
            "set CHUANG_PROVIDER_ENV_FILE to {} in the Chuang Feishu env, or export it explicitly before rerunning the checklist",
            provider_env.display()
        )
    );
    assert_eq!(data["checks"]["provider_env_file"]["exists"], true);
    assert_eq!(
        data["checks"]["provider_env_file"]["required"]["CHUANG_PROVIDER_ENV_FILE"],
        "<set>"
    );
    assert_eq!(
        data["checks"]["provider_env_file"]["required"]["CODEX_PPTOKEN_API_KEY"],
        "<set>"
    );
    assert!(data["manual_steps"]
        .as_array()
        .expect("manual steps should be an array")
        .iter()
        .any(|step| step
            .as_str()
            .unwrap_or("")
            .contains(provider_env.display().to_string().as_str())));
}

#[test]
fn live_operator_checklist_still_blocks_when_default_provider_env_is_missing() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-operator-checklist.sh");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "chuang-live-operator-checklist-default-provider-missing-{}-{nanos}",
        std::process::id()
    ));
    let workspace = root.join("workspace");
    let home_dir = root.join("home");
    let provider_env = home_dir.join(".config/chuang-agent/provider.env");
    let feishu_env = root.join("chuang-feishu.env");
    fs::create_dir_all(&workspace).expect("workspace should be created");
    fs::create_dir_all(&home_dir).expect("home dir should be created");
    fs::write(
        workspace.join("config.toml"),
        "provider = \"openai_compatible\"\n",
    )
    .expect("workspace config should write");
    fs::write(
        &feishu_env,
        format!(
            "CHUANG_FEISHU_APP_ID=cli_a_test\nCHUANG_FEISHU_APP_SECRET=secret-feishu-value\nCHUANG_AGENT_WORKSPACE_ROOT={}\nCHUANG_FEISHU_CONNECTION_MODE=websocket\n",
            workspace.display()
        ),
    )
    .expect("feishu env should write");

    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_LIVE_OPERATOR_ENV_FILE", &feishu_env)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .env("HOME", &home_dir)
        .env_remove("CHUANG_PROVIDER_ENV_FILE")
        .current_dir(&manifest_dir)
        .output()
        .expect("live operator checklist should execute");

    assert!(
        !output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"suggested_provider_env_file\""));
    assert!(!stdout.contains("secret-feishu-value"));

    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("checklist output should be json");
    assert_eq!(data["ok"], false);
    assert_eq!(data["status"], "blocked");
    assert_eq!(
        data["paths"]["provider_env_file"],
        provider_env.display().to_string()
    );
    assert_eq!(
        data["suggested_provider_env_file"]["path"],
        provider_env.display().to_string()
    );
    assert_eq!(data["suggested_provider_env_file"]["exists"], false);
    assert_eq!(data["suggested_provider_env_file"]["state"], "<missing>");
    assert_eq!(data["checks"]["provider_env_file"]["exists"], false);
    assert_eq!(
        data["checks"]["provider_env_file"]["required"]["CHUANG_PROVIDER_ENV_FILE"],
        "<missing>"
    );
    assert_eq!(
        data["commands"]["provider_env_next_step"],
        format!(
            "set CHUANG_PROVIDER_ENV_FILE to {} in the Chuang Feishu env, or export it explicitly before rerunning the checklist",
            provider_env.display()
        )
    );
    assert!(data["blockers"]
        .as_array()
        .expect("blockers should be an array")
        .iter()
        .any(|blocker| blocker == "provider_env_file_missing"));
}

#[test]
fn cli_run_command_boots_and_returns_structured_response() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir =
        std::env::temp_dir().join(format!("chuang-agent-cli-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::create_dir_all(temp_dir.join("identity")).expect("identity dir should be created");

    let config_path = write_fake_config(&temp_dir);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--input",
            "创项目现在启动试试",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Default run output is conversational (answer first), not the old field wall.
    assert!(stdout.contains("小创  stub-responder"));
    assert!(stdout.contains("模型 stub-responder"));
    assert!(stdout.contains("provider fake-responder"));
    assert!(stdout.contains("引擎 deterministic_budget"));
    assert!(!stdout.contains("model_name:"));
    assert!(!stdout.contains("body:"));
    assert!(!stdout.contains("context_drop_reasons:"));
    assert!(stdout.contains("runtime_report: report-turn-1"));
    assert_eq!(stdout.matches("governance_decision: allowed:").count(), 1);
    assert!(stdout.contains("创项目现在启动试试"));
}

#[test]
fn cli_run_verbose_keeps_structured_field_wall() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-run-verbose-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::create_dir_all(temp_dir.join("identity")).expect("identity dir should be created");

    let config_path = write_fake_config(&temp_dir);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--verbose",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--input",
            "创项目 verbose 字段墙",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("model_name: stub-responder"));
    assert!(stdout.contains("body:"));
    assert!(stdout.contains("trace:"));
    assert!(stdout.contains("provider: fake-responder"));
    assert!(stdout.contains("context_engine: deterministic_budget"));
    assert!(stdout.contains("context_drop_reasons:"));
    assert!(stdout.contains("context_working_reservation:"));
    assert!(stdout.contains("context_budget_exceeded:"));
    assert!(stdout.contains("runtime_report: report-turn-1"));
}

#[test]
fn cli_run_can_select_summary_compression_context_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-context-engine-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--context-engine",
            "summary_compression",
            "--input",
            "测试上下文引擎切换",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("引擎 summary_compression"));
    assert!(stdout.contains("测试上下文引擎切换"));
}

#[test]
fn cli_run_can_dispatch_runtime_report_to_queued_subagent() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-run-dispatch-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let queue_root = temp_dir.join("queue");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--subagent",
            "queued_external",
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--input",
            "把这一轮交给子代理复核",
            "--dispatch-subagent",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("runtime_report: report-turn-1"));
    assert!(stdout.contains("subagent_dispatch_run_id: queued-run-1"));
    assert!(stdout.contains("subagent_dispatch_agent_id: worker-1"));
    assert!(stdout.contains("subagent_dispatch_task_id: turn-1"));

    let dispatch_path = queue_root.join("dispatch").join("queued-run-1.json");
    assert!(dispatch_path.exists());
    let dispatch = fs::read_to_string(dispatch_path).expect("dispatch should be readable");
    assert!(dispatch.contains("\"source\": \"cli-run\""));
    assert!(dispatch.contains("\"report_id\": \"report-turn-1\""));
    assert!(dispatch.contains("把这一轮交给子代理复核"));

    let run_once = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "run-once",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--json",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo subagent run-once should execute");

    assert!(
        run_once.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run_once.stderr)
    );
    let run_once_stdout = String::from_utf8_lossy(&run_once.stdout);
    assert!(run_once_stdout.contains("\"run_id\": \"queued-run-1\""));
    assert!(run_once_stdout.contains("\"report_admission\""));
    assert!(run_once_stdout.contains("\"status\": \"Accepted\""));
    assert!(queue_root
        .join("reports")
        .join("queued-run-1.json")
        .exists());

    let report = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "report",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--run-id",
            "queued-run-1",
            "--json",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo subagent report should execute");

    assert!(
        report.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&report.stderr)
    );
    let report_stdout = String::from_utf8_lossy(&report.stdout);
    assert!(report_stdout.contains("\"available\": true"));
    assert!(report_stdout.contains("fake runner completed turn-1"));
    assert!(report_stdout.contains("\"report_admission\""));
    assert!(report_stdout.contains("\"status\": \"Accepted\""));

    let collect = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "collect",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--run-id",
            "queued-run-1",
            "--json",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo subagent collect should execute");

    assert!(
        collect.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&collect.stderr)
    );
    let collect_stdout = String::from_utf8_lossy(&collect.stdout);
    assert!(collect_stdout.contains("\"dispatch_available\": true"));
    assert!(collect_stdout.contains("\"report_available\": true"));
    assert!(collect_stdout.contains("fake runner completed turn-1"));
    assert!(collect_stdout.contains("\"report_admission\""));
    assert!(collect_stdout.contains("\"status\": \"Accepted\""));
}

#[test]
fn cli_run_dispatch_subagent_requires_queued_external_slot() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-run-dispatch-reject-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let queue_root = temp_dir.join("queue");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--subagent-queue-root",
            queue_root.to_str().expect("queue path should be utf-8"),
            "--subagent",
            "fake",
            "--input",
            "没有选择 queued external",
            "--dispatch-subagent",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("subagent_dispatch_requires_queued_external"));
    assert!(!queue_root.join("dispatch").exists());
}

#[test]
fn cli_repl_command_accepts_one_turn_and_exits() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir =
        std::env::temp_dir().join(format!("chuang-agent-cli-repl-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::create_dir_all(temp_dir.join("identity")).expect("identity dir should be created");

    let config_path = write_fake_config(&temp_dir);

    let mut child = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "repl",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
        ])
        .current_dir(&workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cargo run repl should start");

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin should exist");
        stdin
            .write_all("创项目继续推进\nexit\n".as_bytes())
            .expect("stdin write should succeed");
    }

    let output = child.wait_with_output().expect("process should finish");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("创项目继续推进"));
    assert!(!stdout.contains("model_name:"));
    assert!(!stdout.contains("trace:"));
}

#[test]
fn cli_repl_interactive_path_carries_recent_history_and_unique_temp_files() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let main_rs =
        fs::read_to_string(manifest_dir.join("src/main.rs")).expect("main.rs should be readable");

    assert!(main_rs.contains("let mut conversation_history: Vec<ConversationHistoryItem>"));
    assert!(main_rs.contains("recent_repl_conversation_history("));
    assert!(main_rs.contains("conversation_history,"));
    assert!(main_rs.contains("record_repl_conversation_turn("));
    assert!(main_rs.contains("REPL_TURN_SEQUENCE.fetch_add"));
    assert!(main_rs.contains("/history   查看最近对话"));
    assert!(!main_rs.contains("started_at.elapsed().as_nanos()"));
}

#[test]
fn cli_run_can_remember_turn_summary_when_requested() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-remember-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let db_arg = db_path.to_str().expect("db path should be utf-8");

    let first = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_arg,
            "--input",
            "MVP记忆写入测试",
            "--remember",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("memory_recorded: turn-memory-turn-1"));

    let second = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_arg,
            "--input",
            "MVP",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("召回 1"));
    assert!(second_stdout.contains("MVP记忆写入测试"));
}

#[test]
fn cli_run_can_remember_identity_memory_when_requested() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-identity-remember-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let identity_root = temp_dir.join("identity");

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--identity-memory-root",
            identity_root
                .to_str()
                .expect("identity path should be utf-8"),
            "--input",
            "身份热记忆写入测试",
            "--remember-identity",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("identity_memory_recorded: identity-turn-1-"));

    let memory_file =
        fs::read_to_string(identity_root.join("MEMORY.md")).expect("memory file should exist");
    assert!(memory_file.contains("身份热记忆写入测试"));
    assert!(memory_file.contains("## identity-turn-1-"));
}

#[test]
fn cli_run_reports_memory_write_hard_limit_clearly() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir;

    let temp_dir = std::env::temp_dir().join(format!(
        "chuang-agent-cli-memory-limit-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let config_path = write_fake_config(&temp_dir);
    let db_path = temp_dir.join("memory.db");
    let oversized_input = "超限".repeat(1200);

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "run",
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--db",
            db_path.to_str().expect("db path should be utf-8"),
            "--input",
            &oversized_input,
            "--remember",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("cargo run should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("memory_write_hard_limit_exceeded"));
    assert!(stderr.contains("limit_chars=2200"));
    assert!(stderr.contains("attempted_chars="));
    assert!(stderr.contains("existing_entries="));
}
