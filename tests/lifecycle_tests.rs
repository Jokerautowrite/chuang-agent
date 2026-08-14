use chuang_agent::lifecycle::{
    CommandEffect, LifecycleCommand, LifecycleState, LifecycleTransitionTable,
};

#[test]
fn start_on_uninitialized_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Uninitialized, &LifecycleCommand::Start);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Starting,
        }
    );
}

#[test]
fn start_on_running_is_noop() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Running, &LifecycleCommand::Start);

    assert_eq!(effect, CommandEffect::Noop);
}

#[test]
fn start_on_failed_is_deferred() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Failed, &LifecycleCommand::Start);

    let CommandEffect::Deferred {
        command,
        inserted_at,
    } = effect
    else {
        panic!("expected deferred effect");
    };
    assert_eq!(command, LifecycleCommand::Start);
    chrono::DateTime::parse_from_rfc3339(&inserted_at.0).unwrap();
}

#[test]
fn pause_on_running_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Running, &LifecycleCommand::Pause);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Pausing,
        }
    );
}

#[test]
fn pause_on_paused_is_noop() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Paused, &LifecycleCommand::Pause);

    assert_eq!(effect, CommandEffect::Noop);
}

#[test]
fn resume_on_starting_is_deferred() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Starting, &LifecycleCommand::Resume);

    let CommandEffect::Deferred {
        command,
        inserted_at,
    } = effect
    else {
        panic!("expected deferred effect");
    };
    assert_eq!(command, LifecycleCommand::Resume);
    chrono::DateTime::parse_from_rfc3339(&inserted_at.0).unwrap();
}

#[test]
fn resume_on_paused_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Paused, &LifecycleCommand::Resume);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Running,
        }
    );
}

#[test]
fn checkpoint_on_running_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Running, &LifecycleCommand::Checkpoint);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Checkpointing,
        }
    );
}

#[test]
fn checkpoint_on_paused_is_rejected() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Paused, &LifecycleCommand::Checkpoint);

    assert_eq!(
        effect,
        CommandEffect::Rejected {
            reason: "reject".to_string(),
        }
    );
}

#[test]
fn drain_on_running_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Running, &LifecycleCommand::Drain);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Draining,
        }
    );
}

#[test]
fn stop_on_running_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Running, &LifecycleCommand::Stop);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Stopped,
        }
    );
}

#[test]
fn stop_on_stopped_is_noop() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Stopped, &LifecycleCommand::Stop);

    assert_eq!(effect, CommandEffect::Noop);
}

#[test]
fn restart_on_failed_is_accepted() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Failed, &LifecycleCommand::Restart);

    assert_eq!(
        effect,
        CommandEffect::Accepted {
            next_state: LifecycleState::Restarting,
        }
    );
}

#[test]
fn restart_on_uninitialized_is_deferred() {
    let table = LifecycleTransitionTable;

    let effect = table.evaluate(&LifecycleState::Uninitialized, &LifecycleCommand::Restart);

    let CommandEffect::Deferred {
        command,
        inserted_at,
    } = effect
    else {
        panic!("expected deferred effect");
    };
    assert_eq!(command, LifecycleCommand::Restart);
    chrono::DateTime::parse_from_rfc3339(&inserted_at.0).unwrap();
}
