use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent_runtime::{AgentRuntime, AgentRuntimeError, RuntimeRequest, RuntimeResult};
use crate::common::{AgentId, AuditRecord, TaskId, Timestamp};
use crate::context_engine::{ContextBudget, ContextEngineKind, ContextSegment, SegmentSource};
use crate::governance::{
    risk_decision_label, risk_decision_parts, ActionKind, Governance, GovernanceError,
    ProposedAction, RiskDecision,
};
use crate::hermes_memory::DualFileMemorySnapshot;
use crate::identity_registry::AgentIdentity;
use crate::memory_admission::{
    preview_chars, MemoryEntryView, TextMemoryAdmission, TextMemoryAdmissionDecision,
};
use crate::memory_store::{MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError};
use crate::responder::Responder;
use crate::runtime_event_ledger::{InMemoryRuntimeEventLedger, RuntimeEventLedger};
use crate::runtime_report::build_runtime_report;
use crate::subagent_report::{GovernanceDecisionSummary, SubagentReport};
use serde::Serialize;

pub use crate::memory_admission::DEFAULT_MEMORY_WRITE_MAX_CHARS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelConfig {
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: Option<ContextBudget>,
    pub context_engine_kind: Option<ContextEngineKind>,
    pub memory_write_max_chars: Option<usize>,
    pub identity_snapshot: Option<DualFileMemorySnapshot>,
    pub identity_bootstrap_snapshot: Option<IdentityBootstrapSnapshot>,
    /// Governance doctrine (rules/core.md) distilled for the model context.
    /// Loaded from the configured rules core path, never hardcoded.
    pub governance_rules: Option<GovernanceRulesSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GovernanceRulesSnapshot {
    pub content: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdentityBootstrapSnapshot {
    pub soul: String,
    pub soul_exists: bool,
    pub story: String,
    pub story_exists: bool,
    pub first_wake: String,
    pub first_wake_exists: bool,
    pub agents_registry: String,
    pub agents_registry_exists: bool,
    pub active_identity: Option<AgentIdentity>,
}

impl ChuangKernelConfig {
    pub fn mvp_default(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            parent_agent_id: None,
            recall_limit: 5,
            metadata: BTreeMap::new(),
            context_budget: None,
            context_engine_kind: None,
            memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
            identity_snapshot: None,
            identity_bootstrap_snapshot: None,
            governance_rules: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelTurn {
    pub turn_id: String,
    pub user_input: String,
    pub result: RuntimeResult,
    pub report: SubagentReport,
    pub governance_decision: Option<RiskDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChuangKernelSnapshot {
    pub agent_id: String,
    pub turn_count: u64,
    pub recall_limit: usize,
    pub metadata_keys: Vec<String>,
    pub context_budget_max_tokens: Option<u32>,
    pub memory_write_max_chars: Option<usize>,
    pub identity_user_chars: Option<usize>,
    pub identity_memory_chars: Option<usize>,
    pub identity_soul_chars: Option<usize>,
    pub identity_soul_exists: Option<bool>,
    pub identity_story_chars: Option<usize>,
    pub identity_story_exists: Option<bool>,
    pub identity_first_wake_chars: Option<usize>,
    pub identity_first_wake_exists: Option<bool>,
    pub identity_agents_registry_chars: Option<usize>,
    pub identity_agents_registry_exists: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChuangKernelMemoryError {
    Store(MemoryStoreError),
    HardLimitExceeded {
        limit_chars: usize,
        attempted_chars: usize,
        existing_entries: Vec<MemoryEntryView>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChuangKernelGovernanceError {
    Governance(GovernanceError),
    NotAllowed { decision: RiskDecision },
    Runtime(AgentRuntimeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RememberTurnReceipt {
    pub record_id: String,
    pub compacted: bool,
    pub attempted_chars: usize,
    pub stored_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTurnMemory {
    pub record: MemoryRecord,
    pub receipt: RememberTurnReceipt,
}

pub struct ChuangKernel<S, R> {
    config: ChuangKernelConfig,
    runtime: AgentRuntime<S, R>,
    turn_count: u64,
}

impl<S, R> ChuangKernel<S, R> {
    pub fn with_responder(config: ChuangKernelConfig, store: S, responder: R) -> Self {
        Self {
            runtime: AgentRuntime::with_responder_and_context_engine(
                store,
                responder,
                config.context_engine_kind.clone().unwrap_or_default(),
            ),
            config,
            turn_count: 0,
        }
    }

    pub fn snapshot(&self) -> ChuangKernelSnapshot {
        ChuangKernelSnapshot {
            agent_id: self.config.agent_id.clone(),
            turn_count: self.turn_count,
            recall_limit: self.config.recall_limit,
            metadata_keys: self.config.metadata.keys().cloned().collect(),
            context_budget_max_tokens: self
                .config
                .context_budget
                .as_ref()
                .map(|budget| budget.max_tokens),
            memory_write_max_chars: self.config.memory_write_max_chars,
            identity_user_chars: self
                .config
                .identity_snapshot
                .as_ref()
                .map(|snapshot| snapshot.user.chars().count()),
            identity_memory_chars: self
                .config
                .identity_snapshot
                .as_ref()
                .map(|snapshot| snapshot.memory.chars().count()),
            identity_soul_chars: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.soul.chars().count()),
            identity_soul_exists: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.soul_exists),
            identity_story_chars: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.story.chars().count()),
            identity_story_exists: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.story_exists),
            identity_first_wake_chars: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.first_wake.chars().count()),
            identity_first_wake_exists: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.first_wake_exists),
            identity_agents_registry_chars: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.agents_registry.chars().count()),
            identity_agents_registry_exists: self
                .config
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.agents_registry_exists),
        }
    }
}

impl<S: MemoryStore, R: Responder> ChuangKernel<S, R> {
    pub fn run_governed_turn<G: Governance>(
        &mut self,
        user_input: impl Into<String>,
        governance: &mut G,
    ) -> Result<ChuangKernelTurn, ChuangKernelGovernanceError> {
        self.run_governed_turn_with_extra_context(user_input, governance, Vec::new())
    }

    pub fn run_governed_turn_with_extra_context<G: Governance>(
        &mut self,
        user_input: impl Into<String>,
        governance: &mut G,
        extra_context_segments: Vec<ContextSegment>,
    ) -> Result<ChuangKernelTurn, ChuangKernelGovernanceError> {
        let next_turn = self.turn_count + 1;
        let turn_id = format!("turn-{next_turn}");
        let action = self.propose_runtime_turn_action(&turn_id);
        let decision = governance
            .classify(&action)
            .map_err(ChuangKernelGovernanceError::Governance)?;

        if !matches!(decision, RiskDecision::Allowed { .. }) {
            return Err(ChuangKernelGovernanceError::NotAllowed { decision });
        }

        let mut turn = self
            .run_turn_with_id_and_extra_context(
                turn_id.clone(),
                user_input.into(),
                extra_context_segments,
            )
            .map_err(ChuangKernelGovernanceError::Runtime)?;
        turn.report.governance_decision = Some(governance_decision_summary(&action, &decision));
        turn.governance_decision = Some(decision.clone());

        governance
            .audit(AuditRecord {
                operation: "run_governed_turn".to_string(),
                agent_id: AgentId(self.config.agent_id.clone()),
                task_id: TaskId(turn_id),
                delta_bytes: turn.report.summary.len() as i64,
                reason: risk_decision_label(&decision),
                timestamp: Timestamp("2026-05-01T00:00:00Z".to_string()),
            })
            .map_err(ChuangKernelGovernanceError::Governance)?;

        // 持久化治理审计记录：把本次 turn 累积的 AuditRecord 序列化进 meta，
        // 随 turn 存档（session_turn_archive）落库，避免审计明细仅存内存丢失。
        let audit_snapshot = governance.audit_records().to_vec();
        let audit_json = serde_json::to_string(&audit_snapshot).map_err(|error| {
            ChuangKernelGovernanceError::Governance(GovernanceError {
                message: format!("serialize audit records: {error}"),
            })
        })?;
        turn.result
            .response
            .meta
            .extra
            .insert("governance_audit_records_json".to_string(), audit_json);

        Ok(turn)
    }

    fn run_turn_with_id_and_extra_context(
        &mut self,
        turn_id: String,
        user_input: String,
        extra_context_segments: Vec<ContextSegment>,
    ) -> Result<ChuangKernelTurn, AgentRuntimeError> {
        let mut context_segments = self.identity_context_segments();
        context_segments.extend(extra_context_segments);
        let thread_id = self
            .config
            .metadata
            .get("session_id")
            .cloned()
            .unwrap_or_else(|| format!("agent:{}", self.config.agent_id));
        let mut runtime_event_ledger = InMemoryRuntimeEventLedger::new();
        let mut result = self.runtime.run_with_ledger(
            &RuntimeRequest {
                user_input: user_input.clone(),
                recall_limit: self.config.recall_limit,
                metadata: self.config.metadata.clone(),
                context_budget: self.config.context_budget.clone(),
                extra_context_segments: context_segments,
            },
            &mut runtime_event_ledger,
            &thread_id,
            &turn_id,
        )?;
        let runtime_events = runtime_event_ledger
            .list()
            .map_err(|error| AgentRuntimeError::EventLedger(error.to_string()))?;
        result.response.meta.extra.insert(
            "runtime_event_ledger_available".to_string(),
            "true".to_string(),
        );
        result.response.meta.extra.insert(
            "runtime_event_count".to_string(),
            runtime_events.len().to_string(),
        );
        result.response.meta.extra.insert(
            "runtime_event_ledger_json".to_string(),
            serde_json::to_string(&runtime_events)
                .map_err(|error| AgentRuntimeError::EventLedger(error.to_string()))?,
        );
        let report = build_runtime_report(
            &result,
            format!("report-{turn_id}"),
            turn_id.clone(),
            self.config.agent_id.clone(),
            self.config.parent_agent_id.clone(),
        );
        self.turn_count += 1;

        Ok(ChuangKernelTurn {
            turn_id,
            user_input,
            result,
            report,
            governance_decision: None,
        })
    }

    pub fn remember_turn(
        &mut self,
        turn: &ChuangKernelTurn,
    ) -> Result<String, ChuangKernelMemoryError> {
        self.remember_turn_with_metadata(turn, BTreeMap::new(), None, false)
            .map(|receipt| receipt.record_id)
    }

    /// 记忆本轮并附额外元数据标签（如情感状态 emotion_axes/emotion_state）。
    /// 标签进入 MemoryRecord.metadata，供未来检索按情绪维度召回。
    pub fn remember_turn_with_metadata_tags(
        &mut self,
        turn: &ChuangKernelTurn,
        extra_metadata: BTreeMap<String, String>,
    ) -> Result<String, ChuangKernelMemoryError> {
        self.remember_turn_with_metadata(turn, extra_metadata, None, false)
            .map(|receipt| receipt.record_id)
    }

    pub fn remember_session_turn(
        &mut self,
        turn: &ChuangKernelTurn,
        session_id: &str,
    ) -> Result<RememberTurnReceipt, ChuangKernelMemoryError> {
        self.remember_turn_with_metadata(
            turn,
            BTreeMap::from([
                ("memory_scope".to_string(), "session".to_string()),
                ("session_id".to_string(), session_id.to_string()),
            ]),
            Some(format!("session-{session_id}")),
            true,
        )
    }

    pub fn prepare_session_turn_memory(
        &mut self,
        turn: &ChuangKernelTurn,
        session_id: &str,
    ) -> Result<PreparedTurnMemory, ChuangKernelMemoryError> {
        self.prepare_turn_memory_with_metadata(
            turn,
            BTreeMap::from([
                ("memory_scope".to_string(), "session".to_string()),
                ("session_id".to_string(), session_id.to_string()),
            ]),
            Some(format!("session-{session_id}")),
            true,
        )
    }

    fn remember_turn_with_metadata(
        &mut self,
        turn: &ChuangKernelTurn,
        extra_metadata: BTreeMap<String, String>,
        record_scope: Option<String>,
        allow_compaction: bool,
    ) -> Result<RememberTurnReceipt, ChuangKernelMemoryError> {
        let prepared = self.prepare_turn_memory_with_metadata(
            turn,
            extra_metadata,
            record_scope,
            allow_compaction,
        )?;
        self.runtime
            .memory_store_mut()
            .put(prepared.record)
            .map_err(ChuangKernelMemoryError::Store)?;
        Ok(prepared.receipt)
    }

    fn prepare_turn_memory_with_metadata(
        &mut self,
        turn: &ChuangKernelTurn,
        extra_metadata: BTreeMap<String, String>,
        record_scope: Option<String>,
        allow_compaction: bool,
    ) -> Result<PreparedTurnMemory, ChuangKernelMemoryError> {
        let record_id = record_scope
            .map(|scope| {
                format!(
                    "turn-memory-{}-{}-{}",
                    sanitize_record_id_part(&scope),
                    turn.turn_id,
                    unique_record_suffix()
                )
            })
            // 非 session 路径也要唯一后缀：CLI 每次进程内 turn_id 都是 turn-1，
            // 同一 db 连续 --remember 会撞主键（DuplicateId）。
            .unwrap_or_else(|| format!("turn-memory-{}-{}", turn.turn_id, unique_record_suffix()));
        let original_content = turn_summary_content(turn);
        let original_chars = original_content.chars().count();
        let mut content = original_content.clone();
        let mut compacted = false;

        if let Some(limit_chars) = self.config.memory_write_max_chars {
            match TextMemoryAdmission::new(limit_chars)
                .evaluate(&content, self.existing_turn_summary_entries()?)
            {
                TextMemoryAdmissionDecision::Accepted => {}
                TextMemoryAdmissionDecision::Rejected {
                    limit_chars,
                    attempted_chars,
                    existing_entries,
                } => {
                    if allow_compaction {
                        if let Some(compacted_content) =
                            compact_turn_summary_content(turn, original_chars, limit_chars)
                        {
                            match TextMemoryAdmission::new(limit_chars)
                                .evaluate(&compacted_content, existing_entries.clone())
                            {
                                TextMemoryAdmissionDecision::Accepted => {
                                    content = compacted_content;
                                    compacted = true;
                                }
                                TextMemoryAdmissionDecision::Rejected {
                                    limit_chars,
                                    attempted_chars,
                                    existing_entries,
                                } => {
                                    return Err(ChuangKernelMemoryError::HardLimitExceeded {
                                        limit_chars,
                                        attempted_chars,
                                        existing_entries,
                                    });
                                }
                            }
                        } else {
                            return Err(ChuangKernelMemoryError::HardLimitExceeded {
                                limit_chars,
                                attempted_chars,
                                existing_entries,
                            });
                        }
                    } else {
                        return Err(ChuangKernelMemoryError::HardLimitExceeded {
                            limit_chars,
                            attempted_chars,
                            existing_entries,
                        });
                    }
                }
            }
        }

        let mut metadata = BTreeMap::from([
            ("kind".to_string(), "turn_summary".to_string()),
            ("agent_id".to_string(), self.config.agent_id.clone()),
            ("turn_id".to_string(), turn.turn_id.clone()),
        ]);
        if compacted {
            metadata.insert(
                "summary_kind".to_string(),
                "compacted_turn_summary".to_string(),
            );
            metadata.insert("compacted".to_string(), "true".to_string());
            metadata.insert(
                "compacted_from_chars".to_string(),
                original_chars.to_string(),
            );
            metadata.insert(
                "compacted_to_chars".to_string(),
                content.chars().count().to_string(),
            );
        }
        metadata.extend(self.config.metadata.clone());
        metadata.extend(extra_metadata);
        let stored_chars = content.chars().count();
        Ok(PreparedTurnMemory {
            record: MemoryRecord {
                id: record_id.clone(),
                content,
                metadata,
                created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                expires_at: None,
            },
            receipt: RememberTurnReceipt {
                record_id,
                compacted,
                attempted_chars: original_chars,
                stored_chars,
            },
        })
    }

    fn existing_turn_summary_entries(
        &mut self,
    ) -> Result<Vec<MemoryEntryView>, ChuangKernelMemoryError> {
        let hits = self
            .runtime
            .memory_store_mut()
            .search(&MemoryQuery {
                text: None,
                metadata: BTreeMap::from([("kind".to_string(), "turn_summary".to_string())]),
                limit: 1000,
                match_mode: crate::memory_store::MemoryMatchMode::Token,
            })
            .map_err(ChuangKernelMemoryError::Store)?;

        Ok(hits
            .into_iter()
            .map(|hit| MemoryEntryView {
                id: hit.record.id,
                chars: hit.record.content.chars().count(),
                content_preview: preview_chars(&hit.record.content, 80),
            })
            .collect())
    }

    fn identity_context_segments(&self) -> Vec<ContextSegment> {
        let mut segments = Vec::new();
        if let Some(snapshot) = &self.config.identity_bootstrap_snapshot {
            if !snapshot.first_wake.trim().is_empty() {
                segments.push(identity_segment(
                    "identity-first-wake",
                    "FIRST_WAKE.md",
                    &compact_identity_bootstrap_content(&snapshot.first_wake, 220, 8),
                    251,
                ));
            }
            if !snapshot.soul.trim().is_empty() {
                segments.push(identity_segment(
                    "identity-soul",
                    "SOUL.md",
                    &compact_identity_bootstrap_content(&snapshot.soul, 160, 6),
                    250,
                ));
            }
            if !snapshot.story.trim().is_empty() {
                segments.push(identity_segment(
                    "identity-story",
                    "STORY.md",
                    &compact_identity_bootstrap_content(&snapshot.story, 180, 8),
                    248,
                ));
            }
            if let Some(identity) = &snapshot.active_identity {
                segments.push(active_identity_segment(identity));
            }
        }

        // Governance doctrine must be visible to the model, not only enforced
        // at tool time. Loaded from the configured rules core path.
        if let Some(rules) = &self.config.governance_rules {
            if !rules.content.trim().is_empty() {
                segments.push(identity_segment(
                    "identity-governance-rules",
                    "RULES/core.md",
                    &compact_identity_bootstrap_content(&rules.content, 3200, 64),
                    247,
                ));
            }
        }

        if let Some(segment) = self.session_context_segment() {
            segments.push(segment);
        }

        if let Some(snapshot) = &self.config.identity_snapshot {
            if !snapshot.user.trim().is_empty() {
                segments.push(identity_segment(
                    "identity-user",
                    "USER.md",
                    &snapshot.user,
                    245,
                ));
            }
            if !snapshot.memory.trim().is_empty() {
                segments.push(identity_segment(
                    "identity-memory",
                    "MEMORY.md",
                    &snapshot.memory,
                    210,
                ));
            }
        }

        segments
    }

    fn session_context_segment(&self) -> Option<ContextSegment> {
        let session_id = self
            .config
            .metadata
            .get("session_id")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let workspace_root = self
            .config
            .metadata
            .get("workspace_root")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let memory_scope = self
            .config
            .metadata
            .get("memory_scope")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if session_id.is_none() && workspace_root.is_none() && memory_scope.is_none() {
            return None;
        }

        let mut lines = vec!["[session-context]".to_string()];
        if let Some(value) = &session_id {
            lines.push(format!("session_id={value}"));
        }
        if let Some(value) = &workspace_root {
            lines.push(format!("workspace_root={value}"));
        }
        if let Some(value) = &memory_scope {
            lines.push(format!("memory_scope={value}"));
        }
        lines.push("continue=current chat/thread and workspace".to_string());
        lines.push(
            "if identity/tool context is missing, recover from workspace/memory first".to_string(),
        );
        let content = lines.join("\n");
        let now = default_identity_timestamp();
        Some(ContextSegment {
            id: "session-context".to_string(),
            source: SegmentSource::Identity,
            content,
            tokens: None,
            priority: 253,
            created_at: now,
            last_accessed: now,
            metadata: std::collections::HashMap::from([(
                "kind".to_string(),
                "session_context".to_string(),
            )]),
        })
    }

    fn propose_runtime_turn_action(&self, turn_id: &str) -> ProposedAction {
        ProposedAction {
            action_id: format!("run-{turn_id}"),
            kind: ActionKind::Draft,
            target: format!("{}:{turn_id}", self.config.agent_id),
            summary: "run local runtime turn and build auditable report".to_string(),
        }
    }
}

fn turn_summary_content(turn: &ChuangKernelTurn) -> String {
    format!(
        "user={}\nresponse={}\nsummary={}",
        turn.user_input, turn.result.response.body, turn.report.summary
    )
}

fn compact_turn_summary_content(
    turn: &ChuangKernelTurn,
    original_chars: usize,
    limit_chars: usize,
) -> Option<String> {
    const OVERHEAD_ROOM: usize = 96;
    const MIN_FIELD_CHARS: usize = 48;

    let fixed_chars =
        format!("compacted=true\noriginal_chars={original_chars}\nuser=\nresponse=\nsummary=")
            .chars()
            .count();
    let available = limit_chars.checked_sub(fixed_chars + OVERHEAD_ROOM)?;
    if available / 3 < MIN_FIELD_CHARS {
        return None;
    }

    let user_limit = (available.saturating_mul(2) / 5).min(900);
    let response_limit = (available.saturating_mul(2) / 5).min(900);
    let summary_limit = available
        .saturating_sub(user_limit + response_limit)
        .min(320);
    let compacted = format!(
        "compacted=true\noriginal_chars={original_chars}\nuser={}\nresponse={}\nsummary={}",
        truncate_chars(&turn.user_input, user_limit),
        truncate_chars(&turn.result.response.body, response_limit),
        truncate_chars(&turn.report.summary, summary_limit)
    );

    if compacted.chars().count() <= limit_chars {
        Some(compacted)
    } else {
        None
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut iter = value.chars();
    let truncated = iter.by_ref().take(limit).collect::<String>();
    if iter.next().is_some() {
        if limit <= 3 {
            ".".repeat(limit)
        } else {
            format!(
                "{}...",
                truncated.chars().take(limit - 3).collect::<String>()
            )
        }
    } else {
        truncated
    }
}

fn governance_decision_summary(
    action: &ProposedAction,
    decision: &RiskDecision,
) -> GovernanceDecisionSummary {
    let (decision, reason) = risk_decision_parts(decision);

    GovernanceDecisionSummary {
        action_id: action.action_id.clone(),
        decision: decision.to_string(),
        reason: reason.to_string(),
    }
}

fn identity_segment(id: &str, source_file: &str, content: &str, priority: u8) -> ContextSegment {
    ContextSegment {
        id: id.to_string(),
        source: SegmentSource::Identity,
        content: content.to_string(),
        tokens: Some(content.chars().count().min(u32::MAX as usize) as u32),
        priority,
        created_at: default_identity_timestamp(),
        last_accessed: default_identity_timestamp(),
        metadata: [("source_file".to_string(), source_file.to_string())]
            .into_iter()
            .collect(),
    }
}

fn active_identity_segment(identity: &AgentIdentity) -> ContextSegment {
    let content =
        serde_json::to_string(identity).expect("AgentIdentity serialization should be infallible");
    identity_segment("identity-active-agent", "agents.toml", &content, 247)
}

fn compact_identity_bootstrap_content(content: &str, max_chars: usize, max_lines: usize) -> String {
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut compacted = lines.by_ref().take(max_lines).collect::<Vec<_>>().join(" ");
    if lines.next().is_some() {
        if !compacted.is_empty() {
            compacted.push_str(" ...");
        } else {
            compacted.push_str("...");
        }
    }
    truncate_chars(&compacted, max_chars)
}

fn sanitize_record_id_part(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "session".to_string()
    } else {
        sanitized
    }
}

fn unique_record_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

fn default_identity_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
        .expect("static identity timestamp should parse")
        .with_timezone(&chrono::Utc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::StaticRuleGovernance;
    use crate::memory_store::InMemoryMemoryStore;
    use crate::responder::FakeResponder;

    #[test]
    fn governance_rules_are_injected_into_model_context() {
        let mut config = ChuangKernelConfig::mvp_default("chuang");
        config.governance_rules = Some(GovernanceRulesSnapshot {
            content: "13. Subagents never write core memory directly.\n14. Verify before trusting subagent output.\n15. No silent fallback.".to_string(),
            exists: true,
        });
        let kernel = ChuangKernel::with_responder(
            config,
            InMemoryMemoryStore::new(),
            FakeResponder::new("fake-responder".to_string()),
        );
        let segments = kernel.identity_context_segments();
        let rules = segments
            .iter()
            .find(|segment| segment.id == "identity-governance-rules");
        assert!(rules.is_some(), "governance rules segment must exist");
        let content = rules.expect("segment").content.clone();
        assert!(content.contains("never write core memory directly"));
        assert!(content.contains("No silent fallback"));
    }

    #[test]
    fn absent_governance_rules_are_skipped() {
        let config = ChuangKernelConfig::mvp_default("chuang");
        let kernel = ChuangKernel::with_responder(
            config,
            InMemoryMemoryStore::new(),
            FakeResponder::new("fake-responder".to_string()),
        );
        let segments = kernel.identity_context_segments();
        assert!(segments
            .iter()
            .all(|segment| segment.id != "identity-governance-rules"));
    }

    #[test]
    fn governed_turn_persists_audit_records_in_meta() {
        let mut config = ChuangKernelConfig::mvp_default("chuang");
        config.recall_limit = 1;
        let mut kernel = ChuangKernel::with_responder(
            config,
            InMemoryMemoryStore::new(),
            FakeResponder::new("fake-responder".to_string()),
        );
        let mut governance = StaticRuleGovernance::new();
        let turn = kernel
            .run_governed_turn("hello", &mut governance)
            .expect("governed turn should run");
        let audit_json = turn
            .result
            .response
            .meta
            .extra
            .get("governance_audit_records_json")
            .expect("audit records json should be present in meta");
        let records: Vec<AuditRecord> =
            serde_json::from_str(audit_json).expect("audit json should parse");
        assert!(
            !records.is_empty(),
            "at least the run_governed_turn audit record should be persisted"
        );
        assert!(
            records.iter().any(|r| r.operation == "run_governed_turn"),
            "run_governed_turn audit record should be in snapshot"
        );
    }
}
