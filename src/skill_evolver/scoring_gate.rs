//! `skill_evolver::scoring_gate` 模块。评分门禁：改技能必须跑分、分数严格提升
//! 才 upsert，未达标进候选池，变更前自动快照（复用 RuleChangeJournal 的
//! before/after 回滚机制）。公开接口：struct SkillScoringGateConfig,
//! NoBaselinePolicy, SkillBenchmarkScore, BenchmarkEvaluatorScorer,
//! FixedScoreScorer, ScoringGateDecision, BenchmarkScoreGate, CandidatePoolEntry,
//! SkillCandidatePool, SkillChangeSnapshot；trait SkillProposalScorer,
//! SkillScoringGate；fn verify_proposal_statement_rubric_isolation。
//!
//! 对照蓝本（docs/reference-dig-20260810.md §2.1 / §4 P1.4）四条硬规则：
//! 1. **statement/rubric 隔离**：提案（Target）只看题面 statement；评分标准
//!    rubric 私有（0600）落盘，复用现有 benchmark 框架的隔离模式，门禁每次
//!    校验隔离不变量，破坏即 fail-closed。
//! 2. **无基线不优化**：scoreboard 没有已记录基线时，不接受「优化」类升级
//!    （UpdateRule），只允许首次登记（CreateRule）。
//! 3. **分数严格提升才接受**：提案先跑分（现有 BenchmarkEvaluator 或本模块
//!    评分器），分数必须严格高于当前基线才 upsert；未达标进候选池（记录但
//!    不落盘为正式规则）。
//! 4. **快照回滚**：变更前自动快照；回滚复用 RuleChangeJournal 的 before/after。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::rule_change::{RuleChangeKind, RuleChangeProposal};
use super::EvolutionError;
use crate::benchmark::BenchmarkStore;
use crate::benchmark_evaluator::{BenchmarkEvaluator, CaseAnswer, EvaluateRequest};

/// 评分门禁配置：绑定一个 benchmark（statement 公开 / rubric 0600 私有）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillScoringGateConfig {
    pub benchmark_id: String,
    pub benchmark_root: PathBuf,
    pub no_baseline_policy: NoBaselinePolicy,
}

impl SkillScoringGateConfig {
    pub fn new(benchmark_id: impl Into<String>, benchmark_root: impl Into<PathBuf>) -> Self {
        Self {
            benchmark_id: benchmark_id.into(),
            benchmark_root: benchmark_root.into(),
            no_baseline_policy: NoBaselinePolicy::default(),
        }
    }
}

/// 无基线时的门禁策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoBaselinePolicy {
    /// 无基线不优化：只允许首次登记（CreateRule）；UpdateRule（优化）拒绝，
    /// 不盲目改技能。
    RejectOptimization,
}

impl Default for NoBaselinePolicy {
    fn default() -> Self {
        Self::RejectOptimization
    }
}

impl NoBaselinePolicy {
    /// 无基线时是否允许该变更类型作为首次登记放行。
    fn allows_first_registration(self, change_kind: RuleChangeKind) -> bool {
        match self {
            Self::RejectOptimization => matches!(change_kind, RuleChangeKind::CreateRule),
        }
    }
}

/// 一次评分结果（0..max_score 固定刻度，与 scoreboard 语义一致）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillBenchmarkScore {
    pub benchmark_id: String,
    pub total_score: u16,
    pub max_score: u16,
    pub run_id: Option<String>,
}

/// 评分器槽位：给一个规则修改提案跑分。实现可以是现有 BenchmarkEvaluator
/// （真实模型评审，rubric 只给评审员看）或确定性评分器（离线/测试）。
pub trait SkillProposalScorer {
    fn score(&self, proposal: &RuleChangeProposal) -> Result<SkillBenchmarkScore, EvolutionError>;
}

/// 复用现有 BenchmarkEvaluator 的评分器：把提案渲染成每个 case 的 Target
/// 回答，由 evaluator 按私有 rubric（0600）评审。statement/rubric 隔离沿用
/// benchmark 框架，Target（提案）看不到 rubric。
pub struct BenchmarkEvaluatorScorer {
    evaluator: BenchmarkEvaluator,
    store: BenchmarkStore,
    benchmark_id: String,
}

impl BenchmarkEvaluatorScorer {
    pub fn new(
        evaluator: BenchmarkEvaluator,
        store: BenchmarkStore,
        benchmark_id: impl Into<String>,
    ) -> Self {
        Self {
            evaluator,
            store,
            benchmark_id: benchmark_id.into(),
        }
    }

    pub fn benchmark_id(&self) -> &str {
        &self.benchmark_id
    }
}

impl SkillProposalScorer for BenchmarkEvaluatorScorer {
    fn score(&self, proposal: &RuleChangeProposal) -> Result<SkillBenchmarkScore, EvolutionError> {
        let def = self.store.load_def(&self.benchmark_id).map_err(|e| {
            EvolutionError::StorageError(format!("scorer load def failed: {}", e.0))
        })?;
        let answers = def
            .cases
            .iter()
            .map(|case| CaseAnswer {
                case_id: case.id.clone(),
                answer: render_proposal_answer(proposal),
            })
            .collect::<Vec<_>>();
        let receipt = self
            .evaluator
            .evaluate(&EvaluateRequest {
                benchmark_id: self.benchmark_id.clone(),
                answers,
                dry_run: false,
            })
            .map_err(|e| EvolutionError::StorageError(format!("scorer evaluate failed: {e}")))?;
        Ok(SkillBenchmarkScore {
            benchmark_id: self.benchmark_id.clone(),
            total_score: receipt.case_scores.iter().map(|c| c.score).sum(),
            max_score: receipt.case_scores.iter().map(|c| c.max_score).sum(),
            run_id: None,
        })
    }
}

/// 确定性评分器：返回调用方给定的分数。用于离线门禁（分数已由外部评测
/// 记录）与测试，不调用任何模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedScoreScorer {
    pub benchmark_id: String,
    pub total_score: u16,
    pub max_score: u16,
    pub run_id: Option<String>,
}

impl FixedScoreScorer {
    pub fn new(benchmark_id: impl Into<String>, total_score: u16, max_score: u16) -> Self {
        Self {
            benchmark_id: benchmark_id.into(),
            total_score,
            max_score,
            run_id: None,
        }
    }
}

impl SkillProposalScorer for FixedScoreScorer {
    fn score(&self, _proposal: &RuleChangeProposal) -> Result<SkillBenchmarkScore, EvolutionError> {
        Ok(SkillBenchmarkScore {
            benchmark_id: self.benchmark_id.clone(),
            total_score: self.total_score,
            max_score: self.max_score,
            run_id: self.run_id.clone(),
        })
    }
}

/// 一次评分门禁判定结果（可审计、可序列化）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoringGateDecision {
    pub proposal_id: String,
    pub rule_id: String,
    pub change_kind: RuleChangeKind,
    pub admitted: bool,
    pub baseline_score: Option<u16>,
    pub after_score: Option<u16>,
    pub reasons: Vec<String>,
}

/// 评分门禁槽位：只做判定，不写任何文件。admitted=true 表示可以继续走既有
/// 写路径；admitted=false 表示未达标（调用方负责记录候选池）。
pub trait SkillScoringGate {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
    ) -> Result<ScoringGateDecision, EvolutionError>;
}

/// 具体评分门禁（绑定一个 benchmark 的 scoreboard 作为基线）：
/// 1. statement/rubric 隔离校验，破坏即 Err（fail-closed）；
/// 2. 无基线不优化：无基线时只放行首次登记（CreateRule），UpdateRule 拒绝；
/// 3. 有基线时先跑分，分数严格高于基线才 admitted。
pub struct BenchmarkScoreGate {
    config: SkillScoringGateConfig,
    store: BenchmarkStore,
    scorer: Box<dyn SkillProposalScorer>,
}

impl BenchmarkScoreGate {
    pub fn new(config: SkillScoringGateConfig, scorer: Box<dyn SkillProposalScorer>) -> Self {
        let store = BenchmarkStore::new(&config.benchmark_root);
        Self {
            config,
            store,
            scorer,
        }
    }

    pub fn config(&self) -> &SkillScoringGateConfig {
        &self.config
    }

    pub fn store(&self) -> &BenchmarkStore {
        &self.store
    }
}

impl SkillScoringGate for BenchmarkScoreGate {
    fn evaluate(
        &self,
        proposal: &RuleChangeProposal,
    ) -> Result<ScoringGateDecision, EvolutionError> {
        // 1. statement/rubric 隔离（防作弊）：任何泄漏都是配置/提案缺陷，
        //    直接 fail-closed，不进候选池（分数本身已不可信）。
        let isolation_issues = verify_proposal_statement_rubric_isolation(
            &self.store,
            &self.config.benchmark_id,
            proposal,
        )?;
        if !isolation_issues.is_empty() {
            return Err(EvolutionError::InvalidRuleChange(format!(
                "scoring gate isolation broken: {}",
                isolation_issues.join("; ")
            )));
        }

        let board = self
            .store
            .load_scoreboard(&self.config.benchmark_id)
            .map_err(|e| {
                EvolutionError::StorageError(format!(
                    "scoring gate load scoreboard failed: {}",
                    e.0
                ))
            })?;
        let baseline_score = board.best.as_ref().map(|entry| entry.total_score);

        match baseline_score {
            // 2. 无基线不优化：只允许首次登记。
            None => {
                if self
                    .config
                    .no_baseline_policy
                    .allows_first_registration(proposal.change_kind)
                {
                    Ok(ScoringGateDecision {
                        proposal_id: proposal.proposal_id.clone(),
                        rule_id: proposal.rule_id.clone(),
                        change_kind: proposal.change_kind,
                        admitted: true,
                        baseline_score: None,
                        after_score: None,
                        reasons: vec![
                            "no baseline recorded; first registration (CreateRule) admitted without optimization claim"
                                .to_string(),
                        ],
                    })
                } else {
                    Ok(ScoringGateDecision {
                        proposal_id: proposal.proposal_id.clone(),
                        rule_id: proposal.rule_id.clone(),
                        change_kind: proposal.change_kind,
                        admitted: false,
                        baseline_score: None,
                        after_score: None,
                        reasons: vec![
                            "no baseline -> no optimize: UpdateRule requires a recorded baseline; not changing the skill blindly"
                                .to_string(),
                        ],
                    })
                }
            }
            // 3. 分数严格提升才接受。
            Some(baseline) => {
                let after = self.scorer.score(proposal)?;
                let admitted = after.total_score > baseline;
                let mut reasons = vec![format!(
                    "baseline={} after_score={} max={}",
                    baseline, after.total_score, after.max_score
                )];
                if admitted {
                    reasons.push(format!(
                        "score {} strictly exceeds baseline {}; upsert admitted",
                        after.total_score, baseline
                    ));
                } else {
                    reasons.push(format!(
                        "score {} does not strictly exceed baseline {}; not upserted",
                        after.total_score, baseline
                    ));
                    reasons.push(
                        "candidate pool records the proposal without persisting a formal rule"
                            .to_string(),
                    );
                }
                Ok(ScoringGateDecision {
                    proposal_id: proposal.proposal_id.clone(),
                    rule_id: proposal.rule_id.clone(),
                    change_kind: proposal.change_kind,
                    admitted,
                    baseline_score: Some(baseline),
                    after_score: Some(after.total_score),
                    reasons,
                })
            }
        }
    }
}

/// 校验 statement/rubric 隔离不变量：
/// - benchmark 公共定义（benchmark.json）只含 statement，不含 rubric；
/// - rubric 文件存在且非空（0600 私有由 benchmark 框架 write_def 保证）；
/// - 提案（Target 侧可见文本）不得内嵌任何 rubric 内容（防作弊）。
pub fn verify_proposal_statement_rubric_isolation(
    store: &BenchmarkStore,
    benchmark_id: &str,
    proposal: &RuleChangeProposal,
) -> Result<Vec<String>, EvolutionError> {
    let def = store.load_def(benchmark_id).map_err(|e| {
        EvolutionError::StorageError(format!("scoring gate load def failed: {}", e.0))
    })?;
    let mut issues = store.verify(benchmark_id).map_err(|e| {
        EvolutionError::StorageError(format!("scoring gate verify failed: {}", e.0))
    })?;
    let proposal_text = render_proposal_answer(proposal);
    for case in &def.cases {
        if let Ok(rubric) = store.read_rubric(benchmark_id, &case.id) {
            let rubric_text = rubric.trim();
            if !rubric_text.is_empty() && proposal_text.contains(rubric_text) {
                issues.push(format!("proposal embeds rubric text for case {}", case.id));
            }
        }
    }
    Ok(issues)
}

/// 把规则修改提案渲染成 Target 侧回答文本（只含题面可见信息，不含 rubric）。
fn render_proposal_answer(proposal: &RuleChangeProposal) -> String {
    let mut out = format!(
        "规则修改提案：{}\n触发条件：{}\n新流程：\n",
        proposal.title, proposal.trigger
    );
    for (index, step) in proposal.new_procedure.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, step));
    }
    out.push_str(&format!("理由：{}\n", proposal.rationale));
    out
}

/// 未达标提案的候选池条目：记录但不落盘为正式规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidatePoolEntry {
    pub entry_id: String,
    pub recorded_at: String,
    pub proposal: RuleChangeProposal,
    pub decision: ScoringGateDecision,
}

/// JSONL 候选池（`.evolver/candidates.jsonl`，与规则修改审计 journal 同目录）。
/// 未通过评分门禁的提案在这里留档供下次诊断，永不写入正式规则文件。
#[derive(Debug, Clone)]
pub struct SkillCandidatePool {
    path: PathBuf,
}

impl SkillCandidatePool {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, entry: &CandidatePoolEntry) -> Result<(), EvolutionError> {
        if entry.entry_id.trim().is_empty() {
            return Err(EvolutionError::InvalidRuleChange(
                "candidate entry_id must not be empty".to_string(),
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            EvolutionError::StorageError("candidate pool path has no parent".to_string())
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

    pub fn load(&self) -> Result<Vec<CandidatePoolEntry>, EvolutionError> {
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
            let entry: CandidatePoolEntry = serde_json::from_str(line).map_err(|err| {
                EvolutionError::StorageError(format!(
                    "corrupt candidate pool line {}: {}",
                    index + 1,
                    err
                ))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

/// 变更前自动快照：捕获目标规则当前内容（CreateRule 无既有内容）。
/// 回滚复用 RuleChangeJournal 的 before/after 语义；本快照在写路径执行前
/// 显式捕获，供审计与 fail-closed（快照读取失败则拒绝继续）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillChangeSnapshot {
    pub rule_id: String,
    pub path: Option<PathBuf>,
    pub before: Option<String>,
    pub captured_at: String,
}

fn storage_error(err: std::io::Error) -> EvolutionError {
    EvolutionError::StorageError(err.to_string())
}

fn serialization_error(err: serde_json::Error) -> EvolutionError {
    EvolutionError::StorageError(format!("candidate pool serialization failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_baseline_policy_allows_only_first_registration() {
        assert!(NoBaselinePolicy::RejectOptimization
            .allows_first_registration(RuleChangeKind::CreateRule));
        assert!(!NoBaselinePolicy::RejectOptimization
            .allows_first_registration(RuleChangeKind::UpdateRule));
    }

    #[test]
    fn fixed_score_scorer_returns_configured_score() {
        let scorer = FixedScoreScorer::new("memory-recall", 7, 10);
        let proposal = RuleChangeProposal {
            proposal_id: "p".to_string(),
            rule_id: "r".to_string(),
            change_kind: RuleChangeKind::UpdateRule,
            title: "t".to_string(),
            trigger: "tr".to_string(),
            old_procedure: vec!["old".to_string()],
            new_procedure: vec!["new".to_string()],
            rationale: "why".to_string(),
            evidence: Vec::new(),
            writes_rules: true,
            requires_governance: true,
            provenance: Vec::new(),
        };
        let score = scorer
            .score(&proposal)
            .expect("fixed scorer should not fail");
        assert_eq!(score.total_score, 7);
        assert_eq!(score.max_score, 10);
        assert_eq!(score.benchmark_id, "memory-recall");
    }

    #[test]
    fn candidate_pool_roundtrip_and_jsonl() {
        let root =
            std::env::temp_dir().join(format!("chuang-candidate-pool-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join(".evolver").join("candidates.jsonl");
        let pool = SkillCandidatePool::new(&path);
        let proposal = RuleChangeProposal {
            proposal_id: "p1".to_string(),
            rule_id: "r1".to_string(),
            change_kind: RuleChangeKind::UpdateRule,
            title: "t".to_string(),
            trigger: "tr".to_string(),
            old_procedure: vec!["old".to_string()],
            new_procedure: vec!["new".to_string()],
            rationale: "why".to_string(),
            evidence: Vec::new(),
            writes_rules: true,
            requires_governance: true,
            provenance: Vec::new(),
        };
        let entry = CandidatePoolEntry {
            entry_id: "cand-p1".to_string(),
            recorded_at: "2026-08-12T00:00:00Z".to_string(),
            proposal: proposal.clone(),
            decision: ScoringGateDecision {
                proposal_id: "p1".to_string(),
                rule_id: "r1".to_string(),
                change_kind: RuleChangeKind::UpdateRule,
                admitted: false,
                baseline_score: Some(6),
                after_score: Some(5),
                reasons: vec!["not improved".to_string()],
            },
        };
        pool.append(&entry).expect("append should work");
        let loaded = pool.load().expect("load should work");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entry_id, "cand-p1");
        assert_eq!(loaded[0].proposal.proposal_id, "p1");
        assert_eq!(loaded[0].decision.baseline_score, Some(6));
        assert!(!loaded[0].decision.admitted);
        assert!(path.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
