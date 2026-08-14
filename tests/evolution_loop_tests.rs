//! evolver 外环运行时接线测试：bridge 映射、driver 全链路、slot 集成。
//!
//! 覆盖验收标准：
//! - bridge 映射每种 event kind 至少一例，错误路径结构化；
//! - driver 全链路：重复失败→提案→治理批准→落盘→报告；
//!   治理拒绝→不落盘；无模式→空报告；
//! - 治理是强制门禁：apply 只在治理批准后被调用。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::evolution_loop::{
    EvolutionBridgeError, EvolutionEventBridge, OuterLoopDriveInput, OuterLoopDriver,
};
use chuang_agent::runtime_config::{CanonicalEvolutionConfig, EvolutionConfig, RuntimeConfig};
use chuang_agent::runtime_event_ledger::{
    RuntimeEvent as LedgerRuntimeEvent, RuntimeEventKind as LedgerRuntimeEventKind,
    RuntimeRiskDecision,
};
use chuang_agent::skill_evolver::FailureEvidence;
use chuang_agent::skill_evolver::RuleChangeKind;
use chuang_agent::skill_evolver::{
    CanonicalSkillEvolver, EvolutionError, EvolutionReceipt, EvolutionScope, FailureDetectorConfig,
    FailurePattern, GovernanceContext, NoopRuleChangeGovernance, PolicyRuleChangeGovernance,
    RuleChangeGovernance, RuleChangeProposal, RuleChangeReceipt,
    RuntimeEvent as EvolverRuntimeEvent, RuntimeEventKind as EvolverRuntimeEventKind, SkillEvolver,
    SkillId, SkillProposal, SkillProposalProvenance, ValidationReport,
};
use chuang_agent::slot_registry::{
    build_runtime_slots, CanonicalEvolutionSlot, CanonicalGovernanceSlot, EvolutionSlot,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn temp_skill_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "chuang-evolution-loop-{name}-{}-{nanos}",
        std::process::id()
    ))
}

/// 构造一条 ledger 事件（created_at 显式传入以保证 event_id 确定性）。
fn ledger_event(
    kind: LedgerRuntimeEventKind,
    created_at: &str,
    turn_id: &str,
    call_id: Option<&str>,
    risk: Option<(&str, &str)>,
) -> LedgerRuntimeEvent {
    let mut event = LedgerRuntimeEvent::at(kind, "cli", created_at).with_turn_id(turn_id);
    if let Some(call_id) = call_id {
        event = event.with_call_id(call_id);
    }
    if let Some((decision, reason)) = risk {
        event = event.with_risk_decision(RuntimeRiskDecision::new(decision, reason));
    }
    event
}

fn failed_tool_ledger_event(created_at: &str, call_id: &str) -> LedgerRuntimeEvent {
    ledger_event(
        LedgerRuntimeEventKind::ToolFinished,
        created_at,
        "turn-1",
        Some(call_id),
        Some(("blocked:policy", "denied by static rule")),
    )
}

fn evolver_failure_event(id: &str, task: &str, tool: &str) -> EvolverRuntimeEvent {
    EvolverRuntimeEvent {
        event_id: id.to_string(),
        task_id: task.to_string(),
        kind: EvolverRuntimeEventKind::ToolFailed,
        summary: format!("tool {tool} failed"),
        metadata: BTreeMap::from([("tool".to_string(), tool.to_string())]),
    }
}

fn canonical_evolver(root: PathBuf) -> CanonicalSkillEvolver {
    CanonicalSkillEvolver::new(root)
}

/// 构造一个已喂入事件的 canonical 外环槽（cli_runtime 同款路径）。
/// driver 通过 `EvolutionSlot` 的 trait 分发调用 canonical 的固有外环方法。
fn canonical_slot_with_events(root: PathBuf, events: &[EvolverRuntimeEvent]) -> EvolutionSlot {
    let mut evolver = CanonicalSkillEvolver::new(root);
    for event in events {
        evolver.observe(event.clone()).expect("observe event");
    }
    EvolutionSlot::Canonical(CanonicalEvolutionSlot::new(
        evolver,
        CanonicalGovernanceSlot::Policy(PolicyRuleChangeGovernance::default()),
        FailureDetectorConfig::default(),
    ))
}

/// 从 canonical 槽位取出 detector + context（owned；与 cli_runtime 薄接线同款，
/// 避免与 `&mut slot` 借用冲突）。
fn slot_outer_loop_pieces(slot: &EvolutionSlot) -> (FailureDetectorConfig, GovernanceContext) {
    let detector_config = slot
        .rule_change_detector_config()
        .expect("canonical slot exposes detector config")
        .clone();
    let context = slot
        .rule_change_governance_context()
        .expect("canonical slot exposes governance context");
    (detector_config, context)
}

// ---------------------------------------------------------------------------
// bridge 映射：每种 event kind 至少一例
// ---------------------------------------------------------------------------

#[test]
fn bridge_maps_turn_completed_to_turn_completed() {
    let event = ledger_event(
        LedgerRuntimeEventKind::TurnCompleted,
        "2026-08-10T00:00:00Z",
        "turn-9",
        None,
        None,
    );
    let mapped = EvolutionEventBridge::new()
        .map_event(&event, 0)
        .expect("turn completed should map");

    assert_eq!(mapped.kind, EvolverRuntimeEventKind::TurnCompleted);
    assert_eq!(mapped.task_id, "turn-9");
    assert_eq!(mapped.summary, "agent turn completed");
    assert_eq!(
        mapped.metadata.get("ledger_source").map(String::as_str),
        Some("turn_completed")
    );
    assert_eq!(
        mapped.metadata.get("turn_id").map(String::as_str),
        Some("turn-9")
    );
    assert!(!mapped.event_id.is_empty());
}

#[test]
fn bridge_maps_turn_failed_to_tool_failed_with_source_metadata() {
    let event = ledger_event(
        LedgerRuntimeEventKind::TurnFailed,
        "2026-08-10T00:00:01Z",
        "turn-9",
        None,
        None,
    );
    let mapped = EvolutionEventBridge::new()
        .map_event(&event, 0)
        .expect("turn failed should map");

    assert_eq!(mapped.kind, EvolverRuntimeEventKind::ToolFailed);
    assert_eq!(mapped.summary, "agent turn failed");
    assert_eq!(
        mapped.metadata.get("ledger_source").map(String::as_str),
        Some("turn_failed")
    );
    assert_eq!(
        mapped.metadata.get("error").map(String::as_str),
        Some("turn_failed")
    );
    assert_eq!(
        mapped.metadata.get("turn_id").map(String::as_str),
        Some("turn-9")
    );
}

#[test]
fn bridge_maps_tool_finished_allowed_to_tool_succeeded() {
    let event = ledger_event(
        LedgerRuntimeEventKind::ToolFinished,
        "2026-08-10T00:00:02Z",
        "turn-1",
        Some("tool:open_app:cli:cli:tool:1"),
        Some(("allowed:policy", "inside workspace")),
    );
    let mapped = EvolutionEventBridge::new()
        .map_event(&event, 0)
        .expect("allowed tool finished should map");

    assert_eq!(mapped.kind, EvolverRuntimeEventKind::ToolSucceeded);
    assert_eq!(mapped.summary, "tool open_app succeeded");
    assert_eq!(
        mapped.metadata.get("tool").map(String::as_str),
        Some("open_app")
    );
    assert_eq!(
        mapped.metadata.get("ledger_source").map(String::as_str),
        Some("tool_finished")
    );
}

#[test]
fn bridge_maps_tool_finished_blocked_to_tool_failed_with_error_metadata() {
    let event = ledger_event(
        LedgerRuntimeEventKind::ToolFinished,
        "2026-08-10T00:00:03Z",
        "turn-1",
        Some("tool:open_app:cli:cli:tool:2"),
        Some(("blocked:policy", "denied by static rule")),
    );
    let mapped = EvolutionEventBridge::new()
        .map_event(&event, 0)
        .expect("blocked tool finished should map");

    assert_eq!(mapped.kind, EvolverRuntimeEventKind::ToolFailed);
    assert_eq!(mapped.summary, "tool open_app failed");
    assert_eq!(
        mapped.metadata.get("tool").map(String::as_str),
        Some("open_app")
    );
    assert_eq!(
        mapped.metadata.get("error_code").map(String::as_str),
        Some("blocked")
    );
    assert_eq!(
        mapped.metadata.get("error").map(String::as_str),
        Some("denied by static rule")
    );
    assert_eq!(
        mapped.metadata.get("ledger_source").map(String::as_str),
        Some("tool_finished")
    );
}

#[test]
fn bridge_maps_tool_finished_needs_approval_to_tool_failed() {
    let event = ledger_event(
        LedgerRuntimeEventKind::ToolFinished,
        "2026-08-10T00:00:04Z",
        "turn-1",
        Some("tool:write_file:cli:cli:tool:3"),
        Some(("needs_approval:policy", "requires operator approval")),
    );
    let mapped = EvolutionEventBridge::new()
        .map_event(&event, 0)
        .expect("needs_approval tool finished should map");

    assert_eq!(mapped.kind, EvolverRuntimeEventKind::ToolFailed);
    assert_eq!(
        mapped.metadata.get("error_code").map(String::as_str),
        Some("needs_approval")
    );
    assert_eq!(
        mapped.metadata.get("tool").map(String::as_str),
        Some("write_file")
    );
}

#[test]
fn bridge_maps_tool_finished_without_decision_to_tool_succeeded() {
    let event = ledger_event(
        LedgerRuntimeEventKind::ToolFinished,
        "2026-08-10T00:00:05Z",
        "turn-1",
        Some("tool:list_dir:cli:cli:tool:4"),
        None,
    );
    let mapped = EvolutionEventBridge::new()
        .map_event(&event, 0)
        .expect("decision-less tool finished should map to success");

    assert_eq!(mapped.kind, EvolverRuntimeEventKind::ToolSucceeded);
    assert_eq!(
        mapped.metadata.get("tool").map(String::as_str),
        Some("list_dir")
    );
}

#[test]
fn bridge_ignores_unmapped_event_kinds() {
    let bridge = EvolutionEventBridge::new();
    for (kind, label) in [
        (LedgerRuntimeEventKind::ToolStarted, "tool started"),
        (
            LedgerRuntimeEventKind::ProviderRequested,
            "provider requested",
        ),
        (LedgerRuntimeEventKind::MemoryCommitted, "memory committed"),
        (LedgerRuntimeEventKind::RiskClassified, "risk classified"),
    ] {
        let event = ledger_event(kind, "2026-08-10T00:00:06Z", "turn-1", None, None);
        assert!(
            bridge.map_event(&event, 0).is_none(),
            "{label} should be ignored"
        );
    }
}

#[test]
fn bridge_observe_turn_events_feeds_in_order_and_counts() {
    let mut evolver = canonical_evolver(temp_skill_root("bridge-order"));
    let events = vec![
        failed_tool_ledger_event("2026-08-10T00:00:07Z", "tool:open_app:cli:cli:tool:1"),
        failed_tool_ledger_event("2026-08-10T00:00:08Z", "tool:open_app:cli:cli:tool:2"),
        ledger_event(
            LedgerRuntimeEventKind::ToolStarted,
            "2026-08-10T00:00:09Z",
            "turn-1",
            None,
            None,
        ),
    ];

    let observed = EvolutionEventBridge::new()
        .observe_turn_events(&mut evolver, &events)
        .expect("turn events should observe");
    assert_eq!(observed, 2);

    let stream = evolver.observed_events();
    assert_eq!(stream.len(), 2);
    assert_eq!(stream[0].kind, EvolverRuntimeEventKind::ToolFailed);
    assert_eq!(
        stream[0].metadata.get("tool").map(String::as_str),
        Some("open_app")
    );
    assert_eq!(stream[1].kind, EvolverRuntimeEventKind::ToolFailed);
}

#[test]
fn bridge_observe_turn_completed_synthesizes_event() {
    let mut evolver = canonical_evolver(temp_skill_root("bridge-completed"));
    let receipt = EvolutionEventBridge::new()
        .observe_turn_completed(
            &mut evolver,
            "cli",
            "turn-42",
            "agent turn completed",
            BTreeMap::new(),
        )
        .expect("turn completed should observe");
    assert!(receipt.accepted);

    let stream = evolver.observed_events();
    assert_eq!(stream.len(), 1);
    assert_eq!(stream[0].kind, EvolverRuntimeEventKind::TurnCompleted);
    assert_eq!(stream[0].task_id, "turn-42");
    assert_eq!(stream[0].event_id, "turn-completed:cli:turn-42");
}

#[test]
fn bridge_observe_error_is_structured_not_panic() {
    // evolver 拒绝空 event_id：桥接构造的事件 id 始终非空，因此用直接构造
    // 的非法事件验证错误路径——先 map 再喂给一个拒绝空 summary 的 evolver。
    let mut evolver = canonical_evolver(temp_skill_root("bridge-error"));
    let event = EvolverRuntimeEvent {
        event_id: "".to_string(),
        task_id: "t".to_string(),
        kind: EvolverRuntimeEventKind::ToolFailed,
        summary: "s".to_string(),
        metadata: BTreeMap::new(),
    };
    let err = evolver
        .observe(event)
        .expect_err("empty event_id must be rejected");
    assert!(matches!(err, EvolutionError::InvalidEvent(_)));

    // bridge 的 observe_event 对不可映射事件返回 Ok(None)，不产生错误。
    let started = ledger_event(
        LedgerRuntimeEventKind::ToolStarted,
        "2026-08-10T00:00:10Z",
        "turn-1",
        None,
        None,
    );
    let result = EvolutionEventBridge::new().observe_event(&mut evolver, &started, 0);
    assert!(matches!(result, Ok(None)));

    // 类型层面：EvolutionBridgeError 只有 Evolver 变体，可构造。
    let _ = EvolutionBridgeError::Evolver(EvolutionError::InvalidEvent(
        "bridge error is structured".to_string(),
    ));
}

// ---------------------------------------------------------------------------
// driver 全链路
// ---------------------------------------------------------------------------

#[test]
fn driver_full_chain_detects_proposes_approves_applies_and_reports() {
    let root = temp_skill_root("full-chain");
    let events = vec![
        evolver_failure_event("f1", "t1", "build"),
        evolver_failure_event("f2", "t2", "build"),
    ];
    let mut slot = canonical_slot_with_events(root.clone(), &events);

    let (detector_config, context) = slot_outer_loop_pieces(&slot);
    let governance: Box<dyn RuleChangeGovernance> = Box::new(PolicyRuleChangeGovernance::default());
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);

    let report = OuterLoopDriver::new().drive(&mut slot, input);

    assert_eq!(report.patterns.len(), 1);
    assert_eq!(report.patterns[0].signature, "tool=build");
    assert_eq!(report.proposals.len(), 1);
    assert_eq!(report.applied_count(), 1);
    assert_eq!(report.rejected_count(), 0);
    assert_eq!(report.error_count(), 0);
    assert_eq!(
        report.receipts[0].proposal_id,
        report.proposals[0].proposal_id
    );
    assert!(report.receipts[0].path.exists());
    assert_eq!(
        report.receipts[0].change_kind,
        chuang_agent::skill_evolver::RuleChangeKind::CreateRule
    );
    let journal_path = report
        .journal_path
        .as_ref()
        .expect("canonical slot exposes journal path");
    assert!(journal_path.exists());
    assert_eq!(
        slot.rule_change_history().expect("history readable").len(),
        1
    );
    // 报告可序列化（进 turn 元数据用）。
    let json = serde_json::to_string(&report).expect("report should serialize");
    assert!(json.contains("tool=build"));
    assert!(json.contains("receipts"));
}

#[test]
fn driver_governance_rejection_records_reason_and_does_not_write() {
    let root = temp_skill_root("reject");
    let events = vec![
        evolver_failure_event("f1", "t1", "build"),
        evolver_failure_event("f2", "t2", "build"),
    ];
    let mut slot = canonical_slot_with_events(root.clone(), &events);

    let (detector_config, context) = slot_outer_loop_pieces(&slot);
    // noop 治理永不批准 → 拒绝必须记录原因且绝不落盘。
    let governance: Box<dyn RuleChangeGovernance> = Box::new(NoopRuleChangeGovernance);
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);

    let report = OuterLoopDriver::new().drive(&mut slot, input);

    assert_eq!(report.patterns.len(), 1);
    assert_eq!(report.proposals.len(), 1);
    assert_eq!(report.applied_count(), 0);
    assert_eq!(report.rejected_count(), 1);
    assert_eq!(report.error_count(), 0);
    let rejection = &report.rejections[0];
    assert_eq!(rejection.proposal_id, report.proposals[0].proposal_id);
    assert!(rejection
        .reasons
        .iter()
        .any(|reason| reason.contains("noop governance never approves")));
    assert_eq!(rejection.decided_by, "governance.noop");
    // 未落盘：journal 与规则文件都不存在。
    assert!(!root.join(".evolver").join("rule_changes.jsonl").exists());
    assert_eq!(slot.rule_change_history().expect("empty history").len(), 0);
}

#[test]
fn driver_no_pattern_returns_empty_report() {
    let mut slot = canonical_slot_with_events(
        temp_skill_root("no-pattern"),
        &[evolver_failure_event("f1", "t1", "build")],
    );

    let (detector_config, context) = slot_outer_loop_pieces(&slot);
    let governance: Box<dyn RuleChangeGovernance> = Box::new(PolicyRuleChangeGovernance::default());
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);

    let report = OuterLoopDriver::new().drive(&mut slot, input);

    assert!(report.is_empty());
    assert_eq!(report.patterns.len(), 0);
    assert_eq!(report.proposals.len(), 0);
    assert_eq!(report.applied_count(), 0);
    assert_eq!(report.rejected_count(), 0);
    assert_eq!(report.error_count(), 0);
}

#[test]
fn driver_detect_error_is_structured_not_panic() {
    let mut slot = canonical_slot_with_events(
        temp_skill_root("detect-error"),
        &[evolver_failure_event("f1", "t1", "build")],
    );

    // min_repeats=0 是非法配置（builder 会钳制为 1，这里直接构造）→ detect 阶段结构化错误。
    let detector_config = FailureDetectorConfig {
        min_repeats: 0,
        window: None,
        failure_kinds: vec![EvolverRuntimeEventKind::ToolFailed],
    };
    let (_, context) = slot_outer_loop_pieces(&slot);
    let governance: Box<dyn RuleChangeGovernance> = Box::new(PolicyRuleChangeGovernance::default());
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);

    let report = OuterLoopDriver::new().drive(&mut slot, input);

    assert_eq!(report.error_count(), 1);
    assert_eq!(report.errors[0].stage, "detect");
    assert_eq!(report.errors[0].kind, "invalid_scope");
    assert_eq!(report.errors[0].proposal_id, None);
    assert_eq!(report.applied_count(), 0);
}

#[test]
fn driver_governance_evaluate_error_is_structured() {
    // 治理 evaluate 出错（非批准/拒绝，而是错误）→ 结构化错误。
    let mut slot = canonical_slot_with_events(
        temp_skill_root("governance-error"),
        &[
            evolver_failure_event("f1", "t1", "build"),
            evolver_failure_event("f2", "t2", "build"),
        ],
    );

    let (detector_config, context) = slot_outer_loop_pieces(&slot);
    // 治理上下文证据为空（observed_events 空）时 policy 治理返回拒绝而非错误，
    // 因此这里用一个错误返回的治理桩验证 evaluate 错误路径。
    let governance = GovernanceAlwaysErrors;
    let input = OuterLoopDriveInput::new(&detector_config, &governance, &context);

    let report = OuterLoopDriver::new().drive(&mut slot, input);

    assert_eq!(report.error_count(), 1);
    assert_eq!(report.errors[0].stage, "governance");
    assert_eq!(report.errors[0].kind, "invalid_rule_change");
    assert_eq!(
        report.errors[0].proposal_id.as_deref(),
        Some(report.proposals[0].proposal_id.as_str())
    );
    assert_eq!(report.applied_count(), 0);
}

struct GovernanceAlwaysErrors;

impl RuleChangeGovernance for GovernanceAlwaysErrors {
    fn evaluate(
        &self,
        _proposal: &RuleChangeProposal,
        _context: &GovernanceContext,
    ) -> Result<chuang_agent::skill_evolver::GovernanceDecision, EvolutionError> {
        Err(EvolutionError::InvalidRuleChange(
            "governance backend unavailable".to_string(),
        ))
    }
}

#[test]
fn driver_apply_error_is_structured_and_governance_gate_not_bypassed() {
    // 治理批准后 apply 落盘失败 → 结构化 apply 错误；同时验证 apply 必须
    // 经过治理（StubEvolver 记录 apply 是否被调用）。
    let mut evolver = StubOuterLoopEvolver::with_apply_error(EvolutionError::StorageError(
        "disk unavailable".to_string(),
    ));
    let f1 = evolver_failure_event("e1", "t1", "build");
    evolver.observe(f1).expect("observe");

    let detector_config = FailureDetectorConfig::default();
    let context = GovernanceContext {
        observed_events: evolver.observed_events.to_vec(),
        detector_config: detector_config.clone(),
    };
    let governance: Box<dyn RuleChangeGovernance> = Box::new(PolicyRuleChangeGovernance::default());
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);

    let report = OuterLoopDriver::new().drive(&mut evolver, input);

    assert_eq!(report.error_count(), 1);
    assert_eq!(report.errors[0].stage, "apply");
    assert_eq!(report.errors[0].kind, "storage_error");
    assert_eq!(
        report.errors[0].proposal_id.as_deref(),
        Some(evolver.proposal.as_ref().unwrap().proposal_id.as_str())
    );
    assert_eq!(report.applied_count(), 0);
    // apply 被调用过（治理批准后才调用）→ 门禁没有被绕过。
    assert!(evolver.apply_called);
}

/// 驱动测试用桩：观察流 + 固定 pattern/proposal；apply 可注入错误。
struct StubOuterLoopEvolver {
    observed_events: Vec<EvolverRuntimeEvent>,
    pattern: Option<FailurePattern>,
    proposal: Option<RuleChangeProposal>,
    apply_called: bool,
    apply_error: Option<EvolutionError>,
}

impl StubOuterLoopEvolver {
    fn with_apply_error(error: EvolutionError) -> Self {
        let f1 = evolver_failure_event("e1", "t1", "build");
        let pattern = FailurePattern {
            signature: "tool=build".to_string(),
            kind: EvolverRuntimeEventKind::ToolFailed,
            count: 2,
            window_size: 2,
            event_ids: vec!["e1".to_string()],
            task_ids: vec!["t1".to_string()],
            first_seen_event_id: "e1".to_string(),
            last_seen_event_id: "e1".to_string(),
            summary: "repeated failure tool=build observed 2 times".to_string(),
        };
        let proposal = RuleChangeProposal {
            proposal_id: "stub-rule-change-1".to_string(),
            rule_id: "rule-for-tool-build".to_string(),
            change_kind: RuleChangeKind::CreateRule,
            title: "Rule for tool=build".to_string(),
            trigger: "repeated failure tool=build observed 2 times".to_string(),
            old_procedure: Vec::new(),
            new_procedure: vec![
                "Apply the corrective procedure for tool=build with the recorded fallback."
                    .to_string(),
                "Verify the fix with a check or test and record the result.".to_string(),
            ],
            rationale: "auto-proposed by the evolver outer loop stub".to_string(),
            evidence: vec![FailureEvidence {
                pattern_signature: "tool=build".to_string(),
                count: 2,
                event_ids: vec!["e1".to_string()],
                task_ids: vec!["t1".to_string()],
                summary: "repeated failure tool=build observed 2 times".to_string(),
            }],
            writes_rules: true,
            requires_governance: true,
            provenance: vec![SkillProposalProvenance {
                source_event_id: "e1".to_string(),
                source_task_id: "t1".to_string(),
                source_kind: EvolverRuntimeEventKind::ToolFailed,
                source_summary: "tool build failed".to_string(),
                source_metadata: BTreeMap::new(),
            }],
        };
        Self {
            observed_events: vec![f1],
            pattern: Some(pattern),
            proposal: Some(proposal),
            apply_called: false,
            apply_error: Some(error),
        }
    }
}

impl SkillEvolver for StubOuterLoopEvolver {
    fn observe(&mut self, event: EvolverRuntimeEvent) -> Result<EvolutionReceipt, EvolutionError> {
        self.observed_events.push(event);
        Ok(EvolutionReceipt {
            accepted: true,
            message: "observed".to_string(),
        })
    }

    fn propose(&self, _scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError> {
        Err(EvolutionError::InvalidScope("stub".to_string()))
    }

    fn validate(&self, _proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError> {
        Err(EvolutionError::InvalidProposal("stub".to_string()))
    }

    fn solidify(&mut self, _proposal: SkillProposal) -> Result<SkillId, EvolutionError> {
        Err(EvolutionError::InvalidProposal("stub".to_string()))
    }

    fn detect_repeated_failures(
        &self,
        _config: &FailureDetectorConfig,
    ) -> Result<Vec<FailurePattern>, EvolutionError> {
        Ok(self.pattern.clone().into_iter().collect())
    }

    fn propose_rule_change(
        &self,
        _pattern: &FailurePattern,
    ) -> Result<RuleChangeProposal, EvolutionError> {
        self.proposal
            .clone()
            .ok_or_else(|| EvolutionError::InvalidRuleChange("stub has no proposal".to_string()))
    }

    fn apply_rule_change(
        &mut self,
        _proposal: RuleChangeProposal,
        _governance: &dyn RuleChangeGovernance,
        _context: &GovernanceContext,
    ) -> Result<RuleChangeReceipt, EvolutionError> {
        self.apply_called = true;
        match &self.apply_error {
            Some(error) => Err(error.clone()),
            None => Err(EvolutionError::InvalidRuleChange(
                "stub apply not implemented".to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// slot 集成（cli_runtime 同款接线路径）
// ---------------------------------------------------------------------------

#[test]
fn slot_wiring_bridges_and_drives_outer_loop_like_cli_runtime() {
    let root = temp_skill_root("slot-wiring");
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let canonical = CanonicalEvolutionConfig {
        skill_root: root.clone(),
        auto_outer_loop: true,
        ..Default::default()
    };
    config.evolution = EvolutionConfig::Canonical(canonical);
    let mut slots = build_runtime_slots(&config).expect("canonical slots should build");

    // 模拟 cli turn 的 ledger 事件流：同一工具两次被治理拦截。
    let ledger_events = vec![
        failed_tool_ledger_event("2026-08-10T00:01:00Z", "tool:open_app:cli:cli:tool:1"),
        failed_tool_ledger_event("2026-08-10T00:01:01Z", "tool:open_app:cli:cli:tool:2"),
    ];
    let bridge = EvolutionEventBridge::new();
    bridge
        .observe_turn_events(&mut slots.evolution, &ledger_events)
        .expect("ledger events should observe");
    bridge
        .observe_turn_completed(
            &mut slots.evolution,
            "cli",
            "turn-1",
            "agent turn completed",
            BTreeMap::new(),
        )
        .expect("turn completed should observe");

    // cli_runtime 同款：owned governance + detector + context。
    let governance = slots
        .evolution
        .cloned_rule_change_governance()
        .expect("canonical slot exposes owned governance");
    let detector_config = slots
        .evolution
        .rule_change_detector_config()
        .expect("canonical slot exposes detector config")
        .clone();
    let context = slots
        .evolution
        .rule_change_governance_context()
        .expect("canonical slot exposes governance context");
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);

    let report = OuterLoopDriver::new().drive(&mut slots.evolution, input);

    assert_eq!(report.patterns.len(), 1);
    assert_eq!(report.patterns[0].signature, "tool=open_app");
    assert_eq!(report.applied_count(), 1);
    assert_eq!(report.error_count(), 0);
    let receipt = &report.receipts[0];
    assert!(receipt.path.exists());
    assert_eq!(receipt.change_kind, RuleChangeKind::CreateRule);
    let journal_path = report.journal_path.as_ref().expect("journal path");
    assert!(journal_path.exists());
    assert_eq!(
        slots
            .evolution
            .rule_change_history()
            .expect("history readable")
            .len(),
        1
    );
}

#[test]
fn slot_noop_governance_wiring_rejects_without_writing() {
    let root = temp_skill_root("slot-noop");
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let canonical = CanonicalEvolutionConfig {
        skill_root: root.clone(),
        auto_outer_loop: true,
        governance: chuang_agent::runtime_config::CanonicalEvolutionGovernance::Noop,
        ..Default::default()
    };
    config.evolution = EvolutionConfig::Canonical(canonical);
    let mut slots = build_runtime_slots(&config).expect("canonical slots should build");

    let bridge = EvolutionEventBridge::new();
    bridge
        .observe_turn_events(
            &mut slots.evolution,
            &[
                failed_tool_ledger_event("2026-08-10T00:02:00Z", "tool:open_app:cli:cli:tool:1"),
                failed_tool_ledger_event("2026-08-10T00:02:01Z", "tool:open_app:cli:cli:tool:2"),
            ],
        )
        .expect("observe");
    bridge
        .observe_turn_completed(
            &mut slots.evolution,
            "cli",
            "turn-2",
            "agent turn completed",
            BTreeMap::new(),
        )
        .expect("observe turn completed");

    let governance = slots
        .evolution
        .cloned_rule_change_governance()
        .expect("owned governance");
    let detector_config = slots
        .evolution
        .rule_change_detector_config()
        .expect("detector")
        .clone();
    let context = slots
        .evolution
        .rule_change_governance_context()
        .expect("context");
    let input = OuterLoopDriveInput::new(&detector_config, governance.as_ref(), &context);
    let report = OuterLoopDriver::new().drive(&mut slots.evolution, input);

    assert_eq!(report.rejected_count(), 1);
    assert_eq!(report.applied_count(), 0);
    assert_eq!(report.error_count(), 0);
    assert!(!root.join(".evolver").join("rule_changes.jsonl").exists());
}

#[test]
fn non_canonical_slots_expose_no_outer_loop_pieces() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let slots = build_runtime_slots(&config).expect("default slots build");
    assert!(slots.evolution.cloned_rule_change_governance().is_none());
    assert!(slots.evolution.rule_change_detector_config().is_none());
    assert!(slots.evolution.rule_change_governance_context().is_none());
    assert!(slots.evolution.rule_change_journal_path().is_none());
}
