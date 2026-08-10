//! `identity_registry` 模块。公开接口：struct AgentIdentity, IdentityRegistry；enum IdentityRegistryError；fn parse, load, select_active, compatibility_default_identity。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub agent_id: String,
    pub display_name: String,
    pub shell_kind: String,
    pub role: String,
    pub memory_body_id: String,
    #[serde(default)]
    pub lineage: Vec<String>,
    #[serde(default)]
    pub allowed_channels: Vec<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IdentityRegistry {
    pub memory_body_id: String,
    #[serde(default)]
    pub active_agent_id: Option<String>,
    pub agents: Vec<AgentIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityRegistryError {
    ReadFailed {
        path: PathBuf,
    },
    ParseFailed {
        message: String,
    },
    Invalid {
        message: String,
    },
    ActiveAgentNotFound {
        agent_id: String,
    },
    ActiveAgentCount {
        count: usize,
    },
    MemoryBodyMismatch {
        agent_id: String,
        expected: String,
        actual: String,
    },
    ChannelNotAllowed {
        agent_id: String,
        channel: String,
    },
}

impl IdentityRegistry {
    pub fn parse(content: &str) -> Result<Self, IdentityRegistryError> {
        let registry: Self =
            toml::from_str(content).map_err(|error| IdentityRegistryError::ParseFailed {
                message: error.to_string(),
            })?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, IdentityRegistryError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|_| IdentityRegistryError::ReadFailed {
            path: path.to_path_buf(),
        })?;
        Self::parse(&content)
    }

    pub fn select_active(
        &self,
        requested_agent_id: Option<&str>,
        channel: Option<&str>,
    ) -> Result<AgentIdentity, IdentityRegistryError> {
        let configured_active = self.configured_active_agent_id()?;
        if let Some(requested) = requested_agent_id {
            if requested != configured_active {
                return Err(IdentityRegistryError::ActiveAgentNotFound {
                    agent_id: requested.to_string(),
                });
            }
        }

        let identity = self
            .agents
            .iter()
            .find(|agent| agent.agent_id == configured_active)
            .cloned()
            .ok_or_else(|| IdentityRegistryError::ActiveAgentNotFound {
                agent_id: configured_active.to_string(),
            })?;

        if identity.memory_body_id != self.memory_body_id {
            return Err(IdentityRegistryError::MemoryBodyMismatch {
                agent_id: identity.agent_id,
                expected: self.memory_body_id.clone(),
                actual: identity.memory_body_id,
            });
        }

        if let Some(channel) = channel.map(str::trim).filter(|value| !value.is_empty()) {
            if !identity
                .allowed_channels
                .iter()
                .any(|allowed| allowed == channel)
            {
                return Err(IdentityRegistryError::ChannelNotAllowed {
                    agent_id: identity.agent_id,
                    channel: channel.to_string(),
                });
            }
        }

        Ok(identity)
    }

    fn configured_active_agent_id(&self) -> Result<&str, IdentityRegistryError> {
        let flagged = self
            .agents
            .iter()
            .filter(|agent| agent.active)
            .map(|agent| agent.agent_id.as_str())
            .collect::<Vec<_>>();

        match (self.active_agent_id.as_deref(), flagged.as_slice()) {
            (Some(active), []) => Ok(active),
            (None, [active]) => Ok(active),
            (Some(active), [flagged_active]) if active == *flagged_active => Ok(active),
            (_, active) => Err(IdentityRegistryError::ActiveAgentCount {
                count: active.len()
                    + usize::from(self.active_agent_id.is_some() && active.is_empty()),
            }),
        }
    }

    fn validate(&self) -> Result<(), IdentityRegistryError> {
        if self.memory_body_id.trim().is_empty() {
            return Err(IdentityRegistryError::Invalid {
                message: "memory_body_id must not be empty".to_string(),
            });
        }
        if self.agents.is_empty() {
            return Err(IdentityRegistryError::Invalid {
                message: "agents must not be empty".to_string(),
            });
        }

        let mut ids = BTreeSet::new();
        for agent in &self.agents {
            if agent.agent_id.trim().is_empty()
                || agent.display_name.trim().is_empty()
                || agent.shell_kind.trim().is_empty()
                || agent.role.trim().is_empty()
                || agent.memory_body_id.trim().is_empty()
            {
                return Err(IdentityRegistryError::Invalid {
                    message: "agent identity fields must not be empty".to_string(),
                });
            }
            if !ids.insert(agent.agent_id.as_str()) {
                return Err(IdentityRegistryError::Invalid {
                    message: format!("duplicate agent_id: {}", agent.agent_id),
                });
            }
        }

        let active = self.configured_active_agent_id()?;
        if !ids.contains(active) {
            return Err(IdentityRegistryError::ActiveAgentNotFound {
                agent_id: active.to_string(),
            });
        }
        Ok(())
    }
}

pub fn compatibility_default_identity(agent_id: impl Into<String>) -> AgentIdentity {
    let agent_id = agent_id.into();
    AgentIdentity {
        display_name: agent_id.clone(),
        shell_kind: "codex".to_string(),
        role: "kernel".to_string(),
        memory_body_id: "default".to_string(),
        lineage: Vec::new(),
        allowed_channels: Vec::new(),
        active: true,
        agent_id,
    }
}
