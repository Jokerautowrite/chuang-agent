#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/chuang-live-operator-receipt-collect.sh")
}

fn operator_receipt_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-live-operator-receipt.sh")
}

fn run_json_script(script_path: &Path, args: &[&Path]) -> Value {
    let mut command = Command::new("bash");
    command.arg(script_path).arg("--json");
    for arg in args {
        command.arg(arg);
    }
    let output = command
        .env("CHUANG_AGENT_ROOT", env!("CARGO_MANIFEST_DIR"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("script should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json")
}

fn write_temp_json(dir: &Path, name: &str, value: &Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        serde_json::to_string_pretty(value).expect("json should serialize"),
    )
    .expect("json file should write");
    path
}

fn make_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

fn complete_live_evidence() -> Value {
    serde_json::json!({
        "feishu": {
            "health_transcript_ref": "receipt://feishu/health",
            "session_transcript_ref": "receipt://feishu/session",
            "tools_or_capabilities_transcript_ref": "receipt://feishu/tools",
            "normal_message_transcript_ref": "receipt://feishu/message",
            "runtime_report_id": "runtime-report-feishu"
        },
        "provider": {
            "provider_kind": "openai_compatible",
            "transport": "cliproxy-local",
            "api_key_state": "<set>",
            "provider_live_request_receipt_ref": "receipt://provider/live-request",
            "runtime_report_id": "runtime-report-provider",
            "does_not_call_provider": false,
            "does_not_read_provider_readiness": false
        },
        "subagent_live_rehearsal": {
            "dispatch_id": "dispatch-verified",
            "worker_id": "worker-verified",
            "gate_receipt_ref": "receipt://subagent/gate",
            "allowlist_receipt_ref": "receipt://subagent/allowlist",
            "capability_routing_ref": "receipt://subagent/capability-routing",
            "report_admission_ref": "receipt://subagent/report-admission"
        },
        "desktop": {
            "audit_label": "actuator.operation.live",
            "action_receipt_ref": "receipt://desktop/action",
            "governance_receipt_ref": "receipt://desktop/governance",
            "real_execution": "true"
        },
        "browser": {
            "adapter_manifest_ref": "receipt://browser/adapter-manifest",
            "session_scope_ref": "receipt://browser/session-scope",
            "browser_snapshot_or_transcript_ref": "receipt://browser/snapshot",
            "report_admission_ref": "receipt://browser/report-admission"
        },
        "wiki": {
            "source_contract_ref": "receipt://wiki/source-contract",
            "query_receipt_ref": "receipt://wiki/query",
            "provenance_ref": "receipt://wiki/provenance",
            "writes_core_memory": false
        },
        "gbrain": {
            "source_contract_ref": "receipt://gbrain/source-contract",
            "query_receipt_ref": "receipt://gbrain/query",
            "provenance_ref": "receipt://gbrain/provenance",
            "writes_core_memory": false
        }
    })
}

#[test]
fn live_operator_receipt_collect_script_is_readonly_overlay_merge_tool() {
    let script = fs::read_to_string(script_path()).expect("collector script should be readable");
    assert!(script.contains("Readonly local receipt collector for operator live receipts."));
    assert!(script.contains("--base-file PATH"));
    assert!(script.contains("--overlay-file PATH"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));
}

#[test]
fn live_operator_receipt_collect_script_merges_base_and_overlay_receipts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = make_temp_dir("chuang-live-operator-receipt-collect");
    let base_output = run_json_script(operator_receipt_script_path().as_path(), &[]);
    let base_path = write_temp_json(&temp_dir, "base.json", &base_output);

    let mut overlay = base_output.clone();
    overlay["receipt_kind"] = Value::String("live_operator_receipt_overlay".to_string());
    overlay["service_evidence"]["feishu"]["normal_message"]["runtime_report_id"] =
        Value::String("runtime-report-1".to_string());
    overlay["service_evidence"]["provider"]["api_key_state"] = Value::String("<set>".to_string());
    overlay["service_evidence"]["subagent_live_rehearsal"]["dispatch_id"] =
        Value::String("dispatch-123".to_string());
    overlay["service_evidence"]["subagent_live_rehearsal"]["worker_id"] =
        Value::String("worker-7".to_string());
    overlay["service_evidence"]["subagent_live_rehearsal"]["gate_receipt_ref"] =
        Value::String("gate-ref-1".to_string());
    overlay["service_evidence"]["subagent_live_rehearsal"]["allowlist_receipt_ref"] =
        Value::String("allowlist-ref-2".to_string());
    overlay["service_evidence"]["subagent_live_rehearsal"]["capability_routing_ref"] =
        Value::String("cap-route-3".to_string());
    overlay["service_evidence"]["subagent_live_rehearsal"]["report_admission_ref"] =
        Value::String("report-admit-4".to_string());
    overlay["service_receipts"][0]["status"] = Value::String("verified".to_string());
    overlay["service_receipts"][1]["status"] = Value::String("blocked".to_string());
    overlay["service_receipts"][2]["status"] = Value::String("verified".to_string());
    overlay["real_live_acceptance"]["services"][0]["completion_state"] =
        Value::String("verified".to_string());
    overlay["real_live_acceptance"]["services"][1]["completion_state"] =
        Value::String("blocked".to_string());
    overlay["real_live_acceptance"]["services"][2]["completion_state"] =
        Value::String("verified".to_string());
    overlay["notes"] = serde_json::json!(["collector merged overlay evidence"]);

    let overlay_path = write_temp_json(&temp_dir, "overlay.json", &overlay);

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .arg("--base-file")
        .arg(&base_path)
        .arg("--overlay-file")
        .arg(&overlay_path)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("collector should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("collector output should be json");

    assert_eq!(data["schema_version"], 1);
    assert!(data["tested_at"].as_str().is_some());
    assert_eq!(data["acceptance_status"], "not_verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["cannot_mark_complete_without_operator_evidence"], true);
    assert_eq!(data["boundaries"], data["readonly_boundaries"]);
    assert_eq!(data["boundaries"]["readonly"], true);
    assert_eq!(data["boundaries"]["connects_real_feishu"], false);
    assert_eq!(data["boundaries"]["connects_real_provider"], false);

    let service_ids = data["service_receipts"]
        .as_array()
        .expect("service_receipts should be an array")
        .iter()
        .map(|item| item["id"].as_str().expect("id should be string"))
        .collect::<Vec<_>>();
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

    assert_eq!(
        data["service_evidence"]["feishu"]["normal_message"]["runtime_report_id"],
        "runtime-report-1"
    );
    assert_eq!(
        data["service_evidence"]["provider"]["api_key_state"],
        "<set>"
    );
    assert_eq!(
        data["service_evidence"]["provider"]["does_not_call_provider"],
        true
    );
    assert_eq!(
        data["service_evidence"]["provider"]["does_not_read_provider_readiness"],
        true
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["dispatch_id"],
        "dispatch-123"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["worker_id"],
        "worker-7"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["gate_receipt_ref"],
        "gate-ref-1"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["allowlist_receipt_ref"],
        "allowlist-ref-2"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["capability_routing_ref"],
        "cap-route-3"
    );
    assert_eq!(
        data["service_evidence"]["subagent_live_rehearsal"]["report_admission_ref"],
        "report-admit-4"
    );
    assert_eq!(data["service_receipts"][0]["status"], "verified");
    assert_eq!(data["service_receipts"][1]["status"], "blocked");
    assert_eq!(data["service_receipts"][2]["status"], "verified");
    assert_eq!(
        data["real_live_acceptance"]["services"][0]["completion_state"],
        "verified"
    );
    assert_eq!(
        data["real_live_acceptance"]["services"][1]["completion_state"],
        "blocked"
    );
    assert_eq!(
        data["real_live_acceptance"]["services"][2]["completion_state"],
        "verified"
    );
    assert_eq!(
        data["notes"],
        serde_json::json!(["collector merged overlay evidence"])
    );
    assert_eq!(
        data["codex_hermes_isolation"],
        "<keep_codex_and_hermes_separate>"
    );
    assert!(!stdout.contains("app_secret"));
    assert!(!stdout.contains("token="));
}

#[test]
fn live_operator_receipt_collect_script_canonicalizes_service_receipt_order() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = make_temp_dir("chuang-live-operator-receipt-collect-reject");
    let base_output = run_json_script(operator_receipt_script_path().as_path(), &[]);
    let base_path = write_temp_json(&temp_dir, "base.json", &base_output);

    let mut overlay = base_output.clone();
    overlay["service_receipts"] = serde_json::json!([
        base_output["service_receipts"][1].clone(),
        base_output["service_receipts"][0].clone(),
        base_output["service_receipts"][2].clone(),
        base_output["service_receipts"][3].clone(),
        base_output["service_receipts"][4].clone(),
        base_output["service_receipts"][5].clone(),
        base_output["service_receipts"][6].clone()
    ]);
    let overlay_path = write_temp_json(&temp_dir, "overlay.json", &overlay);

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .arg("--base-file")
        .arg(&base_path)
        .arg("--overlay-file")
        .arg(&overlay_path)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("collector should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = serde_json::from_str(&stdout).expect("collector output should be json");
    assert_eq!(
        data["service_receipts"]
            .as_array()
            .expect("service receipts")
            .iter()
            .map(|item| item["id"].as_str().expect("id"))
            .collect::<Vec<_>>(),
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
    assert_eq!(
        data["service_receipts"][0]["status"],
        "<not_verified|verified|blocked>"
    );
    assert_eq!(
        data["service_receipts"][1]["status"],
        "<not_verified|verified|blocked>"
    );
}

#[test]
fn live_operator_receipt_collect_script_refuses_overlay_live_ready_boundary_escalation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = make_temp_dir("chuang-live-operator-receipt-collect-boundary");
    let base_output = run_json_script(operator_receipt_script_path().as_path(), &[]);
    let base_path = write_temp_json(&temp_dir, "base.json", &base_output);

    let overlay = serde_json::json!({
        "acceptance_status": "verified",
        "can_mark_real_live_ready": true,
        "cannot_mark_complete_without_operator_evidence": false,
        "readonly_boundaries": {
            "readonly": false,
            "starts_workers": true,
            "dispatches_tasks": true,
            "connects_real_provider": true,
            "modifies_repo": true,
            "prints_secret_values": true
        },
        "boundaries": {
            "readonly": false,
            "starts_workers": true,
            "dispatches_tasks": true,
            "connects_real_provider": true,
            "modifies_repo": true,
            "prints_secret_values": true
        },
        "real_live_acceptance": {
            "complete": true,
            "status": "verified",
            "gap_count": 0,
            "cannot_mark_complete_from_template": false,
            "requires_operator_evidence": false,
            "services": [
                {
                    "id": "feishu",
                    "completion_state": "verified",
                    "manual_live_required": false,
                    "must_not_count_as_complete": false,
                    "required": []
                },
                {
                    "id": "subagent_live_rehearsal",
                    "completion_state": "verified",
                    "manual_live_required": false,
                    "must_not_count_as_complete": false,
                    "required": []
                }
            ]
        }
    });
    let overlay_path = write_temp_json(&temp_dir, "overlay.json", &overlay);

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .arg("--base-file")
        .arg(&base_path)
        .arg("--overlay-file")
        .arg(&overlay_path)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("collector should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value =
        serde_json::from_slice(&output.stdout).expect("collector output should be json");

    assert_eq!(data["acceptance_status"], "not_verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["cannot_mark_complete_without_operator_evidence"], true);
    assert_eq!(data["boundaries"], data["readonly_boundaries"]);
    assert_eq!(data["boundaries"]["readonly"], true);
    assert_eq!(data["boundaries"]["starts_workers"], false);
    assert_eq!(data["boundaries"]["dispatches_tasks"], false);
    assert_eq!(data["boundaries"]["connects_real_provider"], false);
    assert_eq!(data["boundaries"]["modifies_repo"], false);
    assert_eq!(data["boundaries"]["prints_secret_values"], false);
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
    let services = data["real_live_acceptance"]["services"]
        .as_array()
        .expect("real live acceptance services should be an array");
    assert_eq!(services.len(), 7);
    assert_eq!(services[0]["id"], "feishu");
    assert_eq!(services[0]["completion_state"], "verified");
    assert_eq!(services[2]["id"], "subagent_live_rehearsal");
    assert_eq!(services[2]["completion_state"], "verified");
    for service in services {
        assert_eq!(service["manual_live_required"], true);
        assert_eq!(service["must_not_count_as_complete"], true);
        assert!(
            service["required"]
                .as_array()
                .expect("service required list should be an array")
                .len()
                > 0
        );
    }
}

#[test]
fn live_operator_receipt_collect_script_marks_ready_only_with_complete_canonical_evidence() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = make_temp_dir("chuang-live-operator-receipt-collect-complete");
    let base_output = run_json_script(operator_receipt_script_path().as_path(), &[]);
    let base_path = write_temp_json(&temp_dir, "base.json", &base_output);
    let service_ids = vec![
        "feishu",
        "provider",
        "subagent_live_rehearsal",
        "desktop",
        "browser",
        "wiki",
        "gbrain",
    ];
    let evidence = complete_live_evidence();

    let overlay = serde_json::json!({
        "receipt_kind": "live_operator_receipt_overlay",
        "acceptance_status": "verified",
        "can_mark_real_live_ready": true,
        "cannot_mark_complete_without_operator_evidence": false,
        "service_evidence": evidence,
        "service_receipts": service_ids
            .iter()
            .map(|service_id| {
                serde_json::json!({
                    "id": service_id,
                    "status": "verified",
                    "evidence": evidence[*service_id].clone()
                })
            })
            .collect::<Vec<_>>(),
        "real_live_acceptance": {
            "complete": true,
            "status": "verified",
            "gap_count": 0,
            "cannot_mark_complete_from_template": false,
            "requires_operator_evidence": false,
            "services": service_ids
                .iter()
                .map(|service_id| {
                    serde_json::json!({
                        "id": service_id,
                        "completion_state": "verified",
                        "manual_live_required": false,
                        "must_not_count_as_complete": false
                    })
                })
                .collect::<Vec<_>>()
        },
        "blockers": []
    });
    let overlay_path = write_temp_json(&temp_dir, "overlay.json", &overlay);

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .arg("--base-file")
        .arg(&base_path)
        .arg("--overlay-file")
        .arg(&overlay_path)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("collector should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value =
        serde_json::from_slice(&output.stdout).expect("collector output should be json");

    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["can_mark_real_live_ready"], true);
    assert_eq!(
        data["cannot_mark_complete_without_operator_evidence"],
        false
    );
    assert_eq!(data["real_live_acceptance"]["complete"], true);
    assert_eq!(data["real_live_acceptance"]["status"], "verified");
    assert_eq!(data["real_live_acceptance"]["gap_count"], 0);
    assert_eq!(
        data["real_live_acceptance"]["cannot_mark_complete_from_template"],
        false
    );
    assert_eq!(
        data["real_live_acceptance"]["requires_operator_evidence"],
        false
    );
    assert_eq!(
        data["blockers"]
            .as_array()
            .expect("blockers should be an array")
            .len(),
        0
    );
    for service in data["service_receipts"]
        .as_array()
        .expect("service_receipts should be an array")
    {
        assert_eq!(service["status"], "verified");
    }
    for service in data["real_live_acceptance"]["services"]
        .as_array()
        .expect("services should be an array")
    {
        assert_eq!(service["completion_state"], "verified");
        assert_eq!(service["manual_live_required"], false);
        assert_eq!(service["must_not_count_as_complete"], false);
    }
}

#[test]
fn live_operator_receipt_collect_script_blocks_non_canonical_verified_evidence() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp_dir = make_temp_dir("chuang-live-operator-receipt-collect-non-canonical");
    let base_output = run_json_script(operator_receipt_script_path().as_path(), &[]);
    let base_path = write_temp_json(&temp_dir, "base.json", &base_output);
    let service_ids = vec![
        "feishu",
        "provider",
        "subagent_live_rehearsal",
        "desktop",
        "browser",
        "wiki",
        "gbrain",
    ];
    let evidence = serde_json::json!({
        "feishu": {"non_canonical": "receipt://feishu"},
        "provider": {"non_canonical": "receipt://provider"},
        "subagent_live_rehearsal": {"non_canonical": "receipt://subagent"},
        "desktop": {"non_canonical": "receipt://desktop"},
        "browser": {"non_canonical": "receipt://browser"},
        "wiki": {"non_canonical": "receipt://wiki"},
        "gbrain": {"non_canonical": "receipt://gbrain"}
    });

    let overlay = serde_json::json!({
        "service_evidence": evidence,
        "service_receipts": service_ids
            .iter()
            .map(|service_id| {
                serde_json::json!({
                    "id": service_id,
                    "status": "verified",
                    "evidence": evidence[*service_id].clone()
                })
            })
            .collect::<Vec<_>>(),
        "real_live_acceptance": {
            "complete": true,
            "status": "verified",
            "gap_count": 0,
            "services": service_ids
                .iter()
                .map(|service_id| {
                    serde_json::json!({
                        "id": service_id,
                        "completion_state": "verified",
                        "manual_live_required": false,
                        "must_not_count_as_complete": false
                    })
                })
                .collect::<Vec<_>>()
        },
        "blockers": []
    });
    let overlay_path = write_temp_json(&temp_dir, "overlay.json", &overlay);

    let output = Command::new("bash")
        .arg(script_path())
        .arg("--json")
        .arg("--base-file")
        .arg(&base_path)
        .arg("--overlay-file")
        .arg(&overlay_path)
        .env("CHUANG_AGENT_ROOT", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("collector should execute");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value =
        serde_json::from_slice(&output.stdout).expect("collector output should be json");

    assert_eq!(data["acceptance_status"], "not_verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["real_live_acceptance"]["complete"], false);
    assert_eq!(data["real_live_acceptance"]["gap_count"], 7);
    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be an array")
        .iter()
        .map(|item| item.as_str().expect("blocker should be string"))
        .collect::<Vec<_>>();
    assert!(blockers.contains(&"feishu: health_transcript_ref_missing_or_placeholder"));
    assert!(blockers.contains(&"provider: api_key_state_not_<set>"));
    assert!(blockers.contains(&"desktop: real_execution_not_true"));
}
