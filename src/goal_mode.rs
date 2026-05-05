use crate::context_engine::{ContextSegment, SegmentSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalSpec {
    pub goal_id: String,
    pub objective: String,
    pub acceptance_checks: Vec<String>,
    pub budget: GoalBudget,
    pub allowed_slots: Vec<String>,
    pub checkpoint_policy: GoalCheckpointPolicy,
    pub final_report_policy: GoalFinalReportPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalBudget {
    pub max_minutes: Option<u16>,
    pub max_tool_rounds: Option<usize>,
    pub max_subtasks: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalCheckpointPolicy {
    pub update_progress_log: bool,
    pub update_handoff: bool,
    pub commit_checkpoint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalFinalReportPolicy {
    pub include_validation: bool,
    pub include_next_steps: bool,
}

impl GoalSpec {
    pub fn mainline_mvp(objective: impl Into<String>) -> Self {
        Self {
            goal_id: "mainline-mvp".to_string(),
            objective: objective.into(),
            acceptance_checks: vec![
                "cargo fmt --all".to_string(),
                "git diff --check".to_string(),
                "timeout 240s cargo test -q".to_string(),
            ],
            budget: GoalBudget {
                max_minutes: Some(60),
                max_tool_rounds: Some(8),
                max_subtasks: Some(0),
            },
            allowed_slots: vec![
                "context".to_string(),
                "governance".to_string(),
                "execution".to_string(),
                "report".to_string(),
                "memory".to_string(),
            ],
            checkpoint_policy: GoalCheckpointPolicy {
                update_progress_log: true,
                update_handoff: true,
                commit_checkpoint: true,
            },
            final_report_policy: GoalFinalReportPolicy {
                include_validation: true,
                include_next_steps: true,
            },
        }
    }

    pub fn validate(&self) -> Result<(), GoalSpecError> {
        require_non_empty("goal_id", &self.goal_id)?;
        require_non_empty("objective", &self.objective)?;
        if self.acceptance_checks.is_empty() {
            return Err(GoalSpecError::new(
                "acceptance_checks",
                "goal must define at least one acceptance check",
            ));
        }
        if self.allowed_slots.is_empty() {
            return Err(GoalSpecError::new(
                "allowed_slots",
                "goal must define at least one allowed slot",
            ));
        }
        if self.budget.max_minutes == Some(0) {
            return Err(GoalSpecError::new(
                "budget.max_minutes",
                "max_minutes must be greater than zero when set",
            ));
        }
        if self.budget.max_tool_rounds == Some(0) {
            return Err(GoalSpecError::new(
                "budget.max_tool_rounds",
                "max_tool_rounds must be greater than zero when set",
            ));
        }
        Ok(())
    }

    pub fn render_context_block(&self) -> Result<String, GoalSpecError> {
        self.validate()?;
        Ok(format!(
            "GOAL_SPEC\n\
goal_id: {}\n\
objective: {}\n\
acceptance_checks:\n{}\n\
allowed_slots: {}\n\
budget: max_minutes={} max_tool_rounds={} max_subtasks={}\n\
checkpoint_policy: progress_log={} handoff={} commit={}\n\
final_report_policy: validation={} next_steps={}",
            self.goal_id,
            self.objective,
            render_list(&self.acceptance_checks),
            self.allowed_slots.join(","),
            render_optional(self.budget.max_minutes),
            render_optional(self.budget.max_tool_rounds),
            render_optional(self.budget.max_subtasks),
            self.checkpoint_policy.update_progress_log,
            self.checkpoint_policy.update_handoff,
            self.checkpoint_policy.commit_checkpoint,
            self.final_report_policy.include_validation,
            self.final_report_policy.include_next_steps
        ))
    }

    pub fn render_context_segment(&self) -> Result<ContextSegment, GoalSpecError> {
        let content = self.render_context_block()?;
        let now = default_goal_timestamp();
        Ok(ContextSegment {
            id: format!("goal-spec-{}", sanitize_segment_id(&self.goal_id)),
            source: SegmentSource::Goal,
            tokens: Some(estimate_goal_tokens(&content)),
            priority: 241,
            created_at: now,
            last_accessed: now,
            metadata: [
                ("kind".to_string(), "goal_spec".to_string()),
                ("goal_id".to_string(), self.goal_id.clone()),
            ]
            .into_iter()
            .collect(),
            content,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalSpecError {
    pub field: String,
    pub message: String,
}

impl GoalSpecError {
    fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
        }
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), GoalSpecError> {
    if value.trim().is_empty() {
        Err(GoalSpecError::new(field, "field must not be empty"))
    } else {
        Ok(())
    }
}

fn render_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_optional<T: ToString>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unset".to_string())
}

fn estimate_goal_tokens(content: &str) -> u16 {
    content.chars().count().div_ceil(4).min(u16::MAX as usize) as u16
}

fn sanitize_segment_id(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn default_goal_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-05-03T00:00:00Z")
        .expect("static goal timestamp should parse")
        .with_timezone(&chrono::Utc)
}
