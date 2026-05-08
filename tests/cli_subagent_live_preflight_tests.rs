use std::process::Command;

use serde_json::Value;

fn cargo_command() -> Command {
    let mut command = Command::new("cargo");
    command.env("CODEX_PPTOKEN_API_KEY", "test-key");
    command
}

#[test]
fn cli_subagent_live_preflight_is_readonly_and_reports_disabled_gate() {
    let output = cargo_command()
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "rust",
            "--requires-capability",
            "filesystem",
            "--capability",
            "rust",
            "--capability",
            "filesystem",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    assert_eq!(rehearsal["ok"], true);
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["readonly"], true);
    assert_eq!(rehearsal["starts_external_worker"], false);
    assert_eq!(rehearsal["gate_enabled"], false);
    assert_eq!(rehearsal["runner_allowlist_ok"], true);
    assert_eq!(rehearsal["capability_routing_ok"], true);
    assert_eq!(rehearsal["report_admission_ok"], true);
    assert_eq!(rehearsal["forbidden_capabilities_ok"], true);
    assert_eq!(rehearsal["approval_audit_prerequisites_ok"], true);
    assert_eq!(rehearsal["gate"]["enabled"], false);
    assert_eq!(
        rehearsal["gate"]["required_env"],
        "CHUANG_CODEX_RUNNER_ENABLE"
    );
    assert_eq!(rehearsal["gate"]["default_enabled"], false);
    assert!(rehearsal["gate"]["preflight_checks"]
        .as_array()
        .expect("preflight checks")
        .iter()
        .any(|check| check
            .as_str()
            .expect("preflight check")
            .contains("runner command allowlist")));
    assert_eq!(rehearsal["runner_allowlist"]["ok"], true);
    assert_eq!(
        rehearsal["runner_allowlist"]["matched_runner_command"],
        "scripts/chuang-codex-runner.py"
    );
    assert_eq!(rehearsal["capability_routing"]["ok"], true);
    assert_eq!(
        rehearsal["capability_routing"]["matched_capabilities"][0],
        "rust"
    );
    assert_eq!(rehearsal["report_admission"]["ok"], true);
    assert!(rehearsal["report_admission"]["covered_commands"]
        .as_array()
        .expect("covered commands")
        .iter()
        .any(|command| command == "run-once"));
    assert!(rehearsal["report_admission"]["stable_reason_codes"]
        .as_array()
        .expect("stable reason codes")
        .iter()
        .any(|code| code == "command_protocol_report_rejected"));
    assert_eq!(rehearsal["forbidden_capabilities"]["ok"], true);
    assert_eq!(rehearsal["approval_audit_prerequisites"]["ok"], true);
    assert_eq!(
        rehearsal["approval_audit_prerequisites"]["audit_label"],
        "subagent.runner.live"
    );
    assert_eq!(
        rehearsal["approval_audit_prerequisites"]["audit_receipt_required"],
        true
    );
}

#[test]
fn cli_subagent_live_preflight_is_ready_only_when_gate_is_explicitly_enabled() {
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "rust",
            "--capability",
            "rust",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    assert_eq!(rehearsal["ok"], true);
    assert_eq!(rehearsal["ready_for_live"], true);
    assert_eq!(rehearsal["readonly"], true);
    assert_eq!(rehearsal["starts_external_worker"], false);
    assert_eq!(rehearsal["gate_enabled"], true);
    assert_eq!(rehearsal["runner_allowlist_ok"], true);
    assert_eq!(rehearsal["capability_routing_ok"], true);
    assert_eq!(rehearsal["report_admission_ok"], true);
    assert_eq!(rehearsal["forbidden_capabilities_ok"], true);
    assert_eq!(rehearsal["approval_audit_prerequisites_ok"], true);
    assert_eq!(rehearsal["gate"]["env_value_state"], "enabled");
    assert_eq!(
        rehearsal["approval_audit_prerequisites"]["governance_approval_required"],
        true
    );
    assert_eq!(
        rehearsal["approval_audit_prerequisites"]["dispatch_evidence_required"],
        true
    );
    assert!(rehearsal["approval_audit_prerequisites"]["prerequisites"]
        .as_array()
        .expect("prerequisites")
        .iter()
        .any(|item| item
            .as_str()
            .expect("prerequisite")
            .contains("audit receipt includes dispatch id")));
}

#[test]
fn cli_subagent_live_preflight_rejects_non_enabling_gate_value() {
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "true")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "rust",
            "--capability",
            "rust",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    assert_eq!(rehearsal["ok"], true);
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["gate_enabled"], false);
    assert_eq!(rehearsal["gate"]["env_value_state"], "set_non_enabling");
}

#[test]
fn cli_subagent_live_preflight_text_uses_stable_gate_field_names() {
    let output = cargo_command()
        .env_remove("CHUANG_CODEX_RUNNER_ENABLE")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "rust",
            "--capability",
            "rust",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("ready_for_live=false"));
    assert!(stdout.contains("readonly=true"));
    assert!(stdout.contains("gate_enabled=false"));
    assert!(stdout.contains("runner_allowlist_ok=true"));
    assert!(stdout.contains("capability_routing_ok=true"));
    assert!(stdout.contains("report_admission_ok=true"));
    assert!(stdout.contains("forbidden_capabilities_ok=true"));
    assert!(stdout.contains("approval_audit_prerequisites_ok=true"));
    assert!(stdout.contains("gate enabled=false env_value_state=unset"));
    assert!(stdout.contains("runner_allowlist ok=true"));
    assert!(stdout.contains("exact_match_required=true"));
    assert!(stdout.contains("capability_routing ok=true"));
    assert!(stdout.contains("matched_capabilities=rust"));
    assert!(stdout.contains("report_admission ok=true"));
    assert!(stdout.contains("covered_commands=run-once,run-loop,report,collect"));
    assert!(stdout.contains("forbidden_capabilities ok=true"));
    assert!(stdout.contains("checked_capability_sources=dispatch required_capabilities"));
    assert!(stdout.contains("approval_audit_prerequisites ok=true"));
    assert!(stdout.contains("audit_receipt_required=true"));
    assert!(stdout.contains("next_action="));
}

#[test]
fn cli_subagent_live_preflight_requires_runner_allowlist_match() {
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/other-runner.py",
            "--capability",
            "rust",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    assert_eq!(rehearsal["ok"], false);
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["gate"]["enabled"], true);
    assert_eq!(rehearsal["gate_enabled"], true);
    assert_eq!(rehearsal["runner_allowlist"]["ok"], false);
    assert_eq!(rehearsal["runner_allowlist_ok"], false);
    assert!(rehearsal["runner_allowlist"]["reason"]
        .as_str()
        .expect("reason")
        .contains("not present"));
}

#[test]
fn cli_subagent_live_preflight_rejects_capability_mismatch_even_when_gate_is_enabled() {
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "rust",
            "--capability",
            "filesystem",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    assert_eq!(rehearsal["ok"], false);
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["gate_enabled"], true);
    assert_eq!(rehearsal["capability_routing_ok"], false);
    assert_eq!(rehearsal["starts_external_worker"], false);
    assert_eq!(rehearsal["capability_routing"]["ok"], false);
    assert_eq!(
        rehearsal["capability_routing"]["missing_capabilities"],
        serde_json::json!(["rust"])
    );
    assert_eq!(
        rehearsal["capability_routing"]["matched_capabilities"],
        serde_json::json!([])
    );
    assert!(rehearsal["capability_routing"]["reason"]
        .as_str()
        .expect("reason")
        .contains("do not satisfy"));
}

#[test]
fn cli_subagent_live_preflight_requires_declared_dispatch_capability_route() {
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--capability",
            "rust",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let rehearsal = &parsed["rehearsal"];

    assert_eq!(rehearsal["ok"], false);
    assert_eq!(rehearsal["ready_for_live"], false);
    assert_eq!(rehearsal["gate_enabled"], true);
    assert_eq!(rehearsal["runner_allowlist_ok"], true);
    assert_eq!(rehearsal["capability_routing_ok"], false);
    assert_eq!(rehearsal["starts_external_worker"], false);
    assert_eq!(rehearsal["capability_routing"]["ok"], false);
    assert_eq!(
        rehearsal["capability_routing"]["required_capabilities"],
        serde_json::json!([])
    );
    assert_eq!(
        rehearsal["capability_routing"]["worker_capabilities"],
        serde_json::json!(["rust"])
    );
    assert!(rehearsal["capability_routing"]["reason"]
        .as_str()
        .expect("reason")
        .contains("required_capabilities must be declared"));
}

#[test]
fn cli_subagent_live_preflight_rejects_forbidden_subagent_capability() {
    let output = cargo_command()
        .env("CHUANG_CODEX_RUNNER_ENABLE", "1")
        .args([
            "run",
            "--quiet",
            "--",
            "subagent",
            "live-preflight",
            "--runner-command",
            "scripts/chuang-codex-runner.py",
            "--allow-runner-command",
            "scripts/chuang-codex-runner.py",
            "--requires-capability",
            "core-memory-write",
            "--capability",
            "core-memory-write",
            "--json",
        ])
        .output()
        .expect("cargo run should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("stdout json");
    let forbidden = &parsed["rehearsal"]["forbidden_capabilities"];

    assert_eq!(parsed["rehearsal"]["ok"], false);
    assert_eq!(parsed["rehearsal"]["ready_for_live"], false);
    assert_eq!(parsed["rehearsal"]["forbidden_capabilities_ok"], false);
    assert_eq!(forbidden["ok"], false);
    assert_eq!(
        forbidden["requested_forbidden_capabilities"][0],
        "core-memory-write"
    );
}
