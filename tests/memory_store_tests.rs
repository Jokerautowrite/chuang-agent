use std::collections::BTreeMap;

use chuang_agent::memory_store::{
    classify_memory_layer, text_match_score, InMemoryMemoryStore, MemoryLayer, MemoryLayerBoundary,
    MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError,
};

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
        created_at: "2026-04-30T16:30:00Z".to_string(),
        expires_at: expires_at.map(|v| v.to_string()),
    }
}

#[test]
fn memory_store_put_get_roundtrip() {
    let mut store = InMemoryMemoryStore::new();
    let input = record(
        "mem-1",
        "长期记忆主线需要持久化存储",
        &[("kind", "plan")],
        None,
    );

    store.put(input.clone()).expect("put should succeed");
    let loaded = store.get("mem-1").expect("get should succeed");

    assert_eq!(loaded, Some(input));
}

#[test]
fn memory_store_search_content_and_metadata() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "长期记忆主线需要持久化存储",
            &[("kind", "plan")],
            None,
        ))
        .expect("first put should succeed");
    store
        .put(record(
            "mem-2",
            "BrowserWorker 现在不能抢主线",
            &[("kind", "warning")],
            None,
        ))
        .expect("second put should succeed");

    let query = MemoryQuery {
        text: Some("主线".to_string()),
        metadata: BTreeMap::from([(String::from("kind"), String::from("plan"))]),
        limit: 10,
    };

    let hits = store.search(&query).expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.id, "mem-1");
    assert!(hits[0].score > 0);
}

#[test]
fn text_match_score_prefers_exact_substring_and_tolerates_paraphrase() {
    let content = "用户提到心跳链路与子代理调度，需要在 9 点到 22 点之间每两小时主动联系一次";

    // 整句精确包含 → 命中且分数 = 查询长度
    let exact = text_match_score("心跳链路与子代理调度", content).expect("exact should hit");
    assert_eq!(exact, 10);

    // 自然语言改述（无整句命中）→ 靠 token 命中召回
    let paraphrased =
        text_match_score("上次聊的心跳和主动联系机制是什么", content).expect("token hit expected");
    assert!(paraphrased >= 1);

    // 无关查询 → 无命中
    assert!(text_match_score("量子物理和弦理论", content).is_none());
}

#[test]
fn memory_store_search_hits_partial_tokens_not_only_exact_sentence() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-partial",
            "2026-08-07 · 心跳链路验证：proactive_sent 投递成功，outbox 归档",
            &[],
            None,
        ))
        .expect("put should succeed");

    // 查询词不完整出现在内容中，但 token 能命中
    let hits = store
        .search(&MemoryQuery {
            text: Some("心跳投递链路是否通过".to_string()),
            metadata: BTreeMap::new(),
            limit: 5,
        })
        .expect("search should succeed");
    assert!(!hits.is_empty(), "partial token search should hit");
    assert_eq!(hits[0].record.id, "mem-partial");
}

#[test]
fn memory_store_delete_then_get_none() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "将来这里会换成 SQLite 持久化",
            &[("kind", "note")],
            None,
        ))
        .expect("put should succeed");

    store.delete("mem-1").expect("delete should succeed");
    let loaded = store.get("mem-1").expect("get should succeed");

    assert_eq!(loaded, None);
}

#[test]
fn memory_store_expire_removes_ttl_record() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "这条记忆会过期",
            &[("kind", "ttl")],
            Some("2026-04-30T16:40:00Z"),
        ))
        .expect("ttl put should succeed");
    store
        .put(record(
            "mem-2",
            "这条记忆保留",
            &[("kind", "ttl")],
            Some("2026-04-30T18:40:00Z"),
        ))
        .expect("future put should succeed");

    let removed = store
        .expire("2026-04-30T17:00:00Z")
        .expect("expire should succeed");

    assert_eq!(removed, 1);
    assert_eq!(store.get("mem-1").expect("get should succeed"), None);
    assert!(store.get("mem-2").expect("get should succeed").is_some());
}

#[test]
fn memory_store_rejects_zero_limit_query() {
    let store = InMemoryMemoryStore::new();
    let query = MemoryQuery {
        text: None,
        metadata: BTreeMap::new(),
        limit: 0,
    };

    let err = store.search(&query).expect_err("zero limit should fail");
    assert_eq!(
        err,
        MemoryStoreError::InvalidQuery("limit_must_be_positive")
    );
}

#[test]
fn memory_store_classifies_archive_and_decay_boundaries() {
    let archive = record(
        "turn-1",
        "历史会话归档只能被维护报告读取",
        &[("kind", "turn_summary")],
        None,
    );
    assert_eq!(classify_memory_layer(&archive), MemoryLayer::HistoryArchive);
    let archive_boundary = MemoryLayerBoundary::for_record(&archive);
    assert!(archive_boundary.archive_read_only);
    assert!(!archive_boundary.maintenance_writeback_allowed);
    assert_eq!(archive_boundary.writeback_target, "none");

    let hot_memory = record(
        "hot-1",
        "核心热记忆衰减只能走人工 review",
        &[("memory_layer", "internal_identity")],
        None,
    );
    let hot_boundary = MemoryLayerBoundary::for_record(&hot_memory);
    assert_eq!(hot_boundary.layer, MemoryLayer::InternalIdentity);
    assert!(hot_boundary.decay_review_only);
    assert!(!hot_boundary.maintenance_writeback_allowed);
    assert_eq!(hot_boundary.writeback_target, "manual_review_only");
}
