use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::hermes_memory::{
    DualFileMemoryConfig, DualFileMemoryError, DualFileMemoryScope, DualFileMemoryStore,
    FileDualFileMemoryStore, HotMemoryEntry, DEFAULT_HOT_MEMORY_MAX_CHARS,
    DEFAULT_USER_MEMORY_MAX_CHARS,
};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-{name}-{nanos}"))
}

#[test]
fn dual_file_memory_creates_user_and_memory_files() {
    let root = temp_root("creates-files");
    let store =
        FileDualFileMemoryStore::open(DualFileMemoryConfig::new(&root)).expect("open succeeds");

    assert_eq!(store.read_user().expect("user readable"), "");
    assert_eq!(store.read_memory().expect("memory readable"), "");
    assert_eq!(store.read_experiences().expect("experiences readable"), "");
    assert!(root.join("USER.md").exists());
    assert!(root.join("MEMORY.md").exists());
    assert!(root.join("experiences.md").exists());
}

#[test]
fn dual_file_memory_snapshot_freezes_user_and_memory_text() {
    let root = temp_root("snapshot");
    let mut store =
        FileDualFileMemoryStore::open(DualFileMemoryConfig::new(&root)).expect("open succeeds");

    store
        .write_user("老爸偏好中文简洁汇报")
        .expect("user write succeeds");
    store
        .append_memory(HotMemoryEntry {
            id: "mem-1".to_string(),
            content: "MVP 先做核心，不把飞书放主线".to_string(),
        })
        .expect("memory append succeeds");
    fs::write(root.join("experiences.md"), "命令失败先看 stderr").expect("experiences seed");

    let snapshot = store.snapshot().expect("snapshot succeeds");

    assert_eq!(snapshot.user, "老爸偏好中文简洁汇报");
    assert!(snapshot.memory.contains("## mem-1"));
    assert!(snapshot.memory.contains("MVP 先做核心"));
    assert_eq!(snapshot.experiences, "命令失败先看 stderr");
}

#[test]
fn dual_file_memory_can_append_provenanced_experience() {
    let root = temp_root("append-experience");
    let mut store =
        FileDualFileMemoryStore::open(DualFileMemoryConfig::new(&root)).expect("open succeeds");

    store
        .append_experience(HotMemoryEntry {
            id: "exp-1".to_string(),
            content: "source=runtime_turn\nturn_id=turn-1\nlesson=失败先看 stderr".to_string(),
        })
        .expect("experience append succeeds");

    let experiences = store.read_experiences().expect("experiences readable");
    assert!(experiences.contains("## exp-1"));
    assert!(experiences.contains("source=runtime_turn"));
    assert!(experiences.contains("lesson=失败先看 stderr"));
}

#[test]
fn dual_file_memory_rejects_user_text_over_hard_limit_without_mutation() {
    let root = temp_root("user-limit");
    let mut config = DualFileMemoryConfig::new(&root);
    config.user_max_chars = 5;
    let mut store = FileDualFileMemoryStore::open(config).expect("open succeeds");
    store.write_user("abc").expect("initial write succeeds");

    let err = store
        .write_user("abcdef")
        .expect_err("over-limit user write should fail");

    assert_eq!(
        err,
        DualFileMemoryError::HardLimitExceeded {
            scope: DualFileMemoryScope::User,
            limit_chars: 5,
            attempted_chars: 6,
            existing_entries: vec![chuang_agent::memory_admission::MemoryEntryView {
                id: "USER.md".to_string(),
                content_preview: "abc".to_string(),
                chars: 3,
            }],
        }
    );
    assert_eq!(store.read_user().expect("user readable"), "abc");
}

#[test]
fn dual_file_memory_rejects_memory_append_over_hard_limit_with_existing_entries() {
    let root = temp_root("memory-limit");
    let mut config = DualFileMemoryConfig::new(&root);
    config.memory_max_chars = 25;
    let mut store = FileDualFileMemoryStore::open(config).expect("open succeeds");
    store
        .append_memory(HotMemoryEntry {
            id: "mem-1".to_string(),
            content: "abc".to_string(),
        })
        .expect("first append succeeds");

    let before = fs::read_to_string(root.join("MEMORY.md")).expect("memory file readable");
    let err = store
        .append_memory(HotMemoryEntry {
            id: "mem-2".to_string(),
            content: "01234567890123456789".to_string(),
        })
        .expect_err("over-limit append should fail");

    match err {
        DualFileMemoryError::HardLimitExceeded {
            scope,
            limit_chars,
            attempted_chars,
            existing_entries,
        } => {
            assert_eq!(scope, DualFileMemoryScope::Memory);
            assert_eq!(limit_chars, 25);
            assert!(attempted_chars > 25);
            assert_eq!(existing_entries.len(), 1);
            assert_eq!(existing_entries[0].id, "mem-1");
            assert_eq!(existing_entries[0].content_preview, "abc");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(root.join("MEMORY.md")).expect("memory file readable"),
        before
    );
}

#[test]
fn dual_file_memory_rejects_memory_write_over_hard_limit_without_mutation() {
    let root = temp_root("memory-write-limit");
    let mut config = DualFileMemoryConfig::new(&root);
    config.memory_max_chars = 25;
    let mut store = FileDualFileMemoryStore::open(config).expect("open succeeds");
    store
        .write_memory("## seed-1\nabc\n")
        .expect("initial write succeeds");

    let before = fs::read_to_string(root.join("MEMORY.md")).expect("memory file readable");
    let err = store
        .write_memory("## over-limit\n01234567890123456789\n")
        .expect_err("over-limit write should fail");

    match err {
        DualFileMemoryError::HardLimitExceeded {
            scope,
            limit_chars,
            attempted_chars,
            existing_entries,
        } => {
            assert_eq!(scope, DualFileMemoryScope::Memory);
            assert_eq!(limit_chars, 25);
            assert!(attempted_chars > 25);
            assert_eq!(existing_entries.len(), 1);
            assert_eq!(existing_entries[0].id, "seed-1");
            assert_eq!(existing_entries[0].content_preview, "abc");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(root.join("MEMORY.md")).expect("memory file readable"),
        before
    );
}

#[test]
fn dual_file_memory_reports_freeform_preamble_when_append_exceeds_limit() {
    let root = temp_root("memory-preamble-limit");
    let mut config = DualFileMemoryConfig::new(&root);
    config.memory_max_chars = 20;
    let mut store = FileDualFileMemoryStore::open(config).expect("open succeeds");
    fs::write(root.join("MEMORY.md"), "手写热记忆").expect("preamble should be seeded");

    let err = store
        .append_memory(HotMemoryEntry {
            id: "mem-1".to_string(),
            content: "01234567890123456789".to_string(),
        })
        .expect_err("over-limit append should fail");

    match err {
        DualFileMemoryError::HardLimitExceeded {
            scope,
            existing_entries,
            ..
        } => {
            assert_eq!(scope, DualFileMemoryScope::Memory);
            assert_eq!(existing_entries.len(), 1);
            assert_eq!(existing_entries[0].id, "MEMORY.md:preamble");
            assert_eq!(existing_entries[0].content_preview, "手写热记忆");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(root.join("MEMORY.md")).expect("memory file readable"),
        "手写热记忆"
    );
}

#[test]
fn dual_file_memory_rejects_duplicate_memory_entry_id() {
    let root = temp_root("duplicate");
    let mut store =
        FileDualFileMemoryStore::open(DualFileMemoryConfig::new(&root)).expect("open succeeds");
    store
        .append_memory(HotMemoryEntry {
            id: "mem-1".to_string(),
            content: "第一条".to_string(),
        })
        .expect("first append succeeds");

    let err = store
        .append_memory(HotMemoryEntry {
            id: "mem-1".to_string(),
            content: "第二条".to_string(),
        })
        .expect_err("duplicate id should fail");

    assert_eq!(
        err,
        DualFileMemoryError::DuplicateEntry {
            id: "mem-1".to_string(),
        }
    );
}

#[test]
fn dual_file_memory_defaults_match_hermes_style_limits() {
    let config = DualFileMemoryConfig::new(temp_root("defaults"));

    assert_eq!(config.user_max_chars, DEFAULT_USER_MEMORY_MAX_CHARS);
    assert_eq!(config.memory_max_chars, DEFAULT_HOT_MEMORY_MAX_CHARS);
    assert_eq!(config.user_max_chars, 1375);
    assert_eq!(config.memory_max_chars, 2200);
}
