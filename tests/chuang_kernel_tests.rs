use std::collections::BTreeMap;

use chuang_agent::chuang_kernel::{
    ChuangKernel, ChuangKernelConfig, ChuangKernelGovernanceError, ChuangKernelMemoryError,
    IdentityBootstrapSnapshot, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::context_engine::{
    ContextBudget, ContextEngineKind, ContextSegment, SegmentSource,
};
use chuang_agent::governance::{
    ActionKind, Governance, GovernanceError, ProposedAction, RiskDecision, StaticRuleGovernance,
};
use chuang_agent::hermes_memory::DualFileMemorySnapshot;
use chuang_agent::identity_registry::AgentIdentity;
use chuang_agent::memory_store::{InMemoryMemoryStore, MemoryRecord, MemoryStore};
use chuang_agent::responder::FakeResponder;
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
        context_engine_kind: None,
        memory_write_max_chars: Some(2200),
        identity_snapshot: None,
        identity_bootstrap_snapshot: None,
        governance_rules: None,
    }
}

fn kernel<S>(config: ChuangKernelConfig, store: S) -> ChuangKernel<S, FakeResponder> {
    ChuangKernel::with_responder(config, store, FakeResponder::new("stub-responder"))
}

fn run_turn<S: MemoryStore>(
    kernel: &mut ChuangKernel<S, FakeResponder>,
    user_input: impl Into<String>,
) -> chuang_agent::chuang_kernel::ChuangKernelTurn {
    let mut governance = StaticRuleGovernance::new();
    kernel
        .run_governed_turn(user_input, &mut governance)
        .expect("governed kernel turn should run")
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
    let mut kernel = kernel(kernel_config(), store);

    let turn = run_turn(&mut kernel, "创项目 MVP 先跑通");

    assert_eq!(turn.turn_id, "turn-1");
    assert_eq!(turn.user_input, "创项目 MVP 先跑通");
    assert_eq!(turn.report.task_id.0, "turn-1");
    assert_eq!(turn.report.report_id.0, "report-turn-1");
    assert_eq!(turn.report.agent_id.0, "chuang-mvp");
    assert_eq!(turn.report.status, ExecutionStatus::Success);
    assert!(matches!(
        turn.governance_decision,
        Some(RiskDecision::Allowed { .. })
    ));
    assert!(turn.report.governance_decision.is_some());
    assert!(turn.result.prompt.contains("[chuang-agent-runtime]"));
    assert!(turn.result.packed_context_preview.contains("system-core"));
    assert!(turn
        .result
        .packed_context_preview
        .contains("system-capabilities"));
    assert!(turn.report.summary.contains("model=stub-responder"));
    assert_eq!(kernel.snapshot().turn_count, 1);
}

#[test]
fn chuang_kernel_can_run_turn_through_governance_and_audit() {
    let mut kernel = kernel(kernel_config(), InMemoryMemoryStore::new());
    let mut governance = StaticRuleGovernance::new();

    let turn = kernel
        .run_governed_turn("通过治理层跑一轮", &mut governance)
        .expect("governed turn should run");

    assert_eq!(turn.turn_id, "turn-1");
    assert!(matches!(
        turn.governance_decision,
        Some(RiskDecision::Allowed { .. })
    ));
    assert_eq!(kernel.snapshot().turn_count, 1);

    let audit = governance.audit_records();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].operation, "run_governed_turn");
    assert_eq!(audit[0].agent_id.0, "chuang-mvp");
    assert_eq!(audit[0].task_id.0, "turn-1");
    assert!(audit[0].delta_bytes > 0);
    assert!(audit[0].reason.starts_with("allowed:"));
}

#[test]
fn public_turn_entry_with_extra_context_always_returns_governance_decision() {
    let mut kernel = kernel(kernel_config(), InMemoryMemoryStore::new());
    let mut governance = StaticRuleGovernance::new();
    let extra_context = ContextSegment {
        id: "test-extra-context".to_string(),
        source: SegmentSource::Working,
        content: "public governed entry regression".to_string(),
        tokens: Some(4),
        priority: 100,
        created_at: chrono::Utc::now(),
        last_accessed: chrono::Utc::now(),
        metadata: Default::default(),
    };

    let turn = kernel
        .run_governed_turn_with_extra_context(
            "通过带上下文的公开入口跑一轮",
            &mut governance,
            vec![extra_context],
        )
        .expect("public governed turn entry should run");

    assert!(matches!(
        turn.governance_decision,
        Some(RiskDecision::Allowed { .. })
    ));
    assert!(turn.report.governance_decision.is_some());
}

#[derive(Debug, Clone)]
struct BlockingGovernance;

impl Governance for BlockingGovernance {
    fn classify(&self, action: &ProposedAction) -> Result<RiskDecision, GovernanceError> {
        assert_eq!(action.kind, ActionKind::Draft);
        assert_eq!(action.action_id, "run-turn-1");
        Ok(RiskDecision::Blocked {
            reason: "test block before runtime".to_string(),
        })
    }

    fn audit(&mut self, _record: chuang_agent::common::AuditRecord) -> Result<(), GovernanceError> {
        panic!("blocked turn should not be audited as executed");
    }
}

#[test]
fn chuang_kernel_blocks_governed_turn_before_runtime() {
    let mut kernel = kernel(kernel_config(), InMemoryMemoryStore::new());
    let mut governance = BlockingGovernance;

    let err = kernel
        .run_governed_turn("这轮不应该执行", &mut governance)
        .expect_err("blocked governance decision should stop runtime");

    assert!(matches!(
        err,
        ChuangKernelGovernanceError::NotAllowed {
            decision: RiskDecision::Blocked { .. }
        }
    ));
    assert_eq!(kernel.snapshot().turn_count, 0);
}

#[test]
fn chuang_kernel_can_write_turn_summary_memory_after_execution() {
    let mut kernel = kernel(kernel_config(), InMemoryMemoryStore::new());

    let turn = run_turn(&mut kernel, "记住这次 MVP 闭环");
    let record_id = kernel
        .remember_turn(&turn)
        .expect("turn memory should be written");

    assert!(record_id.starts_with("turn-memory-turn-1-"));

    let second = run_turn(&mut kernel, "MVP");

    assert!(second.result.recall_summary.contains("记住这次 MVP 闭环"));
    assert_eq!(second.result.recall_hit_count, 1);
}

#[test]
fn chuang_kernel_passes_context_engine_choice_to_runtime() {
    let config = ChuangKernelConfig {
        context_engine_kind: Some(ContextEngineKind::SummaryCompression),
        ..kernel_config()
    };
    let mut kernel = kernel(config, InMemoryMemoryStore::new());

    let turn = run_turn(&mut kernel, "内核切换上下文引擎");

    assert_eq!(turn.result.context_engine_kind, "summary_compression");
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
        context_engine_kind: None,
        memory_write_max_chars: Some(2200),
        identity_snapshot: None,
        identity_bootstrap_snapshot: None,
        governance_rules: None,
    };
    let kernel = kernel(config, InMemoryMemoryStore::new());

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
fn chuang_kernel_injects_identity_snapshot_into_runtime_context() {
    let config = ChuangKernelConfig {
        identity_snapshot: Some(DualFileMemorySnapshot {
            user: "老爸偏好简洁中文状态汇报".to_string(),
            memory: "## mem-1\n创项目 MVP 当前聚焦核心记忆层".to_string(),
            experiences: "踩坑经验暂不注入 runtime prompt".to_string(),
        }),
        ..kernel_config()
    };
    let mut kernel = kernel(config, InMemoryMemoryStore::new());

    let turn = run_turn(&mut kernel, "现在读取身份快照");
    let snapshot = kernel.snapshot();

    assert!(turn.result.prompt.contains("identity-user"));
    assert!(turn.result.prompt.contains("老爸偏好简洁中文状态汇报"));
    assert!(turn.result.prompt.contains("identity-memory"));
    assert_eq!(
        snapshot.identity_user_chars,
        Some("老爸偏好简洁中文状态汇报".chars().count())
    );
    assert_eq!(
        snapshot.identity_memory_chars,
        Some("## mem-1\n创项目 MVP 当前聚焦核心记忆层".chars().count())
    );
}

#[test]
fn chuang_kernel_injects_identity_bootstrap_snapshot_into_runtime_context() {
    let config = ChuangKernelConfig {
        identity_bootstrap_snapshot: Some(IdentityBootstrapSnapshot {
            soul: "创的核心锚点：记忆是本体，runtime 是壳。".to_string(),
            soul_exists: true,
            story: "创从小创、小承、OpenClaw 和 Codex 的经验里诞生。".to_string(),
            story_exists: true,
            first_wake: "第一次醒来先确认身份、边界和老爸的禁令。".to_string(),
            first_wake_exists: true,
            agents_registry:
                "agent_id = \"chuang\"\nagent_id = \"xiaochuang\"\nsecret = \"must-not-leak\""
                    .to_string(),
            agents_registry_exists: true,
            active_identity: Some(AgentIdentity {
                agent_id: "chuang".to_string(),
                display_name: "创".to_string(),
                shell_kind: "codex-rust".to_string(),
                role: "local-agent-os-kernel".to_string(),
                memory_body_id: "chuang-local-body".to_string(),
                lineage: vec!["xiaochuang".to_string()],
                allowed_channels: vec!["cli".to_string()],
                active: true,
            }),
        }),
        ..kernel_config()
    };
    let mut kernel = kernel(config, InMemoryMemoryStore::new());

    let turn = run_turn(&mut kernel, "读取 first wake");
    let snapshot = kernel.snapshot();

    assert!(turn.result.prompt.contains("identity-first-wake"));
    assert!(turn.result.prompt.contains("第一次醒来先确认身份"));
    assert!(turn.result.prompt.contains("identity-soul"));
    assert!(turn.result.prompt.contains("identity-active-agent"));
    assert!(turn.result.prompt.contains("\"agent_id\":\"chuang\""));
    assert!(!turn.result.prompt.contains("\"agent_id\":\"xiaochuang\""));
    assert!(!turn.result.prompt.contains("must-not-leak"));
    assert_eq!(
        snapshot.identity_first_wake_chars,
        Some("第一次醒来先确认身份、边界和老爸的禁令。".chars().count())
    );
    assert_eq!(
        snapshot.identity_soul_chars,
        Some("创的核心锚点：记忆是本体，runtime 是壳。".chars().count())
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
    let mut kernel = kernel(config, store);

    let turn = run_turn(&mut kernel, "这次写入应该超过硬上限");
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

    let next = run_turn(&mut kernel, "硬上限");
    assert_eq!(next.result.recall_hit_count, 0);
}

#[test]
fn chuang_kernel_compacts_session_turn_memory_when_hard_limit_is_exceeded() {
    let config = ChuangKernelConfig {
        memory_write_max_chars: Some(500),
        ..kernel_config()
    };
    let mut kernel = kernel(config, InMemoryMemoryStore::new());

    let turn = run_turn(&mut kernel, "会话记忆需要压缩".repeat(120));
    let receipt = kernel
        .remember_session_turn(&turn, "alpha")
        .expect("session memory should compact and write");

    assert!(receipt
        .record_id
        .starts_with("turn-memory-session-alpha-turn-1-"));
    assert!(receipt.compacted);
    assert!(receipt.attempted_chars > 500);
    assert!(receipt.stored_chars <= 500);
}

#[test]
fn chuang_kernel_compaction_strips_image_payloads_before_truncation() {
    // 压缩入口先 strip images：user_input 携带 base64 图片时，压缩摘要不残留图片内容。
    let config = ChuangKernelConfig {
        memory_write_max_chars: Some(500),
        ..kernel_config()
    };
    let mut kernel = kernel(config, InMemoryMemoryStore::new());

    let image = format!("data:image/png;base64,{}", "E".repeat(400));
    let turn = run_turn(&mut kernel, &format!("看图 {image} 继续说"));
    let prepared = kernel
        .prepare_session_turn_memory(&turn, "img")
        .expect("session memory should compact and prepare");
    assert!(prepared.receipt.compacted, "image-heavy turn should need compaction");

    let stored = &prepared.record;
    assert!(
        !stored.content.contains("base64"),
        "compacted turn summary must not contain base64 image payload"
    );
    assert!(
        stored.content.contains("data:image/png;base64") == false,
        "image data URL should be stripped in compacted summary"
    );
    assert_eq!(
        stored.metadata.get("compaction_source").map(String::as_str),
        Some("true"),
        "turn summary memory must be marked as compaction source for recursion guard"
    );
    assert_eq!(
        stored.metadata.get("kind").map(String::as_str),
        Some("turn_summary")
    );
}

#[test]
fn chuang_kernel_rejects_invalid_runtime_request_without_incrementing_turn() {
    let config = ChuangKernelConfig {
        recall_limit: 0,
        ..kernel_config()
    };
    let mut kernel = kernel(config, InMemoryMemoryStore::new());

    let mut governance = StaticRuleGovernance::new();
    let error = kernel
        .run_governed_turn("这个请求应该失败", &mut governance)
        .expect_err("zero recall limit should fail");

    assert_eq!(
        format!("{:?}", error),
        "Runtime(Recall(InvalidRequest(\"limit_must_be_positive\")))"
    );
    assert_eq!(kernel.snapshot().turn_count, 0);
}
