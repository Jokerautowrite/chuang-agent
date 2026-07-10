use std::fs;
use std::path::{Path, PathBuf};

use chuang_agent::governance::{risk_decision_label, OperatorApprovalEvidence};
use chuang_agent::operator_approval::{verify_operator_approval_ticket, OperatorApprovalTicket};
use chuang_agent::runtime_event_ledger::{InMemoryRuntimeEventLedger, RuntimeEventLedger};
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::tool_runtime::{
    OperatorApprovalReceipt, PendingApproval, ToolExecutionRecord, PENDING_APPROVAL_MAX_CALL_BYTES,
};
use serde::Serialize;

use crate::cli_args::parse_cli_options;
use crate::cli_output::{print_json, ControlOutputFormat};

const OPERATOR_APPROVAL_TRUST_ANCHOR_PATH: &str = "/etc/chuang-agent/operator-approval.pub";

#[derive(Debug, Serialize)]
pub(crate) struct ApprovalResumeOutput {
    pub(crate) approval_id: String,
    pub(crate) decision: String,
    pub(crate) ok: bool,
    pub(crate) approval_consumed: bool,
    pub(crate) record: ToolExecutionRecord,
    pub(crate) runtime_events: Vec<chuang_agent::runtime_event_ledger::RuntimeEvent>,
}

pub(crate) fn approval_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("resume") => approval_resume_command(&args[1..]),
        _ => Err(
            "usage: cargo run -- approval resume --workspace-root PATH --pending-file PATH --approval-ticket PATH --approve [--config PATH] [--json]"
                .to_string(),
        ),
    }
}

fn approval_resume_command(args: &[String]) -> Result<(), String> {
    let mut workspace_root = None;
    let mut pending_file = None;
    let mut approval_ticket = None;
    let mut approved = false;
    let mut output = ControlOutputFormat::Text;
    let mut runtime_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--workspace-root" => {
                workspace_root = Some(required_value(args, index, "--workspace-root")?);
                index += 2;
            }
            "--pending-file" => {
                pending_file = Some(required_value(args, index, "--pending-file")?);
                index += 2;
            }
            "--approval-ticket" => {
                approval_ticket = Some(required_value(args, index, "--approval-ticket")?);
                index += 2;
            }
            "--approval-public-key-file" => {
                return Err("approval_trust_anchor_override_forbidden".to_string())
            }
            "--approve" => {
                approved = true;
                index += 1;
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => {
                runtime_args.push(args[index].clone());
                if option_takes_value(&args[index]) {
                    runtime_args.push(required_value(args, index, &args[index])?);
                    index += 2;
                } else {
                    index += 1;
                }
            }
        }
    }

    if !approved {
        return Err("approval_resume_requires_explicit_--approve".to_string());
    }
    let workspace_root = PathBuf::from(
        workspace_root.ok_or_else(|| "approval_resume_requires_workspace_root".to_string())?,
    );
    let pending_file = PathBuf::from(
        pending_file.ok_or_else(|| "approval_resume_requires_pending_file".to_string())?,
    );
    let approval_ticket = PathBuf::from(
        approval_ticket.ok_or_else(|| "approval_resume_requires_approval_ticket".to_string())?,
    );
    let approval_public_key = read_trusted_approval_public_key()?;
    let approval_ticket = read_operator_approval_ticket(&approval_ticket)?;
    let options = parse_cli_options(&runtime_args)?;
    let result = resume_approval(
        &options.runtime,
        &workspace_root,
        &pending_file,
        &approval_ticket,
        &approval_public_key,
    )?;

    match output {
        ControlOutputFormat::Json => print_json(&result)?,
        ControlOutputFormat::Text => {
            println!("approval_id: {}", result.approval_id);
            println!("decision: {}", result.decision);
            println!("ok: {}", result.ok);
            println!("approval_consumed: {}", result.approval_consumed);
            println!("summary: {}", result.record.summary);
        }
    }
    Ok(())
}

fn resume_approval(
    runtime: &chuang_agent::runtime_config::RuntimeConfig,
    workspace_root: &Path,
    pending_file: &Path,
    approval_ticket: &OperatorApprovalTicket,
    approval_public_key: &str,
) -> Result<ApprovalResumeOutput, String> {
    let pending = read_pending_approval(workspace_root, pending_file)?;
    verify_operator_approval_ticket(approval_ticket, approval_public_key)?;
    let workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
        format!(
            "approval_workspace_invalid path={} error={error}",
            workspace_root.display()
        )
    })?;
    let active_identity = crate::cli_runtime::load_identity_bootstrap_snapshot(runtime)?
        .active_identity
        .ok_or_else(|| "approval_active_identity_unavailable".to_string())?;
    if pending.agent_id != active_identity.agent_id {
        return Err("approval_agent_identity_mismatch".to_string());
    }
    let receipt = OperatorApprovalReceipt {
        approval_id: approval_ticket.approval_id.clone(),
        call_id: approval_ticket.call_id.clone(),
        call_fingerprint: approval_ticket.call_fingerprint.clone(),
        target_fingerprint: approval_ticket.target_fingerprint.clone(),
        workspace_fingerprint: approval_ticket.workspace_fingerprint.clone(),
        policy_marker: approval_ticket.policy_marker.clone(),
        approved: true,
        operator_ref: approval_ticket.operator_ref.clone(),
        evidence_ref: approval_ticket.evidence_ref.clone(),
    };
    let mut slots = build_runtime_slots(runtime)
        .map_err(|error| format!("config_invalid: {}: {}", error.field, error.message))?;
    slots
        .governance
        .register_operator_approval(OperatorApprovalEvidence {
            approval_id: receipt.approval_id.clone(),
            operator_ref: receipt.operator_ref.clone(),
            evidence_ref: receipt.evidence_ref.clone(),
        })
        .map_err(|error| format!("operator_approval_register_failed: {}", error.message))?;
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let outcome = slots.execution.resume_approved_with_ledger(
        &mut ledger,
        "approval-resume",
        pending.call_id.clone(),
        &workspace_root,
        &mut slots.governance,
        pending.clone(),
        &receipt,
    )?;
    let runtime_events = ledger
        .list()
        .map_err(|error| format!("approval_event_ledger_read_failed: {error}"))?;

    Ok(ApprovalResumeOutput {
        approval_id: pending.approval_id,
        decision: risk_decision_label(&outcome.decision),
        ok: outcome.record.ok,
        approval_consumed: true,
        record: outcome.record,
        runtime_events,
    })
}

pub(crate) fn resume_local_tty_approval(
    runtime: &chuang_agent::runtime_config::RuntimeConfig,
    workspace_root: &Path,
    pending_file: &Path,
) -> Result<ApprovalResumeOutput, String> {
    let pending = read_pending_approval(workspace_root, pending_file)?;
    let workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
        format!(
            "approval_workspace_invalid path={} error={error}",
            workspace_root.display()
        )
    })?;
    let active_identity = crate::cli_runtime::load_identity_bootstrap_snapshot(runtime)?
        .active_identity
        .ok_or_else(|| "approval_active_identity_unavailable".to_string())?;
    if pending.agent_id != active_identity.agent_id {
        return Err("approval_agent_identity_mismatch".to_string());
    }

    let operator_ref = "operator:local-tty".to_string();
    let evidence_ref = format!("local-tty://approval/{}", pending.approval_id);
    let receipt = OperatorApprovalReceipt {
        approval_id: pending.approval_id.clone(),
        call_id: pending.call_id.clone(),
        call_fingerprint: pending.call_fingerprint.clone(),
        target_fingerprint: pending.target_fingerprint.clone(),
        workspace_fingerprint: pending.workspace_fingerprint.clone(),
        policy_marker: pending.policy_marker.clone(),
        approved: true,
        operator_ref: operator_ref.clone(),
        evidence_ref: evidence_ref.clone(),
    };
    let mut slots = build_runtime_slots(runtime)
        .map_err(|error| format!("config_invalid: {}: {}", error.field, error.message))?;
    slots
        .governance
        .register_operator_approval(OperatorApprovalEvidence {
            approval_id: pending.approval_id.clone(),
            operator_ref,
            evidence_ref,
        })
        .map_err(|error| format!("operator_approval_register_failed: {}", error.message))?;
    let mut ledger = InMemoryRuntimeEventLedger::new();
    let outcome = slots.execution.resume_approved_with_ledger(
        &mut ledger,
        "repl-local-approval",
        pending.call_id.clone(),
        &workspace_root,
        &mut slots.governance,
        pending.clone(),
        &receipt,
    )?;
    let runtime_events = ledger
        .list()
        .map_err(|error| format!("approval_event_ledger_read_failed: {error}"))?;

    Ok(ApprovalResumeOutput {
        approval_id: pending.approval_id,
        decision: risk_decision_label(&outcome.decision),
        ok: outcome.record.ok,
        approval_consumed: true,
        record: outcome.record,
        runtime_events,
    })
}

fn read_operator_approval_ticket(path: &Path) -> Result<OperatorApprovalTicket, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "operator_approval_ticket_read_failed path={} error={error}",
            path.display()
        )
    })?;
    if bytes.len() > PENDING_APPROVAL_MAX_CALL_BYTES {
        return Err("operator_approval_ticket_too_large".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|_| "operator_approval_ticket_invalid_json".to_string())
}

fn read_trusted_approval_public_key() -> Result<String, String> {
    let path = Path::new(OPERATOR_APPROVAL_TRUST_ANCHOR_PATH);
    let parent = path
        .parent()
        .ok_or_else(|| "operator_approval_trust_anchor_parent_missing".to_string())?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(|error| {
        format!(
            "operator_approval_trust_anchor_parent_read_failed path={} error={error}",
            parent.display()
        )
    })?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err("operator_approval_trust_anchor_parent_not_directory".to_string());
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "operator_approval_trust_anchor_read_failed path={} error={error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("operator_approval_trust_anchor_not_regular_file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if parent_metadata.uid() != 0 {
            return Err("operator_approval_trust_anchor_parent_not_root_owned".to_string());
        }
        if parent_metadata.mode() & 0o022 != 0 {
            return Err("operator_approval_trust_anchor_parent_is_writable".to_string());
        }
        if metadata.uid() != 0 {
            return Err("operator_approval_trust_anchor_not_root_owned".to_string());
        }
        if metadata.mode() & 0o022 != 0 {
            return Err("operator_approval_trust_anchor_is_writable".to_string());
        }
    }
    let value = fs::read_to_string(path).map_err(|error| {
        format!(
            "operator_approval_trust_anchor_read_failed path={} error={error}",
            path.display()
        )
    })?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err("operator_approval_trust_anchor_empty".to_string());
    }
    Ok(value)
}

fn read_pending_approval(
    workspace_root: &Path,
    pending_file: &Path,
) -> Result<PendingApproval, String> {
    let workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
        format!(
            "approval_workspace_invalid path={} error={error}",
            workspace_root.display()
        )
    })?;
    let pending_file = fs::canonicalize(pending_file).map_err(|error| {
        format!(
            "approval_pending_file_invalid path={} error={error}",
            pending_file.display()
        )
    })?;
    if !pending_file.starts_with(&workspace_root) {
        return Err("approval_pending_file_outside_workspace".to_string());
    }
    let bytes = fs::read(&pending_file)
        .map_err(|error| format!("approval_pending_file_read_failed: {error}"))?;
    if bytes.len() > PENDING_APPROVAL_MAX_CALL_BYTES.saturating_mul(2) {
        return Err("approval_pending_file_too_large".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|_| "approval_pending_file_invalid_json".to_string())
}

fn required_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn option_takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--config"
            | "--db"
            | "--identity-memory-root"
            | "--subagent"
            | "--subagent-queue-root"
            | "--context-engine"
            | "--context-max-tokens"
            | "--context-reserve-system-tokens"
            | "--context-min-working-tokens"
            | "--context-max-tool-results"
            | "--context-max-memory-segments"
            | "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport"
            | "--provider-request-timeout-ms"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use chuang_agent::runtime_event_ledger::InMemoryRuntimeEventLedger;
    use chuang_agent::tool_runtime::ToolCall;
    use ed25519_dalek::{Signer, SigningKey};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn signed_ticket(
        pending: &PendingApproval,
        signing_key: &SigningKey,
    ) -> OperatorApprovalTicket {
        let mut ticket = OperatorApprovalTicket {
            schema_version:
                chuang_agent::operator_approval::OPERATOR_APPROVAL_TICKET_SCHEMA_VERSION,
            approval_id: pending.approval_id.clone(),
            call_id: pending.call_id.clone(),
            call_fingerprint: pending.call_fingerprint.clone(),
            target_fingerprint: pending.target_fingerprint.clone(),
            workspace_fingerprint: pending.workspace_fingerprint.clone(),
            policy_marker: pending.policy_marker.clone(),
            operator_ref: "operator:test".to_string(),
            evidence_ref: "operator-evidence://test".to_string(),
            issued_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            signature: String::new(),
        };
        ticket.signature = STANDARD.encode(
            signing_key
                .sign(
                    &ticket
                        .signing_payload()
                        .expect("ticket payload should serialize"),
                )
                .to_bytes(),
        );
        ticket
    }

    #[test]
    fn explicit_resume_executes_persisted_pending_once_across_new_runtime_slots() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "chuang-approval-resume-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&workspace).expect("workspace should create");
        let runtime = chuang_agent::runtime_config::RuntimeConfig::new(workspace.join("memory.db"));
        let active_agent_id = crate::cli_runtime::load_identity_bootstrap_snapshot(&runtime)
            .expect("identity snapshot should load")
            .active_identity
            .expect("active identity should exist")
            .agent_id;
        let mut slots =
            build_runtime_slots(&runtime).expect("runtime slots should build for approval test");
        let mut ledger = InMemoryRuntimeEventLedger::new();
        let pending = slots
            .execution
            .execute_or_reject_with_governance_and_ledger(
                &mut ledger,
                "thread",
                "turn",
                &workspace,
                &mut slots.governance,
                &ToolCall::ShellExec {
                    command: "printf 'approved\\n' > approved.txt; # rm -rf notes".to_string(),
                    cwd: Some(".".to_string()),
                },
                active_agent_id,
                "turn:tool:1",
            )
            .expect("approval request should return")
            .pending_approval
            .expect("pending approval should exist");
        let pending_file = workspace.join("pending.json");
        fs::write(
            &pending_file,
            serde_json::to_vec_pretty(&pending).expect("pending should serialize"),
        )
        .expect("pending file should write");
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let public_key = STANDARD.encode(signing_key.verifying_key().to_bytes());
        let ticket = signed_ticket(&pending, &signing_key);

        let result = resume_approval(&runtime, &workspace, &pending_file, &ticket, &public_key)
            .expect("explicit approval should resume");
        assert!(result.ok);
        assert_eq!(
            fs::read_to_string(workspace.join("approved.txt")).expect("output should exist"),
            "approved\n"
        );

        let duplicate = resume_approval(&runtime, &workspace, &pending_file, &ticket, &public_key)
            .expect_err("consumed approval must not replay");
        assert_eq!(duplicate, "approval_already_consumed");
    }

    #[test]
    fn local_tty_resume_executes_exact_pending_once() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "chuang-local-tty-approval-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&workspace).expect("workspace should create");
        let runtime = chuang_agent::runtime_config::RuntimeConfig::new(workspace.join("memory.db"));
        let active_agent_id = crate::cli_runtime::load_identity_bootstrap_snapshot(&runtime)
            .expect("identity snapshot should load")
            .active_identity
            .expect("active identity should exist")
            .agent_id;
        let mut slots =
            build_runtime_slots(&runtime).expect("runtime slots should build for approval test");
        let mut ledger = InMemoryRuntimeEventLedger::new();
        let pending = slots
            .execution
            .execute_or_reject_with_governance_and_ledger(
                &mut ledger,
                "thread",
                "turn",
                &workspace,
                &mut slots.governance,
                &ToolCall::ShellExec {
                    command: "printf 'tty-approved\\n' > tty-approved.txt; # rm -rf notes"
                        .to_string(),
                    cwd: Some(".".to_string()),
                },
                active_agent_id,
                "turn:tool:1",
            )
            .expect("approval request should return")
            .pending_approval
            .expect("pending approval should exist");
        let pending_file = workspace.join("pending.json");
        fs::write(
            &pending_file,
            serde_json::to_vec_pretty(&pending).expect("pending should serialize"),
        )
        .expect("pending file should write");

        let result = resume_local_tty_approval(&runtime, &workspace, &pending_file)
            .expect("local tty approval should resume");
        assert!(result.ok);
        assert_eq!(
            fs::read_to_string(workspace.join("tty-approved.txt")).expect("output should exist"),
            "tty-approved\n"
        );
        assert!(result.runtime_events.iter().any(|event| event.event_type
            == chuang_agent::runtime_event_ledger::RuntimeEventKind::ApprovalResolved));

        let duplicate = resume_local_tty_approval(&runtime, &workspace, &pending_file)
            .expect_err("local tty approval must remain one-shot");
        assert_eq!(duplicate, "approval_already_consumed");
    }

    #[test]
    fn approval_resume_rejects_forged_ticket_before_execution() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let other_key = SigningKey::from_bytes(&[9_u8; 32]);
        let public_key = STANDARD.encode(signing_key.verifying_key().to_bytes());
        let pending = PendingApproval {
            approval_id: "approval-test".to_string(),
            call_id: "call-test".to_string(),
            agent_id: "chuang".to_string(),
            task_id: "task-test".to_string(),
            serialized_tool_call: "{}".to_string(),
            call_fingerprint: "call-fingerprint".to_string(),
            target_fingerprint: "target-fingerprint".to_string(),
            workspace_fingerprint: "workspace-fingerprint".to_string(),
            policy_marker: "policy-marker".to_string(),
            risk_decision: chuang_agent::tool_runtime::PendingRiskDecision {
                decision: "needs_approval".to_string(),
                reason: "test".to_string(),
            },
        };
        let forged = signed_ticket(&pending, &other_key);

        assert_eq!(
            verify_operator_approval_ticket(&forged, &public_key),
            Err("operator_approval_ticket_signature_invalid".to_string())
        );
    }

    #[test]
    fn approval_command_rejects_call_scoped_trust_anchor_override() {
        let args = vec![
            "resume".to_string(),
            "--approval-public-key-file".to_string(),
            "/tmp/attacker.pub".to_string(),
        ];

        assert_eq!(
            approval_command(&args),
            Err("approval_trust_anchor_override_forbidden".to_string())
        );
    }
}
