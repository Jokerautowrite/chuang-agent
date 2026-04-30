use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chuang_agent::memory_store::{MemoryQuery, MemoryRecord, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;

fn temp_db_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("chuang-agent-{name}-{}.db", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

fn record(
    id: &str,
    content: &str,
    metadata: &[(&str, &str)],
    expires_at: Option<&str>,
) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        content: content.to_string(),
        metadata: metadata
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        created_at: "2026-04-30T17:00:00Z".to_string(),
        expires_at: expires_at.map(|v| v.to_string()),
    }
}

#[test]
fn sqlite_memory_store_put_and_get() {
    let path = temp_db_path("put-get");
    let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
    let input = record(
        "mem-sqlite-1",
        "长期记忆应该跨重启保留",
        &[("kind", "plan")],
        None,
    );

    store.put(input.clone()).expect("put should succeed");
    let loaded = store.get("mem-sqlite-1").expect("get should succeed");

    assert_eq!(loaded, Some(input));
    let _ = fs::remove_file(path);
}

#[test]
fn sqlite_memory_store_persist_across_reopen() {
    let path = temp_db_path("reopen");
    {
        let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
        store
            .put(record(
                "mem-sqlite-1",
                "这条记忆写入后需要跨 reopen 保留",
                &[("kind", "memory")],
                None,
            ))
            .expect("put should succeed");
    }

    let reopened = SqliteMemoryStore::open(&path).expect("sqlite store should reopen");
    let loaded = reopened
        .get("mem-sqlite-1")
        .expect("get after reopen should succeed");

    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().content, "这条记忆写入后需要跨 reopen 保留");
    let _ = fs::remove_file(path);
}

#[test]
fn sqlite_memory_store_search_and_delete() {
    let path = temp_db_path("search-delete");
    let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
    store
        .put(record(
            "mem-sqlite-1",
            "长期记忆主线优先，BrowserWorker 不要抢主线",
            &[("kind", "warning")],
            None,
        ))
        .expect("first put should succeed");
    store
        .put(record(
            "mem-sqlite-2",
            "长期记忆下一步要补 SQLite 持久化",
            &[("kind", "plan")],
            None,
        ))
        .expect("second put should succeed");

    let query = MemoryQuery {
        text: Some("长期记忆".to_string()),
        metadata: BTreeMap::new(),
        limit: 10,
    };
    let hits = store.search(&query).expect("search should succeed");
    assert_eq!(hits.len(), 2);

    store.delete("mem-sqlite-1").expect("delete should succeed");
    assert!(store
        .get("mem-sqlite-1")
        .expect("get should succeed")
        .is_none());
    let _ = fs::remove_file(path);
}

#[test]
fn sqlite_memory_store_expire_removes_expired_records_only() {
    let path = temp_db_path("expire");
    let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
    store
        .put(record(
            "mem-expired",
            "这条短期记忆应该被回收",
            &[("kind", "ephemeral")],
            Some("2026-04-30T16:00:00Z"),
        ))
        .expect("expired put should succeed");
    store
        .put(record(
            "mem-fresh",
            "这条长期记忆还不能被删",
            &[("kind", "longterm")],
            Some("2026-04-30T18:00:00Z"),
        ))
        .expect("fresh put should succeed");
    store
        .put(record(
            "mem-no-expire",
            "没有 TTL 的记忆要保留",
            &[("kind", "longterm")],
            None,
        ))
        .expect("non expiring put should succeed");

    let removed = store
        .expire("2026-04-30T17:00:00Z")
        .expect("expire should succeed");

    assert_eq!(removed, 1);
    assert!(store
        .get("mem-expired")
        .expect("get should succeed")
        .is_none());
    assert!(store
        .get("mem-fresh")
        .expect("get should succeed")
        .is_some());
    assert!(store
        .get("mem-no-expire")
        .expect("get should succeed")
        .is_some());
    let _ = fs::remove_file(path);
}

#[test]
fn sqlite_memory_store_search_excludes_expired_records_after_expire() {
    let path = temp_db_path("search-after-expire");
    let mut store = SqliteMemoryStore::open(&path).expect("sqlite store should open");
    store
        .put(record(
            "mem-expired",
            "长期记忆候选：已过期",
            &[("kind", "candidate")],
            Some("2026-04-30T16:59:59Z"),
        ))
        .expect("expired put should succeed");
    store
        .put(record(
            "mem-fresh",
            "长期记忆候选：仍有效",
            &[("kind", "candidate")],
            Some("2026-04-30T18:00:00Z"),
        ))
        .expect("fresh put should succeed");

    store
        .expire("2026-04-30T17:00:00Z")
        .expect("expire should succeed");

    let hits = store
        .search(&MemoryQuery {
            text: Some("长期记忆候选".to_string()),
            metadata: BTreeMap::new(),
            limit: 10,
        })
        .expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.id, "mem-fresh");
    let _ = fs::remove_file(path);
}
