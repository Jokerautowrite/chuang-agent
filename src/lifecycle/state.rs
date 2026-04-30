use crate::common::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    Uninitialized,
    Starting,
    Running,
    Checkpointing,
    Pausing,
    Paused,
    Draining,
    Restarting,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleCommand {
    Start,
    Pause,
    Resume,
    Checkpoint,
    Drain,
    Stop,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEffect<Cmd> {
    Accepted {
        next_state: LifecycleState,
    },
    Rejected {
        reason: String,
    },
    Noop,
    Deferred {
        command: Cmd,
        inserted_at: Timestamp,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRejectReason {
    InvalidState {
        current: LifecycleState,
        expected_states: Vec<LifecycleState>,
    },
    TimeoutDeferred {
        command: LifecycleCommand,
        elapsed_ms: u64,
    },
    ConcurrencyLocked,
}
