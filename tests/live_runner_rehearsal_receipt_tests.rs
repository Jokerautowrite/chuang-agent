use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn run_receipt_script(script_path: &PathBuf) -> Value {
    let output = Command::new("bash")
        .arg(script_path)
        .arg("--json")
        .env("CHUANG_AGENT_BIN", env!("CARGO_BIN_EXE_chuang-agent"))
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
fn live_runner_rehearsal_receipt_script_uses_bounded_command_runner_protocol() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-runner-rehearsal-receipt.sh");
    let script = fs::read_to_string(&script_path).expect("receipt script should be readable");

    assert!(script.contains("subagent live-preflight"));
    assert!(script.contains("--runner-command scripts/chuang-subagent-runner-example.sh"));
    assert!(script.contains("--allow-runner-command scripts/chuang-subagent-runner-example.sh"));
    assert!(script.contains("--requires-capability rehearsal"));
    assert!(script.contains("subagent dispatch"));
    assert!(script.contains("--requires-capability rehearsal"));
    assert!(script.contains("subagent run-loop"));
    assert!(script.contains("--runner command"));
    assert!(script.contains("--approve-exec"));
    assert!(script.contains("--require-live-gate"));
    assert!(script.contains("--max-runs 1"));
    assert!(script.contains("--max-concurrency 1"));
    assert!(script.contains("subagent report"));
    assert!(script.contains("subagent collect"));
    assert!(script.contains("approval_receipt=cli_flag:--approve-exec"));
    assert!(script.contains("single_worker_rehearsal_live_receipt"));

    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("hermes-gateway"));
    assert!(!script.contains("FEISHU_"));
}

#[test]
fn live_runner_rehearsal_receipt_script_outputs_positive_rehearsal_receipt() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = manifest_dir.join("scripts/chuang-live-runner-rehearsal-receipt.sh");
    let data = run_receipt_script(&script_path);

    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["receipt_kind"], "single_worker_rehearsal_live_receipt");
    assert!(data["tested_at"].as_str().is_some());

    let boundaries = &data["readonly_boundaries"];
    assert_eq!(boundaries["readonly"], false);
    assert_eq!(boundaries["starts_external_worker"], true);
    assert_eq!(boundaries["enables_live_gate"], true);
    assert_eq!(boundaries["starts_workers"], true);
    assert_eq!(boundaries["dispatches_tasks"], true);
    assert_eq!(boundaries["connects_real_provider"], false);
    assert_eq!(boundaries["connects_real_feishu"], false);
    assert_eq!(boundaries["modifies_repo"], false);
    assert_eq!(boundaries["deletes_files"], false);

    let preflight = &data["preflight"];
    assert_eq!(preflight["ok"], true);
    assert_eq!(preflight["ready_for_live"], true);
    assert_eq!(preflight["readonly"], true);
    assert_eq!(preflight["starts_external_worker"], false);
    assert_eq!(preflight["gate_enabled"], true);
    assert_eq!(preflight["runner_allowlist_ok"], true);
    assert_eq!(preflight["capability_routing_ok"], true);
    assert_eq!(preflight["report_admission_ok"], true);
    assert_eq!(preflight["required_env"], "CHUANG_CODEX_RUNNER_ENABLE");
    assert_eq!(preflight["audit_label"], "subagent.runner.live");

    let dispatch = &data["dispatch"];
    assert!(dispatch["run_id"].as_str().is_some());
    assert!(dispatch["task_id"].as_str().is_some());
    assert!(dispatch["agent_id"].as_str().is_some());
    assert_eq!(
        dispatch["required_capabilities"],
        serde_json::json!(["rehearsal"])
    );

    let execution = &data["worker_execution"];
    assert_eq!(execution["runner"], "command");
    assert_eq!(execution["max_runs"], 1);
    assert_eq!(execution["max_concurrency"], 1);
    assert_eq!(execution["ran_count"], 1);
    assert_eq!(execution["idle"], false);
    assert_eq!(execution["run_ids"].as_array().expect("run ids").len(), 1);
    let execution_admission = &execution["report_admissions"][0];
    assert_eq!(execution_admission["status"], "Accepted");
    assert_eq!(execution_admission["reason_code"], "report_validated");
    assert_eq!(
        execution_admission["controller_agent_id"],
        "cli-subagent-controller"
    );

    let report = &data["report"];
    assert_eq!(report["available"], true);
    assert_eq!(report["status"], "Success");
    assert_eq!(report["exit_code"], 0);
    assert!(report["report_id"].as_str().is_some());
    assert_eq!(report["governance_decision"]["decision"], "allowed");
    assert_eq!(
        report["governance_decision"]["reason"],
        "approval_receipt=cli_flag:--approve-exec"
    );
    assert_eq!(report["report_admission"]["status"], "Accepted");
    assert_eq!(
        report["report_admission"]["reason_code"],
        "report_validated"
    );

    let collect = &data["collect"];
    assert_eq!(collect["dispatch_available"], true);
    assert_eq!(collect["report_available"], true);
    assert_eq!(collect["admission_status"], "Accepted");
    assert_eq!(collect["admission_reason_code"], "report_validated");
    let admission_refs = collect["admission_refs"]
        .as_array()
        .expect("admission refs");
    assert_eq!(admission_refs.len(), 1);
    assert_eq!(admission_refs[0]["admission_status"], "Accepted");
    assert_eq!(admission_refs[0]["reason_code"], "report_validated");
    assert!(admission_refs[0]["evidence_ref"]
        .as_str()
        .expect("evidence ref")
        .starts_with("report://"));

    let acceptance = &data["real_live_acceptance"];
    assert_eq!(acceptance["single_worker_rehearsal_complete"], true);
    assert_eq!(acceptance["status"], "single_worker_rehearsal_completed");
    assert_eq!(acceptance["global_real_live_ready"], false);
    assert_eq!(acceptance["remaining_gap_count"], 3);
    assert_eq!(
        acceptance["next_gaps"],
        serde_json::json!([
            "feishu_live_receipt",
            "browser_desktop_boundary_receipt",
            "wiki_gbrain_readonly_adapter_receipt"
        ])
    );

    assert_eq!(data["notes"], serde_json::json!([]));
    assert_eq!(data["blockers"], serde_json::json!([]));
}
