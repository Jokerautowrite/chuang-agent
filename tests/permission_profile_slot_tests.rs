use chuang_agent::permission_profile_slot::{
    classify_tag, decide_descriptor_risk, decide_tag, full_local_workspace_profile,
    local_ga_profile, safe_default_profile, PermissionDecision, PermissionProfileId, PermissionTag,
    ToolDescriptorRisk,
};

fn descriptor<'a>(
    name: &'a str,
    risk_tags: &'a [&'a str],
    read_only: bool,
    mutating: bool,
    destructive: bool,
    external_commit: bool,
    requires_approval: bool,
) -> ToolDescriptorRisk<'a> {
    ToolDescriptorRisk {
        name,
        risk_tags,
        read_only,
        mutating,
        destructive,
        external_commit,
        requires_approval,
    }
}

#[test]
fn local_ga_profile_maps_readonly_tags_to_allow() {
    let profile = local_ga_profile();
    assert_eq!(profile.id, PermissionProfileId::LocalGa);
    assert_eq!(profile.name, "local_ga");

    for tag in [
        PermissionTag::Read,
        PermissionTag::List,
        PermissionTag::Status,
        PermissionTag::Observe,
        PermissionTag::Screenshot,
    ] {
        let decision = decide_tag(&profile, tag);
        assert_eq!(decision.decision, PermissionDecision::Allow, "{tag:?}");
    }
}

#[test]
fn full_local_workspace_is_the_explicit_autonomous_workspace_profile() {
    let profile = full_local_workspace_profile();
    assert_eq!(profile.id, PermissionProfileId::FullLocalWorkspace);
    assert_eq!(profile.name, "full_local_workspace");
    assert_eq!(
        decide_tag(&profile, PermissionTag::CodeExecute).decision,
        PermissionDecision::AllowWithAudit
    );
    assert_eq!(
        decide_tag(&profile, PermissionTag::Delete).decision,
        PermissionDecision::RequireExplicitTargetApproval
    );
    assert_eq!(
        decide_tag(&profile, PermissionTag::SecretAccess).decision,
        PermissionDecision::RequireApprovalOrDeny
    );
}

#[test]
fn local_ga_profile_allows_local_mutation_with_audit() {
    let profile = local_ga_profile();

    for tag in [
        PermissionTag::FileWrite,
        PermissionTag::CodeExecute,
        PermissionTag::OpenApp,
        PermissionTag::Click,
        PermissionTag::Input,
    ] {
        let decision = decide_tag(&profile, tag);
        assert_eq!(
            decision.decision,
            PermissionDecision::AllowWithAudit,
            "{tag:?}"
        );
    }
}

#[test]
fn local_ga_profile_requires_approval_for_external_commit_tags() {
    let profile = local_ga_profile();

    for tag in [
        PermissionTag::ExternalSend,
        PermissionTag::PublicPost,
        PermissionTag::Payment,
        PermissionTag::Order,
        PermissionTag::VerificationCode,
    ] {
        let decision = decide_tag(&profile, tag);
        assert_eq!(
            decision.decision,
            PermissionDecision::RequireApproval,
            "{tag:?}"
        );
    }
}

#[test]
fn local_ga_profile_requires_explicit_target_approval_for_destructive_tags() {
    let profile = local_ga_profile();

    for raw_tag in [
        "delete",
        "rm",
        "destructive",
        "destructive_action",
        "cleanup",
        "reset",
        "uninstall",
        "purge",
    ] {
        let tag = classify_tag(raw_tag).expect("tag should classify");
        let decision = decide_tag(&profile, tag);
        assert_eq!(
            decision.decision,
            PermissionDecision::RequireExplicitTargetApproval,
            "{raw_tag}"
        );
    }
}

#[test]
fn local_ga_profile_requires_approval_or_deny_for_service_network_or_secret() {
    let profile = local_ga_profile();

    for tag in [
        PermissionTag::ServiceControl,
        PermissionTag::NetworkChange,
        PermissionTag::SecretAccess,
    ] {
        let decision = decide_tag(&profile, tag);
        assert_eq!(
            decision.decision,
            PermissionDecision::RequireApprovalOrDeny,
            "{tag:?}"
        );
    }
}

#[test]
fn safe_default_profile_keeps_read_status_allowed_but_desktop_and_write_guarded() {
    let profile = safe_default_profile();
    assert_eq!(profile.id, PermissionProfileId::SafeDefault);
    assert_eq!(profile.name, "safe_default");

    for tag in [
        PermissionTag::Read,
        PermissionTag::List,
        PermissionTag::Status,
    ] {
        assert_eq!(
            decide_tag(&profile, tag).decision,
            PermissionDecision::Allow
        );
    }
    for tag in [
        PermissionTag::Observe,
        PermissionTag::Screenshot,
        PermissionTag::FileWrite,
        PermissionTag::CodeExecute,
        PermissionTag::OpenApp,
        PermissionTag::Click,
        PermissionTag::Input,
    ] {
        assert_eq!(
            decide_tag(&profile, tag).decision,
            PermissionDecision::RequireApprovalOrProjectTrust,
            "{tag:?}"
        );
    }
}

#[test]
fn descriptor_decision_uses_highest_risk_matching_tag() {
    let profile = local_ga_profile();
    let risk = decide_descriptor_risk(
        &profile,
        &descriptor(
            "write-then-send",
            &["file_write", "external_send"],
            false,
            true,
            false,
            true,
            false,
        ),
    );

    assert_eq!(risk.decision, PermissionDecision::RequireApproval);
    assert_eq!(risk.matched_tag, Some(PermissionTag::ExternalSend));
}

#[test]
fn descriptor_destructive_flag_cannot_be_downgraded_by_read_tag() {
    let profile = local_ga_profile();
    let risk = decide_descriptor_risk(
        &profile,
        &descriptor(
            "read-but-destructive",
            &["read"],
            true,
            false,
            true,
            false,
            false,
        ),
    );

    assert_eq!(
        risk.decision,
        PermissionDecision::RequireExplicitTargetApproval
    );
}

#[test]
fn classify_tag_accepts_stable_aliases_and_rejects_unknown_tags() {
    assert_eq!(classify_tag("file-read"), Some(PermissionTag::Read));
    assert_eq!(classify_tag("list_dir"), Some(PermissionTag::List));
    assert_eq!(
        classify_tag("screen capture"),
        Some(PermissionTag::Screenshot)
    );
    assert_eq!(
        classify_tag("shell_command"),
        Some(PermissionTag::CodeExecute)
    );
    assert_eq!(classify_tag("otp"), Some(PermissionTag::VerificationCode));
    assert_eq!(
        classify_tag("systemctl"),
        Some(PermissionTag::ServiceControl)
    );
    assert_eq!(classify_tag("not_a_real_risk_tag"), None);
}
