//! `lifecycle::transition` 模块。公开接口：struct LifecycleTransitionTable；fn evaluate。

use crate::lifecycle::checkpoint::now_timestamp;
use crate::lifecycle::{CommandEffect, LifecycleCommand, LifecycleState};

#[derive(Debug, Clone, Default)]
pub struct LifecycleTransitionTable;

impl LifecycleTransitionTable {
    pub fn evaluate(
        &self,
        state: &LifecycleState,
        command: &LifecycleCommand,
    ) -> CommandEffect<LifecycleCommand> {
        match (command, state) {
            (LifecycleCommand::Start, LifecycleState::Uninitialized) => CommandEffect::Accepted {
                next_state: LifecycleState::Starting,
            },
            (LifecycleCommand::Start, LifecycleState::Starting) => Self::reject(),
            (LifecycleCommand::Start, LifecycleState::Running) => CommandEffect::Noop,
            (LifecycleCommand::Start, LifecycleState::Checkpointing) => Self::reject(),
            (LifecycleCommand::Start, LifecycleState::Pausing) => Self::reject(),
            (LifecycleCommand::Start, LifecycleState::Paused) => CommandEffect::Noop,
            (LifecycleCommand::Start, LifecycleState::Draining) => Self::reject(),
            (LifecycleCommand::Start, LifecycleState::Restarting) => {
                Self::defer(LifecycleCommand::Start)
            }
            (LifecycleCommand::Start, LifecycleState::Stopped) => CommandEffect::Accepted {
                next_state: LifecycleState::Starting,
            },
            (LifecycleCommand::Start, LifecycleState::Failed) => {
                Self::defer(LifecycleCommand::Start)
            }

            (LifecycleCommand::Pause, LifecycleState::Running) => CommandEffect::Accepted {
                next_state: LifecycleState::Pausing,
            },
            (LifecycleCommand::Pause, LifecycleState::Paused) => CommandEffect::Noop,
            (LifecycleCommand::Pause, LifecycleState::Stopped) => CommandEffect::Noop,
            (LifecycleCommand::Pause, _) => Self::reject(),

            (LifecycleCommand::Resume, LifecycleState::Starting) => {
                Self::defer(LifecycleCommand::Resume)
            }
            (LifecycleCommand::Resume, LifecycleState::Running) => CommandEffect::Noop,
            (LifecycleCommand::Resume, LifecycleState::Pausing) => {
                Self::defer(LifecycleCommand::Resume)
            }
            (LifecycleCommand::Resume, LifecycleState::Paused) => CommandEffect::Accepted {
                next_state: LifecycleState::Running,
            },
            (LifecycleCommand::Resume, LifecycleState::Stopped) => CommandEffect::Noop,
            (LifecycleCommand::Resume, _) => Self::reject(),

            (LifecycleCommand::Checkpoint, LifecycleState::Running) => CommandEffect::Accepted {
                next_state: LifecycleState::Checkpointing,
            },
            (LifecycleCommand::Checkpoint, LifecycleState::Draining) => {
                Self::defer(LifecycleCommand::Checkpoint)
            }
            (LifecycleCommand::Checkpoint, LifecycleState::Stopped) => CommandEffect::Noop,
            (LifecycleCommand::Checkpoint, _) => Self::reject(),

            (LifecycleCommand::Drain, LifecycleState::Running) => CommandEffect::Accepted {
                next_state: LifecycleState::Draining,
            },
            (LifecycleCommand::Drain, LifecycleState::Checkpointing) => {
                Self::defer(LifecycleCommand::Drain)
            }
            (LifecycleCommand::Drain, LifecycleState::Paused) => CommandEffect::Noop,
            (LifecycleCommand::Drain, LifecycleState::Stopped) => CommandEffect::Noop,
            (LifecycleCommand::Drain, _) => Self::reject(),

            (LifecycleCommand::Stop, LifecycleState::Uninitialized) => CommandEffect::Accepted {
                next_state: LifecycleState::Stopped,
            },
            (LifecycleCommand::Stop, LifecycleState::Starting) => {
                Self::defer(LifecycleCommand::Stop)
            }
            (LifecycleCommand::Stop, LifecycleState::Running) => CommandEffect::Accepted {
                next_state: LifecycleState::Stopped,
            },
            (LifecycleCommand::Stop, LifecycleState::Checkpointing) => {
                Self::defer(LifecycleCommand::Stop)
            }
            (LifecycleCommand::Stop, LifecycleState::Pausing) => CommandEffect::Accepted {
                next_state: LifecycleState::Stopped,
            },
            (LifecycleCommand::Stop, LifecycleState::Paused) => CommandEffect::Accepted {
                next_state: LifecycleState::Stopped,
            },
            (LifecycleCommand::Stop, LifecycleState::Draining) => CommandEffect::Accepted {
                next_state: LifecycleState::Stopped,
            },
            (LifecycleCommand::Stop, LifecycleState::Restarting) => {
                Self::defer(LifecycleCommand::Stop)
            }
            (LifecycleCommand::Stop, LifecycleState::Stopped) => CommandEffect::Noop,
            (LifecycleCommand::Stop, LifecycleState::Failed) => CommandEffect::Accepted {
                next_state: LifecycleState::Stopped,
            },

            (LifecycleCommand::Restart, LifecycleState::Uninitialized) => {
                Self::defer(LifecycleCommand::Restart)
            }
            (LifecycleCommand::Restart, LifecycleState::Starting) => Self::reject(),
            (LifecycleCommand::Restart, LifecycleState::Running) => CommandEffect::Accepted {
                next_state: LifecycleState::Restarting,
            },
            (LifecycleCommand::Restart, LifecycleState::Checkpointing) => Self::reject(),
            (LifecycleCommand::Restart, LifecycleState::Pausing) => Self::reject(),
            (LifecycleCommand::Restart, LifecycleState::Paused) => CommandEffect::Accepted {
                next_state: LifecycleState::Restarting,
            },
            (LifecycleCommand::Restart, LifecycleState::Draining) => Self::reject(),
            (LifecycleCommand::Restart, LifecycleState::Restarting) => Self::reject(),
            (LifecycleCommand::Restart, LifecycleState::Stopped) => CommandEffect::Accepted {
                next_state: LifecycleState::Restarting,
            },
            (LifecycleCommand::Restart, LifecycleState::Failed) => CommandEffect::Accepted {
                next_state: LifecycleState::Restarting,
            },
        }
    }

    fn reject() -> CommandEffect<LifecycleCommand> {
        CommandEffect::Rejected {
            reason: "reject".to_string(),
        }
    }

    fn defer(command: LifecycleCommand) -> CommandEffect<LifecycleCommand> {
        CommandEffect::Deferred {
            command,
            inserted_at: now_timestamp(),
        }
    }
}
