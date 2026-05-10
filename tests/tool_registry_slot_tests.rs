use std::collections::BTreeSet;

use chuang_agent::tool_registry_slot::{
    builtin_tool_descriptors, default_tool_registry_slot, descriptor_for_tool, ToolDescriptor,
    ToolRegistrySlot,
};

const REQUIRED_TOOL_NAMES: &[&str] = &[
    "file_read",
    "file_write",
    "code_execute",
    "list_dir",
    "locate",
    "screenshot",
    "open_app",
    "mouse",
    "keyboard",
    "wait",
    "human_suspend",
    "memory_recall",
];

#[test]
fn builtin_descriptors_cover_required_tool_names_once() {
    let descriptors = builtin_tool_descriptors();
    assert_eq!(descriptors.len(), REQUIRED_TOOL_NAMES.len());

    let actual = descriptors
        .iter()
        .map(|descriptor| descriptor.name)
        .collect::<BTreeSet<_>>();
    let expected = REQUIRED_TOOL_NAMES.iter().copied().collect::<BTreeSet<_>>();

    assert_eq!(actual, expected);
    assert_eq!(actual.len(), descriptors.len());
}

#[test]
fn descriptor_lookup_is_static_and_rejects_unknown_tools() {
    for tool_name in REQUIRED_TOOL_NAMES {
        let descriptor = descriptor_for_tool(tool_name).expect("required tool should resolve");

        assert_eq!(descriptor.name, *tool_name);
        assert!(descriptor_for_tool(tool_name).is_some_and(|again| std::ptr::eq(again, descriptor)));
    }

    assert!(descriptor_for_tool("apply_patch").is_none());
    assert!(descriptor_for_tool("read_file").is_none());
    assert!(descriptor_for_tool("unknown_tool").is_none());
}

#[test]
fn descriptor_schema_fields_are_stable() {
    assert_eq!(ToolDescriptor::schema_version(), 1);
    assert_eq!(
        ToolDescriptor::descriptor_schema_fields(),
        &[
            "name",
            "namespace",
            "title",
            "schema_fields",
            "read_only",
            "mutating",
            "destructive",
            "external_commit",
            "concurrent_safe",
            "requires_approval",
            "risk_tags",
        ]
    );
    assert_eq!(ToolRegistrySlot::schema_fields(), &["descriptors"]);
}

#[test]
fn read_only_descriptors_do_not_mutate_or_commit_externally() {
    for tool_name in [
        "file_read",
        "list_dir",
        "locate",
        "screenshot",
        "wait",
        "human_suspend",
        "memory_recall",
    ] {
        let descriptor = descriptor_for_tool(tool_name).expect("read-only tool should resolve");

        assert!(descriptor.read_only, "{tool_name} should be read-only");
        assert!(!descriptor.mutating, "{tool_name} should not mutate");
        assert!(
            !descriptor.destructive,
            "{tool_name} should not be destructive"
        );
        assert!(
            !descriptor.external_commit,
            "{tool_name} should not make external commits"
        );
        assert!(
            !descriptor.requires_approval,
            "{tool_name} descriptor should not require approval by itself"
        );
        assert!(
            descriptor.risk_tags.contains(&"read_only"),
            "{tool_name} should carry read_only risk tag"
        );
    }
}

#[test]
fn mutating_descriptors_are_local_non_destructive_operations() {
    for tool_name in [
        "file_write",
        "code_execute",
        "open_app",
        "mouse",
        "keyboard",
    ] {
        let descriptor = descriptor_for_tool(tool_name).expect("mutating tool should resolve");

        assert!(!descriptor.read_only, "{tool_name} should not be read-only");
        assert!(descriptor.mutating, "{tool_name} should be mutating");
        assert!(
            !descriptor.destructive,
            "{tool_name} should not be destructive"
        );
        assert!(
            !descriptor.external_commit,
            "{tool_name} should not be an external commit"
        );
        assert!(
            !descriptor.requires_approval,
            "{tool_name} approval is decided by governance, not the descriptor"
        );
        assert!(
            !descriptor.risk_tags.is_empty(),
            "{tool_name} should carry risk tags"
        );
    }
}

#[test]
fn descriptor_namespaces_and_schema_fields_match_the_tool_surface() {
    assert_descriptor_shape("file_read", "workspace", &["path"]);
    assert_descriptor_shape("file_write", "workspace", &["path", "content"]);
    assert_descriptor_shape("code_execute", "workspace", &["command", "cwd"]);
    assert_descriptor_shape("list_dir", "workspace", &["path"]);
    assert_descriptor_shape("locate", "desktop", &["target"]);
    assert_descriptor_shape("screenshot", "desktop", &["target"]);
    assert_descriptor_shape("open_app", "desktop", &["app_name"]);
    assert_descriptor_shape("mouse", "desktop", &["x", "y"]);
    assert_descriptor_shape("keyboard", "desktop", &["text", "secret"]);
    assert_descriptor_shape("wait", "runtime", &["millis"]);
    assert_descriptor_shape("human_suspend", "runtime", &["reason", "prompt"]);
    assert_descriptor_shape("memory_recall", "memory", &["query", "session_id", "limit"]);
}

#[test]
fn registry_slot_is_a_serializable_description_only_surface() {
    let slot = default_tool_registry_slot();

    assert_eq!(slot.descriptors, builtin_tool_descriptors());

    let json = serde_json::to_value(slot).expect("slot should serialize");
    assert!(json.get("descriptors").is_some());
    assert_eq!(json["descriptors"].as_array().expect("array").len(), 12);

    let keyboard = json["descriptors"]
        .as_array()
        .expect("descriptors array")
        .iter()
        .find(|value| value["name"] == "keyboard")
        .expect("keyboard descriptor should serialize");

    assert_eq!(keyboard["namespace"], "desktop");
    assert_eq!(
        keyboard["schema_fields"],
        serde_json::json!(["text", "secret"])
    );
    assert_eq!(keyboard["read_only"], false);
    assert_eq!(keyboard["mutating"], true);
    assert_eq!(keyboard["destructive"], false);
    assert_eq!(keyboard["external_commit"], false);
    assert_eq!(keyboard["requires_approval"], false);
    assert!(keyboard["risk_tags"].as_array().expect("risk_tags").len() >= 2);
}

fn assert_descriptor_shape(name: &str, expected_namespace: &str, expected_schema_fields: &[&str]) {
    let descriptor = descriptor_for_tool(name).expect("descriptor should exist");

    assert_eq!(descriptor.namespace, expected_namespace);
    assert_eq!(descriptor.schema_fields, expected_schema_fields);
    assert!(!descriptor.title.is_empty());
}
