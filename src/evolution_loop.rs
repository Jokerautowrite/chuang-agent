//! evolver 外环的运行时接线：把 ledger 运行时事件桥接进 evolver 观察流，
//! 并在每个 agent turn 结束后自动驱动 detect → propose → governance → apply。
//!
//! 设计约束：
//! - 桥接映射是纯函数、可单测；每条 ledger 事件要么映射成一条 evolver 事件，要么被忽略。
//! - 外环驱动不 panic：任何阶段失败都收集进结构化的 `OuterLoopReport`。
//! - 治理是强制门禁：`apply_rule_change` 只有在治理批准后才被调用，且 apply 内部
//!   仍会再次执行治理门禁（双保险，绝不绕过治理直接落盘）。

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

use crate::runtime_event_ledger::{RuntimeEvent as LedgerRuntimeEvent, RuntimeEventKind};
use crate::skill_evolver::{
    EvolutionError, EvolutionReceipt, FailureDetectorConfig, FailurePattern, GovernanceContext,
    RuleChangeGovernance, RuleChangeProposal, RuleChangeReceipt,
    RuntimeEvent as EvolverRuntimeEvent, RuntimeEventKind as EvolverRuntimeEventKind, SkillEvolver,
};

/// 桥接层错误：结构化返回，不 panic。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionBridgeError {
    /// evolver 拒绝该事件（事件结构不合法等）。
    Evolver(EvolutionError),
}

impl fmt::Display for EvolutionBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evolver(error) => write!(f, "evolution_bridge_evolver_rejected: {error:?}"),
        }
    }
}

impl std::error::Error for EvolutionBridgeError {}

/// 把 ledger 层 `RuntimeEvent`（turn/tool 生命周期事件）桥接成 evolver 层
/// `RuntimeEvent`（观察流）。映射规则：
/// - `TurnCompleted` → `TurnCompleted`
/// - `TurnFailed` → `ToolFailed`（带 `ledger_source=turn_failed` 元数据）
/// - `ToolFinished` 且执行失败（risk_decision 为 blocked / draft_only /
///   needs_approval）→ `ToolFailed`（带 tool / error_code / error 元数据，
///   供重复失败检测器形成签名）
/// - `ToolFinished` 且执行成功（allowed 或无 risk_decision）→ `ToolSucceeded`
/// - 其他事件（ToolStarted、ProviderRequested、MemoryCommitted 等）忽略
///
/// 注意：ledger 事件本身没有自由格式 metadata 字段，失败判定使用其结构化
/// 字段 `risk_decision`（执行被治理拦截/挂起即视为该工具调用失败）。
#[derive(Debug, Clone, Copy, Default)]
pub struct EvolutionEventBridge;

impl EvolutionEventBridge {
    pub fn new() -> Self {
        Self
    }

    /// 把一条 ledger 事件映射成 evolver 事件；忽略的事件返回 `None`。
    /// 纯函数：同样的输入永远得到同样的输出，便于单测。
    pub fn map_event(
        &self,
        ledger_event: &LedgerRuntimeEvent,
        index: usize,
    ) -> Option<EvolverRuntimeEvent> {
        let task_id = ledger_event
            .turn_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if ledger_event.thread_id.trim().is_empty() {
                    "unknown".to_string()
                } else {
                    ledger_event.thread_id.clone()
                }
            });
        let event_id = format!(
            "ledger:{}:{}:{}",
            ledger_event.created_at, index, ledger_event.thread_id
        );

        match ledger_event.event_type {
            RuntimeEventKind::TurnCompleted => Some(EvolverRuntimeEvent {
                event_id,
                task_id,
                kind: EvolverRuntimeEventKind::TurnCompleted,
                summary: "agent turn completed".to_string(),
                metadata: base_metadata(ledger_event, "turn_completed"),
            }),
            RuntimeEventKind::TurnFailed => {
                let mut metadata = base_metadata(ledger_event, "turn_failed");
                metadata.insert("error".to_string(), "turn_failed".to_string());
                Some(EvolverRuntimeEvent {
                    event_id,
                    task_id,
                    kind: EvolverRuntimeEventKind::ToolFailed,
                    summary: "agent turn failed".to_string(),
                    metadata,
                })
            }
            RuntimeEventKind::ToolFinished => {
                let tool = tool_name_from_call_id(ledger_event.call_id.as_deref())
                    .unwrap_or_else(|| "unknown".to_string());
                if tool_finished_is_failure(ledger_event) {
                    let risk = ledger_event.risk_decision.as_ref();
                    let error_code = risk
                        .map(|risk| risk_decision_label_prefix(&risk.decision))
                        .unwrap_or("failed")
                        .to_string();
                    let error = risk
                        .map(|risk| risk.reason.clone())
                        .filter(|reason| !reason.trim().is_empty())
                        .unwrap_or_else(|| {
                            "tool finished with a failing risk decision".to_string()
                        });
                    let mut metadata = base_metadata(ledger_event, "tool_finished");
                    metadata.insert("tool".to_string(), tool.clone());
                    metadata.insert("error_code".to_string(), error_code);
                    metadata.insert("error".to_string(), error);
                    Some(EvolverRuntimeEvent {
                        event_id,
                        task_id,
                        kind: EvolverRuntimeEventKind::ToolFailed,
                        summary: format!("tool {tool} failed"),
                        metadata,
                    })
                } else {
                    let mut metadata = base_metadata(ledger_event, "tool_finished");
                    metadata.insert("tool".to_string(), tool.clone());
                    Some(EvolverRuntimeEvent {
                        event_id,
                        task_id,
                        kind: EvolverRuntimeEventKind::ToolSucceeded,
                        summary: format!("tool {tool} succeeded"),
                        metadata,
                    })
                }
            }
            _ => None,
        }
    }

    /// 观察一条 ledger 事件（映射 + 喂入 evolver）。忽略的事件返回 `Ok(None)`。
    pub fn observe_event(
        &self,
        evolver: &mut impl SkillEvolver,
        ledger_event: &LedgerRuntimeEvent,
        index: usize,
    ) -> Result<Option<EvolutionReceipt>, EvolutionBridgeError> {
        let Some(event) = self.map_event(ledger_event, index) else {
            return Ok(None);
        };
        evolver
            .observe(event)
            .map(Some)
            .map_err(EvolutionBridgeError::Evolver)
    }

    /// 保序观察一个 turn 的完整 ledger 事件流，返回实际喂入的事件数。
    pub fn observe_turn_events(
        &self,
        evolver: &mut impl SkillEvolver,
        ledger_events: &[LedgerRuntimeEvent],
    ) -> Result<usize, EvolutionBridgeError> {
        let mut observed = 0usize;
        for (index, ledger_event) in ledger_events.iter().enumerate() {
            match self.observe_event(evolver, ledger_event, index) {
                Ok(Some(_)) => observed += 1,
                Ok(None) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(observed)
    }

    /// 合成并观察一条 `TurnCompleted`（cli 主 turn 结束时调用；ledger 自身
    /// 不写 TurnCompleted，由接线层补齐）。
    pub fn observe_turn_completed(
        &self,
        evolver: &mut impl SkillEvolver,
        thread_id: &str,
        turn_id: &str,
        summary: &str,
        extra: BTreeMap<String, String>,
    ) -> Result<EvolutionReceipt, EvolutionBridgeError> {
        let mut metadata = extra;
        metadata.insert("ledger_source".to_string(), "turn_completed".to_string());
        metadata.insert("thread_id".to_string(), thread_id.to_string());
        metadata.insert("turn_id".to_string(), turn_id.to_string());
        let event = EvolverRuntimeEvent {
            event_id: format!("turn-completed:{thread_id}:{turn_id}"),
            task_id: if turn_id.trim().is_empty() {
                thread_id.to_string()
            } else {
                turn_id.to_string()
            },
            kind: EvolverRuntimeEventKind::TurnCompleted,
            summary: if summary.trim().is_empty() {
                "agent turn completed".to_string()
            } else {
                summary.to_string()
            },
            metadata,
        };
        evolver
            .observe(event)
            .map_err(EvolutionBridgeError::Evolver)
    }
}

/// 外环驱动所需的槽位输入（均为 owned/借用局部值，避免与 `&mut evolver`
/// 发生借用冲突）。
pub struct OuterLoopDriveInput<'a> {
    pub detector_config: &'a FailureDetectorConfig,
    pub governance: &'a dyn RuleChangeGovernance,
    pub governance_context: &'a GovernanceContext,
}

impl<'a> OuterLoopDriveInput<'a> {
    pub fn new(
        detector_config: &'a FailureDetectorConfig,
        governance: &'a dyn RuleChangeGovernance,
        governance_context: &'a GovernanceContext,
    ) -> Self {
        Self {
            detector_config,
            governance,
            governance_context,
        }
    }
}

/// 外环自动驱动：detect 重复失败 → 逐个 propose 规则修改 → 治理 evaluate →
/// 批准才 apply 落盘，拒绝记录原因不落盘。任何阶段错误都结构化收集进报告。
#[derive(Debug, Clone, Copy, Default)]
pub struct OuterLoopDriver;

impl OuterLoopDriver {
    pub fn new() -> Self {
        Self
    }

    pub fn drive(
        &self,
        evolver: &mut impl SkillEvolver,
        input: OuterLoopDriveInput<'_>,
    ) -> OuterLoopReport {
        let mut report = OuterLoopReport::default();
        report.journal_path = evolver.rule_change_journal_path();

        let patterns = match evolver.detect_repeated_failures(input.detector_config) {
            Ok(patterns) => patterns,
            Err(error) => {
                report
                    .errors
                    .push(OuterLoopStageError::new("detect", None, error));
                return report;
            }
        };
        report.patterns = patterns.clone();

        for pattern in &patterns {
            let proposal = match evolver.propose_rule_change(pattern) {
                Ok(proposal) => proposal,
                Err(error) => {
                    report
                        .errors
                        .push(OuterLoopStageError::new("propose", None, error));
                    continue;
                }
            };
            report.proposals.push(proposal.clone());

            let decision = match input.governance.evaluate(&proposal, input.governance_context) {
                Ok(decision) => decision,
                Err(error) => {
                    report.errors.push(OuterLoopStageError::new(
                        "governance",
                        Some(proposal.proposal_id.clone()),
                        error,
                    ));
                    continue;
                }
            };

            if !decision.approved {
                report.rejections.push(RejectedProposal {
                    proposal_id: proposal.proposal_id.clone(),
                    rule_id: proposal.rule_id.clone(),
                    reasons: decision.reasons.clone(),
                    approval_source: decision.approval_source.clone(),
                    decided_by: decision.decided_by.clone(),
                });
                continue;
            }

            // 治理批准后才允许走写路径；apply 内部仍会再跑一次治理门禁（强制）。
            match evolver.apply_rule_change(
                proposal.clone(),
                input.governance,
                input.governance_context,
            ) {
                Ok(receipt) => report.receipts.push(receipt),
                Err(error) => report.errors.push(OuterLoopStageError::new(
                    "apply",
                    Some(proposal.proposal_id.clone()),
                    error,
                )),
            }
        }

        report
    }
}

/// 一次外环驱动的结构化报告（可序列化进 turn 元数据/日志）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OuterLoopReport {
    pub patterns: Vec<FailurePattern>,
    pub proposals: Vec<RuleChangeProposal>,
    pub receipts: Vec<RuleChangeReceipt>,
    pub rejections: Vec<RejectedProposal>,
    pub errors: Vec<OuterLoopStageError>,
    pub journal_path: Option<PathBuf>,
}

impl OuterLoopReport {
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
            && self.proposals.is_empty()
            && self.receipts.is_empty()
            && self.rejections.is_empty()
            && self.errors.is_empty()
    }

    pub fn applied_count(&self) -> usize {
        self.receipts.len()
    }

    pub fn rejected_count(&self) -> usize {
        self.rejections.len()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// 一行人类可读摘要（进 turn 元数据用）。
    pub fn summary_line(&self) -> String {
        format!(
            "patterns={} proposals={} applied={} rejected={} errors={}",
            self.patterns.len(),
            self.proposals.len(),
            self.receipts.len(),
            self.rejections.len(),
            self.errors.len()
        )
    }
}

/// 被治理拒绝、未落盘的提案（记录原因供审计）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RejectedProposal {
    pub proposal_id: String,
    pub rule_id: String,
    pub reasons: Vec<String>,
    pub approval_source: String,
    pub decided_by: String,
}

/// 外环某阶段的失败（结构化，不 panic）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OuterLoopStageError {
    pub stage: String,
    pub proposal_id: Option<String>,
    pub kind: String,
    pub message: String,
}

impl OuterLoopStageError {
    pub fn new(stage: &str, proposal_id: Option<String>, error: EvolutionError) -> Self {
        Self {
            stage: stage.to_string(),
            proposal_id,
            kind: evolution_error_kind(&error).to_string(),
            message: format!("{error:?}"),
        }
    }
}

fn evolution_error_kind(error: &EvolutionError) -> &'static str {
    match error {
        EvolutionError::InvalidEvent(_) => "invalid_event",
        EvolutionError::InvalidScope(_) => "invalid_scope",
        EvolutionError::InvalidProposal(_) => "invalid_proposal",
        EvolutionError::InvalidRuleChange(_) => "invalid_rule_change",
        EvolutionError::ValidationRejected(_) => "validation_rejected",
        EvolutionError::StorageError(_) => "storage_error",
    }
}

fn base_metadata(
    ledger_event: &LedgerRuntimeEvent,
    ledger_source: &str,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("ledger_source".to_string(), ledger_source.to_string());
    metadata.insert("thread_id".to_string(), ledger_event.thread_id.clone());
    if let Some(turn_id) = &ledger_event.turn_id {
        metadata.insert("turn_id".to_string(), turn_id.clone());
    }
    if let Some(call_id) = &ledger_event.call_id {
        metadata.insert("call_id".to_string(), call_id.clone());
    }
    if let Some(evidence_ref) = &ledger_event.evidence_ref {
        metadata.insert("evidence_ref".to_string(), evidence_ref.clone());
    }
    metadata.insert("created_at".to_string(), ledger_event.created_at.clone());
    if let Some(risk) = &ledger_event.risk_decision {
        metadata.insert("risk_decision".to_string(), risk.decision.clone());
        if let Some(policy_ref) = &risk.policy_ref {
            metadata.insert("policy_ref".to_string(), policy_ref.clone());
        }
    }
    metadata
}

/// 从 ledger call_id（形如 `tool:<name>:<agent>:<task>`）提取工具名。
fn tool_name_from_call_id(call_id: Option<&str>) -> Option<String> {
    let name = call_id?.strip_prefix("tool:")?.split(':').next()?;
    if name.trim().is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// risk_decision 标签前缀（`blocked:reason` → `blocked`）。
fn risk_decision_label_prefix(decision: &str) -> &str {
    decision.split(':').next().unwrap_or(decision)
}

/// ToolFinished 是否视为失败：执行被治理拦截（blocked）/ 草稿（draft_only）/
/// 挂起审批（needs_approval）即失败；allowed 或无 risk_decision 视为成功。
fn tool_finished_is_failure(ledger_event: &LedgerRuntimeEvent) -> bool {
    let Some(risk) = &ledger_event.risk_decision else {
        return false;
    };
    matches!(
        risk_decision_label_prefix(&risk.decision),
        "blocked" | "draft_only" | "needs_approval"
    )
}
