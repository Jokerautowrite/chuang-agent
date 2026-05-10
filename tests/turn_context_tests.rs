use std::path::PathBuf;

use chuang_agent::permission_profile_slot::PermissionProfileId;
use chuang_agent::tool_registry_slot::{builtin_tool_descriptors, descriptor_for_tool};
use chuang_agent::turn_context::{
    TurnContextSnapshot, TurnContextSnapshotError, TurnContextSnapshotInput,
};

#[test]
fn turn_context_snapshot_is_deterministic_and_stable() {
    let mut tools = vec![
        *descriptor_for_tool("keyboard").expect("keyboard exists"),
        *descriptor_for_tool("file_read").expect("file_read exists"),
    ];
    tools.reverse();

    let snapshot = TurnContextSnapshot::from_fake_input(TurnContextSnapshotInput {
        thread_id: "thread-001".to_string(),
        turn_id: "turn-002".to_string(),
        workspace_root: PathBuf::from("/workspace/chuang"),
        provider_id: "openai-compatible".to_string(),
        model_name: "gpt-5.3-codex".to_string(),
        permission_profile_id: PermissionProfileId::LocalGa,
        tools,
        memory_segment_ids: vec!["mem-2".to_string(), "mem-9".to_string()],
        recent_history_segment_ids: vec!["hist-1".to_string(), "hist-3".to_string()],
        env_pairs: vec![
            (
                "OPENAI_API_KEY".to_string(),
                Some("sk-live-real".to_string()),
            ),
            ("TRACE_FLAG".to_string(), Some("1".to_string())),
            ("EMPTY_FLAG".to_string(), Some("".to_string())),
        ],
    })
    .expect("build snapshot");

    let encoded = serde_json::to_string(&snapshot).expect("serialize snapshot");
    assert!(encoded.contains("\"thread_id\":\"thread-001\""));
    assert!(encoded.contains("\"turn_id\":\"turn-002\""));
    assert!(encoded.contains("\"permission_profile_id\":\"LocalGa\""));
    assert!(encoded.contains("\"recent_history_segment_ids\":[\"hist-1\",\"hist-3\"]"));
    assert_eq!(snapshot.tools[0].name, "file_read");
    assert_eq!(snapshot.tools[1].name, "keyboard");
    assert_eq!(snapshot.env_vars[0].value_state, "<missing>");
    assert_eq!(snapshot.env_vars[1].value_state, "<redacted>");
    assert_eq!(snapshot.env_vars[2].value_state, "<set>");
    assert!(!encoded.contains("sk-live-real"));
}

#[test]
fn turn_context_snapshot_rejects_missing_required_fields() {
    let err = TurnContextSnapshot::from_fake_input(TurnContextSnapshotInput {
        thread_id: "".to_string(),
        turn_id: "turn-1".to_string(),
        workspace_root: PathBuf::from("/workspace/chuang"),
        provider_id: "fake".to_string(),
        model_name: "stub".to_string(),
        permission_profile_id: PermissionProfileId::SafeDefault,
        tools: builtin_tool_descriptors().to_vec(),
        memory_segment_ids: vec![],
        recent_history_segment_ids: vec![],
        env_pairs: vec![],
    })
    .expect_err("missing thread_id should fail");
    assert_eq!(
        err,
        TurnContextSnapshotError::MissingRequiredField { field: "thread_id" }
    );
}

#[test]
fn turn_context_snapshot_rejects_missing_workspace_root() {
    let err = TurnContextSnapshot::from_fake_input(TurnContextSnapshotInput {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        workspace_root: PathBuf::new(),
        provider_id: "fake".to_string(),
        model_name: "stub".to_string(),
        permission_profile_id: PermissionProfileId::SafeDefault,
        tools: vec![],
        memory_segment_ids: vec![],
        recent_history_segment_ids: vec!["hist-a".to_string()],
        env_pairs: vec![],
    })
    .expect_err("missing workspace_root should fail");
    assert_eq!(
        err,
        TurnContextSnapshotError::MissingRequiredField {
            field: "workspace_root"
        }
    );
}

#[test]
fn turn_context_snapshot_rejects_missing_provider_or_model_identity() {
    let err = TurnContextSnapshot::from_fake_input(TurnContextSnapshotInput {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        workspace_root: PathBuf::from("/workspace/chuang"),
        provider_id: " ".to_string(),
        model_name: "stub".to_string(),
        permission_profile_id: PermissionProfileId::SafeDefault,
        tools: vec![],
        memory_segment_ids: vec![],
        recent_history_segment_ids: vec![],
        env_pairs: vec![],
    })
    .expect_err("missing provider_id should fail");
    assert_eq!(
        err,
        TurnContextSnapshotError::MissingRequiredField {
            field: "provider_id"
        }
    );

    let err = TurnContextSnapshot::from_fake_input(TurnContextSnapshotInput {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        workspace_root: PathBuf::from("/workspace/chuang"),
        provider_id: "fake".to_string(),
        model_name: "".to_string(),
        permission_profile_id: PermissionProfileId::SafeDefault,
        tools: vec![],
        memory_segment_ids: vec![],
        recent_history_segment_ids: vec![],
        env_pairs: vec![],
    })
    .expect_err("missing model_name should fail");
    assert_eq!(
        err,
        TurnContextSnapshotError::MissingRequiredField {
            field: "model_name"
        }
    );
}
