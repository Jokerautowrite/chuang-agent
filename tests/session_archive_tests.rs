use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::memory_store::{MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::session_archive::{SessionArchiveError, SqliteSessionArchive};
use rusqlite::{params, Connection};

fn temp_db_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "chuang-agent-session-archive-{name}-{}-{nonce}.db",
        std::process::id(),
    ));
    path
}

#[test]
fn append_records_raw_turn_with_real_timestamp_and_references() {
    let path = temp_db_path("append");
    let archive = SqliteSessionArchive::open(&path).expect("archive should open");

    let turn = archive
        .append(
            "session-a",
            "raw user input",
            "raw response",
            vec!["event://turn-a/1".to_string()],
            vec!["report://turn-a".to_string()],
            Some("summary://session-a/1".to_string()),
        )
        .expect("append should succeed");

    assert_eq!(turn.session_id, "session-a");
    assert_eq!(turn.sequence, 1);
    chrono::DateTime::parse_from_rfc3339(&turn.created_at).expect("created_at should be RFC3339");
    assert_eq!(turn.raw_user_input, "raw user input");
    assert_eq!(turn.raw_response, "raw response");
    assert_eq!(turn.runtime_event_refs, vec!["event://turn-a/1"]);
    assert_eq!(turn.runtime_report_refs, vec!["report://turn-a"]);
    assert_eq!(
        turn.searchable_summary_pointer.as_deref(),
        Some("summary://session-a/1")
    );
}

#[test]
fn replay_preserves_per_session_append_order_across_reopen() {
    let path = temp_db_path("reopen");
    {
        let archive = SqliteSessionArchive::open(&path).expect("archive should open");
        archive
            .append("session-a", "first", "response-1", vec![], vec![], None)
            .expect("first append should succeed");
        archive
            .append("session-b", "other", "response-b", vec![], vec![], None)
            .expect("other session append should succeed");
        archive
            .append(
                "session-a",
                "second",
                "response-2",
                vec!["event://2".to_string()],
                vec![],
                None,
            )
            .expect("second append should succeed");
    }

    let reopened = SqliteSessionArchive::open(&path).expect("archive should reopen");
    let replay = reopened.replay("session-a").expect("replay should succeed");

    assert_eq!(replay.len(), 2);
    assert_eq!(replay[0].sequence, 1);
    assert_eq!(replay[0].raw_user_input, "first");
    assert_eq!(replay[1].sequence, 2);
    assert_eq!(replay[1].raw_user_input, "second");
    assert_eq!(replay[1].runtime_event_refs, vec!["event://2"]);
}

#[test]
fn each_session_has_an_independent_ordered_sequence() {
    let path = temp_db_path("per-session-sequence");
    let archive = SqliteSessionArchive::open(&path).expect("archive should open");

    let first_a = archive
        .append("session-a", "a1", "r", vec![], vec![], None)
        .expect("append should succeed");
    let first_b = archive
        .append("session-b", "b1", "r", vec![], vec![], None)
        .expect("append should succeed");
    let second_a = archive
        .append("session-a", "a2", "r", vec![], vec![], None)
        .expect("append should succeed");

    assert_eq!(first_a.sequence, 1);
    assert_eq!(first_b.sequence, 1);
    assert_eq!(second_a.sequence, 2);
}

#[test]
fn invalid_input_returns_structured_error_without_raw_content() {
    let path = temp_db_path("invalid");
    let archive = SqliteSessionArchive::open(&path).expect("archive should open");
    let secret = "secret-user-input-value";

    let error = archive
        .append("", secret, "secret-response-value", vec![], vec![], None)
        .expect_err("empty session id should fail");

    assert_eq!(
        error,
        SessionArchiveError::InvalidInput {
            field: "session_id",
            code: "must_not_be_empty",
        }
    );
    let display = error.to_string();
    assert!(!display.contains(secret));
    assert!(!display.contains("secret-response-value"));
    assert!(!display.contains(path.to_string_lossy().as_ref()));
}

#[test]
fn replay_of_unknown_session_is_empty() {
    let path = temp_db_path("unknown");
    let archive = SqliteSessionArchive::open(&path).expect("archive should open");

    assert_eq!(
        archive
            .replay("unknown-session")
            .expect("replay should succeed"),
        Vec::new()
    );
}

#[test]
fn append_with_summary_rolls_back_both_writes_and_can_retry_once() {
    let path = temp_db_path("summary-transaction-retry");
    let memory_store =
        SqliteMemoryStore::open(&path).expect("memory store should initialize schema");
    let archive = SqliteSessionArchive::open(&path).expect("archive should open");
    let conn = Connection::open(&path).expect("test connection should open");
    conn.execute_batch(
        "
        CREATE TRIGGER fail_session_archive_insert
        BEFORE INSERT ON session_turn_archive
        BEGIN
            SELECT RAISE(FAIL, 'injected archive failure');
        END;
        ",
    )
    .expect("failure trigger should install");

    let summary = MemoryRecord {
        id: "summary-session-a-1".to_string(),
        content: "searchable turn summary".to_string(),
        metadata: BTreeMap::from([
            ("kind".to_string(), "turn_summary".to_string()),
            ("session_id".to_string(), "session-a".to_string()),
        ]),
        created_at: "2026-07-10T00:00:00Z".to_string(),
        expires_at: None,
    };

    let error = archive
        .append_with_summary(
            "session-a",
            "raw user input",
            "raw response",
            vec!["event://turn-a/1".to_string()],
            vec!["report://turn-a".to_string()],
            summary.clone(),
        )
        .expect_err("injected archive failure should roll back the transaction");
    assert_eq!(
        error,
        SessionArchiveError::StorageUnavailable {
            operation: "insert_turn",
        }
    );
    assert_eq!(
        memory_store
            .get(&summary.id)
            .expect("summary lookup should succeed"),
        None
    );
    assert!(archive
        .replay("session-a")
        .expect("replay after rollback should succeed")
        .is_empty());

    conn.execute_batch("DROP TRIGGER fail_session_archive_insert;")
        .expect("failure trigger should be removed");
    let turn = archive
        .append_with_summary(
            "session-a",
            "raw user input",
            "raw response",
            vec!["event://turn-a/1".to_string()],
            vec!["report://turn-a".to_string()],
            summary.clone(),
        )
        .expect("retry should commit summary and archive together");

    assert_eq!(turn.sequence, 1);
    assert_eq!(
        turn.searchable_summary_pointer.as_deref(),
        Some("memory://summary-session-a-1")
    );
    assert_eq!(
        memory_store
            .get(&summary.id)
            .expect("summary lookup should succeed"),
        Some(summary)
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE id = ?1",
            params!["summary-session-a-1"],
            |row| row.get::<_, i64>(0),
        )
        .expect("summary count should be readable"),
        1
    );
    assert_eq!(
        archive
            .replay("session-a")
            .expect("replay after retry should succeed")
            .len(),
        1
    );
}

#[test]
fn open_initializes_memories_table_for_append_with_summary_without_memory_store_bootstrap() {
    let path = temp_db_path("summary-open-inits-memories");
    let archive = SqliteSessionArchive::open(&path).expect("archive should open");

    let summary = MemoryRecord {
        id: "summary-session-b-1".to_string(),
        content: "summary content".to_string(),
        metadata: BTreeMap::from([
            ("kind".to_string(), "turn_summary".to_string()),
            ("session_id".to_string(), "session-b".to_string()),
        ]),
        created_at: "2026-07-10T00:00:00Z".to_string(),
        expires_at: None,
    };

    let turn = archive
        .append_with_summary(
            "session-b",
            "raw user input",
            "raw response",
            vec![],
            vec!["report://turn-b".to_string()],
            summary.clone(),
        )
        .expect("append_with_summary should initialize required tables");

    assert_eq!(turn.sequence, 1);
    let memory_store = SqliteMemoryStore::open(&path).expect("memory store should reopen");
    assert_eq!(
        memory_store
            .get(&summary.id)
            .expect("summary lookup should succeed"),
        Some(summary)
    );
}
