#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/chuang-global-real-live-receipt.sh")
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

fn write_json(dir: &Path, name: &str, value: &Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(
        &path,
        serde_json::to_string_pretty(value).expect("json should serialize"),
    )
    .expect("json file should write");
    path
}

fn run_global_receipt(args: &[(&str, &Path)]) -> Value {
    let mut command = Command::new("bash");
    command.arg(script_path()).arg("--json");
    for (flag, path) in args {
        command.arg(flag).arg(path);
    }
    let output = command
        .env("CHUANG_AGENT_ROOT", env!("CARGO_MANIFEST_DIR"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("global receipt script should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("global receipt output should be json")
}

fn verified_feishu_receipt() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "receipt_kind": "feishu_live_readonly_receipt",
        "request_id": "feishu-live-verified",
        "acceptance_status": "verified",
        "blockers": []
    })
}

fn verified_provider_receipt() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "receipt_kind": "provider_live_request_receipt",
        "request_id": "provider-live-verified",
        "ok": true,
        "status": "verified",
        "provider_kind": "openai_compatible",
        "transport_mode": "native",
        "api_key_state": "<set>",
        "request_path": "/v1/responses",
        "provider_response_ok": "true",
        "provider_fallback_used": "false",
        "runtime_report_id": "report-turn-provider-1"
    })
}

fn verified_subagent_receipt() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "receipt_kind": "single_worker_rehearsal_live_receipt",
        "dispatch": {
            "run_id": "run-subagent-verified",
            "task_id": "task-subagent-verified"
        },
        "worker_execution": {
            "run_ids": ["worker-subagent-verified"]
        },
        "collect": {
            "admission_status": "Accepted",
            "report_id": "report-subagent-verified"
        },
        "real_live_acceptance": {
            "single_worker_rehearsal_complete": true
        },
        "blockers": []
    })
}

fn verified_desktop_receipt() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "receipt_kind": "desktop_action_live_receipt",
        "request_id": "desktop-live-verified",
        "audit_label": "actuator.operation.live",
        "real_execution": true,
        "blockers": []
    })
}

fn verified_browser_receipt() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "receipt_kind": "browser_read_live_readonly_receipt",
        "request_id": "browser-live-verified",
        "acceptance_status": "verified",
        "browser_read_evidence": {
            "adapter_kind": "cdp"
        },
        "blockers": []
    })
}

fn verified_knowledge_receipt(service: &str) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "receipt_kind": format!("{service}_live_readonly_receipt"),
        "request_id": format!("{service}-live-verified"),
        "source": service,
        "acceptance_status": "verified",
        "request_sent": true,
        "read_only": true,
        "writes_automatically": false,
        "http_status": 200,
        "blockers": []
    })
}

fn write_all_verified_receipts(dir: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        (
            "--feishu-file",
            write_json(dir, "feishu.json", &verified_feishu_receipt()),
        ),
        (
            "--provider-file",
            write_json(dir, "provider.json", &verified_provider_receipt()),
        ),
        (
            "--subagent-file",
            write_json(dir, "subagent.json", &verified_subagent_receipt()),
        ),
        (
            "--desktop-file",
            write_json(dir, "desktop.json", &verified_desktop_receipt()),
        ),
        (
            "--browser-file",
            write_json(dir, "browser.json", &verified_browser_receipt()),
        ),
        (
            "--wiki-file",
            write_json(dir, "wiki.json", &verified_knowledge_receipt("wiki")),
        ),
        (
            "--gbrain-file",
            write_json(dir, "gbrain.json", &verified_knowledge_receipt("gbrain")),
        ),
    ]
}

#[test]
fn global_real_live_receipt_script_is_bounded_aggregator() {
    let script =
        fs::read_to_string(script_path()).expect("global receipt script should be readable");

    assert!(script.contains("provider live request is not executed unless"));
    assert!(script.contains("CHUANG_GLOBAL_RECEIPT_INCLUDE_PROVIDER_LIVE"));
    assert!(script.contains("desktop dry-run rehearsal is not promoted"));
    assert!(script.contains("chuang-live-operator-receipt-collect.sh"));
    assert!(!script.contains("systemctl"));
    assert!(!script.contains("git reset"));
    assert!(!script.contains("git checkout"));
    assert!(!script.contains("rm "));
    assert!(!script.contains("hermes-gateway"));
}

#[test]
fn global_real_live_receipt_blocks_provider_without_live_opt_in() {
    let temp_dir = make_temp_dir("chuang-global-real-live-receipt-provider-blocked");
    let mut receipt_files = write_all_verified_receipts(&temp_dir);
    receipt_files.retain(|(flag, _)| *flag != "--provider-file");
    let args = receipt_files
        .iter()
        .map(|(flag, path)| (*flag, path.as_path()))
        .collect::<Vec<_>>();

    let data = run_global_receipt(&args);

    assert_eq!(data["acceptance_status"], "not_verified");
    assert_eq!(data["can_mark_real_live_ready"], false);
    assert_eq!(data["real_live_acceptance"]["complete"], false);
    assert_eq!(data["service_receipts"][1]["id"], "provider");
    assert_eq!(data["service_receipts"][1]["status"], "blocked");

    let blockers = data["blockers"]
        .as_array()
        .expect("blockers should be an array")
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(blockers.contains(&"provider: provider_live_request_not_enabled"));
}

#[test]
fn global_real_live_receipt_marks_ready_only_from_all_verified_source_receipts() {
    let temp_dir = make_temp_dir("chuang-global-real-live-receipt-complete");
    let receipt_files = write_all_verified_receipts(&temp_dir);
    let args = receipt_files
        .iter()
        .map(|(flag, path)| (*flag, path.as_path()))
        .collect::<Vec<_>>();

    let data = run_global_receipt(&args);

    assert_eq!(data["acceptance_status"], "verified");
    assert_eq!(data["can_mark_real_live_ready"], true);
    assert_eq!(
        data["cannot_mark_complete_without_operator_evidence"],
        false
    );
    assert_eq!(data["real_live_acceptance"]["complete"], true);
    assert_eq!(data["real_live_acceptance"]["gap_count"], 0);
    assert_eq!(data["blockers"], serde_json::json!([]));

    for service in data["service_receipts"]
        .as_array()
        .expect("service receipts should be an array")
    {
        assert_eq!(service["status"], "verified");
    }
    for service in data["real_live_acceptance"]["services"]
        .as_array()
        .expect("acceptance services should be an array")
    {
        assert_eq!(service["completion_state"], "verified");
        assert_eq!(service["manual_live_required"], false);
        assert_eq!(service["must_not_count_as_complete"], false);
    }
}
