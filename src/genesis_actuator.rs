use std::path::PathBuf;

use serde::Serialize;

mod autocli;
mod fake;

pub use autocli::{
    AutoCliGenesisActuator, GenesisCommandOutput, GenesisCommandRunner, GenesisCommandSpec,
    SystemGenesisCommandRunner,
};
pub use fake::FakeGenesisActuator;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum GenesisChannel {
    UserDataDir,
    Cdp,
}

impl GenesisChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserDataDir => "user_data_dir",
            Self::Cdp => "cdp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisAskRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenesisAskResponse {
    pub answer: String,
    pub channel: GenesisChannel,
    pub primary_repair: Option<GenesisRepairPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenesisRepairPlan {
    pub reason: String,
    pub recommended_action: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisConfig {
    pub program: String,
    pub profile_dir: PathBuf,
    pub cdp_port: u16,
    pub timeout_ms: u64,
}

impl GenesisConfig {
    pub fn new(profile_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: "autocli".to_string(),
            profile_dir: profile_dir.into(),
            cdp_port: 9222,
            timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenesisError {
    EmptyPrompt,
    CommandNotFound(String),
    CommandFailed {
        channel: GenesisChannel,
        status_code: Option<i32>,
        stderr_preview: String,
    },
    SessionExpired {
        channel: GenesisChannel,
        marker: String,
    },
    Timeout {
        channel: GenesisChannel,
        message: String,
    },
    AllChannelsDown {
        primary: Box<GenesisError>,
        fallback: Box<GenesisError>,
    },
}

pub trait GenesisActuator {
    fn ask(&mut self, request: GenesisAskRequest) -> Result<GenesisAskResponse, GenesisError>;
}

pub fn session_expired_marker(text: &str) -> Option<&'static str> {
    const MARKERS: &[&str] = &["请登录", "验证码", "登录后查看"];
    MARKERS.iter().copied().find(|marker| text.contains(marker))
}
