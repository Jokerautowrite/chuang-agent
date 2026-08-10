//! `lifecycle::state` 模块。公开接口：enum LifecycleState, LifecycleCommand, CommandEffect, CommandRejectReason。

use crate::common::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCommand {
    Start,
    Pause,
    Resume,
    Checkpoint,
    Drain,
    Stop,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
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
