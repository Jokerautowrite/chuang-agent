use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentRequest {
    pub goal: String,
    pub success_criteria: String,
    pub time_budget_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExperimentReceipt {
    pub experiment_id: String,
    pub plan_path: String,
    pub time_budget_minutes: u16,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentCompleteRequest {
    pub experiment_id: String,
    pub outcome: ExperimentOutcome,
    pub summary: String,
    pub next_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExperimentOutcome {
    Success,
    Failure,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExperimentReportReceipt {
    pub experiment_id: String,
    pub report_path: String,
    pub outcome: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExperimentListOutput {
    pub root: String,
    pub count: usize,
    pub items: Vec<ExperimentListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExperimentListItem {
    pub experiment_id: String,
    pub status: String,
    pub has_plan: bool,
    pub has_report: bool,
    pub plan_path: String,
    pub report_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExperimentShowOutput {
    pub experiment_id: String,
    pub status: String,
    pub plan_path: String,
    pub report_path: String,
    pub plan_markdown: Option<String>,
    pub report_markdown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfExperimentPlanner {
    root: PathBuf,
}

impl SelfExperimentPlanner {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn create_plan(&self, request: &ExperimentRequest) -> Result<ExperimentReceipt, String> {
        validate_request(request)?;
        let experiment_id = experiment_id(&request.goal)?;
        let dir = self.root.join(&experiment_id);
        fs::create_dir_all(&dir).map_err(|e| {
            format!(
                "experiment_dir_create_failed path={} error={e}",
                dir.display()
            )
        })?;
        let plan_path = dir.join("experiment.md");
        let markdown = render_experiment_plan(&experiment_id, request);
        fs::write(&plan_path, markdown).map_err(|e| {
            format!(
                "experiment_plan_write_failed path={} error={e}",
                plan_path.display()
            )
        })?;

        Ok(ExperimentReceipt {
            experiment_id,
            plan_path: plan_path.display().to_string(),
            time_budget_minutes: request.time_budget_minutes,
            status: "planned".to_string(),
        })
    }

    pub fn complete(
        &self,
        request: &ExperimentCompleteRequest,
    ) -> Result<ExperimentReportReceipt, String> {
        validate_complete_request(request)?;
        let dir = self.root.join(&request.experiment_id);
        let plan_path = dir.join("experiment.md");
        if !plan_path.exists() {
            return Err(format!(
                "experiment_plan_missing path={}",
                plan_path.display()
            ));
        }
        let report_path = dir.join("report.md");
        let markdown = render_experiment_report(request);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&report_path)
            .map_err(|e| {
                format!(
                    "experiment_report_write_failed path={} error={e}",
                    report_path.display()
                )
            })?;
        file.write_all(markdown.as_bytes()).map_err(|e| {
            format!(
                "experiment_report_write_failed path={} error={e}",
                report_path.display()
            )
        })?;

        Ok(ExperimentReportReceipt {
            experiment_id: request.experiment_id.clone(),
            report_path: report_path.display().to_string(),
            outcome: request.outcome.as_str().to_string(),
            status: "completed".to_string(),
        })
    }

    pub fn list(&self) -> Result<ExperimentListOutput, String> {
        let mut items = Vec::new();
        if self.root.exists() {
            let entries = fs::read_dir(&self.root).map_err(|e| {
                format!(
                    "experiment_list_read_failed path={} error={e}",
                    self.root.display()
                )
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("experiment_list_entry_failed: {e}"))?;
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let experiment_id = entry.file_name().to_string_lossy().to_string();
                let plan_path = path.join("experiment.md");
                let report_path = path.join("report.md");
                let has_plan = plan_path.exists();
                let has_report = report_path.exists();
                let status = if has_report {
                    "completed"
                } else if has_plan {
                    "planned"
                } else {
                    "unknown"
                };
                items.push(ExperimentListItem {
                    experiment_id,
                    status: status.to_string(),
                    has_plan,
                    has_report,
                    plan_path: plan_path.display().to_string(),
                    report_path: report_path.display().to_string(),
                });
            }
        }
        items.sort_by(|left, right| left.experiment_id.cmp(&right.experiment_id));

        Ok(ExperimentListOutput {
            root: self.root.display().to_string(),
            count: items.len(),
            items,
        })
    }

    pub fn show(&self, experiment_id: &str) -> Result<ExperimentShowOutput, String> {
        let experiment_id = experiment_id.trim();
        if experiment_id.is_empty() {
            return Err("experiment_id_required".to_string());
        }

        let dir = self.root.join(experiment_id);
        let plan_path = dir.join("experiment.md");
        let report_path = dir.join("report.md");
        let plan_markdown = read_optional_markdown(&plan_path)?;
        let report_markdown = read_optional_markdown(&report_path)?;
        if plan_markdown.is_none() && report_markdown.is_none() {
            return Err(format!("experiment_not_found: {experiment_id}"));
        }
        let status = if report_markdown.is_some() {
            "completed"
        } else if plan_markdown.is_some() {
            "planned"
        } else {
            "unknown"
        };

        Ok(ExperimentShowOutput {
            experiment_id: experiment_id.to_string(),
            status: status.to_string(),
            plan_path: plan_path.display().to_string(),
            report_path: report_path.display().to_string(),
            plan_markdown,
            report_markdown,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn read_optional_markdown(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|e| format!("experiment_read_failed path={} error={e}", path.display()))
}

fn validate_request(request: &ExperimentRequest) -> Result<(), String> {
    if request.goal.trim().is_empty() {
        return Err("experiment_goal_required".to_string());
    }
    if request.success_criteria.trim().is_empty() {
        return Err("experiment_success_criteria_required".to_string());
    }
    if request.time_budget_minutes == 0 {
        return Err("experiment_time_budget_must_be_positive".to_string());
    }
    if request.time_budget_minutes > 240 {
        return Err("experiment_time_budget_too_large: max 240 minutes".to_string());
    }
    Ok(())
}

fn validate_complete_request(request: &ExperimentCompleteRequest) -> Result<(), String> {
    if request.experiment_id.trim().is_empty() {
        return Err("experiment_id_required".to_string());
    }
    if request.summary.trim().is_empty() {
        return Err("experiment_summary_required".to_string());
    }
    if request.next_step.trim().is_empty() {
        return Err("experiment_next_step_required".to_string());
    }
    Ok(())
}

impl ExperimentOutcome {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "success" => Ok(Self::Success),
            "failure" => Ok(Self::Failure),
            "inconclusive" => Ok(Self::Inconclusive),
            other => Err(format!(
                "invalid_experiment_outcome: {other} (supported: success, failure, inconclusive)"
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Inconclusive => "inconclusive",
        }
    }
}

fn experiment_id(goal: &str) -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    let slug = goal
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || matches!(ch, '-' | '_') {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "experiment" } else { &slug };
    Ok(format!("{slug}-{nanos}"))
}

fn render_experiment_plan(experiment_id: &str, request: &ExperimentRequest) -> String {
    format!(
        r#"# Experiment

experiment_id: {experiment_id}
status: planned
time_budget_minutes: {time_budget}

## Goal

{goal}

## Success Criteria

{success_criteria}

## Fixed Safety Constraints

1. Do not run `git reset --hard`.
2. Do not delete files, directories, queues, reports, claims, memories, or credentials.
3. Do not purge, clean, uninstall, or destructively roll back state.
4. Keep all work outside the main branch unless 老爸 explicitly approves integration.
5. Produce an experiment report before any proposed integration.
6. Keep secrets out of logs, reports, fixtures, and chat output.

## Suggested Loop

1. Restate the target and success criteria.
2. Inspect only the files needed for the hypothesis.
3. Make the smallest isolated change or prototype.
4. Run the narrowest useful verification.
5. Write results, risks, and next recommendation.

## Result

Not run yet.
"#,
        experiment_id = experiment_id,
        time_budget = request.time_budget_minutes,
        goal = request.goal.trim(),
        success_criteria = request.success_criteria.trim()
    )
}

fn render_experiment_report(request: &ExperimentCompleteRequest) -> String {
    format!(
        r#"# Experiment Report

experiment_id: {experiment_id}
status: completed
outcome: {outcome}

## Summary

{summary}

## Next Step

{next_step}

## Safety Confirmation

1. No `git reset --hard` was performed by this report step.
2. No deletion, cleanup, purge, uninstall, or destructive rollback was performed by this report step.
3. This report is an append-only artifact created next to the original experiment plan.
"#,
        experiment_id = request.experiment_id.trim(),
        outcome = request.outcome.as_str(),
        summary = request.summary.trim(),
        next_step = request.next_step.trim()
    )
}
