//! `operator_approval` 模块。公开接口：struct OperatorApprovalTicket；fn signing_payload, verify_operator_approval_ticket；const OPERATOR_APPROVAL_TICKET_SCHEMA_VERSION, OPERATOR_APPROVAL_MAX_AGE_SECONDS, OPERATOR_APPROVAL_MAX_FUTURE_SKEW_SECONDS。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const OPERATOR_APPROVAL_TICKET_SCHEMA_VERSION: u16 = 1;
pub const OPERATOR_APPROVAL_MAX_AGE_SECONDS: i64 = 15 * 60;
pub const OPERATOR_APPROVAL_MAX_FUTURE_SKEW_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorApprovalTicket {
    pub schema_version: u16,
    pub approval_id: String,
    pub call_id: String,
    pub call_fingerprint: String,
    pub target_fingerprint: String,
    pub workspace_fingerprint: String,
    pub policy_marker: String,
    pub operator_ref: String,
    pub evidence_ref: String,
    pub issued_at: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct OperatorApprovalPayload<'a> {
    schema_version: u16,
    approval_id: &'a str,
    call_id: &'a str,
    call_fingerprint: &'a str,
    target_fingerprint: &'a str,
    workspace_fingerprint: &'a str,
    policy_marker: &'a str,
    operator_ref: &'a str,
    evidence_ref: &'a str,
    issued_at: &'a str,
}

impl OperatorApprovalTicket {
    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&OperatorApprovalPayload {
            schema_version: self.schema_version,
            approval_id: &self.approval_id,
            call_id: &self.call_id,
            call_fingerprint: &self.call_fingerprint,
            target_fingerprint: &self.target_fingerprint,
            workspace_fingerprint: &self.workspace_fingerprint,
            policy_marker: &self.policy_marker,
            operator_ref: &self.operator_ref,
            evidence_ref: &self.evidence_ref,
            issued_at: &self.issued_at,
        })
        .map_err(|_| "operator_approval_ticket_payload_serialize_failed".to_string())
    }
}

pub fn verify_operator_approval_ticket(
    ticket: &OperatorApprovalTicket,
    public_key_base64: &str,
) -> Result<(), String> {
    verify_operator_approval_ticket_at(ticket, public_key_base64, Utc::now())
}

fn verify_operator_approval_ticket_at(
    ticket: &OperatorApprovalTicket,
    public_key_base64: &str,
    now: DateTime<Utc>,
) -> Result<(), String> {
    if ticket.schema_version != OPERATOR_APPROVAL_TICKET_SCHEMA_VERSION {
        return Err("operator_approval_ticket_schema_unsupported".to_string());
    }
    if ticket.approval_id.trim().is_empty()
        || ticket.call_id.trim().is_empty()
        || ticket.call_fingerprint.trim().is_empty()
        || ticket.target_fingerprint.trim().is_empty()
        || ticket.workspace_fingerprint.trim().is_empty()
        || ticket.policy_marker.trim().is_empty()
        || ticket.operator_ref.trim().is_empty()
        || ticket.evidence_ref.trim().is_empty()
        || ticket.issued_at.trim().is_empty()
        || ticket.signature.trim().is_empty()
    {
        return Err("operator_approval_ticket_fields_required".to_string());
    }
    let issued_at = DateTime::parse_from_rfc3339(ticket.issued_at.trim())
        .map_err(|_| "operator_approval_ticket_issued_at_invalid".to_string())?
        .with_timezone(&Utc);
    if issued_at > now + Duration::seconds(OPERATOR_APPROVAL_MAX_FUTURE_SKEW_SECONDS) {
        return Err("operator_approval_ticket_issued_in_future".to_string());
    }
    if issued_at < now - Duration::seconds(OPERATOR_APPROVAL_MAX_AGE_SECONDS) {
        return Err("operator_approval_ticket_expired".to_string());
    }

    let public_key_bytes = STANDARD
        .decode(public_key_base64.trim())
        .map_err(|_| "operator_approval_public_key_invalid_base64".to_string())?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| "operator_approval_public_key_invalid_length".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|_| "operator_approval_public_key_invalid".to_string())?;

    let signature_bytes = STANDARD
        .decode(ticket.signature.trim())
        .map_err(|_| "operator_approval_ticket_signature_invalid_base64".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "operator_approval_ticket_signature_invalid_length".to_string())?;
    verifying_key
        .verify(&ticket.signing_payload()?, &signature)
        .map_err(|_| "operator_approval_ticket_signature_invalid".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_ticket(signing_key: &SigningKey, issued_at: &str) -> OperatorApprovalTicket {
        let mut ticket = OperatorApprovalTicket {
            schema_version: OPERATOR_APPROVAL_TICKET_SCHEMA_VERSION,
            approval_id: "approval-test".to_string(),
            call_id: "call-test".to_string(),
            call_fingerprint: "call-fingerprint".to_string(),
            target_fingerprint: "target-fingerprint".to_string(),
            workspace_fingerprint: "workspace-fingerprint".to_string(),
            policy_marker: "policy-marker".to_string(),
            operator_ref: "operator:test".to_string(),
            evidence_ref: "operator-evidence://test".to_string(),
            issued_at: issued_at.to_string(),
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
    fn valid_recent_ticket_verifies() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let now = DateTime::parse_from_rfc3339("2026-07-10T08:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);
        let ticket = signed_ticket(&signing_key, "2026-07-10T07:59:30Z");
        let public_key = STANDARD.encode(signing_key.verifying_key().to_bytes());

        assert_eq!(
            verify_operator_approval_ticket_at(&ticket, &public_key, now),
            Ok(())
        );
    }

    #[test]
    fn modifying_any_signed_field_rejects_ticket() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let now = DateTime::parse_from_rfc3339("2026-07-10T08:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);
        let public_key = STANDARD.encode(signing_key.verifying_key().to_bytes());
        let mut ticket = signed_ticket(&signing_key, "2026-07-10T07:59:30Z");
        ticket.target_fingerprint.push_str("-modified");

        assert_eq!(
            verify_operator_approval_ticket_at(&ticket, &public_key, now),
            Err("operator_approval_ticket_signature_invalid".to_string())
        );
    }

    #[test]
    fn wrong_key_and_invalid_signature_length_are_rejected() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let wrong_key = SigningKey::from_bytes(&[9_u8; 32]);
        let now = DateTime::parse_from_rfc3339("2026-07-10T08:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);
        let public_key = STANDARD.encode(wrong_key.verifying_key().to_bytes());
        let mut ticket = signed_ticket(&signing_key, "2026-07-10T07:59:30Z");

        assert_eq!(
            verify_operator_approval_ticket_at(&ticket, &public_key, now),
            Err("operator_approval_ticket_signature_invalid".to_string())
        );

        ticket.signature = STANDARD.encode([0_u8; 8]);
        assert_eq!(
            verify_operator_approval_ticket_at(&ticket, &public_key, now),
            Err("operator_approval_ticket_signature_invalid_length".to_string())
        );
    }

    #[test]
    fn stale_future_and_malformed_timestamps_are_rejected() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let public_key = STANDARD.encode(signing_key.verifying_key().to_bytes());
        let now = DateTime::parse_from_rfc3339("2026-07-10T08:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc);

        let stale = signed_ticket(&signing_key, "2026-07-10T07:44:59Z");
        assert_eq!(
            verify_operator_approval_ticket_at(&stale, &public_key, now),
            Err("operator_approval_ticket_expired".to_string())
        );

        let future = signed_ticket(&signing_key, "2026-07-10T08:01:01Z");
        assert_eq!(
            verify_operator_approval_ticket_at(&future, &public_key, now),
            Err("operator_approval_ticket_issued_in_future".to_string())
        );

        let malformed = signed_ticket(&signing_key, "not-a-timestamp");
        assert_eq!(
            verify_operator_approval_ticket_at(&malformed, &public_key, now),
            Err("operator_approval_ticket_issued_at_invalid".to_string())
        );
    }
}
