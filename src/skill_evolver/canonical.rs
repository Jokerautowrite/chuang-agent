//! `skill_evolver::canonical` 模块。公开接口：struct SkillScoreDimension, SkillScoreCard, SkillSelfApprovalDecision, DuplicateDecision, SkillUpsertReceipt, SkillRetirementRequest, SkillRetirementReceipt, CanonicalSkillEvolver；enum SkillLifecycleStatus, SkillUpsertKind；fn new, with_approval_threshold, observed_events, skill_root, observed_events_path, last_solidify_receipt, last_rule_change_receipt, self_approve。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

use super::failure::{FailureDetectorConfig, FailurePattern, RepeatedFailureDetector};
use super::rule_change::{
    FailureEvidence, GovernanceContext, RuleChangeGovernance, RuleChangeJournal,
    RuleChangeJournalEntry, RuleChangeKind, RuleChangeProposal, RuleChangeReceipt,
};
use super::{
    validate_event, validate_governance_decision, validate_proposal, validate_rule_change_proposal,
    validate_scope, EvolutionError, EvolutionReceipt, EvolutionScope, RuntimeEvent,
    SkillApprovalReceipt, SkillEvolver, SkillId, SkillProposal, SkillProposalProvenance,
    ValidationReport,
};

/// 观察流持久化的最大事件数：超过后丢弃最旧事件，防止 `observed-events.jsonl`
/// 无限膨胀。检测窗口由 `FailureDetectorConfig.window` 控制，这里是存储级硬上限。
const OBSERVED_EVENTS_MAX: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillLifecycleStatus {
    Active,
    Deprecated,
    Retired,
}

impl SkillLifecycleStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
        }
    }

    fn from_str(value: &str) -> Self {
        match value.trim() {
            "deprecated" => Self::Deprecated,
            "retired" => Self::Retired,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillScoreDimension {
    pub name: String,
    pub score: u16,
    pub max_score: u16,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillScoreCard {
    pub total_score: u16,
    pub approval_threshold: u16,
    pub approved: bool,
    pub dimensions: Vec<SkillScoreDimension>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillSelfApprovalDecision {
    pub proposal_id: String,
    pub approved: bool,
    pub approval_source: String,
    pub scorecard: SkillScoreCard,
    pub receipt: SkillApprovalReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillUpsertKind {
    Created,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateDecision {
    pub duplicate_found: bool,
    pub canonical_skill_id: String,
    pub existing_path: Option<PathBuf>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillUpsertReceipt {
    pub skill_id: String,
    pub path: PathBuf,
    pub version: u32,
    pub status: SkillLifecycleStatus,
    pub kind: SkillUpsertKind,
    pub duplicate_decision: DuplicateDecision,
    pub approval_decision: SkillSelfApprovalDecision,
    pub writes_skills: bool,
    pub deletes_skill: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRetirementRequest {
    pub skill_id: String,
    pub target_status: SkillLifecycleStatus,
    pub reason: String,
    pub score: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillRetirementReceipt {
    pub skill_id: String,
    pub path: PathBuf,
    pub previous_status: SkillLifecycleStatus,
    pub status: SkillLifecycleStatus,
    pub reason: String,
    pub score: Option<u16>,
    pub writes_skills: bool,
    pub deletes_skill: bool,
}

#[derive(Debug, Clone)]
pub struct CanonicalSkillEvolver {
    observed_events: Vec<RuntimeEvent>,
    skill_root: PathBuf,
    approval_threshold: u16,
    last_solidify_receipt: Option<SkillUpsertReceipt>,
    last_rule_change_receipt: Option<RuleChangeReceipt>,
}

impl CanonicalSkillEvolver {
    pub fn new(skill_root: impl Into<PathBuf>) -> Self {
        let skill_root = skill_root.into();
        let observed_events = Self::load_observed_events(&skill_root);
        Self {
            observed_events,
            skill_root,
            approval_threshold: 75,
            last_solidify_receipt: None,
            last_rule_change_receipt: None,
        }
    }

    pub fn with_approval_threshold(mut self, approval_threshold: u16) -> Self {
        self.approval_threshold = approval_threshold.min(100);
        self
    }

    pub fn observed_events(&self) -> &[RuntimeEvent] {
        &self.observed_events
    }

    pub fn skill_root(&self) -> &Path {
        &self.skill_root
    }

    /// 观察流持久化文件：`<skill_root>/.evolver/observed-events.jsonl`（与规则
    /// 修改审计 journal 同一目录）。跨 turn 累积失败证据依赖它：每个 CLI 进程
    /// 结束时观察流不会丢失，下次启动恢复，`min_repeats>=2` 才可能在多 turn
    /// 上真实触发。
    pub fn observed_events_path(&self) -> PathBuf {
        self.skill_root
            .join(".evolver")
            .join("observed-events.jsonl")
    }

    /// 从磁盘恢复观察流：文件不存在返回空；损坏行跳过（容忍，不 panic）；
    /// IO 错误仅告警并返回空（存储问题不应阻断 evolver 启动）。
    fn load_observed_events(skill_root: &Path) -> Vec<RuntimeEvent> {
        let path = skill_root.join(".evolver").join("observed-events.jsonl");
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => {
                eprintln!(
                    "observed_events_load_failed path={}: {error}",
                    path.display()
                );
                return Vec::new();
            }
        };
        let mut events = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<RuntimeEvent>(line) {
                Ok(event) => events.push(event),
                Err(error) => eprintln!(
                    "observed_events_skip_corrupt_line path={}: {error}",
                    path.display()
                ),
            }
        }
        events
    }

    /// 全量写观察流（JSONL）。截断或恢复后调用；写失败只告警不 panic，
    /// 调用方自行决定是否继续（内存中的观察流不受影响）。
    fn persist_observed_events(&self) -> Result<(), EvolutionError> {
        let path = self.observed_events_path();
        let parent = path.parent().ok_or_else(|| {
            EvolutionError::StorageError("observed events path has no parent".to_string())
        })?;
        fs::create_dir_all(parent).map_err(storage_error)?;
        let mut content = String::new();
        for event in &self.observed_events {
            content.push_str(
                &serde_json::to_string(event)
                    .map_err(|e| EvolutionError::StorageError(e.to_string()))?,
            );
            content.push('\n');
        }
        fs::write(&path, content).map_err(storage_error)
    }

    /// append 单条事件到观察流 JSONL（常规路径，避免每次 observe 全量重写）。
    fn append_observed_event(&self, event: &RuntimeEvent) -> Result<(), EvolutionError> {
        let path = self.observed_events_path();
        let parent = path.parent().ok_or_else(|| {
            EvolutionError::StorageError("observed events path has no parent".to_string())
        })?;
        fs::create_dir_all(parent).map_err(storage_error)?;
        let mut line = serde_json::to_string(event)
            .map_err(|e| EvolutionError::StorageError(e.to_string()))?;
        line.push('\n');
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(storage_error)?;
        file.write_all(line.as_bytes()).map_err(storage_error)?;
        file.sync_all().map_err(storage_error)
    }

    pub fn last_solidify_receipt(&self) -> Option<&SkillUpsertReceipt> {
        self.last_solidify_receipt.as_ref()
    }

    pub fn last_rule_change_receipt(&self) -> Option<&RuleChangeReceipt> {
        self.last_rule_change_receipt.as_ref()
    }

    pub fn self_approve(
        &self,
        proposal: &SkillProposal,
    ) -> Result<SkillSelfApprovalDecision, EvolutionError> {
        validate_proposal(proposal)?;
        let scorecard = self.score(proposal)?;
        let validation_report = ValidationReport {
            proposal_id: proposal.proposal_id.clone(),
            accepted: scorecard.approved,
            reasons: scorecard.reasons.clone(),
        };
        let receipt = if scorecard.approved {
            SkillApprovalReceipt::approved_receipt(
                proposal.proposal_id.clone(),
                validation_report,
                "self_policy:darwin_rubric".to_string(),
                None,
                Some(format!(
                    "score={} threshold={}",
                    scorecard.total_score, scorecard.approval_threshold
                )),
            )
        } else {
            SkillApprovalReceipt {
                proposal_id: proposal.proposal_id.clone(),
                validation_report,
                approved: false,
                approval_source: "self_policy:darwin_rubric_rejected".to_string(),
                approved_at: None,
                approval_note: Some(format!(
                    "score={} threshold={}; improve proposal before upsert",
                    scorecard.total_score, scorecard.approval_threshold
                )),
            }
        };

        Ok(SkillSelfApprovalDecision {
            proposal_id: proposal.proposal_id.clone(),
            approved: scorecard.approved,
            approval_source: receipt.approval_source.clone(),
            scorecard,
            receipt,
        })
    }

    pub fn solidify_with_receipt(
        &mut self,
        proposal: SkillProposal,
    ) -> Result<SkillUpsertReceipt, EvolutionError> {
        validate_proposal(&proposal)?;
        let approval_decision = self.self_approve(&proposal)?;
        if !approval_decision.approved {
            return Err(EvolutionError::ValidationRejected(
                approval_decision.scorecard.reasons.clone(),
            ));
        }

        fs::create_dir_all(&self.skill_root).map_err(storage_error)?;

        let existing_records = self.load_existing_records()?;
        let duplicate_decision = duplicate_decision(&proposal, &existing_records);
        let existing = existing_records
            .iter()
            .find(|record| record.skill_id == duplicate_decision.canonical_skill_id);
        let version = existing.map(|record| record.version + 1).unwrap_or(1);
        let path = existing
            .map(|record| record.path.clone())
            .unwrap_or_else(|| {
                self.skill_root
                    .join(format!("{}.md", duplicate_decision.canonical_skill_id))
            });
        let rendered = render_skill_markdown(
            &duplicate_decision.canonical_skill_id,
            version,
            SkillLifecycleStatus::Active,
            &proposal,
            &approval_decision,
        );
        fs::write(&path, rendered).map_err(storage_error)?;

        let receipt = SkillUpsertReceipt {
            skill_id: duplicate_decision.canonical_skill_id.clone(),
            path,
            version,
            status: SkillLifecycleStatus::Active,
            kind: if duplicate_decision.duplicate_found {
                SkillUpsertKind::Updated
            } else {
                SkillUpsertKind::Created
            },
            duplicate_decision,
            approval_decision,
            writes_skills: true,
            deletes_skill: false,
        };
        self.last_solidify_receipt = Some(receipt.clone());
        Ok(receipt)
    }

    pub fn retire(
        &self,
        request: SkillRetirementRequest,
    ) -> Result<SkillRetirementReceipt, EvolutionError> {
        if request.skill_id.trim().is_empty() {
            return Err(EvolutionError::InvalidProposal(
                "skill_id must not be empty".to_string(),
            ));
        }
        if request.reason.trim().is_empty() {
            return Err(EvolutionError::InvalidProposal(
                "retirement reason must not be empty".to_string(),
            ));
        }
        if matches!(request.target_status, SkillLifecycleStatus::Active) {
            return Err(EvolutionError::InvalidProposal(
                "retirement target_status must be deprecated or retired".to_string(),
            ));
        }

        let records = self.load_existing_records()?;
        let record = records
            .iter()
            .find(|record| record.skill_id == request.skill_id)
            .ok_or_else(|| {
                EvolutionError::StorageError(format!(
                    "skill not found for retirement: {}",
                    request.skill_id
                ))
            })?;
        let existing = fs::read_to_string(&record.path).map_err(storage_error)?;
        let updated = update_lifecycle_frontmatter(
            &existing,
            record,
            request.target_status,
            &request.reason,
            request.score,
        );
        fs::write(&record.path, updated).map_err(storage_error)?;

        Ok(SkillRetirementReceipt {
            skill_id: record.skill_id.clone(),
            path: record.path.clone(),
            previous_status: record.status,
            status: request.target_status,
            reason: request.reason,
            score: request.score,
            writes_skills: true,
            deletes_skill: false,
        })
    }

    /// Stage 1 of the evolver outer loop: detect repeated-failure patterns in
    /// the observed runtime stream. Pure and read-only.
    pub fn detect_repeated_failures(
        &self,
        config: &FailureDetectorConfig,
    ) -> Result<Vec<FailurePattern>, EvolutionError> {
        if config.min_repeats == 0 {
            return Err(EvolutionError::InvalidScope(
                "min_repeats must be greater than zero".to_string(),
            ));
        }
        if config.failure_kinds.is_empty() {
            return Err(EvolutionError::InvalidScope(
                "failure_kinds must not be empty".to_string(),
            ));
        }
        Ok(RepeatedFailureDetector::new(config.clone()).detect(&self.observed_events))
    }

    /// Stage 2 of the evolver outer loop: turn one detected failure pattern
    /// into a structured, auditable rule-modification proposal. Every cited
    /// evidence event must exist in the observed runtime stream.
    pub fn propose_rule_change(
        &self,
        pattern: &FailurePattern,
    ) -> Result<RuleChangeProposal, EvolutionError> {
        if pattern.signature.trim().is_empty() {
            return Err(EvolutionError::InvalidProposal(
                "pattern signature must not be empty".to_string(),
            ));
        }
        if pattern.count == 0 || pattern.event_ids.is_empty() {
            return Err(EvolutionError::InvalidProposal(
                "pattern must carry failure evidence".to_string(),
            ));
        }
        for event_id in &pattern.event_ids {
            if !self
                .observed_events
                .iter()
                .any(|event| &event.event_id == event_id)
            {
                return Err(EvolutionError::InvalidProposal(format!(
                    "pattern references unknown observed event {event_id}"
                )));
            }
        }

        let title = format!("Rule for {}", pattern.signature);
        let proposal_id = format!(
            "rule-change-{}-{}",
            stable_id_part(&pattern.signature),
            stable_id_part(&pattern.last_seen_event_id)
        );
        let rule_id = canonical_skill_id_for(&title, &proposal_id);
        let old_procedure = self.existing_procedure(&rule_id)?;
        let change_kind = if old_procedure.is_empty() {
            RuleChangeKind::CreateRule
        } else {
            RuleChangeKind::UpdateRule
        };

        let provenance = pattern
            .event_ids
            .iter()
            .filter_map(|event_id| {
                self.observed_events
                    .iter()
                    .find(|event| &event.event_id == event_id)
                    .map(|event| SkillProposalProvenance {
                        source_event_id: event.event_id.clone(),
                        source_task_id: event.task_id.clone(),
                        source_kind: event.kind.clone(),
                        source_summary: event.summary.clone(),
                        source_metadata: event.metadata.clone(),
                    })
            })
            .collect::<Vec<_>>();

        Ok(RuleChangeProposal {
            proposal_id,
            rule_id: rule_id.clone(),
            change_kind,
            title,
            trigger: format!(
                "repeated failure {} observed {} times across {} task(s)",
                pattern.signature,
                pattern.count,
                pattern.task_ids.len()
            ),
            old_procedure,
            new_procedure: default_repair_procedure(&pattern.signature),
            rationale: format!(
                "auto-proposed by the evolver outer loop: repeated failure {} observed {} times across {} task(s); the existing rule needs to change so the failure does not recur",
                pattern.signature,
                pattern.count,
                pattern.task_ids.len()
            ),
            evidence: vec![FailureEvidence::from_pattern(pattern)],
            writes_rules: true,
            requires_governance: true,
            provenance,
        })
    }

    /// Stage 3+4 of the evolver outer loop: run a rule-change proposal through
    /// governance and, only after approval, persist the new rule with the
    /// existing canonical markdown write path. Every applied change is
    /// appended to the audit journal so the loop stays observable and
    /// rollback-able.
    pub fn apply_rule_change(
        &mut self,
        proposal: RuleChangeProposal,
        governance: &dyn RuleChangeGovernance,
        context: &GovernanceContext,
    ) -> Result<RuleChangeReceipt, EvolutionError> {
        validate_rule_change_proposal(&proposal)?;
        let decision = governance.evaluate(&proposal, context)?;
        validate_governance_decision(&decision, &proposal.proposal_id)?;
        if !decision.approved {
            return Err(EvolutionError::ValidationRejected(decision.reasons));
        }

        let skill_proposal = skill_proposal_from_rule_change(&proposal)?;
        let records = self.load_existing_records()?;
        let duplicate = duplicate_decision(&skill_proposal, &records);
        let before = match &duplicate.existing_path {
            Some(path) => Some(fs::read_to_string(path).map_err(storage_error)?),
            None => None,
        };

        let receipt = self.solidify_with_receipt(skill_proposal)?;
        let after = fs::read_to_string(&receipt.path).map_err(storage_error)?;
        let actual_kind = if duplicate.duplicate_found {
            RuleChangeKind::UpdateRule
        } else {
            RuleChangeKind::CreateRule
        };
        let previous_version = if duplicate.duplicate_found {
            Some(receipt.version - 1)
        } else {
            None
        };

        let journal_entry = RuleChangeJournalEntry {
            entry_id: format!("rc-{}", proposal.proposal_id),
            applied_at: Utc::now().to_rfc3339(),
            proposal: proposal.clone(),
            decision: decision.clone(),
            rule_id: receipt.skill_id.clone(),
            path: receipt.path.display().to_string(),
            version: receipt.version,
            previous_version,
            before,
            after,
        };
        self.rule_change_journal().append(&journal_entry)?;

        let rule_change_receipt = RuleChangeReceipt {
            proposal_id: proposal.proposal_id,
            rule_id: receipt.skill_id,
            change_kind: actual_kind,
            path: receipt.path,
            version: receipt.version,
            previous_version,
            decision,
            writes_rules: true,
            deletes_rules: false,
        };
        self.last_rule_change_receipt = Some(rule_change_receipt.clone());
        Ok(rule_change_receipt)
    }

    /// Restore the rule content that existed before a previously applied rule
    /// change, by replaying the change's old procedure through the same
    /// governance-gated write path. Refuses to roll back a rule creation
    /// because that would require deletion.
    pub fn rollback_rule_change(
        &mut self,
        entry_id: &str,
        governance: &dyn RuleChangeGovernance,
        context: &GovernanceContext,
    ) -> Result<RuleChangeReceipt, EvolutionError> {
        if entry_id.trim().is_empty() {
            return Err(EvolutionError::InvalidRuleChange(
                "entry_id must not be empty".to_string(),
            ));
        }
        let entry = self.rule_change_journal().find(entry_id)?.ok_or_else(|| {
            EvolutionError::StorageError(format!("rule change journal entry not found: {entry_id}"))
        })?;
        if entry.proposal.old_procedure.is_empty() {
            return Err(EvolutionError::InvalidRuleChange(format!(
                "refusing to roll back rule creation {}; use retire instead of delete",
                entry.proposal.proposal_id
            )));
        }

        let rollback_proposal = RuleChangeProposal {
            proposal_id: format!("rollback-{}", entry.proposal.proposal_id),
            rule_id: entry.proposal.rule_id,
            change_kind: RuleChangeKind::UpdateRule,
            title: entry.proposal.title,
            trigger: entry.proposal.trigger,
            old_procedure: entry.proposal.new_procedure,
            new_procedure: entry.proposal.old_procedure,
            rationale: format!(
                "rollback of {} applied at {}",
                entry.proposal.proposal_id, entry.applied_at
            ),
            evidence: entry.proposal.evidence,
            writes_rules: true,
            requires_governance: true,
            provenance: entry.proposal.provenance,
        };
        self.apply_rule_change(rollback_proposal, governance, context)
    }

    /// Observable history of every applied rule change (append-only journal).
    pub fn rule_change_history(&self) -> Result<Vec<RuleChangeJournalEntry>, EvolutionError> {
        self.rule_change_journal().load()
    }

    pub fn rule_change_journal_path(&self) -> PathBuf {
        self.skill_root.join(".evolver").join("rule_changes.jsonl")
    }

    fn rule_change_journal(&self) -> RuleChangeJournal {
        RuleChangeJournal::new(self.rule_change_journal_path())
    }

    fn existing_procedure(&self, rule_id: &str) -> Result<Vec<String>, EvolutionError> {
        let records = self.load_existing_records()?;
        let record = records.iter().find(|record| record.skill_id == rule_id);
        match record {
            Some(record) => {
                let content = fs::read_to_string(&record.path).map_err(storage_error)?;
                Ok(extract_procedure_from_markdown(&content))
            }
            None => Ok(Vec::new()),
        }
    }

    fn score(&self, proposal: &SkillProposal) -> Result<SkillScoreCard, EvolutionError> {
        validate_proposal(proposal)?;

        let mut dimensions = vec![
            dimension(
                "frontmatter_quality",
                10,
                required_text_score(
                    10,
                    &[&proposal.proposal_id, &proposal.title, &proposal.trigger],
                ),
                "proposal has stable identity, title, and trigger",
            ),
            dimension(
                "workflow_clarity",
                15,
                if proposal.procedure.len() >= 3 {
                    15
                } else if proposal.procedure.len() >= 2 {
                    10
                } else {
                    5
                },
                "procedure contains enough ordered steps",
            ),
            dimension(
                "boundary_coverage",
                15,
                boundary_score(proposal),
                "procedure names verification, approval, governance, risk, or write boundaries",
            ),
            dimension(
                "checkpoint_design",
                10,
                keyword_score(
                    proposal,
                    10,
                    &["verify", "test", "check", "record", "report"],
                ),
                "procedure includes a checkpoint or verification action",
            ),
            dimension(
                "instruction_specificity",
                15,
                specificity_score(proposal),
                "steps are concrete enough to execute repeatedly",
            ),
            dimension(
                "resource_integration",
                10,
                if proposal.provenance.is_empty() || proposal.evidence_event_ids.is_empty() {
                    0
                } else {
                    10
                },
                "proposal preserves source event provenance",
            ),
            dimension(
                "overall_architecture",
                15,
                if canonical_skill_id_for(&proposal.title, &proposal.proposal_id).len() >= 4 {
                    15
                } else {
                    5
                },
                "proposal maps to a stable canonical skill identity",
            ),
            dimension(
                "real_world_test_performance",
                10,
                if proposal
                    .provenance
                    .iter()
                    .any(|item| !item.source_summary.trim().is_empty())
                {
                    10
                } else {
                    5
                },
                "proposal is grounded in observed runtime evidence",
            ),
        ];
        let total_score = dimensions.iter().map(|dimension| dimension.score).sum();
        let approved = total_score >= self.approval_threshold;
        let mut reasons = dimensions
            .iter()
            .map(|dimension| {
                format!(
                    "{}={}/{}",
                    dimension.name, dimension.score, dimension.max_score
                )
            })
            .collect::<Vec<_>>();
        reasons.push(format!(
            "total_score={} approval_threshold={} approved={}",
            total_score, self.approval_threshold, approved
        ));
        if !approved {
            reasons.push(
                "self approval rejected; keep as proposal for further improvement".to_string(),
            );
        }

        dimensions.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(SkillScoreCard {
            total_score,
            approval_threshold: self.approval_threshold,
            approved,
            dimensions,
            reasons,
        })
    }

    fn load_existing_records(&self) -> Result<Vec<StoredSkillRecord>, EvolutionError> {
        if !self.skill_root.exists() {
            return Ok(Vec::new());
        }

        let mut records = Vec::new();
        let entries = fs::read_dir(&self.skill_root).map_err(storage_error)?;
        for entry in entries {
            let entry = entry.map_err(storage_error)?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let content = fs::read_to_string(&path).map_err(storage_error)?;
            records.push(parse_record(&path, &content));
        }
        records.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
        Ok(records)
    }
}

impl SkillEvolver for CanonicalSkillEvolver {
    fn observe(&mut self, event: RuntimeEvent) -> Result<EvolutionReceipt, EvolutionError> {
        validate_event(&event)?;
        self.observed_events.push(event);
        let overflow = self
            .observed_events
            .len()
            .saturating_sub(OBSERVED_EVENTS_MAX);
        if overflow > 0 {
            // 超存储级上限：丢弃最旧事件并全量重写（保持 bounded）。
            self.observed_events.drain(..overflow);
            if let Err(error) = self.persist_observed_events() {
                eprintln!("observed_events_persist_failed: {error:?}");
            }
        } else {
            // 常规路径：append 单行。
            let last = self.observed_events.last().expect("just pushed");
            if let Err(error) = self.append_observed_event(last) {
                eprintln!("observed_events_persist_failed: {error:?}");
            }
        }

        Ok(EvolutionReceipt {
            accepted: true,
            message: "event recorded; canonical evolver can self-approve and upsert skills"
                .to_string(),
        })
    }

    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>, EvolutionError> {
        validate_scope(&scope)?;

        Ok(self
            .observed_events
            .iter()
            .filter(|event| event_matches_scope(&scope, event))
            .take(scope.max_proposals)
            .map(|event| SkillProposal {
                proposal_id: format!(
                    "canonical-{}-{}",
                    stable_id_part(&scope.agent_id),
                    stable_id_part(&event.event_id)
                ),
                title: format!("{} reusable workflow", scope.agent_id),
                trigger: format!("repeatable task observed in {}", event.task_id),
                procedure: vec![
                    "Review preserved provenance before applying the workflow.".to_string(),
                    format!("Repeat the successful task pattern: {}", event.summary),
                    "Verify the result and record the outcome before maintenance.".to_string(),
                ],
                evidence_event_ids: vec![event.event_id.clone()],
                dry_run: false,
                writes_skills: true,
                requires_approval: false,
                provenance: vec![super::SkillProposalProvenance {
                    source_event_id: event.event_id.clone(),
                    source_task_id: event.task_id.clone(),
                    source_kind: event.kind.clone(),
                    source_summary: event.summary.clone(),
                    source_metadata: event.metadata.clone(),
                }],
            })
            .collect())
    }

    fn validate(&self, proposal: &SkillProposal) -> Result<ValidationReport, EvolutionError> {
        Ok(self.self_approve(proposal)?.receipt.validation_report)
    }

    fn solidify(&mut self, proposal: SkillProposal) -> Result<SkillId, EvolutionError> {
        let receipt = self.solidify_with_receipt(proposal)?;
        Ok(SkillId(receipt.skill_id))
    }
}

#[derive(Debug, Clone)]
struct StoredSkillRecord {
    skill_id: String,
    title: String,
    trigger: String,
    version: u32,
    status: SkillLifecycleStatus,
    path: PathBuf,
}

fn duplicate_decision(
    proposal: &SkillProposal,
    existing_records: &[StoredSkillRecord],
) -> DuplicateDecision {
    let candidate_id = canonical_skill_id_for(&proposal.title, &proposal.proposal_id);
    let candidate_title = normalize_match_text(&proposal.title);
    let candidate_trigger = normalize_match_text(&proposal.trigger);

    for record in existing_records {
        if record.skill_id == candidate_id {
            return DuplicateDecision {
                duplicate_found: true,
                canonical_skill_id: record.skill_id.clone(),
                existing_path: Some(record.path.clone()),
                reason: "canonical_id_match".to_string(),
            };
        }
        if !candidate_title.is_empty() && normalize_match_text(&record.title) == candidate_title {
            return DuplicateDecision {
                duplicate_found: true,
                canonical_skill_id: record.skill_id.clone(),
                existing_path: Some(record.path.clone()),
                reason: "normalized_title_match".to_string(),
            };
        }
        if !candidate_trigger.is_empty()
            && normalize_match_text(&record.trigger) == candidate_trigger
        {
            return DuplicateDecision {
                duplicate_found: true,
                canonical_skill_id: record.skill_id.clone(),
                existing_path: Some(record.path.clone()),
                reason: "normalized_trigger_match".to_string(),
            };
        }
    }

    DuplicateDecision {
        duplicate_found: false,
        canonical_skill_id: candidate_id,
        existing_path: None,
        reason: "new_canonical_skill".to_string(),
    }
}

fn render_skill_markdown(
    skill_id: &str,
    version: u32,
    status: SkillLifecycleStatus,
    proposal: &SkillProposal,
    decision: &SkillSelfApprovalDecision,
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("skill_id: {}\n", skill_id));
    out.push_str(&format!("canonical_id: {}\n", skill_id));
    out.push_str(&format!("title: {}\n", yaml_scalar(&proposal.title)));
    out.push_str(&format!("trigger: {}\n", yaml_scalar(&proposal.trigger)));
    out.push_str(&format!("version: {}\n", version));
    out.push_str(&format!("status: {}\n", status.as_str()));
    out.push_str(&format!("score: {}\n", decision.scorecard.total_score));
    out.push_str(&format!("approval_source: {}\n", decision.approval_source));
    out.push_str("source_proposal_ids:\n");
    out.push_str(&format!("  - {}\n", proposal.proposal_id));
    out.push_str("evidence_event_ids:\n");
    for event_id in &proposal.evidence_event_ids {
        out.push_str(&format!("  - {}\n", event_id));
    }
    out.push_str("---\n\n");
    out.push_str(&format!("# {}\n\n", proposal.title));
    out.push_str("## Trigger\n\n");
    out.push_str(&proposal.trigger);
    out.push_str("\n\n## Procedure\n\n");
    for step in &proposal.procedure {
        out.push_str(&format!("- {}\n", step));
    }
    out.push_str("\n## Provenance\n\n");
    for item in &proposal.provenance {
        out.push_str(&format!(
            "- event={} task={} kind={:?}\n",
            item.source_event_id, item.source_task_id, item.source_kind
        ));
    }
    out.push_str("\n## Maintenance\n\n");
    out.push_str(&format!(
        "- status={} version={} score={}\n",
        status.as_str(),
        version,
        decision.scorecard.total_score
    ));
    out.push_str("- duplicate policy: update the canonical skill instead of creating copies.\n");
    out.push_str(
        "- retirement policy: deprecate or retire in place; never delete skill history.\n",
    );
    out
}

fn update_lifecycle_frontmatter(
    existing: &str,
    record: &StoredSkillRecord,
    target_status: SkillLifecycleStatus,
    reason: &str,
    score: Option<u16>,
) -> String {
    let mut lines = existing.lines().map(String::from).collect::<Vec<_>>();
    if lines.first().map(String::as_str) == Some("---") {
        let mut in_frontmatter = true;
        let mut saw_status = false;
        let mut saw_version = false;
        let mut insert_at = 1;
        for (index, line) in lines.iter_mut().enumerate().skip(1) {
            if line == "---" {
                insert_at = index;
                in_frontmatter = false;
                break;
            }
            if let Some((key, _)) = line.split_once(':') {
                match key.trim() {
                    "status" => {
                        *line = format!("status: {}", target_status.as_str());
                        saw_status = true;
                    }
                    "version" => {
                        *line = format!("version: {}", record.version + 1);
                        saw_version = true;
                    }
                    _ => {}
                }
            }
        }
        if !in_frontmatter {
            let mut additions = Vec::new();
            if !saw_status {
                additions.push(format!("status: {}", target_status.as_str()));
            }
            if !saw_version {
                additions.push(format!("version: {}", record.version + 1));
            }
            additions.push(format!("retirement_reason: {}", yaml_scalar(reason)));
            if let Some(score) = score {
                additions.push(format!("retirement_score: {}", score));
            }
            for addition in additions.into_iter().rev() {
                lines.insert(insert_at, addition);
            }
            let mut updated = lines.join("\n");
            updated.push('\n');
            return updated;
        }
    }

    let mut updated = String::new();
    updated.push_str("---\n");
    updated.push_str(&format!("skill_id: {}\n", record.skill_id));
    updated.push_str(&format!("canonical_id: {}\n", record.skill_id));
    updated.push_str(&format!("title: {}\n", yaml_scalar(&record.title)));
    updated.push_str(&format!("trigger: {}\n", yaml_scalar(&record.trigger)));
    updated.push_str(&format!("version: {}\n", record.version + 1));
    updated.push_str(&format!("status: {}\n", target_status.as_str()));
    updated.push_str(&format!("retirement_reason: {}\n", yaml_scalar(reason)));
    if let Some(score) = score {
        updated.push_str(&format!("retirement_score: {}\n", score));
    }
    updated.push_str("---\n\n");
    updated.push_str(existing);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}

fn parse_record(path: &Path, content: &str) -> StoredSkillRecord {
    let mut skill_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "skill".to_string());
    let mut title = heading_title(content).unwrap_or_else(|| skill_id.replace('-', " "));
    let mut trigger = String::new();
    let mut version = 1;
    let mut status = SkillLifecycleStatus::Active;

    if let Some(frontmatter) = frontmatter_lines(content) {
        for line in frontmatter {
            if let Some((key, value)) = line.split_once(':') {
                let value = unquote_yaml_scalar(value.trim());
                match key.trim() {
                    "skill_id" | "canonical_id" => skill_id = stable_id_part(&value),
                    "title" => title = value,
                    "trigger" => trigger = value,
                    "version" => version = value.parse::<u32>().unwrap_or(1),
                    "status" => status = SkillLifecycleStatus::from_str(&value),
                    _ => {}
                }
            }
        }
    }

    StoredSkillRecord {
        skill_id,
        title,
        trigger,
        version,
        status,
        path: path.to_path_buf(),
    }
}

fn frontmatter_lines(content: &str) -> Option<Vec<&str>> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut frontmatter = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(frontmatter);
        }
        frontmatter.push(line);
    }
    None
}

fn heading_title(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    })
}

fn dimension(name: &str, max_score: u16, score: u16, reason: &str) -> SkillScoreDimension {
    SkillScoreDimension {
        name: name.to_string(),
        score: score.min(max_score),
        max_score,
        reason: reason.to_string(),
    }
}

fn required_text_score(max_score: u16, values: &[&str]) -> u16 {
    if values.iter().all(|value| !value.trim().is_empty()) {
        max_score
    } else {
        0
    }
}

fn boundary_score(proposal: &SkillProposal) -> u16 {
    let text = proposal_text(proposal);
    let keywords = [
        "approval",
        "approve",
        "governance",
        "risk",
        "boundary",
        "secret",
        "delete",
        "write",
        "solidify",
        "verify",
        "test",
        "audit",
    ];
    let hits = keywords
        .iter()
        .filter(|keyword| text.contains(**keyword))
        .count();
    match hits {
        0 => 5,
        1 => 10,
        _ => 15,
    }
}

fn keyword_score(proposal: &SkillProposal, max_score: u16, keywords: &[&str]) -> u16 {
    let text = proposal_text(proposal);
    if keywords.iter().any(|keyword| text.contains(*keyword)) {
        max_score
    } else {
        max_score / 2
    }
}

fn specificity_score(proposal: &SkillProposal) -> u16 {
    let average_len = proposal
        .procedure
        .iter()
        .map(|step| step.trim().len())
        .sum::<usize>()
        / proposal.procedure.len().max(1);
    if average_len >= 24 {
        15
    } else if average_len >= 12 {
        10
    } else {
        5
    }
}

fn proposal_text(proposal: &SkillProposal) -> String {
    let mut parts = vec![proposal.title.as_str(), proposal.trigger.as_str()];
    parts.extend(proposal.procedure.iter().map(String::as_str));
    parts.join(" ").to_ascii_lowercase()
}

fn event_matches_scope(scope: &EvolutionScope, event: &RuntimeEvent) -> bool {
    match &scope.task_kind {
        Some(task_kind) => match event.metadata.get("task_kind") {
            Some(event_task_kind) => event_task_kind == task_kind,
            None => true,
        },
        None => true,
    }
}

fn canonical_skill_id_for(title: &str, fallback: &str) -> String {
    let from_title = stable_id_part(&normalize_match_text(title));
    if from_title == "skill" {
        stable_id_part(fallback)
    } else {
        from_title
    }
}

fn normalize_match_text(value: &str) -> String {
    let mut words = BTreeSet::new();
    for word in value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|word| !word.is_empty())
    {
        words.insert(word.to_ascii_lowercase());
    }
    words.into_iter().collect::<Vec<_>>().join("-")
}

fn stable_id_part(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    let collapsed = normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "skill".to_string()
    } else {
        collapsed
    }
}

fn yaml_scalar(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn unquote_yaml_scalar(value: &str) -> String {
    value
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

fn storage_error(err: std::io::Error) -> EvolutionError {
    EvolutionError::StorageError(err.to_string())
}

/// Bridge a governance-approved rule change into the existing canonical skill
/// proposal shape so it is written through the exact same markdown storage
/// format as ordinary skills (same frontmatter, duplicate handling, version).
fn skill_proposal_from_rule_change(
    proposal: &RuleChangeProposal,
) -> Result<SkillProposal, EvolutionError> {
    validate_rule_change_proposal(proposal)?;
    let mut evidence_event_ids = Vec::new();
    for evidence in &proposal.evidence {
        for event_id in &evidence.event_ids {
            if !evidence_event_ids.contains(event_id) {
                evidence_event_ids.push(event_id.clone());
            }
        }
    }
    Ok(SkillProposal {
        proposal_id: format!("rule-change-{}", proposal.proposal_id),
        title: proposal.title.clone(),
        trigger: proposal.trigger.clone(),
        procedure: proposal.new_procedure.clone(),
        evidence_event_ids,
        dry_run: false,
        writes_skills: true,
        requires_approval: false,
        provenance: proposal.provenance.clone(),
    })
}

/// Default repair procedure produced by the evolver outer loop. It is written
/// to be concrete enough to execute repeatedly and to keep governance/verify
/// boundaries visible, so the canonical darwin self-approval gate stays
/// meaningful as a second quality gate after governance.
fn default_repair_procedure(signature: &str) -> Vec<String> {
    vec![
        "Review the repeated failure evidence and the existing rule before changing it.".to_string(),
        format!(
            "Apply the corrective procedure for {signature}: retry with the recorded fallback and capture the outcome."
        ),
        "Verify the fix with a check or test, record the result, and keep governance and approval boundaries visible.".to_string(),
    ]
}

/// Extract the bullet steps under `## Procedure` from an existing rule file so
/// an update proposal can carry the auditable old procedure.
fn extract_procedure_from_markdown(content: &str) -> Vec<String> {
    let mut in_procedure = false;
    let mut steps = Vec::new();
    for line in content.lines() {
        if let Some(section) = line.strip_prefix("## ") {
            in_procedure = section.trim() == "Procedure";
            continue;
        }
        if in_procedure {
            if let Some(step) = line.strip_prefix("- ") {
                steps.push(step.trim().to_string());
            }
        }
    }
    steps
}
