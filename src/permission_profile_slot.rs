use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionProfileId {
    LocalGa,
    SafeDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionTag {
    Read,
    List,
    Status,
    Observe,
    Screenshot,
    FileWrite,
    CodeExecute,
    OpenApp,
    Click,
    Input,
    ExternalSend,
    PublicPost,
    Payment,
    Order,
    VerificationCode,
    Delete,
    Cleanup,
    Reset,
    Uninstall,
    Purge,
    ServiceControl,
    NetworkChange,
    SecretAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    AllowWithAudit,
    RequireApproval,
    RequireApprovalOrProjectTrust,
    RequireApprovalOrDeny,
    RequireExplicitTargetApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tag: PermissionTag,
    pub decision: PermissionDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionProfile {
    pub id: PermissionProfileId,
    pub name: &'static str,
    pub rules: &'static [PermissionRule],
    pub default_decision: PermissionDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolDescriptorRisk<'a> {
    pub name: &'a str,
    pub risk_tags: &'a [&'a str],
    pub read_only: bool,
    pub mutating: bool,
    pub destructive: bool,
    pub external_commit: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRiskDecision {
    pub profile: PermissionProfileId,
    pub decision: PermissionDecision,
    pub matched_tag: Option<PermissionTag>,
    pub reason: String,
}

const LOCAL_GA_RULES: &[PermissionRule] = &[
    PermissionRule {
        tag: PermissionTag::Read,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::List,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::Status,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::Observe,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::Screenshot,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::FileWrite,
        decision: PermissionDecision::AllowWithAudit,
    },
    PermissionRule {
        tag: PermissionTag::CodeExecute,
        decision: PermissionDecision::AllowWithAudit,
    },
    PermissionRule {
        tag: PermissionTag::OpenApp,
        decision: PermissionDecision::AllowWithAudit,
    },
    PermissionRule {
        tag: PermissionTag::Click,
        decision: PermissionDecision::AllowWithAudit,
    },
    PermissionRule {
        tag: PermissionTag::Input,
        decision: PermissionDecision::AllowWithAudit,
    },
    PermissionRule {
        tag: PermissionTag::ExternalSend,
        decision: PermissionDecision::RequireApproval,
    },
    PermissionRule {
        tag: PermissionTag::PublicPost,
        decision: PermissionDecision::RequireApproval,
    },
    PermissionRule {
        tag: PermissionTag::Payment,
        decision: PermissionDecision::RequireApproval,
    },
    PermissionRule {
        tag: PermissionTag::Order,
        decision: PermissionDecision::RequireApproval,
    },
    PermissionRule {
        tag: PermissionTag::VerificationCode,
        decision: PermissionDecision::RequireApproval,
    },
    PermissionRule {
        tag: PermissionTag::Delete,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Cleanup,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Reset,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Uninstall,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Purge,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::ServiceControl,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::NetworkChange,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::SecretAccess,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
];

const SAFE_DEFAULT_RULES: &[PermissionRule] = &[
    PermissionRule {
        tag: PermissionTag::Read,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::List,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::Status,
        decision: PermissionDecision::Allow,
    },
    PermissionRule {
        tag: PermissionTag::Observe,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::Screenshot,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::FileWrite,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::CodeExecute,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::OpenApp,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::Click,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::Input,
        decision: PermissionDecision::RequireApprovalOrProjectTrust,
    },
    PermissionRule {
        tag: PermissionTag::ExternalSend,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::PublicPost,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::Payment,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::Order,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::VerificationCode,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::Delete,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Cleanup,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Reset,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Uninstall,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::Purge,
        decision: PermissionDecision::RequireExplicitTargetApproval,
    },
    PermissionRule {
        tag: PermissionTag::ServiceControl,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::NetworkChange,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
    PermissionRule {
        tag: PermissionTag::SecretAccess,
        decision: PermissionDecision::RequireApprovalOrDeny,
    },
];

pub fn local_ga_profile() -> PermissionProfile {
    PermissionProfile {
        id: PermissionProfileId::LocalGa,
        name: "local_ga",
        rules: LOCAL_GA_RULES,
        default_decision: PermissionDecision::RequireApproval,
    }
}

pub fn safe_default_profile() -> PermissionProfile {
    PermissionProfile {
        id: PermissionProfileId::SafeDefault,
        name: "safe_default",
        rules: SAFE_DEFAULT_RULES,
        default_decision: PermissionDecision::RequireApprovalOrDeny,
    }
}

pub fn classify_tag(raw: &str) -> Option<PermissionTag> {
    let normalized = raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    match normalized.as_str() {
        "read" | "file_read" | "read_file" => Some(PermissionTag::Read),
        "list" | "list_dir" | "dir_list" => Some(PermissionTag::List),
        "status" | "health" | "diagnostic" => Some(PermissionTag::Status),
        "observe" | "locate" => Some(PermissionTag::Observe),
        "screenshot" | "screen_capture" => Some(PermissionTag::Screenshot),
        "file_write" | "write" | "write_file" => Some(PermissionTag::FileWrite),
        "code_execute" | "exec" | "execute" | "shell" | "shell_command" => {
            Some(PermissionTag::CodeExecute)
        }
        "open_app" | "open_application" | "launch_app" => Some(PermissionTag::OpenApp),
        "click" | "mouse_click" => Some(PermissionTag::Click),
        "input" | "keyboard" | "type_text" => Some(PermissionTag::Input),
        "external_send" | "send_external" => Some(PermissionTag::ExternalSend),
        "public_post" | "post_public" => Some(PermissionTag::PublicPost),
        "payment" | "pay" => Some(PermissionTag::Payment),
        "order" | "purchase" => Some(PermissionTag::Order),
        "verification_code" | "verification_code_input" | "otp" => {
            Some(PermissionTag::VerificationCode)
        }
        "delete" | "remove" | "rm" => Some(PermissionTag::Delete),
        "cleanup" | "clean_up" => Some(PermissionTag::Cleanup),
        "reset" => Some(PermissionTag::Reset),
        "uninstall" => Some(PermissionTag::Uninstall),
        "purge" => Some(PermissionTag::Purge),
        "service_control" | "service_change" | "systemctl" => Some(PermissionTag::ServiceControl),
        "network_change" | "network" => Some(PermissionTag::NetworkChange),
        "secret_access" | "secret" | "credential_access" => Some(PermissionTag::SecretAccess),
        _ => None,
    }
}

pub fn decide_tag(profile: &PermissionProfile, tag: PermissionTag) -> PermissionRiskDecision {
    let decision = profile
        .rules
        .iter()
        .find(|rule| rule.tag == tag)
        .map(|rule| rule.decision)
        .unwrap_or(profile.default_decision);
    PermissionRiskDecision {
        profile: profile.id,
        decision,
        matched_tag: Some(tag),
        reason: format!("profile={} tag={tag:?} decision={decision:?}", profile.name),
    }
}

pub fn decide_descriptor_risk(
    profile: &PermissionProfile,
    descriptor: &ToolDescriptorRisk<'_>,
) -> PermissionRiskDecision {
    let mut best: Option<PermissionRiskDecision> = None;
    for raw_tag in descriptor.risk_tags {
        if let Some(tag) = classify_tag(raw_tag) {
            let decision = decide_tag(profile, tag);
            if best
                .as_ref()
                .map(|current| decision_rank(decision.decision) > decision_rank(current.decision))
                .unwrap_or(true)
            {
                best = Some(decision);
            }
        }
    }

    if descriptor.destructive {
        let decision = PermissionRiskDecision {
            profile: profile.id,
            decision: PermissionDecision::RequireExplicitTargetApproval,
            matched_tag: None,
            reason: format!(
                "profile={} descriptor={} destructive=true",
                profile.name, descriptor.name
            ),
        };
        if best
            .as_ref()
            .map(|current| decision_rank(decision.decision) > decision_rank(current.decision))
            .unwrap_or(true)
        {
            best = Some(decision);
        }
    }

    if descriptor.external_commit {
        let decision = PermissionRiskDecision {
            profile: profile.id,
            decision: PermissionDecision::RequireApproval,
            matched_tag: None,
            reason: format!(
                "profile={} descriptor={} external_commit=true",
                profile.name, descriptor.name
            ),
        };
        if best
            .as_ref()
            .map(|current| decision_rank(decision.decision) > decision_rank(current.decision))
            .unwrap_or(true)
        {
            best = Some(decision);
        }
    }

    if descriptor.requires_approval {
        let decision = PermissionRiskDecision {
            profile: profile.id,
            decision: PermissionDecision::RequireApproval,
            matched_tag: None,
            reason: format!(
                "profile={} descriptor={} requires_approval=true",
                profile.name, descriptor.name
            ),
        };
        if best
            .as_ref()
            .map(|current| decision_rank(decision.decision) > decision_rank(current.decision))
            .unwrap_or(true)
        {
            best = Some(decision);
        }
    }

    best.unwrap_or_else(|| {
        let decision = if descriptor.read_only && !descriptor.mutating {
            PermissionDecision::Allow
        } else if descriptor.mutating {
            profile.default_decision
        } else {
            profile.default_decision
        };
        PermissionRiskDecision {
            profile: profile.id,
            decision,
            matched_tag: None,
            reason: format!(
                "profile={} descriptor={} fallback",
                profile.name, descriptor.name
            ),
        }
    })
}

fn decision_rank(decision: PermissionDecision) -> u8 {
    match decision {
        PermissionDecision::Allow => 0,
        PermissionDecision::AllowWithAudit => 1,
        PermissionDecision::RequireApprovalOrProjectTrust => 2,
        PermissionDecision::RequireApproval => 3,
        PermissionDecision::RequireApprovalOrDeny => 4,
        PermissionDecision::RequireExplicitTargetApproval => 5,
    }
}
