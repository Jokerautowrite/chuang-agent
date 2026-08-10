//! `session_archive` 模块。公开接口：struct SessionTurnArchive, SessionRollingSummary,
//! SqliteSessionArchive；enum SessionArchiveError；fn open, append, append_with_summary,
//! replay, rolling_summary。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::memory_store::MemoryRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurnArchive {
    pub session_id: String,
    pub sequence: u64,
    pub created_at: String,
    pub raw_user_input: String,
    pub raw_response: String,
    pub runtime_event_refs: Vec<String>,
    pub runtime_report_refs: Vec<String>,
    pub searchable_summary_pointer: Option<String>,
}

/// 可在会话中断或上下文压缩后恢复的、会话级累计摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRollingSummary {
    pub session_id: String,
    pub updated_at: String,
    pub turn_count: u64,
    pub completed: String,
    pub in_progress: String,
    pub pending: String,
    pub constraints: String,
}

impl SessionRollingSummary {
    pub fn render(&self) -> String {
        format!(
            "[session-rolling-summary]\n已完成：{}\n进行中：{}\n待办：{}\n约束：{}\n轮次：{}\n更新时间：{}",
            self.completed, self.in_progress, self.pending, self.constraints, self.turn_count, self.updated_at
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteSessionArchive {
    path: PathBuf,
}

impl SqliteSessionArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionArchiveError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|_| SessionArchiveError::storage("create_parent"))?;
        }

        let archive = Self { path };
        archive.init_schema()?;
        Ok(archive)
    }

    pub fn append(
        &self,
        session_id: impl Into<String>,
        raw_user_input: impl Into<String>,
        raw_response: impl Into<String>,
        runtime_event_refs: Vec<String>,
        runtime_report_refs: Vec<String>,
        searchable_summary_pointer: Option<String>,
    ) -> Result<SessionTurnArchive, SessionArchiveError> {
        let session_id = session_id.into();
        validate_session_id(&session_id)?;

        let raw_user_input = raw_user_input.into();
        let raw_response = raw_response.into();
        let event_refs_json = serde_json::to_string(&runtime_event_refs)
            .map_err(|_| SessionArchiveError::storage("serialize_event_refs"))?;
        let report_refs_json = serde_json::to_string(&runtime_report_refs)
            .map_err(|_| SessionArchiveError::storage("serialize_report_refs"))?;
        let created_at = current_rfc3339_timestamp();

        let mut conn = self.connection()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SessionArchiveError::storage("begin_append"))?;
        let next_sequence = insert_archive_turn(
            &transaction,
            &session_id,
            &created_at,
            &raw_user_input,
            &raw_response,
            &event_refs_json,
            &report_refs_json,
            searchable_summary_pointer.as_deref(),
        )?;
        transaction
            .commit()
            .map_err(|_| SessionArchiveError::storage("commit_append"))?;

        Ok(SessionTurnArchive {
            session_id,
            sequence: u64::try_from(next_sequence)
                .map_err(|_| SessionArchiveError::corrupt("sequence", "out_of_range"))?,
            created_at,
            raw_user_input,
            raw_response,
            runtime_event_refs,
            runtime_report_refs,
            searchable_summary_pointer,
        })
    }

    pub fn append_with_summary(
        &self,
        session_id: impl Into<String>,
        raw_user_input: impl Into<String>,
        raw_response: impl Into<String>,
        runtime_event_refs: Vec<String>,
        runtime_report_refs: Vec<String>,
        summary: MemoryRecord,
    ) -> Result<SessionTurnArchive, SessionArchiveError> {
        let session_id = session_id.into();
        validate_session_id(&session_id)?;

        let raw_user_input = raw_user_input.into();
        let raw_response = raw_response.into();
        let event_refs_json = serde_json::to_string(&runtime_event_refs)
            .map_err(|_| SessionArchiveError::storage("serialize_event_refs"))?;
        let report_refs_json = serde_json::to_string(&runtime_report_refs)
            .map_err(|_| SessionArchiveError::storage("serialize_report_refs"))?;
        let metadata_json = serde_json::to_string(&summary.metadata)
            .map_err(|_| SessionArchiveError::storage("serialize_summary_metadata"))?;
        let searchable_summary_pointer = format!("memory://{}", summary.id);
        let created_at = current_rfc3339_timestamp();

        let mut conn = self.connection()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SessionArchiveError::storage("begin_append_with_summary"))?;
        transaction
            .execute(
                "INSERT INTO memories (id, content, metadata_json, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    summary.id,
                    summary.content,
                    metadata_json,
                    summary.created_at,
                    summary.expires_at,
                ],
            )
            .map_err(|_| SessionArchiveError::storage("insert_summary"))?;
        let next_sequence = insert_archive_turn(
            &transaction,
            &session_id,
            &created_at,
            &raw_user_input,
            &raw_response,
            &event_refs_json,
            &report_refs_json,
            Some(&searchable_summary_pointer),
        )?;
        transaction
            .commit()
            .map_err(|_| SessionArchiveError::storage("commit_append_with_summary"))?;

        Ok(SessionTurnArchive {
            session_id,
            sequence: u64::try_from(next_sequence)
                .map_err(|_| SessionArchiveError::corrupt("sequence", "out_of_range"))?,
            created_at,
            raw_user_input,
            raw_response,
            runtime_event_refs,
            runtime_report_refs,
            searchable_summary_pointer: Some(searchable_summary_pointer),
        })
    }

    pub fn replay(&self, session_id: &str) -> Result<Vec<SessionTurnArchive>, SessionArchiveError> {
        validate_session_id(session_id)?;
        let conn = self.connection()?;
        let mut statement = conn
            .prepare(
                "SELECT
                    session_id,
                    sequence,
                    created_at,
                    raw_user_input,
                    raw_response,
                    runtime_event_refs_json,
                    runtime_report_refs_json,
                    searchable_summary_pointer
                 FROM session_turn_archive
                 WHERE session_id = ?1
                 ORDER BY sequence ASC",
            )
            .map_err(|_| SessionArchiveError::storage("prepare_replay"))?;

        let rows = statement
            .query_map(params![session_id], |row| {
                Ok(StoredSessionTurn {
                    session_id: row.get(0)?,
                    sequence: row.get(1)?,
                    created_at: row.get(2)?,
                    raw_user_input: row.get(3)?,
                    raw_response: row.get(4)?,
                    runtime_event_refs_json: row.get(5)?,
                    runtime_report_refs_json: row.get(6)?,
                    searchable_summary_pointer: row.get(7)?,
                })
            })
            .map_err(|_| SessionArchiveError::storage("query_replay"))?;

        let mut turns = Vec::new();
        for row in rows {
            let stored = row.map_err(|_| SessionArchiveError::storage("read_replay"))?;
            turns.push(stored.try_into()?);
        }
        Ok(turns)
    }

    pub fn rolling_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionRollingSummary>, SessionArchiveError> {
        validate_session_id(session_id)?;
        let conn = self.connection()?;
        conn.query_row(
            "SELECT session_id, updated_at, turn_count, completed, in_progress, pending, constraints
             FROM session_rolling_summary WHERE session_id = ?1",
            params![session_id],
            |row| Ok(SessionRollingSummary {
                session_id: row.get(0)?, updated_at: row.get(1)?, turn_count: row.get(2)?,
                completed: row.get(3)?, in_progress: row.get(4)?, pending: row.get(5)?,
                constraints: row.get(6)?,
            }),
        )
        .optional()
        .map_err(|_| SessionArchiveError::storage("read_rolling_summary"))
    }

    /// 在已落盘的回合之后更新会话摘要。采用 UPSERT，重复恢复安全。
    pub fn update_rolling_summary(
        &self,
        summary: SessionRollingSummary,
    ) -> Result<(), SessionArchiveError> {
        validate_session_id(&summary.session_id)?;
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO session_rolling_summary
                (session_id, updated_at, turn_count, completed, in_progress, pending, constraints)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id) DO UPDATE SET
                updated_at=excluded.updated_at, turn_count=excluded.turn_count,
                completed=excluded.completed, in_progress=excluded.in_progress,
                pending=excluded.pending, constraints=excluded.constraints",
            params![
                summary.session_id,
                summary.updated_at,
                summary.turn_count,
                summary.completed,
                summary.in_progress,
                summary.pending,
                summary.constraints
            ],
        )
        .map_err(|_| SessionArchiveError::storage("write_rolling_summary"))?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, SessionArchiveError> {
        Connection::open(&self.path).map_err(|_| SessionArchiveError::storage("open_database"))
    }

    fn init_schema(&self) -> Result<(), SessionArchiveError> {
        let conn = self.connection()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at);
            CREATE INDEX IF NOT EXISTS idx_memories_expires_at ON memories(expires_at);
            CREATE TABLE IF NOT EXISTS session_turn_archive (
                session_id TEXT NOT NULL,
                sequence INTEGER NOT NULL CHECK(sequence > 0),
                created_at TEXT NOT NULL,
                raw_user_input TEXT NOT NULL,
                raw_response TEXT NOT NULL,
                runtime_event_refs_json TEXT NOT NULL,
                runtime_report_refs_json TEXT NOT NULL,
                searchable_summary_pointer TEXT NULL,
                PRIMARY KEY (session_id, sequence)
            );
            CREATE INDEX IF NOT EXISTS idx_session_turn_archive_created_at
                ON session_turn_archive(created_at);
            CREATE TABLE IF NOT EXISTS session_rolling_summary (
                session_id TEXT PRIMARY KEY,
                updated_at TEXT NOT NULL,
                turn_count INTEGER NOT NULL CHECK(turn_count >= 0),
                completed TEXT NOT NULL,
                in_progress TEXT NOT NULL,
                pending TEXT NOT NULL,
                constraints TEXT NOT NULL
            );
            ",
        )
        .map_err(|_| SessionArchiveError::storage("initialize_schema"))
    }
}

fn insert_archive_turn(
    transaction: &Transaction<'_>,
    session_id: &str,
    created_at: &str,
    raw_user_input: &str,
    raw_response: &str,
    event_refs_json: &str,
    report_refs_json: &str,
    searchable_summary_pointer: Option<&str>,
) -> Result<i64, SessionArchiveError> {
    let next_sequence = transaction
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1
             FROM session_turn_archive
             WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(|_| SessionArchiveError::storage("allocate_sequence"))?;

    transaction
        .execute(
            "INSERT INTO session_turn_archive (
                session_id,
                sequence,
                created_at,
                raw_user_input,
                raw_response,
                runtime_event_refs_json,
                runtime_report_refs_json,
                searchable_summary_pointer
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                session_id,
                next_sequence,
                created_at,
                raw_user_input,
                raw_response,
                event_refs_json,
                report_refs_json,
                searchable_summary_pointer,
            ],
        )
        .map_err(|_| SessionArchiveError::storage("insert_turn"))?;

    Ok(next_sequence)
}

#[derive(Debug)]
struct StoredSessionTurn {
    session_id: String,
    sequence: i64,
    created_at: String,
    raw_user_input: String,
    raw_response: String,
    runtime_event_refs_json: String,
    runtime_report_refs_json: String,
    searchable_summary_pointer: Option<String>,
}

impl TryFrom<StoredSessionTurn> for SessionTurnArchive {
    type Error = SessionArchiveError;

    fn try_from(stored: StoredSessionTurn) -> Result<Self, Self::Error> {
        let sequence = u64::try_from(stored.sequence)
            .map_err(|_| SessionArchiveError::corrupt("sequence", "out_of_range"))?;
        DateTime::parse_from_rfc3339(&stored.created_at)
            .map_err(|_| SessionArchiveError::corrupt("created_at", "invalid_rfc3339"))?;
        let runtime_event_refs = serde_json::from_str(&stored.runtime_event_refs_json)
            .map_err(|_| SessionArchiveError::corrupt("runtime_event_refs", "invalid_json"))?;
        let runtime_report_refs = serde_json::from_str(&stored.runtime_report_refs_json)
            .map_err(|_| SessionArchiveError::corrupt("runtime_report_refs", "invalid_json"))?;

        Ok(Self {
            session_id: stored.session_id,
            sequence,
            created_at: stored.created_at,
            raw_user_input: stored.raw_user_input,
            raw_response: stored.raw_response,
            runtime_event_refs,
            runtime_report_refs,
            searchable_summary_pointer: stored.searchable_summary_pointer,
        })
    }
}

fn validate_session_id(session_id: &str) -> Result<(), SessionArchiveError> {
    if session_id.trim().is_empty() {
        return Err(SessionArchiveError::InvalidInput {
            field: "session_id",
            code: "must_not_be_empty",
        });
    }
    Ok(())
}

fn current_rfc3339_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionArchiveError {
    InvalidInput {
        field: &'static str,
        code: &'static str,
    },
    StorageUnavailable {
        operation: &'static str,
    },
    CorruptRecord {
        field: &'static str,
        code: &'static str,
    },
}

impl SessionArchiveError {
    fn storage(operation: &'static str) -> Self {
        Self::StorageUnavailable { operation }
    }

    fn corrupt(field: &'static str, code: &'static str) -> Self {
        Self::CorruptRecord { field, code }
    }
}

impl fmt::Display for SessionArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, code } => {
                write!(formatter, "invalid session archive input: {field} ({code})")
            }
            Self::StorageUnavailable { operation } => {
                write!(
                    formatter,
                    "session archive storage unavailable: {operation}"
                )
            }
            Self::CorruptRecord { field, code } => {
                write!(
                    formatter,
                    "corrupt session archive record: {field} ({code})"
                )
            }
        }
    }
}

impl std::error::Error for SessionArchiveError {}
