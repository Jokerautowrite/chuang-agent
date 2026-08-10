use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{validate_rule_change_proposal, EvolutionError, RuntimeEvent, SkillProposalProvenance};
use crate::skill_evolver::failure::{FailureDetectorConfig, FailurePattern};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleChangeKind {
    CreateRule,
    UpdateRule,
}

/// One auditable piece of evidence cited by a rule-change proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureEvidence {
    pub pattern_signature: String,
    pub count: usize,
    pub event_ids: Vec<String>,
    pub task_ids: Vec<String>,
    pub summary: String,
}

impl FailureEvidence {
    pub fn from_pattern(pattern: &FailurePattern) -> Self {
        Self {
            pattern_signature: pattern.signature.clone(),
            count: pattern.count,
            event_ids: pattern.event_ids.clone(),
            task_ids: pattern.task_ids.clone(),
            summary: pattern.summary.clone(),
        }
    }
}

/// Structured, auditable rule-modification proposal produced by the evolver
/// outer loop. It is a review artifact: nothing is written until governance
/// approves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleChangeProposal {
    pub proposal_id: String,
    pub rule_id: String,
    pub change_kind: RuleChangeKind,
    pub title: String,
    pub trigger: String,
    pub old_procedure: Vec<String>,
    pub new_procedure: Vec<String>,
    pub rationale: String,
    pub evidence: Vec<FailureEvidence>,
    pub writes_rules: bool,
    pub requires_governance: bool,
    pub provenance: Vec<SkillProposalProvenance>,
}

/// Read-only context handed to governance so a decision can verify the
/// proposal's evidence against the actual observed runtime stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceContext {
    pub observed_events: Vec<RuntimeEvent>,
    pub detector_config: FailureDetectorConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceDecision {
    pub proposal_id: String,
    pub approved: bool,
    pub reasons: Vec<String>,
    pub approval_source: String,
    pub decided_by: String,
    pub decided_at: Option<String>,
}

/// Governance slot of the evolver outer loop. An implementation decides
/// whether a rule-change proposal may be written; it never writes itself.
pub trait RuleChangeGovernance {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
        context: &GovernanceContext,
    ) -> Result<GovernanceDecision, EvolutionError>;
}

/// Deterministic policy governance: approves only when the proposal is
/// structurally valid and every cited failure event can be verified against
/// the observed runtime stream with enough repeats. No model judgment, no
/// silent fallback: any missing or weak evidence rejects the proposal.
#[derive(Debug, Clone)]
pub struct PolicyRuleChangeGovernance {
    pub approval_source: String,
}

impl Default for PolicyRuleChangeGovernance {
    fn default() -> Self {
        Self {
            approval_source: "policy:repeated_failure_evidence_v1".to_string(),
        }
    }
}

impl RuleChangeGovernance for PolicyRuleChangeGovernance {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
        context: &GovernanceContext,
    ) -> Result<GovernanceDecision, EvolutionError> {
        validate_rule_change_proposal(proposal)?;

        let mut reasons = Vec::new();
        let mut approved = true;

        let mut verified_event_ids = Vec::new();
        for evidence in &proposal.evidence {
            if evidence.event_ids.is_empty() {
                reasons.push(format!(
                    "evidence for {} carries no event ids",
                    evidence.pattern_signature
                ));
                approved = false;
                continue;
            }
            if evidence.count < context.detector_config.min_repeats {
                reasons.push(format!(
                    "evidence for {} has count {} below min_repeats {}",
                    evidence.pattern_signature, evidence.count, context.detector_config.min_repeats
                ));
                approved = false;
            }
            for event_id in &evidence.event_ids {
                let event = context
                    .observed_events
                    .iter()
                    .find(|event| &event.event_id == event_id);
                match event {
                    Some(event) if context.detector_config.failure_kinds.contains(&event.kind) => {
                        verified_event_ids.push(event_id.clone());
                    }
                    Some(_) => {
                        reasons.push(format!(
                            "evidence event {event_id} exists but is not a configured failure kind"
                        ));
                        approved = false;
                    }
                    None => {
                        reasons.push(format!(
                            "evidence event {event_id} not found in observed runtime events"
                        ));
                        approved = false;
                    }
                }
            }
        }

        if verified_event_ids.is_empty() {
            reasons.push("no verified failure evidence against the runtime stream".to_string());
            approved = false;
        }

        if approved {
            reasons.push(format!(
                "verified {} evidence event(s) against the observed runtime stream",
                verified_event_ids.len()
            ));
            reasons.push(format!(
                "rule change {} targets {} as {}",
                proposal.proposal_id,
                proposal.rule_id,
                match proposal.change_kind {
                    RuleChangeKind::CreateRule => "create",
                    RuleChangeKind::UpdateRule => "update",
                }
            ));
        } else {
            reasons.push("governance rejected; nothing is written to disk".to_string());
        }

        Ok(GovernanceDecision {
            proposal_id: proposal.proposal_id.clone(),
            approved,
            reasons,
            approval_source: self.approval_source.clone(),
            decided_by: "governance.policy".to_string(),
            decided_at: Some(Utc::now().to_rfc3339()),
        })
    }
}

/// Fake governance slot: never approves, so it can never write rules. Used as
/// the safe default before a stronger real adapter is stable.
#[derive(Debug, Clone, Default)]
pub struct NoopRuleChangeGovernance;

impl RuleChangeGovernance for NoopRuleChangeGovernance {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
        _context: &GovernanceContext,
    ) -> Result<GovernanceDecision, EvolutionError> {
        validate_rule_change_proposal(proposal)?;
        Ok(GovernanceDecision {
            proposal_id: proposal.proposal_id.clone(),
            approved: false,
            reasons: vec!["noop governance never approves rule changes".to_string()],
            approval_source: "noop_governance".to_string(),
            decided_by: "governance.noop".to_string(),
            decided_at: Some(Utc::now().to_rfc3339()),
        })
    }
}

/// Receipt returned after an approved rule change has been persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleChangeReceipt {
    pub proposal_id: String,
    pub rule_id: String,
    pub change_kind: RuleChangeKind,
    pub path: PathBuf,
    pub version: u32,
    pub previous_version: Option<u32>,
    pub decision: GovernanceDecision,
    pub writes_rules: bool,
    pub deletes_rules: bool,
}

/// Append-only audit entry persisted for every applied rule change. It keeps
/// the full before/after rule content so the outer loop stays observable and
/// rollback-able without relying on external state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleChangeJournalEntry {
    pub entry_id: String,
    pub applied_at: String,
    pub proposal: RuleChangeProposal,
    pub decision: GovernanceDecision,
    pub rule_id: String,
    pub path: String,
    pub version: u32,
    pub previous_version: Option<u32>,
    pub before: Option<String>,
    pub after: String,
}

/// JSONL journal under the skill root (`.evolver/rule_changes.jsonl`) that
/// records every approved rule change. The rules themselves are persisted in
/// the existing markdown skill format; this journal is the audit/rollback
/// trail of the outer loop.
#[derive(Debug, Clone)]
pub struct RuleChangeJournal {
    path: PathBuf,
}

impl RuleChangeJournal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, entry: &RuleChangeJournalEntry) -> Result<(), EvolutionError> {
        if entry.entry_id.trim().is_empty() {
            return Err(EvolutionError::InvalidRuleChange(
                "journal entry_id must not be empty".to_string(),
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            EvolutionError::StorageError("journal path has no parent".to_string())
        })?;
        fs::create_dir_all(parent).map_err(storage_error)?;
        let line = serde_json::to_string(entry).map_err(serialization_error)?;
        let mut contents = if self.path.exists() {
            fs::read_to_string(&self.path).map_err(storage_error)?
        } else {
            String::new()
        };
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&line);
        contents.push('\n');
        fs::write(&self.path, contents).map_err(storage_error)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<RuleChangeJournalEntry>, EvolutionError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let contents = fs::read_to_string(&self.path).map_err(storage_error)?;
        let mut entries = Vec::new();
        for (index, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: RuleChangeJournalEntry = serde_json::from_str(line).map_err(|err| {
                EvolutionError::StorageError(format!("corrupt journal line {}: {}", index + 1, err))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub fn find(&self, entry_id: &str) -> Result<Option<RuleChangeJournalEntry>, EvolutionError> {
        Ok(self
            .load()?
            .into_iter()
            .find(|entry| entry.entry_id == entry_id))
    }
}

fn storage_error(err: std::io::Error) -> EvolutionError {
    EvolutionError::StorageError(err.to_string())
}

fn serialization_error(err: serde_json::Error) -> EvolutionError {
    EvolutionError::StorageError(format!("journal serialization failed: {err}"))
}
