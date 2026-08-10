use std::process::Command;

use serde_json::Value;

#[test]
fn cli_external_ai_dispatch_outputs_dry_run_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "external-ai",
            "dispatch",
            "--platform",
            "kimi",
            "--task",
            "collect architecture concerns",
            "--context",
            "bounded context only",
            "--session-hint",
            "session-1",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("external-ai dispatch should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");
    assert_eq!(parsed["adapter"], "unified_identity_engine");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["connects_real_service"], false);
    assert_eq!(parsed["writes_memory"], false);
    assert_eq!(parsed["request"]["platform"], "kimi");
    assert_eq!(parsed["request"]["session_hint"], "session-1");
    assert_eq!(parsed["result"]["quality"], "acceptable");
    assert!(parsed["result"]["audit_id"]
        .as_str()
        .expect("audit id")
        .starts_with("external-ai-kimi-"));
}

#[test]
fn cli_external_ai_dispatch_rejects_unsupported_live_platform() {
    let output = Command::new(env!("CARGO_BIN_EXE_chuang-agent"))
        .args([
            "external-ai",
            "dispatch",
            "--platform",
            "kimi",
            "--task",
            "collect architecture concerns",
            "--context",
            "bounded context only",
        ])
        .output()
        .expect("external-ai dispatch should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("live platform must be opencodex or openai-compatible"));
}
