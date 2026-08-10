//! `lifecycle::checkpoint` 模块。公开接口：struct DeferredLifecycleCommand, RuntimeCheckpoint, LocalCheckpointStore；enum CheckpointStoreError；fn new, with_optional_runtime_refs, with_runtime_refs, path, append, replace, load_all, load_latest；const RUNTIME_CHECKPOINT_SCHEMA_VERSION。

use crate::common::Timestamp;
use crate::lifecycle::{LifecycleCommand, LifecycleState};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const RUNTIME_CHECKPOINT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeferredLifecycleCommand {
    pub command: LifecycleCommand,
    pub inserted_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCheckpoint {
    pub schema_version: u32,
    pub saved_at: Timestamp,
    pub state: LifecycleState,
    pub deferred: Vec<DeferredLifecycleCommand>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub packed_segment_ids: Vec<String>,
    #[serde(default)]
    pub memory_cursor: Option<String>,
    #[serde(default)]
    pub unfinished_tool_call_ids: Vec<String>,
}

impl RuntimeCheckpoint {
    pub fn new(state: LifecycleState, deferred: Vec<DeferredLifecycleCommand>) -> Self {
        Self {
            schema_version: RUNTIME_CHECKPOINT_SCHEMA_VERSION,
            saved_at: now_timestamp(),
            state,
            deferred,
            agent_id: None,
            thread_id: None,
            turn_id: None,
            packed_segment_ids: Vec::new(),
            memory_cursor: None,
            unfinished_tool_call_ids: Vec::new(),
        }
    }

    pub fn with_optional_runtime_refs(
        mut self,
        agent_id: Option<String>,
        thread_id: Option<String>,
        turn_id: Option<String>,
        packed_segment_ids: Vec<String>,
        memory_cursor: Option<String>,
        unfinished_tool_call_ids: Vec<String>,
    ) -> Self {
        self.agent_id = agent_id;
        self.thread_id = thread_id;
        self.turn_id = turn_id;
        self.packed_segment_ids = packed_segment_ids;
        self.memory_cursor = memory_cursor;
        self.unfinished_tool_call_ids = unfinished_tool_call_ids;
        self
    }

    pub fn with_runtime_refs(
        mut self,
        agent_id: impl Into<String>,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        packed_segment_ids: Vec<String>,
        memory_cursor: Option<String>,
        unfinished_tool_call_ids: Vec<String>,
    ) -> Self {
        self.agent_id = Some(agent_id.into());
        self.thread_id = Some(thread_id.into());
        self.turn_id = Some(turn_id.into());
        self.packed_segment_ids = packed_segment_ids;
        self.memory_cursor = memory_cursor;
        self.unfinished_tool_call_ids = unfinished_tool_call_ids;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), CheckpointStoreError> {
        if self.schema_version != RUNTIME_CHECKPOINT_SCHEMA_VERSION {
            return Err(CheckpointStoreError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        parse_timestamp(&self.saved_at)?;
        for deferred in &self.deferred {
            parse_timestamp(&deferred.inserted_at)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct CheckpointFile {
    schema_version: u32,
    checkpoints: Vec<RuntimeCheckpoint>,
}

#[derive(Debug)]
pub enum CheckpointStoreError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedSchemaVersion(u32),
    InvalidTimestamp(String),
    Empty,
}

impl fmt::Display for CheckpointStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "checkpoint I/O error: {error}"),
            Self::Json(error) => write!(formatter, "checkpoint JSON error: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "unsupported checkpoint schema version: {version}"
                )
            }
            Self::InvalidTimestamp(value) => {
                write!(formatter, "invalid RFC3339 checkpoint timestamp: {value}")
            }
            Self::Empty => write!(formatter, "checkpoint store is empty"),
        }
    }
}

impl std::error::Error for CheckpointStoreError {}

impl From<io::Error> for CheckpointStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for CheckpointStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone)]
pub struct LocalCheckpointStore {
    path: PathBuf,
}

impl LocalCheckpointStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, checkpoint: &RuntimeCheckpoint) -> Result<(), CheckpointStoreError> {
        checkpoint.validate()?;
        let mut file = self.load_file_or_default()?;
        file.checkpoints.push(checkpoint.clone());
        self.atomic_write(&file)
    }

    pub fn replace(&self, checkpoint: &RuntimeCheckpoint) -> Result<(), CheckpointStoreError> {
        checkpoint.validate()?;
        self.atomic_write(&CheckpointFile {
            schema_version: RUNTIME_CHECKPOINT_SCHEMA_VERSION,
            checkpoints: vec![checkpoint.clone()],
        })
    }

    pub fn load_all(&self) -> Result<Vec<RuntimeCheckpoint>, CheckpointStoreError> {
        let file = self.load_file()?;
        if file.checkpoints.is_empty() {
            return Err(CheckpointStoreError::Empty);
        }
        Ok(file.checkpoints)
    }

    pub fn load_latest(&self) -> Result<RuntimeCheckpoint, CheckpointStoreError> {
        self.load_all()?.pop().ok_or(CheckpointStoreError::Empty)
    }

    fn load_file_or_default(&self) -> Result<CheckpointFile, CheckpointStoreError> {
        if !self.path.exists() {
            return Ok(CheckpointFile {
                schema_version: RUNTIME_CHECKPOINT_SCHEMA_VERSION,
                checkpoints: Vec::new(),
            });
        }
        self.load_file()
    }

    fn load_file(&self) -> Result<CheckpointFile, CheckpointStoreError> {
        let bytes = fs::read(&self.path)?;
        let file: CheckpointFile = serde_json::from_slice(&bytes)?;
        if file.schema_version != RUNTIME_CHECKPOINT_SCHEMA_VERSION {
            return Err(CheckpointStoreError::UnsupportedSchemaVersion(
                file.schema_version,
            ));
        }
        for checkpoint in &file.checkpoints {
            checkpoint.validate()?;
        }
        Ok(file)
    }

    fn atomic_write(&self, file: &CheckpointFile) -> Result<(), CheckpointStoreError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runtime-checkpoint.json");
        let nonce = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let temp_path = parent.join(format!(".{file_name}.{}.{nonce}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(file)?;

        (|| -> Result<(), CheckpointStoreError> {
            let mut temp = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)?;
            temp.write_all(&bytes)?;
            temp.sync_all()?;
            fs::rename(&temp_path, &self.path)?;
            if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })()
    }
}

pub(crate) fn now_timestamp() -> Timestamp {
    Timestamp(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub(crate) fn parse_timestamp(
    timestamp: &Timestamp,
) -> Result<chrono::DateTime<chrono::FixedOffset>, CheckpointStoreError> {
    chrono::DateTime::parse_from_rfc3339(&timestamp.0)
        .map_err(|_| CheckpointStoreError::InvalidTimestamp(timestamp.0.clone()))
}
