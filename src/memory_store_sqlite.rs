//! `memory_store_sqlite` 模块。公开接口：struct SqliteMemoryStore；fn open。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json;

use crate::memory_store::{
    text_match_score_in_mode, MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError, SearchHit,
};

#[derive(Debug)]
pub struct SqliteMemoryStore {
    conn: RefCell<Connection>,
}

impl SqliteMemoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MemoryStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| MemoryStoreError::StorageUnavailable)?;
        }

        let conn = Connection::open(&path).map_err(|_| MemoryStoreError::StorageUnavailable)?;
        // WAL：显著改善并发读写（多 session 同时写记忆时不互相锁库）。
        // busy_timeout：并发写撞锁时等待而非立即报 database is locked。
        conn.busy_timeout(std::time::Duration::from_millis(5000))
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;

        let store = Self {
            conn: RefCell::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn connection(&self) -> Result<std::cell::RefMut<'_, Connection>, MemoryStoreError> {
        self.conn
            .try_borrow_mut()
            .map_err(|_| MemoryStoreError::StorageUnavailable)
    }

    fn init_schema(&self) -> Result<(), MemoryStoreError> {
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
            ",
        )
        .map_err(|_| MemoryStoreError::StorageUnavailable)?;
        Ok(())
    }
}

impl MemoryStore for SqliteMemoryStore {
    fn put(&mut self, record: MemoryRecord) -> Result<(), MemoryStoreError> {
        let conn = self.connection()?;
        let metadata_json = serde_json::to_string(&record.metadata)
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;

        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO memories (id, content, metadata_json, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.id,
                    record.content,
                    metadata_json,
                    record.created_at,
                    record.expires_at,
                ],
            )
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;

        if inserted == 0 {
            return Err(MemoryStoreError::DuplicateId {
                id: "duplicate".to_string(),
            });
        }
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, MemoryStoreError> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT id, content, metadata_json, created_at, expires_at FROM memories WHERE id = ?1",
            params![id],
            |row| {
                let metadata_json: String = row.get(2)?;
                let metadata: BTreeMap<String, String> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();
                Ok(MemoryRecord {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    metadata,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|_| MemoryStoreError::StorageUnavailable)
    }

    fn search(&self, query: &MemoryQuery) -> Result<Vec<SearchHit>, MemoryStoreError> {
        if query.limit == 0 {
            return Err(MemoryStoreError::InvalidQuery("limit_must_be_positive"));
        }

        let conn = self.connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, content, metadata_json, created_at, expires_at FROM memories ORDER BY id ASC",
            )
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;

        let rows = stmt
            .query_map([], |row| {
                let metadata_json: String = row.get(2)?;
                let metadata: BTreeMap<String, String> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();
                Ok(MemoryRecord {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    metadata,
                    created_at: row.get(3)?,
                    expires_at: row.get(4)?,
                })
            })
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;

        let mut hits = Vec::new();
        for row in rows {
            let record = row.map_err(|_| MemoryStoreError::StorageUnavailable)?;
            let metadata_match = query
                .metadata
                .iter()
                .all(|(key, value)| record.metadata.get(key) == Some(value));

            let text_score = query
                .text
                .as_ref()
                .and_then(|text| text_match_score_in_mode(text, &record.content, query.match_mode))
                .unwrap_or(0);
            let text_match = query.text.is_none() || text_score > 0;

            if text_match && metadata_match {
                hits.push(SearchHit {
                    record,
                    score: text_score,
                });
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.record.id.cmp(&b.record.id))
        });
        hits.truncate(query.limit);
        Ok(hits)
    }

    fn delete(&mut self, id: &str) -> Result<(), MemoryStoreError> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;
        Ok(())
    }

    fn expire(&mut self, now: &str) -> Result<usize, MemoryStoreError> {
        let conn = self.connection()?;
        let removed = conn
            .execute(
                "DELETE FROM memories WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                params![now],
            )
            .map_err(|_| MemoryStoreError::StorageUnavailable)?;
        Ok(removed)
    }
}
