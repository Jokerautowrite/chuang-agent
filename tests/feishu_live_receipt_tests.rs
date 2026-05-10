use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn feishu_live_receipt_script_outputs_readonly_template_json() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-feishu-live-receipt.sh");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg("--json")
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
        serde_json::from_str(&stdout).expect("receipt output should be valid json");

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "feishu_live_readonly_receipt");
    assert!(data["tested_at"].as_str().is_some());
    assert_eq!(data["request_id"], "<fill_request_id>");
    assert_eq!(data["operator"], "<operator>");
    assert_eq!(data["workspace_root"], manifest_dir.display().to_string());
    assert_eq!(data["acceptance_status"], "not_verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["cannot_mark_complete_without_operator_evidence"], true);
    assert_eq!(data["readonly"], true);

    let boundaries = &data["readonly_boundaries"];
    assert_eq!(boundaries["readonly"], true);
    assert_eq!(boundaries["connects_real_feishu"], false);
    assert_eq!(boundaries["sends_feishu_messages"], false);
    assert_eq!(boundaries["connects_real_provider"], false);
    assert_eq!(boundaries["starts_workers"], false);
    assert_eq!(boundaries["dispatches_tasks"], false);
    assert_eq!(boundaries["performs_desktop_actions"], false);
    assert_eq!(boundaries["performs_browser_actions"], false);
    assert_eq!(boundaries["connects_real_wiki"], false);
    assert_eq!(boundaries["connects_real_gbrain"], false);
    assert_eq!(boundaries["reads_secret_values"], false);
    assert_eq!(boundaries["prints_secret_values"], false);
    assert_eq!(boundaries["starts_services"], false);
    assert_eq!(boundaries["stops_services"], false);
    assert_eq!(boundaries["touches_services"], false);
    assert_eq!(boundaries["modifies_repo"], false);
    assert_eq!(boundaries["deletes_files"], false);
    assert_eq!(boundaries["reuses_codex_or_hermes_credentials"], false);

    let evidence = &data["feishu_live_evidence"];
    assert_eq!(
        evidence["transcript_refs"]["health"],
        "<fill_health_transcript_ref>"
    );
    assert_eq!(
        evidence["transcript_refs"]["session"],
        "<fill_session_transcript_ref>"
    );
    assert_eq!(
        evidence["transcript_refs"]["tools"],
        "<fill_tools_transcript_ref>"
    );
    assert_eq!(
        evidence["session_binding_refs"]["chat_binding_ref"],
        "<fill_chat_binding_ref>"
    );
    assert_eq!(
        evidence["session_binding_refs"]["thread_binding_ref"],
        "<fill_thread_binding_ref>"
    );
    assert_eq!(
        evidence["session_binding_refs"]["binding_state_ref"],
        "<fill_binding_state_ref>"
    );
    assert_eq!(
        evidence["normal_message"]["transcript_ref"],
        "<fill_normal_message_transcript_ref>"
    );
    assert_eq!(
        evidence["normal_message"]["runtime_report_id"],
        "<fill_runtime_report_id>"
    );
    assert_eq!(
        evidence["secret_redaction_notes"],
        serde_json::json!(["<record_only_set_or_missing_values>"])
    );
    assert_eq!(evidence["codex_hermes_isolation"]["kept_separate"], true);
    assert_eq!(
        evidence["codex_hermes_isolation"]["notes"],
        "<keep_codex_and_hermes_separate>"
    );

    assert_eq!(data["notes"], serde_json::json!([]));
    assert_eq!(data["blockers"], serde_json::json!([]));
    assert!(!stdout.contains("app_secret"));
    assert!(!stdout.contains("token="));
    assert!(!stdout.contains("Authorization"));
}

#[test]
fn feishu_live_receipt_script_is_readonly_skeleton_only() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-feishu-live-receipt.sh");
    let script = fs::read_to_string(&script_path).expect("receipt script should be readable");

    assert!(script.contains("Readonly Feishu live receipt template."));
    assert!(script.contains("connects_real_feishu=false"));
    assert!(script.contains("prints_secret_values=false"));
    assert!(script.contains("modifies_repo=false"));
    assert!(script.contains("feishu_live_evidence"));
    assert!(script.contains("transcript_refs"));
    assert!(script.contains("session_binding_refs"));
    assert!(script.contains("runtime_report_id"));
    assert!(script.contains("secret_redaction_notes"));
    assert!(script.contains("codex_hermes_isolation"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("curl "));
    assert!(!script.contains("rm "));
    assert!(!script.contains("git reset"));
}
