use crate::goal_mode::{
    AcceptanceCheck, AcceptanceCheckContract, AcceptanceVerdict, GoalAcceptancePlan,
    GoalCheckpointPolicy, GoalConvergencePolicy, GoalEvidence, GoalSpec, GoalSpecError,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 命令类验收检查的执行超时（秒）。`goal verify` 显式执行验收命令时兜底，
/// 避免声明错误的命令把 operator 卡死。
pub const ACCEPTANCE_COMMAND_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRun {
    pub goal_spec: GoalSpec,
    pub worker_plan: Vec<GoalWorkerPlan>,
    pub disjoint_write_scopes: Vec<GoalWriteScope>,
    pub validation_plan: GoalValidationPlan,
    pub integration_policy: GoalIntegrationPolicy,
    pub checkpoint_log: Vec<GoalCheckpoint>,
    /// Wall-clock start for hard max_minutes budget. Optional for old on-disk runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalWorkerPlan {
    pub worker_id: String,
    pub objective: String,
    pub write_scope_ids: Vec<String>,
    pub validation_checks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalWriteScope {
    pub scope_id: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalValidationPlan {
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalCheckpointWriteback {
    pub manual_only: bool,
    pub update_progress_log: bool,
    pub update_handoff: bool,
    pub commit_checkpoint: bool,
    pub documentation_targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalIntegrationPolicy {
    pub main_process_owns_integration: bool,
    pub workers_may_commit: bool,
    pub workers_may_touch_secrets: bool,
    pub require_worker_reports: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalCheckpoint {
    pub checkpoint_id: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub completed_worker_ids: Vec<String>,
    pub validation_notes: Vec<String>,
    /// 规范化失败原因键（去重键）。同一 blocker_key 尾部连续出现达到
    /// `max_repeated_blockers` 次时判定 blocked。旧 checkpoint JSON 无此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker_key: Option<String>,
    /// 验收证据检查结果（verifier-first）：checkpoint 时由 CLI 对 goal spec
    /// 的 acceptance_evidence 检查文件系统得到。旧 checkpoint JSON 无此字段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_verdicts: Vec<EvidenceVerdict>,
}

/// 单条验收证据的检查结果（证据在模型自述之外，由文件系统判定）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceVerdict {
    /// acceptance_evidence 中的下标。
    pub evidence_index: usize,
    pub path: String,
    pub passed: bool,
    /// passed=false 时说明失败原因；passed=true 时为 "ok"。
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceStatus {
    /// 尚无 checkpoint，无法判定。
    Unknown,
    /// 尾部 checkpoint 无重复卡点，正在收敛。
    Converging,
    /// 尾部出现相同卡点/相同验证结果（重复但未达上限），原地打转风险。
    Spinning,
    /// 同一卡点重复达到上限，判定 blocked，禁止以同策略重试。
    Blocked,
}

/// 收敛判定结果（确定性纯函数输出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvergenceVerdict {
    pub status: ConvergenceStatus,
    /// 触发重复判定的 blocker_key（无时用 validation_notes 指纹）。
    pub repeated_fingerprint: Option<String>,
    /// 尾部连续相同指纹的 checkpoint 数。
    pub repeated_count: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRunDiagnostics {
    pub schema_version: u16,
    pub executes_automatically: bool,
    pub bypasses_governance: bool,
    pub checkpoint_writeback: GoalCheckpointWriteback,
    pub worker_scope_complete: bool,
    pub worker_validation_complete: bool,
    pub validation_plan_complete: bool,
    pub checkpoint_log_complete: bool,
    pub last_checkpoint_id: Option<String>,
    pub last_checkpoint_summary: Option<String>,
    pub last_checkpoint_created_at: Option<String>,
    pub last_checkpoint_completed_worker_ids: Option<Vec<String>>,
    pub last_checkpoint_validation_notes: Option<Vec<String>>,
    pub incomplete_reasons: Vec<String>,
    /// 收敛状态：converging / spinning / blocked / unknown。
    #[serde(default)]
    pub convergence_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convergence_repeated_fingerprint: Option<String>,
    #[serde(default)]
    pub convergence_repeated_count: usize,
    #[serde(default)]
    pub convergence_reason: String,
    /// 是否定义了验收证据（spec.acceptance_evidence 非空）。
    #[serde(default)]
    pub evidence_expected: bool,
    /// 最新 checkpoint 的证据是否全部通过（未定义证据时为 true）。
    #[serde(default)]
    pub evidence_complete: bool,
    /// 未通过/未检查的证据列表（`path: reason`）。
    #[serde(default)]
    pub evidence_missing: Vec<String>,
    /// 执行过证据检查的 checkpoint id（未检查时为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_checked_at_checkpoint: Option<String>,
}

/// 验收证据汇总结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceDiagnostics {
    pub evidence_expected: bool,
    pub evidence_complete: bool,
    pub evidence_missing: Vec<String>,
    pub evidence_checked_at_checkpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRunReceipt {
    pub goal_id: String,
    pub path: String,
    pub checkpoint_count: usize,
    pub checkpoint_writeback: GoalCheckpointWriteback,
    pub last_checkpoint_id: Option<String>,
    pub last_checkpoint_summary: Option<String>,
    pub last_checkpoint_created_at: Option<String>,
    pub last_checkpoint_completed_worker_ids: Option<Vec<String>>,
    pub last_checkpoint_validation_notes: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalRunError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalRunStore {
    root: PathBuf,
}

impl GoalRun {
    pub fn new(
        goal_spec: GoalSpec,
        worker_plan: Vec<GoalWorkerPlan>,
        disjoint_write_scopes: Vec<GoalWriteScope>,
        validation_plan: GoalValidationPlan,
        integration_policy: GoalIntegrationPolicy,
    ) -> Result<Self, GoalRunError> {
        let run = Self {
            goal_spec,
            worker_plan,
            disjoint_write_scopes,
            validation_plan,
            integration_policy,
            checkpoint_log: Vec::new(),
            started_at: Some(current_rfc3339_timestamp()),
        };
        run.validate()?;
        Ok(run)
    }

    /// Hard stop when wall-clock `max_minutes` budget is exhausted.
    /// Returns `Ok(())` if budget unset, `started_at` missing (legacy runs), or still within budget.
    pub fn assert_time_budget_allows_continue(&self) -> Result<(), GoalRunError> {
        let Some(max_minutes) = self.goal_spec.budget.max_minutes else {
            return Ok(());
        };
        let Some(started_at) = self.started_at.as_deref() else {
            // Legacy goal JSON without started_at — do not block mid-flight runs.
            return Ok(());
        };
        let started = chrono::DateTime::parse_from_rfc3339(started_at).map_err(|_| {
            GoalRunError::new(
                "started_at",
                "started_at must be RFC3339 when time budget is enforced",
            )
        })?;
        let elapsed = chrono::Utc::now().signed_duration_since(started.with_timezone(&chrono::Utc));
        let elapsed_minutes = elapsed.num_minutes().max(0) as u64;
        if elapsed_minutes >= u64::from(max_minutes) {
            return Err(GoalRunError::new(
                "budget.max_minutes",
                &format!(
                    "goal time budget exhausted: max_minutes={max_minutes} elapsed_minutes={elapsed_minutes} started_at={started_at}"
                ),
            ));
        }
        Ok(())
    }

    /// Cap how many worker runs a single `goal step` may execute.
    pub fn step_run_cap(&self, requested_max_runs: usize) -> usize {
        let requested = requested_max_runs.max(1);
        match self.goal_spec.budget.max_tool_rounds {
            Some(cap) if cap > 0 => requested.min(cap),
            _ => requested,
        }
    }

    pub fn record_checkpoint(&mut self, checkpoint: GoalCheckpoint) -> Result<(), GoalRunError> {
        checkpoint.validate()?;
        validate_checkpoint_worker_ids(&checkpoint, &self.worker_plan)?;
        if self
            .checkpoint_log
            .iter()
            .any(|existing| existing.checkpoint_id == checkpoint.checkpoint_id)
        {
            return Err(GoalRunError::new(
                "checkpoint_log.checkpoint_id",
                "checkpoint_id must be unique within a goal run",
            ));
        }
        self.checkpoint_log.push(checkpoint);
        Ok(())
    }

    pub fn diagnostics(&self) -> GoalRunDiagnostics {
        let validation_plan_complete = self.validation_plan.validate().is_ok();
        let worker_scope_complete =
            worker_scope_complete(&self.worker_plan, &self.disjoint_write_scopes);
        let worker_validation_complete = self
            .worker_plan
            .iter()
            .all(|worker| !worker.validation_checks.is_empty());
        let last_checkpoint = self.checkpoint_log.last();
        let checkpoint_writeback =
            GoalCheckpointWriteback::from_policy(&self.goal_spec.checkpoint_policy);
        let checkpoint_log_complete = last_checkpoint
            .map(|checkpoint| {
                !checkpoint.completed_worker_ids.is_empty()
                    && !checkpoint.validation_notes.is_empty()
                    && validate_checkpoint_worker_ids(checkpoint, &self.worker_plan).is_ok()
            })
            .unwrap_or(false);
        let mut incomplete_reasons = Vec::new();
        if !worker_scope_complete {
            incomplete_reasons
                .push("worker scopes do not cover declared disjoint scopes".to_string());
        }
        if !worker_validation_complete {
            incomplete_reasons.push("one or more workers have no validation checks".to_string());
        }
        if !validation_plan_complete {
            incomplete_reasons.push("validation plan has no runnable command".to_string());
        }
        let evidence = self.evidence_diagnostics();
        if evidence.evidence_expected && !evidence.evidence_complete {
            incomplete_reasons
                .push("acceptance evidence is missing or not yet checked".to_string());
        }
        if self.checkpoint_log.is_empty() {
            incomplete_reasons.push("no checkpoint has been recorded".to_string());
        } else if !checkpoint_log_complete {
            incomplete_reasons.push(
                "latest checkpoint is missing completed worker evidence, validation notes, or references an unknown worker"
                    .to_string(),
            );
        }

        let convergence = self.convergence_verdict();
        if convergence.status == ConvergenceStatus::Blocked {
            incomplete_reasons.push(convergence.reason.clone());
        }

        GoalRunDiagnostics {
            schema_version: 1,
            executes_automatically: false,
            bypasses_governance: false,
            checkpoint_writeback,
            worker_scope_complete,
            worker_validation_complete,
            validation_plan_complete,
            checkpoint_log_complete,
            last_checkpoint_id: last_checkpoint.map(|checkpoint| checkpoint.checkpoint_id.clone()),
            last_checkpoint_summary: last_checkpoint.map(|checkpoint| checkpoint.summary.clone()),
            last_checkpoint_created_at: last_checkpoint
                .and_then(|checkpoint| checkpoint.created_at.clone()),
            last_checkpoint_completed_worker_ids: last_checkpoint
                .map(|checkpoint| checkpoint.completed_worker_ids.clone()),
            last_checkpoint_validation_notes: last_checkpoint
                .map(|checkpoint| checkpoint.validation_notes.clone()),
            incomplete_reasons,
            convergence_status: convergence_status_string(convergence.status),
            convergence_repeated_fingerprint: convergence.repeated_fingerprint.clone(),
            convergence_repeated_count: convergence.repeated_count,
            convergence_reason: convergence.reason,
            evidence_expected: evidence.evidence_expected,
            evidence_complete: evidence.evidence_complete,
            evidence_missing: evidence.evidence_missing,
            evidence_checked_at_checkpoint: evidence.evidence_checked_at_checkpoint,
        }
    }

    /// 验收证据汇总（verifier-first）：取最新 checkpoint 的证据检查结果。
    /// 未定义证据 → expected=false, complete=true；定义了但最新 checkpoint
    /// 未检查 → complete=false（missing 注明 "not checked"）。
    pub fn evidence_diagnostics(&self) -> EvidenceDiagnostics {
        let evidence_expected = !self.goal_spec.acceptance_evidence.is_empty();
        let last_checkpoint = self.checkpoint_log.last();
        if !evidence_expected {
            return EvidenceDiagnostics {
                evidence_expected: false,
                evidence_complete: true,
                evidence_missing: Vec::new(),
                evidence_checked_at_checkpoint: None,
            };
        }
        let Some(checkpoint) = last_checkpoint else {
            return EvidenceDiagnostics {
                evidence_expected: true,
                evidence_complete: false,
                evidence_missing: vec!["no checkpoint recorded; evidence never checked".to_string()],
                evidence_checked_at_checkpoint: None,
            };
        };
        let checkpoint_id = checkpoint.checkpoint_id.clone();
        let mut missing = Vec::new();
        for verdict in &checkpoint.evidence_verdicts {
            if !verdict.passed {
                missing.push(format!("{}: {}", verdict.path, verdict.reason));
            }
        }
        if checkpoint.evidence_verdicts.is_empty() {
            missing.push(format!(
                "not checked at checkpoint {checkpoint_id}: expected {} evidence item(s)",
                self.goal_spec.acceptance_evidence.len()
            ));
        }
        EvidenceDiagnostics {
            evidence_expected: true,
            evidence_complete: missing.is_empty(),
            evidence_missing: missing,
            evidence_checked_at_checkpoint: Some(checkpoint_id),
        }
    }

    /// 基于当前 checkpoint 日志计算收敛判定（converged vs spinning vs blocked）。
    pub fn convergence_verdict(&self) -> ConvergenceVerdict {
        enforce_convergence_gate(&self.checkpoint_log, &self.goal_spec.convergence_policy)
    }

    pub fn validate(&self) -> Result<(), GoalRunError> {
        self.goal_spec
            .validate()
            .map_err(|error| GoalRunError::new("goal_spec", &format_goal_spec_error(error)))?;
        require_non_empty_vec("worker_plan", self.worker_plan.len())?;
        require_non_empty_vec("disjoint_write_scopes", self.disjoint_write_scopes.len())?;
        if let Some(max_subtasks) = self.goal_spec.budget.max_subtasks {
            if self.worker_plan.len() > max_subtasks {
                return Err(GoalRunError::new(
                    "budget.max_subtasks",
                    "worker plan exceeds goal subtask budget",
                ));
            }
        }
        self.validation_plan.validate()?;
        self.integration_policy.validate()?;
        validate_write_scopes(&self.disjoint_write_scopes)?;
        validate_worker_plan(&self.worker_plan, &self.disjoint_write_scopes)?;

        validate_checkpoint_log_strict(&self.checkpoint_log, &self.worker_plan)?;
        Ok(())
    }

    fn validate_persisted(&self) -> Result<(), GoalRunError> {
        self.goal_spec
            .validate()
            .map_err(|error| GoalRunError::new("goal_spec", &format_goal_spec_error(error)))?;
        require_non_empty_vec("worker_plan", self.worker_plan.len())?;
        require_non_empty_vec("disjoint_write_scopes", self.disjoint_write_scopes.len())?;
        if let Some(max_subtasks) = self.goal_spec.budget.max_subtasks {
            if self.worker_plan.len() > max_subtasks {
                return Err(GoalRunError::new(
                    "budget.max_subtasks",
                    "worker plan exceeds goal subtask budget",
                ));
            }
        }
        self.validation_plan.validate()?;
        self.integration_policy.validate()?;
        validate_write_scopes(&self.disjoint_write_scopes)?;
        validate_worker_plan(&self.worker_plan, &self.disjoint_write_scopes)?;
        validate_checkpoint_log_ids(&self.checkpoint_log)?;
        Ok(())
    }
}

impl GoalRunStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create(&self, run: &GoalRun) -> Result<GoalRunReceipt, GoalRunError> {
        let path = self.goal_path(&run.goal_spec.goal_id)?;
        if path.exists() {
            return Err(GoalRunError::new(
                "goal_id",
                "goal run already exists; use show or checkpoint to continue it",
            ));
        }
        self.write_run(&path, run)
    }

    pub fn load(&self, goal_id: &str) -> Result<GoalRun, GoalRunError> {
        let path = self.goal_path(goal_id)?;
        let content = fs::read_to_string(&path).map_err(|e| {
            GoalRunError::new(
                "goal_run.path",
                &format!("goal run read failed path={} error={e}", path.display()),
            )
        })?;
        let run = serde_json::from_str::<GoalRun>(&content).map_err(|e| {
            GoalRunError::new(
                "goal_run.json",
                &format!("goal run parse failed path={} error={e}", path.display()),
            )
        })?;
        run.validate_persisted()?;
        Ok(run)
    }

    pub fn record_checkpoint(
        &self,
        goal_id: &str,
        checkpoint: GoalCheckpoint,
    ) -> Result<GoalRunReceipt, GoalRunError> {
        let path = self.goal_path(goal_id)?;
        let mut run = self.load(goal_id)?;
        run.record_checkpoint(checkpoint)?;
        self.write_run(&path, &run)
    }

    pub fn goal_path(&self, goal_id: &str) -> Result<PathBuf, GoalRunError> {
        Ok(self
            .root
            .join(format!("{}.json", sanitize_goal_id(goal_id)?)))
    }

    fn write_run(&self, path: &Path, run: &GoalRun) -> Result<GoalRunReceipt, GoalRunError> {
        fs::create_dir_all(&self.root).map_err(|e| {
            GoalRunError::new(
                "goal_run.root",
                &format!(
                    "goal run root create failed path={} error={e}",
                    self.root.display()
                ),
            )
        })?;
        let rendered = serde_json::to_string_pretty(run).map_err(|e| {
            GoalRunError::new("goal_run.json", &format!("goal run render failed: {e}"))
        })?;
        fs::write(path, rendered).map_err(|e| {
            GoalRunError::new(
                "goal_run.path",
                &format!("goal run write failed path={} error={e}", path.display()),
            )
        })?;
        Ok(GoalRunReceipt {
            goal_id: run.goal_spec.goal_id.clone(),
            path: path.display().to_string(),
            checkpoint_count: run.checkpoint_log.len(),
            checkpoint_writeback: GoalCheckpointWriteback::from_policy(
                &run.goal_spec.checkpoint_policy,
            ),
            last_checkpoint_id: run
                .checkpoint_log
                .last()
                .map(|checkpoint| checkpoint.checkpoint_id.clone()),
            last_checkpoint_summary: run
                .checkpoint_log
                .last()
                .map(|checkpoint| checkpoint.summary.clone()),
            last_checkpoint_created_at: run
                .checkpoint_log
                .last()
                .and_then(|checkpoint| checkpoint.created_at.clone()),
            last_checkpoint_completed_worker_ids: run
                .checkpoint_log
                .last()
                .map(|checkpoint| checkpoint.completed_worker_ids.clone()),
            last_checkpoint_validation_notes: run
                .checkpoint_log
                .last()
                .map(|checkpoint| checkpoint.validation_notes.clone()),
        })
    }
}

impl GoalWorkerPlan {
    pub fn new(
        worker_id: impl Into<String>,
        objective: impl Into<String>,
        write_scope_ids: Vec<String>,
        validation_checks: Vec<String>,
    ) -> Self {
        Self {
            worker_id: worker_id.into(),
            objective: objective.into(),
            write_scope_ids,
            validation_checks,
        }
    }
}

impl GoalWriteScope {
    pub fn new(scope_id: impl Into<String>, paths: Vec<String>) -> Self {
        Self {
            scope_id: scope_id.into(),
            paths,
        }
    }
}

impl GoalValidationPlan {
    pub fn new(commands: Vec<String>) -> Self {
        Self { commands }
    }
}

impl GoalCheckpointWriteback {
    fn from_policy(policy: &GoalCheckpointPolicy) -> Self {
        let mut documentation_targets = Vec::new();
        if policy.update_progress_log {
            documentation_targets.push("docs/progress-log.md".to_string());
        }
        if policy.update_handoff {
            documentation_targets.push("docs/handoff-current.md".to_string());
        }
        Self {
            manual_only: true,
            update_progress_log: policy.update_progress_log,
            update_handoff: policy.update_handoff,
            commit_checkpoint: policy.commit_checkpoint,
            documentation_targets,
        }
    }
}

impl GoalIntegrationPolicy {
    pub fn main_process_owned() -> Self {
        Self {
            main_process_owns_integration: true,
            workers_may_commit: false,
            workers_may_touch_secrets: false,
            require_worker_reports: true,
        }
    }

    fn validate(&self) -> Result<(), GoalRunError> {
        if !self.main_process_owns_integration {
            return Err(GoalRunError::new(
                "integration_policy.main_process_owns_integration",
                "main process must retain integration ownership",
            ));
        }
        if self.workers_may_commit {
            return Err(GoalRunError::new(
                "integration_policy.workers_may_commit",
                "workers must not commit directly",
            ));
        }
        if self.workers_may_touch_secrets {
            return Err(GoalRunError::new(
                "integration_policy.workers_may_touch_secrets",
                "workers must not touch secrets",
            ));
        }
        Ok(())
    }
}

impl GoalCheckpoint {
    pub fn new(
        checkpoint_id: impl Into<String>,
        summary: impl Into<String>,
        completed_worker_ids: Vec<String>,
        validation_notes: Vec<String>,
    ) -> Self {
        Self {
            checkpoint_id: checkpoint_id.into(),
            summary: summary.into(),
            created_at: Some(current_rfc3339_timestamp()),
            completed_worker_ids,
            validation_notes,
            blocker_key: None,
            evidence_verdicts: Vec::new(),
        }
    }

    /// 带规范化失败原因键的构造（用于收敛判定：同一 blocker_key 重复 → blocked）。
    pub fn with_blocker_key(
        checkpoint_id: impl Into<String>,
        summary: impl Into<String>,
        completed_worker_ids: Vec<String>,
        validation_notes: Vec<String>,
        blocker_key: impl Into<String>,
    ) -> Self {
        let mut checkpoint = Self::new(
            checkpoint_id,
            summary,
            completed_worker_ids,
            validation_notes,
        );
        checkpoint.blocker_key = Some(blocker_key.into());
        checkpoint
    }

    /// 带验收证据检查结果（verifier-first）的构造。
    pub fn with_evidence(
        checkpoint_id: impl Into<String>,
        summary: impl Into<String>,
        completed_worker_ids: Vec<String>,
        validation_notes: Vec<String>,
        evidence_verdicts: Vec<EvidenceVerdict>,
    ) -> Self {
        let mut checkpoint = Self::new(
            checkpoint_id,
            summary,
            completed_worker_ids,
            validation_notes,
        );
        checkpoint.evidence_verdicts = evidence_verdicts;
        checkpoint
    }

    fn validate(&self) -> Result<(), GoalRunError> {
        require_non_empty("checkpoint_log.checkpoint_id", &self.checkpoint_id)?;
        require_non_empty("checkpoint_log.summary", &self.summary)?;
        validate_checkpoint_created_at(self.created_at.as_deref())?;
        require_non_empty_vec(
            "checkpoint_log.completed_worker_ids",
            self.completed_worker_ids.len(),
        )?;
        require_non_empty_vec(
            "checkpoint_log.validation_notes",
            self.validation_notes.len(),
        )?;
        for note in &self.validation_notes {
            require_non_empty("checkpoint_log.validation_notes", note)?;
        }
        if let Some(blocker_key) = self.blocker_key.as_deref() {
            require_non_empty("checkpoint_log.blocker_key", blocker_key)?;
        }
        for verdict in &self.evidence_verdicts {
            require_non_empty("checkpoint_log.evidence_verdicts.path", &verdict.path)?;
            require_non_empty("checkpoint_log.evidence_verdicts.reason", &verdict.reason)?;
        }
        Ok(())
    }
}

impl GoalValidationPlan {
    fn validate(&self) -> Result<(), GoalRunError> {
        require_non_empty_vec("validation_plan.commands", self.commands.len())?;
        for command in &self.commands {
            require_non_empty("validation_plan.commands", command)?;
        }
        Ok(())
    }
}

impl GoalRunError {
    fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
        }
    }
}

fn validate_worker_plan(
    worker_plan: &[GoalWorkerPlan],
    write_scopes: &[GoalWriteScope],
) -> Result<(), GoalRunError> {
    let scope_ids = write_scopes
        .iter()
        .map(|scope| scope.scope_id.as_str())
        .collect::<HashSet<_>>();
    let mut worker_ids = HashSet::new();
    let mut assigned_scope_ids = HashSet::new();

    for worker in worker_plan {
        require_non_empty("worker_plan.worker_id", &worker.worker_id)?;
        require_non_empty("worker_plan.objective", &worker.objective)?;
        require_non_empty_vec("worker_plan.write_scope_ids", worker.write_scope_ids.len())?;
        require_non_empty_vec(
            "worker_plan.validation_checks",
            worker.validation_checks.len(),
        )?;
        if !worker_ids.insert(worker.worker_id.as_str()) {
            return Err(GoalRunError::new(
                "worker_plan.worker_id",
                "worker_id must be unique within a goal run",
            ));
        }
        for scope_id in &worker.write_scope_ids {
            require_non_empty("worker_plan.write_scope_ids", scope_id)?;
            if !scope_ids.contains(scope_id.as_str()) {
                return Err(GoalRunError::new(
                    "worker_plan.write_scope_ids",
                    "worker references an unknown write scope",
                ));
            }
            if !assigned_scope_ids.insert(scope_id.as_str()) {
                return Err(GoalRunError::new(
                    "worker_plan.write_scope_ids",
                    "write scope must be owned by only one worker",
                ));
            }
        }
        for validation_check in &worker.validation_checks {
            require_non_empty("worker_plan.validation_checks", validation_check)?;
        }
    }
    Ok(())
}

fn validate_checkpoint_worker_ids(
    checkpoint: &GoalCheckpoint,
    worker_plan: &[GoalWorkerPlan],
) -> Result<(), GoalRunError> {
    let worker_ids = worker_plan
        .iter()
        .map(|worker| worker.worker_id.as_str())
        .collect::<HashSet<_>>();
    let mut completed_worker_ids = HashSet::new();
    for worker_id in &checkpoint.completed_worker_ids {
        require_non_empty("checkpoint_log.completed_worker_ids", worker_id)?;
        if !completed_worker_ids.insert(worker_id.as_str()) {
            return Err(GoalRunError::new(
                "checkpoint_log.completed_worker_ids",
                "completed worker ids must be unique within a checkpoint",
            ));
        }
        if !worker_ids.contains(worker_id.as_str()) {
            return Err(GoalRunError::new(
                "checkpoint_log.completed_worker_ids",
                "checkpoint references an unknown worker",
            ));
        }
    }
    Ok(())
}

fn validate_checkpoint_log_strict(
    checkpoint_log: &[GoalCheckpoint],
    worker_plan: &[GoalWorkerPlan],
) -> Result<(), GoalRunError> {
    let mut checkpoint_ids = HashSet::new();
    for checkpoint in checkpoint_log {
        checkpoint.validate()?;
        validate_checkpoint_worker_ids(checkpoint, worker_plan)?;
        if !checkpoint_ids.insert(checkpoint.checkpoint_id.as_str()) {
            return Err(GoalRunError::new(
                "checkpoint_log.checkpoint_id",
                "checkpoint_id must be unique within a goal run",
            ));
        }
    }
    Ok(())
}

fn validate_checkpoint_log_ids(checkpoint_log: &[GoalCheckpoint]) -> Result<(), GoalRunError> {
    let mut checkpoint_ids = HashSet::new();
    for checkpoint in checkpoint_log {
        validate_checkpoint_created_at(checkpoint.created_at.as_deref())?;
        if !checkpoint_ids.insert(checkpoint.checkpoint_id.as_str()) {
            return Err(GoalRunError::new(
                "checkpoint_log.checkpoint_id",
                "checkpoint_id must be unique within a goal run",
            ));
        }
    }
    Ok(())
}

fn validate_checkpoint_created_at(created_at: Option<&str>) -> Result<(), GoalRunError> {
    let Some(created_at) = created_at else {
        return Ok(());
    };
    require_non_empty("checkpoint_log.created_at", created_at)?;
    chrono::DateTime::parse_from_rfc3339(created_at).map_err(|_| {
        GoalRunError::new("checkpoint_log.created_at", "created_at must be RFC3339")
    })?;
    Ok(())
}

fn current_rfc3339_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn worker_scope_complete(worker_plan: &[GoalWorkerPlan], write_scopes: &[GoalWriteScope]) -> bool {
    let declared_scope_ids = write_scopes
        .iter()
        .map(|scope| scope.scope_id.as_str())
        .collect::<HashSet<_>>();
    let assigned_scope_ids = worker_plan
        .iter()
        .flat_map(|worker| worker.write_scope_ids.iter().map(String::as_str))
        .collect::<HashSet<_>>();
    declared_scope_ids == assigned_scope_ids
}

fn validate_write_scopes(write_scopes: &[GoalWriteScope]) -> Result<(), GoalRunError> {
    let mut scope_ids = HashSet::new();
    let mut claimed_paths: Vec<(&str, String)> = Vec::new();

    for scope in write_scopes {
        require_non_empty("disjoint_write_scopes.scope_id", &scope.scope_id)?;
        require_non_empty_vec("disjoint_write_scopes.paths", scope.paths.len())?;
        if !scope_ids.insert(scope.scope_id.as_str()) {
            return Err(GoalRunError::new(
                "disjoint_write_scopes.scope_id",
                "scope_id must be unique within a goal run",
            ));
        }

        for raw_path in &scope.paths {
            require_non_empty("disjoint_write_scopes.paths", raw_path)?;
            let path = normalize_scope_path(raw_path);
            for (existing_scope_id, existing_path) in &claimed_paths {
                if paths_overlap(existing_path, &path) {
                    return Err(GoalRunError::new(
                        "disjoint_write_scopes.paths",
                        &format!(
                            "write scope '{}' overlaps with '{}'",
                            scope.scope_id, existing_scope_id
                        ),
                    ));
                }
            }
            claimed_paths.push((scope.scope_id.as_str(), path));
        }
    }
    Ok(())
}

fn paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn normalize_scope_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

fn sanitize_goal_id(goal_id: &str) -> Result<String, GoalRunError> {
    require_non_empty("goal_id", goal_id)?;
    if goal_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        Ok(goal_id.to_string())
    } else {
        Err(GoalRunError::new(
            "goal_id",
            "goal_id may only contain ASCII letters, numbers, '-' and '_'",
        ))
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), GoalRunError> {
    if value.trim().is_empty() {
        Err(GoalRunError::new(field, "field must not be empty"))
    } else {
        Ok(())
    }
}

fn require_non_empty_vec(field: &str, len: usize) -> Result<(), GoalRunError> {
    if len == 0 {
        Err(GoalRunError::new(field, "field must not be empty"))
    } else {
        Ok(())
    }
}

fn format_goal_spec_error(error: GoalSpecError) -> String {
    format!("{}: {}", error.field, error.message)
}

/// 收敛门禁：判定 checkpoint 序列是"收敛"还是"原地打转"。
///
/// 语义（Penguin goal-file 收敛控制，最小实现）：
/// - 无 checkpoint → `Unknown`。
/// - 尾部连续相同"进度指纹"（blocker_key 优先，否则 validation_notes 全量指纹）的
///   checkpoint 数 `n`：
///   - `n >= max_repeated_blockers`（且 > 0）→ `Blocked`：同一卡点重复到上限，
///     禁止继续以同策略重试；必须换策略或由外部裁决。
///   - `n >= 2` → `Spinning`：已出现重复，但尚未到上限。
///   - 否则 → `Converging`。
///
/// 纯函数：无 I/O、无随机，输出完全由入参决定，便于测试与审计。
pub fn enforce_convergence_gate(
    checkpoint_log: &[GoalCheckpoint],
    policy: &GoalConvergencePolicy,
) -> ConvergenceVerdict {
    let Some(last) = checkpoint_log.last() else {
        return ConvergenceVerdict {
            status: ConvergenceStatus::Unknown,
            repeated_fingerprint: None,
            repeated_count: 0,
            reason: "no checkpoint recorded; convergence cannot be judged".to_string(),
        };
    };

    let tail_fingerprint = checkpoint_progress_fingerprint(last);
    let max_repeated = policy.max_repeated_blockers;
    let mut repeated_count = 0usize;
    for checkpoint in checkpoint_log.iter().rev() {
        if checkpoint_progress_fingerprint(checkpoint) == tail_fingerprint {
            repeated_count += 1;
        } else {
            break;
        }
    }

    if max_repeated > 0 && repeated_count >= max_repeated {
        return ConvergenceVerdict {
            status: ConvergenceStatus::Blocked,
            repeated_fingerprint: Some(tail_fingerprint.clone()),
            repeated_count,
            reason: format!(
                "repeated blocker threshold reached: fingerprint={tail_fingerprint} count={repeated_count} max={max_repeated}; stop retrying the same strategy"
            ),
        };
    }
    if repeated_count >= 2 {
        return ConvergenceVerdict {
            status: ConvergenceStatus::Spinning,
            repeated_fingerprint: Some(tail_fingerprint.clone()),
            repeated_count,
            reason: format!(
                "no progress between checkpoints: fingerprint={tail_fingerprint} repeated={repeated_count} max={max_repeated}"
            ),
        };
    }
    ConvergenceVerdict {
        status: ConvergenceStatus::Converging,
        repeated_fingerprint: None,
        repeated_count,
        reason: "checkpoint shows progress; convergence gate passed".to_string(),
    }
}

/// 检查单条验收证据（verifier-first 纯函数）：
/// 1. 文件必须存在；2. 行数 >= min_lines（若设置）；3. 内容包含 min_content（若设置）。
pub fn check_evidence_at(
    root: &Path,
    evidence: &GoalEvidence,
    evidence_index: usize,
) -> EvidenceVerdict {
    let path = if Path::new(&evidence.path).is_absolute() {
        evidence.path.clone()
    } else {
        root.join(&evidence.path).to_string_lossy().to_string()
    };
    let description = evidence
        .description
        .as_deref()
        .unwrap_or(&evidence.path)
        .to_string();
    let Ok(content) = fs::read_to_string(&path) else {
        return EvidenceVerdict {
            evidence_index,
            path: path.clone(),
            passed: false,
            reason: format!("{description}: file not found"),
        };
    };
    if let Some(min_lines) = evidence.min_lines {
        let line_count = content.lines().count();
        if line_count < min_lines {
            return EvidenceVerdict {
                evidence_index,
                path: path.clone(),
                passed: false,
                reason: format!("{description}: too few lines ({line_count} < {min_lines})"),
            };
        }
    }
    if let Some(min_content) = evidence.min_content.as_deref() {
        if !min_content.is_empty() && !content.contains(min_content) {
            return EvidenceVerdict {
                evidence_index,
                path: path.clone(),
                passed: false,
                reason: format!("{description}: missing required content {min_content:?}"),
            };
        }
    }
    EvidenceVerdict {
        evidence_index,
        path: path.clone(),
        passed: true,
        reason: "ok".to_string(),
    }
}

/// 检查 goal spec 全部验收证据（按声明顺序，含 evidence_index）。
pub fn check_evidence_plan(root: &Path, spec: &GoalSpec) -> Vec<EvidenceVerdict> {
    check_evidence_items(root, &spec.acceptance_evidence)
}

/// 检查一组验收证据条目（按声明顺序，含 evidence_index）。
/// 供 dispatch/collect 对 manifest 快照直接判定，无需构造 GoalSpec。
pub fn check_evidence_items(root: &Path, items: &[GoalEvidence]) -> Vec<EvidenceVerdict> {
    items
        .iter()
        .enumerate()
        .map(|(index, evidence)| check_evidence_at(root, evidence, index))
        .collect()
}

/// 评估单条类型化验收检查（verifier-first 纯逻辑 + 只读/命令判定）：
/// - `Evidence` 检查 → 文件系统证据判定（复用 `check_evidence_at`）；
/// - `Command` 检查 → 显式执行命令（`goal verify` 入口），按退出码判定。
pub fn evaluate_acceptance_check(
    root: &Path,
    check: &AcceptanceCheck,
    check_index: usize,
) -> AcceptanceVerdict {
    let evaluator = check.evaluator().to_string();
    let description = check.description();
    match check {
        AcceptanceCheck::Evidence(evidence) => {
            let verdict = check_evidence_at(root, evidence, check_index);
            AcceptanceVerdict {
                check_index,
                evaluator,
                description,
                passed: verdict.passed,
                reason: verdict.reason,
                exit_code: None,
            }
        }
        AcceptanceCheck::Command(command) => evaluate_command_check(command, check_index),
    }
}

/// 评估类型化验收计划全部检查（按声明顺序，含 check_index）。
pub fn evaluate_acceptance_plan(root: &Path, plan: &GoalAcceptancePlan) -> Vec<AcceptanceVerdict> {
    plan.checks
        .iter()
        .enumerate()
        .map(|(index, check)| evaluate_acceptance_check(root, check, index))
        .collect()
}

/// 命令类验收检查：`sh -c` 执行并等待退出码（带超时兜底）。
/// 输出不进入 verdict（避免命令输出中的敏感内容泄漏到日志/回执）。
fn evaluate_command_check(command: &str, check_index: usize) -> AcceptanceVerdict {
    let description = command.to_string();
    let started = Instant::now();
    let timeout = Duration::from_secs(ACCEPTANCE_COMMAND_TIMEOUT_SECS);
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return AcceptanceVerdict {
                check_index,
                evaluator: "command".to_string(),
                description,
                passed: false,
                reason: format!("command could not be started: {error}"),
                exit_code: None,
            };
        }
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.code();
                return AcceptanceVerdict {
                    check_index,
                    evaluator: "command".to_string(),
                    description,
                    passed: status.success(),
                    reason: if status.success() {
                        "ok".to_string()
                    } else {
                        format!(
                            "command exited with code {}",
                            exit_code
                                .map(|code| code.to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        )
                    },
                    exit_code,
                };
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return AcceptanceVerdict {
                        check_index,
                        evaluator: "command".to_string(),
                        description,
                        passed: false,
                        reason: format!(
                            "command timed out after {ACCEPTANCE_COMMAND_TIMEOUT_SECS}s"
                        ),
                        exit_code: None,
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                let _ = child.kill();
                return AcceptanceVerdict {
                    check_index,
                    evaluator: "command".to_string(),
                    description,
                    passed: false,
                    reason: format!("command wait failed: {error}"),
                    exit_code: None,
                };
            }
        }
    }
}

/// verifier-first 验收检查契约实现：可验证（定义时）+ 可评估（运行时证据判定）。
impl AcceptanceCheckContract for AcceptanceCheck {
    fn validate_contract(&self) -> Result<(), GoalSpecError> {
        self.validate()
    }

    fn evaluator(&self) -> &'static str {
        self.evaluator()
    }

    fn description(&self) -> String {
        self.description()
    }

    fn evaluate_contract(&self, root: &Path, check_index: usize) -> AcceptanceVerdict {
        evaluate_acceptance_check(root, self, check_index)
    }
}

/// checkpoint 的进度指纹：有 blocker_key 用它（同一失败原因去重），
/// 否则用 validation_notes 全量拼接（完全相同的验证结果视为无进展）。
fn checkpoint_progress_fingerprint(checkpoint: &GoalCheckpoint) -> String {
    match checkpoint.blocker_key.as_deref() {
        Some(key) => format!("blocker:{key}"),
        None => format!("notes:{}", checkpoint.validation_notes.join("|")),
    }
}

fn convergence_status_string(status: ConvergenceStatus) -> String {
    match status {
        ConvergenceStatus::Unknown => "unknown".to_string(),
        ConvergenceStatus::Converging => "converging".to_string(),
        ConvergenceStatus::Spinning => "spinning".to_string(),
        ConvergenceStatus::Blocked => "blocked".to_string(),
    }
}
