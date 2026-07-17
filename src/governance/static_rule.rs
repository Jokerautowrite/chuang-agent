use crate::common::AuditRecord;
use std::collections::BTreeSet;

use super::{
    ActionKind, Governance, GovernanceError, MarkdownRuleSet, OperatorApprovalEvidence,
    ProposedAction, RiskDecision,
};
use crate::permission_profile_slot::{
    decide_tag, full_local_workspace_profile, PermissionDecision, PermissionProfile, PermissionTag,
};

#[derive(Debug, Clone)]
pub struct StaticRuleGovernance {
    audit_records: Vec<AuditRecord>,
    rules: Option<MarkdownRuleSet>,
    operator_approvals: BTreeSet<OperatorApprovalEvidence>,
    permission_profile: PermissionProfile,
}

impl Default for StaticRuleGovernance {
    fn default() -> Self {
        Self {
            audit_records: Vec::new(),
            rules: None,
            operator_approvals: BTreeSet::new(),
            permission_profile: full_local_workspace_profile(),
        }
    }
}

impl StaticRuleGovernance {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rules(rules: MarkdownRuleSet) -> Self {
        Self {
            audit_records: Vec::new(),
            rules: Some(rules),
            operator_approvals: BTreeSet::new(),
            permission_profile: full_local_workspace_profile(),
        }
    }

    pub fn with_profile(permission_profile: PermissionProfile) -> Self {
        Self {
            permission_profile,
            ..Self::default()
        }
    }

    pub fn audit_records(&self) -> &[AuditRecord] {
        &self.audit_records
    }

    pub fn register_operator_approval(
        &mut self,
        evidence: OperatorApprovalEvidence,
    ) -> Result<(), GovernanceError> {
        if evidence.approval_id.trim().is_empty()
            || evidence.operator_ref.trim().is_empty()
            || evidence.evidence_ref.trim().is_empty()
        {
            return Err(GovernanceError {
                message: "operator approval evidence fields must not be empty".to_string(),
            });
        }
        self.operator_approvals.insert(evidence);
        Ok(())
    }
}

impl Governance for StaticRuleGovernance {
    fn classify(&self, action: &ProposedAction) -> Result<RiskDecision, GovernanceError> {
        if action.target.trim().is_empty() {
            return Ok(self.attach_rules(
                RiskDecision::Blocked {
                    reason: "target must be explicit".to_string(),
                },
                action,
            ));
        }

        let decision = permission_decision_for_action(&self.permission_profile, &action.kind);

        Ok(self.attach_rules(decision, action))
    }

    fn audit(&mut self, record: AuditRecord) -> Result<(), GovernanceError> {
        self.audit_records.push(record);
        Ok(())
    }

    fn verify_operator_approval(
        &self,
        evidence: &OperatorApprovalEvidence,
    ) -> Result<(), GovernanceError> {
        if self.operator_approvals.contains(evidence) {
            Ok(())
        } else {
            Err(GovernanceError {
                message: "operator approval evidence is not registered".to_string(),
            })
        }
    }
}

fn permission_decision_for_action(profile: &PermissionProfile, kind: &ActionKind) -> RiskDecision {
    let (tag, label) = match kind {
        ActionKind::Observe | ActionKind::Draft => (PermissionTag::Read, "read-only or draft"),
        ActionKind::LocalDesktopInteraction => {
            (PermissionTag::OpenApp, "local desktop interaction")
        }
        ActionKind::LocalFileWrite => (PermissionTag::FileWrite, "local file write"),
        ActionKind::ShellCommand => (PermissionTag::CodeExecute, "local code execution"),
        ActionKind::SubagentDispatch => (PermissionTag::CodeExecute, "local subagent dispatch"),
        ActionKind::ExternalSend => (PermissionTag::ExternalSend, "external send"),
        ActionKind::PublicPost => (PermissionTag::PublicPost, "public post"),
        ActionKind::Payment => (PermissionTag::Payment, "payment"),
        ActionKind::VerificationCodeInput => {
            (PermissionTag::VerificationCode, "verification code input")
        }
        ActionKind::DeleteOrCleanup => (PermissionTag::Delete, "delete or cleanup"),
        ActionKind::SecretAccess => (PermissionTag::SecretAccess, "secret material access"),
        ActionKind::PrivilegeEscalation => {
            (PermissionTag::PrivilegeEscalation, "privilege escalation")
        }
        ActionKind::ServiceChange => (PermissionTag::ServiceControl, "service change"),
        ActionKind::NetworkChange => (PermissionTag::NetworkChange, "network change"),
    };
    let risk = decide_tag(profile, tag);
    let reason = format!(
        "profile={} action={} permission={:?}",
        profile.name, label, risk.decision
    );

    match risk.decision {
        PermissionDecision::Allow | PermissionDecision::AllowWithAudit => {
            RiskDecision::Allowed { reason }
        }
        PermissionDecision::RequireApproval
        | PermissionDecision::RequireApprovalOrProjectTrust
        | PermissionDecision::RequireApprovalOrDeny
        | PermissionDecision::RequireExplicitTargetApproval => {
            RiskDecision::NeedsApproval { reason }
        }
    }
}

impl StaticRuleGovernance {
    fn attach_rules(&self, decision: RiskDecision, action: &ProposedAction) -> RiskDecision {
        let Some(rules) = &self.rules else {
            return decision;
        };
        let check = rules.check(action);
        let suffix = format!("; rules={}", check.fingerprint);

        match decision {
            RiskDecision::Allowed { reason } => RiskDecision::Allowed {
                reason: format!("{reason}{suffix}"),
            },
            RiskDecision::DraftOnly { reason } => RiskDecision::DraftOnly {
                reason: format!("{reason}{suffix}"),
            },
            RiskDecision::NeedsApproval { reason } => RiskDecision::NeedsApproval {
                reason: format!("{reason}{suffix}"),
            },
            RiskDecision::Blocked { reason } => RiskDecision::Blocked {
                reason: format!("{reason}{suffix}"),
            },
        }
    }
}
