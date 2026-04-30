use std::collections::BTreeMap;

use chuang_agent::chuang_kernel::{
    ChuangKernel, ChuangKernelConfig, ChuangKernelMemoryError, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::context_engine::ContextBudget;
use chuang_agent::memory_store::{InMemoryMemoryStore, MemoryRecord, MemoryStore};
use chuang_agent::subagent_report::ExecutionStatus;

fn record(id: &str, content: &str, metadata: &[(&str, &str)], created_at: &str) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        content: content.to_string(),
        metadata: metadata
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        created_at: created_at.to_string(),
        expires_at: None,
    }
}

fn kernel_config() -> ChuangKernelConfig {
    ChuangKernelConfig {
        agent_id: "chuang-mvp".to_string(),
        parent_agent_id: None,
        recall_limit: 3,
        metadata: BTreeMap::new(),
        context_budget: None,
        memory_write_max_chars: Some(2200),
    }
}

#[test]
fn chuang_kernel_runs_minimal_auditable_turn() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "mem-1",
            "创项目 MVP 要先把记忆、上下文、响应、报告闭环跑通。",
            &[("scope", "mvp")],
            "2026-05-01T10:00:00Z",
        ))
        .expect("memory seed should succeed");
    let mut kernel = ChuangKernel::new(kernel_config(), store);

    let turn = kernel
        .run_turn("创项目 MVP 先跑通")
        .expect("kernel turn should run");

    assert_eq!(turn.turn_id, "turn-1");
    assert_eq!(turn.user_input, "创项目 MVP 先跑通");
    assert_eq!(turn.report.task_id.0, "turn-1");
    assert_eq!(turn.report.report_id.0, "report-turn-1");
    assert_eq!(turn.report.agent_id.0, "chuang-mvp");
    assert_eq!(turn.report.status, ExecutionStatus::Success);
    assert!(turn.result.prompt.contains("[chuang-agent-runtime]"));
    assert!(turn.result.packed_context_preview.contains("system-core"));
    assert!(turn.report.summary.contains("model=stub-responder"));
    assert_eq!(kernel.snapshot().turn_count, 1);
}

#[test]
fn chuang_kernel_can_write_turn_summary_memory_after_execution() {
    let mut kernel = ChuangKernel::new(kernel_config(), InMemoryMemoryStore::new());

    let turn = kernel
        .run_turn("记住这次 MVP 闭环")
        .expect("kernel turn should run");
    let record_id = kernel
        .remember_turn(&turn)
        .expect("turn memory should be written");

    assert_eq!(record_id, "turn-memory-turn-1");

    let second = kernel
        .run_turn("MVP")
        .expect("second turn should recall stored memory");

    assert!(second.result.recall_summary.contains("记住这次 MVP 闭环"));
    assert_eq!(second.result.recall_hit_count, 1);
}

#[test]
fn chuang_kernel_snapshot_exposes_mvp_health_fields() {
    let config = ChuangKernelConfig {
        agent_id: "chuang-mvp".to_string(),
        parent_agent_id: Some("parent-agent".to_string()),
        recall_limit: 2,
        metadata: BTreeMap::from([("scope".to_string(), "mvp".to_string())]),
        context_budget: Some(ContextBudget {
            max_tokens: 128,
            reserve_system_tokens: 16,
            min_working_tokens: 1,
            max_tool_results: 3,
            max_memory_segments: 5,
        }),
        memory_write_max_chars: Some(2200),
    };
    let kernel = ChuangKernel::new(config, InMemoryMemoryStore::new());

    let snapshot = kernel.snapshot();

    assert_eq!(snapshot.agent_id, "chuang-mvp");
    assert_eq!(snapshot.turn_count, 0);
    assert_eq!(snapshot.recall_limit, 2);
    assert_eq!(snapshot.metadata_keys, vec!["scope".to_string()]);
    assert_eq!(snapshot.context_budget_max_tokens, Some(128));
    assert_eq!(
        snapshot.memory_write_max_chars,
        Some(DEFAULT_MEMORY_WRITE_MAX_CHARS)
    );
}

#[test]
fn chuang_kernel_mvp_default_config_sets_memory_hard_limit() {
    let config = ChuangKernelConfig::mvp_default("chuang-default");

    assert_eq!(config.agent_id, "chuang-default");
    assert_eq!(config.recall_limit, 5);
    assert_eq!(
        config.memory_write_max_chars,
        Some(DEFAULT_MEMORY_WRITE_MAX_CHARS)
    );
}

#[test]
fn chuang_kernel_rejects_turn_memory_when_hard_limit_is_exceeded() {
    let mut store = InMemoryMemoryStore::new();
    store
        .put(record(
            "existing-turn",
            "旧的 turn summary",
            &[("kind", "turn_summary")],
            "2026-05-01T10:00:00Z",
        ))
        .expect("seed should succeed");
    let config = ChuangKernelConfig {
        memory_write_max_chars: Some(12),
        ..kernel_config()
    };
    let mut kernel = ChuangKernel::new(config, store);

    let turn = kernel
        .run_turn("这次写入应该超过硬上限")
        .expect("kernel turn should run");
    let err = kernel
        .remember_turn(&turn)
        .expect_err("oversized memory write should fail");

    match err {
        ChuangKernelMemoryError::HardLimitExceeded {
            limit_chars,
            attempted_chars,
            existing_entries,
        } => {
            assert_eq!(limit_chars, 12);
            assert!(attempted_chars > 12);
            assert_eq!(existing_entries.len(), 1);
            assert_eq!(existing_entries[0].id, "existing-turn");
            assert_eq!(existing_entries[0].content_preview, "旧的 turn summary");
            assert_eq!(
                existing_entries[0].chars,
                "旧的 turn summary".chars().count()
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let next = kernel
        .run_turn("硬上限")
        .expect("next turn should still run");
    assert_eq!(next.result.recall_hit_count, 0);
}

#[test]
fn chuang_kernel_rejects_invalid_runtime_request_without_incrementing_turn() {
    let config = ChuangKernelConfig {
        recall_limit: 0,
        ..kernel_config()
    };
    let mut kernel = ChuangKernel::new(config, InMemoryMemoryStore::new());

    let error = kernel
        .run_turn("这个请求应该失败")
        .expect_err("zero recall limit should fail");

    assert_eq!(
        format!("{:?}", error),
        "Recall(InvalidRequest(\"limit_must_be_positive\"))"
    );
    assert_eq!(kernel.snapshot().turn_count, 0);
}
