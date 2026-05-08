use crate::goal_mode::{GoalCheckpointPolicy, GoalSpec, GoalSpecError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRun {
    pub goal_spec: GoalSpec,
    pub worker_plan: Vec<GoalWorkerPlan>,
    pub disjoint_write_scopes: Vec<GoalWriteScope>,
    pub validation_plan: GoalValidationPlan,
    pub integration_policy: GoalIntegrationPolicy,
    pub checkpoint_log: Vec<GoalCheckpoint>,
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
        };
        run.validate()?;
        Ok(run)
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
        if self.checkpoint_log.is_empty() {
            incomplete_reasons.push("no checkpoint has been recorded".to_string());
        } else if !checkpoint_log_complete {
            incomplete_reasons.push(
                "latest checkpoint is missing completed worker evidence, validation notes, or references an unknown worker"
                    .to_string(),
            );
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
        }
    }

    pub fn validate(&self) -> Result<(), GoalRunError> {
        self.goal_spec
            .validate()
            .map_err(|error| GoalRunError::new("goal_spec", &format_goal_spec_error(error)))?;
        require_non_empty_vec("worker_plan", self.worker_plan.len())?;
        require_non_empty_vec("disjoint_write_scopes", self.disjoint_write_scopes.len())?;
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
        }
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
