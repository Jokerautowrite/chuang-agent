use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn run_receipt_script(script_path: &PathBuf) -> Value {
    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_LIVE_REHEARSAL_DISPATCH_ID", "dispatch-123")
        .env("CHUANG_LIVE_REHEARSAL_WORKER_ID", "worker-7")
        .env("CHUANG_LIVE_REHEARSAL_GATE_RECEIPT_REF", "gate-ref-1")
        .env(
            "CHUANG_LIVE_REHEARSAL_ALLOWLIST_RECEIPT_REF",
            "allowlist-ref-2",
        )
        .env(
            "CHUANG_LIVE_REHEARSAL_CAPABILITY_ROUTING_REF",
            "cap-route-3",
        )
        .env(
            "CHUANG_LIVE_REHEARSAL_REPORT_ADMISSION_REF",
            "report-admit-4",
        )
        .output()
        .expect("receipt script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json")
}

#[test]
fn live_runner_rehearsal_receipt_script_is_readonly_template() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-runner-rehearsal-receipt.sh");
    let script = fs::read_to_string(&script_path).expect("receipt script should be readable");

    assert!(script.contains("Readonly single worker rehearsal receipt skeleton."));
    assert!(script.contains("CHUANG_LIVE_REHEARSAL_DISPATCH_ID"));
    assert!(script.contains("CHUANG_LIVE_REHEARSAL_WORKER_ID"));
    assert!(script.contains("\"starts_external_worker\": False"));
    assert!(script.contains("\"enables_live_gate\": False"));
    assert!(script.contains("\"dispatches_tasks\": False"));
    assert!(script.contains("\"modifies_repo\": False"));
    assert!(script.contains("single worker rehearsal is read-only and is not runner pool ready"));
    assert!(!script.contains("subagent dispatch"));
    assert!(!script.contains("subagent run-once"));
    assert!(!script.contains("subagent run-loop"));
    assert!(!script.contains("CHUANG_CODEX_RUNNER_ENABLE"));
    assert!(!script.contains("CHUANG_REAL_CONTROL_ENABLE"));
    assert!(!script.contains("CHUANG_REAL_ACTUATOR_ENABLE"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));
}

#[test]
fn live_runner_rehearsal_receipt_script_outputs_readonly_json_template() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-runner-rehearsal-receipt.sh");
    let data = run_receipt_script(&script_path);

    assert_eq!(data["schema_version"], 1);
    assert_eq!(
        data["receipt_kind"],
        "single_worker_rehearsal_live_receipt_skeleton"
    );
    assert!(data["tested_at"].as_str().is_some());
    assert_eq!(data["dispatch_id"], "dispatch-123");
    assert_eq!(data["worker_id"], "worker-7");
    assert_eq!(data["gate_receipt_ref"], "gate-ref-1");
    assert_eq!(data["allowlist_receipt_ref"], "allowlist-ref-2");
    assert_eq!(data["capability_routing_ref"], "cap-route-3");
    assert_eq!(data["report_admission_ref"], "report-admit-4");

    let boundaries = &data["readonly_boundaries"];
    assert_eq!(boundaries["readonly"], true);
    assert_eq!(boundaries["connects_real_feishu"], false);
    assert_eq!(boundaries["sends_feishu_messages"], false);
    assert_eq!(boundaries["connects_real_provider"], false);
    assert_eq!(boundaries["starts_external_worker"], false);
    assert_eq!(boundaries["enables_live_gate"], false);
    assert_eq!(boundaries["starts_workers"], false);
    assert_eq!(boundaries["dispatches_tasks"], false);
    assert_eq!(boundaries["restarts_worker"], false);
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

    let prerequisites = &data["approval_audit_prerequisites"];
    assert_eq!(prerequisites["ok"], false);
    assert_eq!(prerequisites["explicit_operator_approval_required"], true);
    assert_eq!(prerequisites["governance_approval_required"], true);
    assert_eq!(prerequisites["audit_receipt_required"], true);
    assert_eq!(prerequisites["dispatch_evidence_required"], true);
    assert_eq!(
        prerequisites["audit_label"],
        "subagent.runner.single-worker-rehearsal.live"
    );
    assert_eq!(
        prerequisites["prerequisites"],
        serde_json::json!([
            "operator approval for the exact single worker rehearsal dispatch",
            "governance approval for the exact gate, allowlist, capability routing, and report admission refs",
            "dispatch evidence must exist before runner pool readiness can be claimed"
        ])
    );
    assert!(prerequisites["reason"]
        .as_str()
        .expect("prerequisites reason")
        .contains("read-only receipt skeleton"));

    let real_live = &data["real_live_acceptance"];
    assert_eq!(real_live["complete"], false);
    assert_eq!(real_live["status"], "not_runner_pool_ready");
    assert_eq!(real_live["runner_pool_ready"], false);
    assert_eq!(
        real_live["single_worker_rehearsal_is_runner_pool_ready"],
        false
    );
    assert_eq!(real_live["cannot_mark_complete_from_template"], true);
    assert_eq!(
        real_live["cannot_mark_runner_pool_ready_from_template"],
        true
    );
    assert_eq!(real_live["requires_operator_evidence"], true);
    assert_eq!(real_live["gap_count"], 1);
    assert!(real_live["reason"]
        .as_str()
        .expect("real live reason")
        .contains("not runner pool ready"));

    assert_eq!(data["notes"], serde_json::json!([]));
    assert_eq!(data["blockers"], serde_json::json!([]));
}
