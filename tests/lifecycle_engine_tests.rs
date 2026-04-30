use chuang_agent::lifecycle::{
    CommandEffect, LifecycleCommand, LifecycleEngine, LifecycleState, LifecycleStateMachine,
};

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

    assert_eq!(
        effect,
        CommandEffect::Deferred {
            command: LifecycleCommand::Resume,
            inserted_at: chuang_agent::common::Timestamp("deferred".to_string()),
        }
    );
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
