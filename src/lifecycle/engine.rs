use crate::lifecycle::{
    CommandEffect, CommandRejectReason, LifecycleCommand, LifecycleState, LifecycleTransitionTable,
};

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
}

impl LifecycleEngine {
    pub fn new(state: LifecycleState) -> Self {
        Self {
            state,
            deferred: vec![],
            transition_table: LifecycleTransitionTable::default(),
        }
    }
}

impl LifecycleStateMachine for LifecycleEngine {
    type Command = LifecycleCommand;

    fn handle_command(
        &mut self,
        command: Self::Command,
    ) -> Result<CommandEffect<Self::Command>, CommandRejectReason> {
        let effect = self.transition_table.evaluate(&self.state, &command);
        match &effect {
            CommandEffect::Accepted { next_state } => {
                self.state = next_state.clone();
            }
            CommandEffect::Deferred { command, .. } => {
                self.deferred.push(command.clone());
            }
            CommandEffect::Rejected { .. } | CommandEffect::Noop => {}
        }
        Ok(effect)
    }

    fn current_state(&self) -> LifecycleState {
        self.state.clone()
    }

    fn drive_deferred(&mut self) -> Vec<CommandEffect<Self::Command>> {
        let pending = std::mem::take(&mut self.deferred);
        let mut effects = Vec::with_capacity(pending.len());

        for command in pending {
            let effect = self.transition_table.evaluate(&self.state, &command);
            match &effect {
                CommandEffect::Accepted { next_state } => {
                    self.state = next_state.clone();
                }
                CommandEffect::Deferred { command, .. } => {
                    self.deferred.push(command.clone());
                }
                CommandEffect::Rejected { .. } | CommandEffect::Noop => {}
            }
            effects.push(effect);
        }

        effects
    }
}
