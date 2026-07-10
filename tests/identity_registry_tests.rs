use chuang_agent::identity_registry::{IdentityRegistry, IdentityRegistryError};

const REGISTRY: &str = r#"
memory_body_id = "chuang-body"
active_agent_id = "chuang"

[[agents]]
agent_id = "chuang"
display_name = "Chuang"
shell_kind = "codex-rust"
role = "kernel"
memory_body_id = "chuang-body"
allowed_channels = ["cli", "app-server"]

[[agents]]
agent_id = "worker"
display_name = "Worker"
shell_kind = "openclaw"
role = "worker"
memory_body_id = "worker-body"
allowed_channels = ["worker"]
"#;

#[test]
fn parses_and_selects_exactly_one_active_identity() {
    let registry = IdentityRegistry::parse(REGISTRY).expect("registry should parse");
    let active = registry
        .select_active(Some("chuang"), Some("cli"))
        .expect("active identity should select");

    assert_eq!(active.agent_id, "chuang");
    assert_eq!(active.memory_body_id, "chuang-body");
}

#[test]
fn rejects_missing_active_agent() {
    let error = IdentityRegistry::parse(&REGISTRY.replace(
        "active_agent_id = \"chuang\"",
        "active_agent_id = \"missing\"",
    ))
    .expect_err("missing active identity must fail");

    assert!(matches!(
        error,
        IdentityRegistryError::ActiveAgentNotFound { .. }
    ));
}

#[test]
fn rejects_active_identity_memory_body_mismatch() {
    let registry = IdentityRegistry::parse(&REGISTRY.replace(
        "memory_body_id = \"chuang-body\"\nallowed_channels",
        "memory_body_id = \"wrong-body\"\nallowed_channels",
    ))
    .expect("registry shape should parse");
    let error = registry
        .select_active(Some("chuang"), None)
        .expect_err("memory body mismatch must fail");

    assert!(matches!(
        error,
        IdentityRegistryError::MemoryBodyMismatch { .. }
    ));
}

#[test]
fn rejects_disallowed_metadata_channel() {
    let registry = IdentityRegistry::parse(REGISTRY).expect("registry should parse");
    let error = registry
        .select_active(Some("chuang"), Some("hermes-feishu"))
        .expect_err("disallowed channel must fail");

    assert!(matches!(
        error,
        IdentityRegistryError::ChannelNotAllowed { .. }
    ));
}

#[test]
fn rejects_multiple_active_markers() {
    let content = REGISTRY
        .replace("active_agent_id = \"chuang\"\n", "")
        .replace("role = \"kernel\"", "role = \"kernel\"\nactive = true")
        .replace("role = \"worker\"", "role = \"worker\"\nactive = true");
    let error = IdentityRegistry::parse(&content).expect_err("multiple active agents must fail");

    assert!(matches!(
        error,
        IdentityRegistryError::ActiveAgentCount { count: 2 }
    ));
}
