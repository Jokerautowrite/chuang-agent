use chuang_agent::lifecycle::{
    CommandEffect, CommandRejectReason, LifecycleCommand, LifecycleEngine, LifecycleState,
    LifecycleStateMachine, LocalCheckpointStore,
};
use std::path::PathBuf;

#[test]
fn engine_handles_start_from_uninitialized() {
    let mut engine = LifecycleEngine::new(LifecycleState::Uninitialized);

    let effect = engine.handle_command(LifecycleCommand::Start).unwrap();

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Starting,
        }
    );
    assert_eq!(engine.current_state(), LifecycleState::Starting);
}

#[test]
fn engine_defers_resume_from_starting() {
    let mut engine = LifecycleEngine::new(LifecycleState::Starting);

    let effect = engine.handle_command(LifecycleCommand::Resume).unwrap();

    let CommandEffect::Deferred {
        command,
        inserted_at,
    } = effect
    else {
        panic!("expected deferred effect");
    };
    assert_eq!(command, LifecycleCommand::Resume);
    chrono::DateTime::parse_from_rfc3339(&inserted_at.0).unwrap();
    assert_eq!(engine.deferred.len(), 1);
}

#[test]
fn drive_deferred_replays_when_state_changes() {
    let mut engine = LifecycleEngine::new(LifecycleState::Starting);
    let _ = engine.handle_command(LifecycleCommand::Resume).unwrap();
    engine.state = LifecycleState::Paused;

    let effects = engine.drive_deferred();

    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0],
        CommandEffect::Accepted {
            next_state: LifecycleState::Running,
        }
    );
    assert_eq!(engine.current_state(), LifecycleState::Running);
}

#[test]
fn checkpoint_store_appends_replaces_and_reopens_latest_state() {
    let path = temp_path("reopen");
    let store = LocalCheckpointStore::new(&path);
    let mut engine = LifecycleEngine::new(LifecycleState::Starting);
    engine.handle_command(LifecycleCommand::Resume).unwrap();

    store.append(&engine.checkpoint()).unwrap();
    engine.state = LifecycleState::Paused;
    store.append(&engine.checkpoint()).unwrap();
    assert_eq!(store.load_all().unwrap().len(), 2);

    let reopened = LifecycleEngine::reopen(&LocalCheckpointStore::new(&path)).unwrap();
    assert_eq!(reopened.current_state(), LifecycleState::Paused);
    assert_eq!(reopened.deferred, vec![LifecycleCommand::Resume]);

    store.replace(&reopened.checkpoint()).unwrap();
    assert_eq!(store.load_all().unwrap().len(), 1);
}

#[test]
fn checkpoint_preserves_runtime_resume_references() {
    let path = temp_path("runtime-refs");
    let store = LocalCheckpointStore::new(&path);
    let checkpoint = LifecycleEngine::new(LifecycleState::Running)
        .checkpoint()
        .with_runtime_refs(
            "chuang",
            "thread-1",
            "turn-2",
            vec!["system-core".to_string(), "working-user-input".to_string()],
            Some("memory://cursor/42".to_string()),
            vec!["tool-call-7".to_string()],
        );

    store.replace(&checkpoint).unwrap();
    let reopened = store.load_latest().unwrap();

    assert_eq!(reopened.agent_id.as_deref(), Some("chuang"));
    assert_eq!(reopened.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(reopened.turn_id.as_deref(), Some("turn-2"));
    assert_eq!(
        reopened.packed_segment_ids,
        vec!["system-core", "working-user-input"]
    );
    assert_eq!(
        reopened.memory_cursor.as_deref(),
        Some("memory://cursor/42")
    );
    assert_eq!(reopened.unfinished_tool_call_ids, vec!["tool-call-7"]);
}

#[test]
fn legacy_v1_checkpoint_without_runtime_refs_still_loads_and_reopens() {
    let path = temp_path("legacy-v1-runtime-refs");
    let legacy_json = r#"{
  "schema_version": 1,
  "checkpoints": [
    {
      "schema_version": 1,
      "saved_at": "2026-07-10T10:00:00.000Z",
      "state": "paused",
      "deferred": [
        {
          "command": "resume",
          "inserted_at": "2026-07-10T09:59:45.000Z"
        }
      ]
    }
  ]
}"#;
    std::fs::write(&path, legacy_json).unwrap();

    let store = LocalCheckpointStore::new(&path);
    let reopened = store.load_latest().unwrap();

    assert_eq!(reopened.state, LifecycleState::Paused);
    assert_eq!(reopened.agent_id, None);
    assert_eq!(reopened.thread_id, None);
    assert_eq!(reopened.turn_id, None);
    assert!(reopened.packed_segment_ids.is_empty());
    assert_eq!(reopened.memory_cursor, None);
    assert!(reopened.unfinished_tool_call_ids.is_empty());

    let engine = LifecycleEngine::reopen(&store).unwrap();
    assert_eq!(engine.current_state(), LifecycleState::Paused);
    assert_eq!(engine.deferred, vec![LifecycleCommand::Resume]);
}

#[test]
fn restore_preserves_runtime_checkpoint_references() {
    let checkpoint = LifecycleEngine::new(LifecycleState::Running)
        .checkpoint()
        .with_runtime_refs(
            "chuang",
            "thread-restore",
            "turn-restore",
            vec!["system-core".to_string(), "memory-summary".to_string()],
            Some("memory://cursor/restore".to_string()),
            vec!["tool-call-restore".to_string()],
        );

    let restored = LifecycleEngine::restore(checkpoint).unwrap().checkpoint();

    assert_eq!(restored.agent_id.as_deref(), Some("chuang"));
    assert_eq!(restored.thread_id.as_deref(), Some("thread-restore"));
    assert_eq!(restored.turn_id.as_deref(), Some("turn-restore"));
    assert_eq!(
        restored.packed_segment_ids,
        vec!["system-core", "memory-summary"]
    );
    assert_eq!(
        restored.memory_cursor.as_deref(),
        Some("memory://cursor/restore")
    );
    assert_eq!(restored.unfinished_tool_call_ids, vec!["tool-call-restore"]);
}

#[test]
fn persisted_transition_after_reopen_keeps_runtime_checkpoint_references() {
    let path = temp_path("persist-runtime-refs");
    let store = LocalCheckpointStore::new(&path);
    let checkpoint = LifecycleEngine::new(LifecycleState::Running)
        .checkpoint()
        .with_runtime_refs(
            "chuang",
            "thread-persist",
            "turn-persist",
            vec!["system-core".to_string(), "working-user-input".to_string()],
            Some("memory://cursor/persist".to_string()),
            vec!["tool-call-persist".to_string()],
        );
    store.replace(&checkpoint).unwrap();

    let mut reopened = LifecycleEngine::reopen(&store).unwrap();
    let effect = reopened
        .handle_command_persisted(LifecycleCommand::Pause, &store)
        .unwrap();

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Pausing,
        }
    );

    let persisted = store.load_latest().unwrap();
    assert_eq!(persisted.state, LifecycleState::Pausing);
    assert_eq!(persisted.agent_id.as_deref(), Some("chuang"));
    assert_eq!(persisted.thread_id.as_deref(), Some("thread-persist"));
    assert_eq!(persisted.turn_id.as_deref(), Some("turn-persist"));
    assert_eq!(
        persisted.packed_segment_ids,
        vec!["system-core", "working-user-input"]
    );
    assert_eq!(
        persisted.memory_cursor.as_deref(),
        Some("memory://cursor/persist")
    );
    assert_eq!(
        persisted.unfinished_tool_call_ids,
        vec!["tool-call-persist"]
    );
}

#[test]
fn deferred_command_times_out_at_exactly_thirty_seconds() {
    let mut engine = LifecycleEngine::new(LifecycleState::Starting);
    engine.handle_command(LifecycleCommand::Resume).unwrap();
    let inserted_at = engine.checkpoint().deferred[0].inserted_at.0.clone();
    let inserted_at = chrono::DateTime::parse_from_rfc3339(&inserted_at).unwrap();

    let first_results = engine.drive_deferred_checked(inserted_at + chrono::Duration::seconds(10));
    assert!(matches!(
        first_results.as_slice(),
        [Ok(CommandEffect::Deferred { .. })]
    ));
    assert_eq!(
        engine.checkpoint().deferred[0].inserted_at.0,
        inserted_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );

    let results = engine.drive_deferred_checked(inserted_at + chrono::Duration::seconds(30));

    assert_eq!(
        results,
        vec![Err(CommandRejectReason::TimeoutDeferred {
            command: LifecycleCommand::Resume,
            elapsed_ms: 30_000,
        })]
    );
    assert!(engine.deferred.is_empty());
    assert_eq!(engine.current_state(), LifecycleState::Starting);
}

#[test]
fn failed_checkpoint_write_rolls_back_command_state() {
    let root = temp_path("blocked-parent");
    std::fs::write(&root, b"not a directory").unwrap();
    let store = LocalCheckpointStore::new(root.join("checkpoint.json"));
    let mut engine = LifecycleEngine::new(LifecycleState::Uninitialized);

    let result = engine.handle_command_persisted(LifecycleCommand::Start, &store);

    assert!(result.is_err());
    assert_eq!(engine.current_state(), LifecycleState::Uninitialized);
    assert!(engine.deferred.is_empty());
    assert_eq!(std::fs::read(&root).unwrap(), b"not a directory");
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chuang-lifecycle-{label}-{}-{}.json",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ))
}
