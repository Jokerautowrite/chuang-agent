use crate::common::AuditRecord;

use super::{
    ActionKind, Governance, GovernanceError, MarkdownRuleSet, ProposedAction, RiskDecision,
};

#[derive(Debug, Default, Clone)]
pub struct StaticRuleGovernance {
    audit_records: Vec<AuditRecord>,
    rules: Option<MarkdownRuleSet>,
}

impl StaticRuleGovernance {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rules(rules: MarkdownRuleSet) -> Self {
        Self {
            audit_records: Vec::new(),
            rules: Some(rules),
        }
    }

    pub fn audit_records(&self) -> &[AuditRecord] {
        &self.audit_records
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

        let decision = match action.kind {
            ActionKind::Observe | ActionKind::Draft => RiskDecision::Allowed {
                reason: "read-only or draft action".to_string(),
            },
            ActionKind::LocalFileWrite | ActionKind::ShellCommand => RiskDecision::Allowed {
                reason: "local action allowed by static policy".to_string(),
            },
            ActionKind::ExternalSend | ActionKind::PublicPost => RiskDecision::NeedsApproval {
                reason: "external communication requires approval".to_string(),
            },
            ActionKind::Payment | ActionKind::VerificationCodeInput => {
                RiskDecision::NeedsApproval {
                    reason: "account-sensitive action requires approval".to_string(),
                }
            }
            ActionKind::DeleteOrCleanup => RiskDecision::NeedsApproval {
                reason: "destructive action requires explicit target approval".to_string(),
            },
            ActionKind::ServiceChange | ActionKind::NetworkChange => RiskDecision::NeedsApproval {
                reason: "system-disrupting action requires approval".to_string(),
            },
            ActionKind::SecretAccess => RiskDecision::DraftOnly {
                reason: "secret-bearing action may only produce a safe plan".to_string(),
            },
        };

        Ok(self.attach_rules(decision, action))
    }

    fn audit(&mut self, record: AuditRecord) -> Result<(), GovernanceError> {
        self.audit_records.push(record);
        Ok(())
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
