//! `genesis_actuator::fake` 模块。公开接口：struct FakeGenesisActuator；fn new, with_channel, calls。

use crate::genesis_actuator::{
    GenesisActuator, GenesisAskRequest, GenesisAskResponse, GenesisChannel, GenesisError,
};

#[derive(Debug, Clone)]
pub struct FakeGenesisActuator {
    answer: String,
    channel: GenesisChannel,
    calls: Vec<String>,
}

impl FakeGenesisActuator {
    pub fn new(answer: impl Into<String>) -> Self {
        Self {
            answer: answer.into(),
            channel: GenesisChannel::UserDataDir,
            calls: Vec::new(),
        }
    }

    pub fn with_channel(mut self, channel: GenesisChannel) -> Self {
        self.channel = channel;
        self
    }

    pub fn calls(&self) -> &[String] {
        &self.calls
    }
}

impl GenesisActuator for FakeGenesisActuator {
    fn ask(&mut self, request: GenesisAskRequest) -> Result<GenesisAskResponse, GenesisError> {
        if request.prompt.trim().is_empty() {
            return Err(GenesisError::EmptyPrompt);
        }
        self.calls.push(request.prompt);
        Ok(GenesisAskResponse {
            answer: self.answer.clone(),
            channel: self.channel.clone(),
            primary_repair: None,
        })
    }
}
