use crate::lifecycle::{
    checkpoint::{now_timestamp, parse_timestamp},
    CheckpointStoreError, CommandEffect, CommandRejectReason, DeferredLifecycleCommand,
    LifecycleCommand, LifecycleState, LifecycleTransitionTable, LocalCheckpointStore,
    RuntimeCheckpoint,
};
use chrono::{DateTime, Duration, FixedOffset};
use std::fmt;

const DEFERRED_TIMEOUT_SECONDS: i64 = 30;

pub trait LifecycleStateMachine {
    type Command;

    fn handle_command(
        &mut self,
        command: Self::Command,
    ) -> Result<CommandEffect<Self::Command>, CommandRejectReason>;

    fn current_state(&self) -> LifecycleState;

    fn drive_deferred(&mut self) -> Vec<CommandEffect<Self::Command>>;
}

#[derive(Debug, Clone)]
pub struct LifecycleEngine {
    pub state: LifecycleState,
    pub deferred: Vec<LifecycleCommand>,
    pub transition_table: LifecycleTransitionTable,
    agent_id: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    packed_segment_ids: Vec<String>,
    memory_cursor: Option<String>,
    unfinished_tool_call_ids: Vec<String>,
    deferred_inserted_at: Vec<crate::common::Timestamp>,
}

impl LifecycleEngine {
    pub fn new(state: LifecycleState) -> Self {
        Self {
            state,
            deferred: vec![],
            transition_table: LifecycleTransitionTable::default(),
            agent_id: None,
            thread_id: None,
            turn_id: None,
            packed_segment_ids: Vec::new(),
            memory_cursor: None,
            unfinished_tool_call_ids: Vec::new(),
            deferred_inserted_at: vec![],
        }
    }

    pub fn checkpoint(&self) -> RuntimeCheckpoint {
        let fallback_timestamp = now_timestamp();
        RuntimeCheckpoint::new(
            self.state.clone(),
            self.deferred
                .iter()
                .enumerate()
                .map(|(index, command)| DeferredLifecycleCommand {
                    command: command.clone(),
                    inserted_at: self
                        .deferred_inserted_at
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| fallback_timestamp.clone()),
                })
                .collect(),
        )
        .with_optional_runtime_refs(
            self.agent_id.clone(),
            self.thread_id.clone(),
            self.turn_id.clone(),
            self.packed_segment_ids.clone(),
            self.memory_cursor.clone(),
            self.unfinished_tool_call_ids.clone(),
        )
    }

    pub fn restore(checkpoint: RuntimeCheckpoint) -> Result<Self, CheckpointStoreError> {
        checkpoint.validate()?;
        Ok(Self {
            state: checkpoint.state,
            deferred: checkpoint
                .deferred
                .iter()
                .map(|item| item.command.clone())
                .collect(),
            deferred_inserted_at: checkpoint
                .deferred
                .iter()
                .map(|item| item.inserted_at.clone())
                .collect(),
            transition_table: LifecycleTransitionTable::default(),
            agent_id: checkpoint.agent_id,
            thread_id: checkpoint.thread_id,
            turn_id: checkpoint.turn_id,
            packed_segment_ids: checkpoint.packed_segment_ids,
            memory_cursor: checkpoint.memory_cursor,
            unfinished_tool_call_ids: checkpoint.unfinished_tool_call_ids,
        })
    }

    pub fn reopen(store: &LocalCheckpointStore) -> Result<Self, CheckpointStoreError> {
        Self::restore(store.load_latest()?)
    }

    pub fn handle_command_persisted(
        &mut self,
        command: LifecycleCommand,
        store: &LocalCheckpointStore,
    ) -> Result<CommandEffect<LifecycleCommand>, LifecyclePersistenceError> {
        let before = self.clone();
        let effect = self.handle_command(command)?;
        if let Err(error) = store.replace(&self.checkpoint()) {
            *self = before;
            return Err(LifecyclePersistenceError::Checkpoint(error));
        }
        Ok(effect)
    }

    pub fn drive_deferred_checked(
        &mut self,
        now: DateTime<FixedOffset>,
    ) -> Vec<Result<CommandEffect<LifecycleCommand>, CommandRejectReason>> {
        let pending = std::mem::take(&mut self.deferred);
        let mut inserted_at = std::mem::take(&mut self.deferred_inserted_at);
        inserted_at.resize_with(pending.len(), now_timestamp);
        let mut effects = Vec::with_capacity(pending.len());

        for (command, inserted_at) in pending.into_iter().zip(inserted_at) {
            let parsed = match parse_timestamp(&inserted_at) {
                Ok(parsed) => parsed,
                Err(_) => {
                    effects.push(Err(CommandRejectReason::TimeoutDeferred {
                        command,
                        elapsed_ms: u64::MAX,
                    }));
                    continue;
                }
            };
            let elapsed = now.signed_duration_since(parsed);
            if elapsed >= Duration::seconds(DEFERRED_TIMEOUT_SECONDS) {
                effects.push(Err(CommandRejectReason::TimeoutDeferred {
                    command,
                    elapsed_ms: elapsed.num_milliseconds().max(0) as u64,
                }));
                continue;
            }

            let effect = self.transition_table.evaluate(&self.state, &command);
            match &effect {
                CommandEffect::Deferred { command, .. } => {
                    self.deferred.push(command.clone());
                    self.deferred_inserted_at.push(inserted_at);
                }
                _ => self.apply_effect(&effect),
            }
            effects.push(Ok(effect));
        }
        effects
    }

    fn apply_effect(&mut self, effect: &CommandEffect<LifecycleCommand>) {
        match effect {
            CommandEffect::Accepted { next_state } => self.state = next_state.clone(),
            CommandEffect::Deferred {
                command,
                inserted_at,
            } => {
                self.deferred.push(command.clone());
                self.deferred_inserted_at.push(inserted_at.clone());
            }
            CommandEffect::Rejected { .. } | CommandEffect::Noop => {}
        }
    }
}

#[derive(Debug)]
pub enum LifecyclePersistenceError {
    Command(CommandRejectReason),
    Checkpoint(CheckpointStoreError),
}

impl fmt::Display for LifecyclePersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(formatter, "lifecycle command rejected: {error:?}"),
            Self::Checkpoint(error) => write!(formatter, "lifecycle checkpoint failed: {error}"),
        }
    }
}

impl std::error::Error for LifecyclePersistenceError {}

impl From<CommandRejectReason> for LifecyclePersistenceError {
    fn from(error: CommandRejectReason) -> Self {
        Self::Command(error)
    }
}

impl LifecycleStateMachine for LifecycleEngine {
    type Command = LifecycleCommand;

    fn handle_command(
        &mut self,
        command: Self::Command,
    ) -> Result<CommandEffect<Self::Command>, CommandRejectReason> {
        let effect = self.transition_table.evaluate(&self.state, &command);
        self.apply_effect(&effect);
        Ok(effect)
    }

    fn current_state(&self) -> LifecycleState {
        self.state.clone()
    }

    fn drive_deferred(&mut self) -> Vec<CommandEffect<Self::Command>> {
        let now = DateTime::parse_from_rfc3339(&now_timestamp().0)
            .expect("internally generated lifecycle timestamp must be RFC3339");
        self.drive_deferred_checked(now)
            .into_iter()
            .map(|result| match result {
                Ok(effect) => effect,
                Err(reason) => CommandEffect::Rejected {
                    reason: format!("{reason:?}"),
                },
            })
            .collect()
    }
}
