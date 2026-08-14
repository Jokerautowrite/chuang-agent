//! `goal_mode` 模块。公开接口：trait AcceptanceCheckContract；struct GoalSpec, GoalEvidence, GoalAcceptancePlan, AcceptanceVerdict, GoalConvergencePolicy, GoalBudget, GoalCheckpointPolicy, GoalFinalReportPolicy, GoalStatus, GoalControlFile；enum AcceptanceCheck；fn new, with_min_lines, with_min_content, with_description, validate, evaluator, description, is_empty；const DEFAULT_MAX_REPEATED_BLOCKERS。

use crate::context_engine::{ContextSegment, SegmentSource};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalSpec {
    pub goal_id: String,
    pub objective: String,
    /// GOAL.yaml 控制通道状态（升级点 1）：
    /// - `Active` 由系统持有（默认，模型不可写）；
    /// - `Complete|Blocked` 模型唯一可写；解析失败归一化为 blocked（fail-closed）。
    #[serde(default)]
    pub status: GoalStatus,
    pub acceptance_checks: Vec<String>,
    /// 验收证据（verifier-first）：目标完成必须有文件系统证据，
    /// 不能只靠模型自述。checkpoint 时由 CLI 自动检查。
    #[serde(default)]
    pub acceptance_evidence: Vec<GoalEvidence>,
    /// 类型化验收检查契约（verifier-first）：goal 定义时先声明可验证、
    /// 可评估的验收检查；运行时按验收标准产出证据判定。
    /// 旧 on-disk goal JSON 无此字段时使用默认空计划（向后兼容）。
    #[serde(default)]
    pub acceptance_plan: GoalAcceptancePlan,
    pub budget: GoalBudget,
    pub allowed_slots: Vec<String>,
    pub checkpoint_policy: GoalCheckpointPolicy,
    pub final_report_policy: GoalFinalReportPolicy,
    /// 收敛策略：判定 checkpoint 是"收敛"还是"原地打转"。
    /// 旧 on-disk goal JSON 无此字段时使用默认值（向后兼容）。
    #[serde(default)]
    pub convergence_policy: GoalConvergencePolicy,
}

/// 验收证据定义：目标完成时磁盘上必须出现的内容。
///
/// - `path`：证据文件路径（相对 goal root 解析）。
/// - `min_lines`：文件内容至少多少行（非空壳检查；None 表示不检查行数）。
/// - `min_content`：文件内容必须包含的子串（如 `RESULT=PASS`；None 表示不检查内容）。
/// - `description`：人类可读说明（show / diagnostics 展示用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalEvidence {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_lines: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl GoalEvidence {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            min_lines: None,
            min_content: None,
            description: None,
        }
    }

    pub fn with_min_lines(mut self, min_lines: usize) -> Self {
        self.min_lines = Some(min_lines);
        self
    }

    pub fn with_min_content(mut self, min_content: impl Into<String>) -> Self {
        self.min_content = Some(min_content.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// 类型化验收检查契约（verifier-first）。
///
/// 两种可评估的验收检查：
/// - `Evidence`：文件系统证据（存在性 + 行数 + 内容），运行时由 CLI/collect 只读判定；
/// - `Command`：命令验收（如 `cargo test`），运行时由 `goal verify` 显式执行判定。
///
/// 定义时 `validate()` 保证检查"可验证"（声明本身合法、有明确评估器）；
/// 运行时 `evaluate_*` 按证据产出 `AcceptanceVerdict`，不依赖模型自评。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AcceptanceCheck {
    /// 文件证据检查：目标完成后磁盘上必须出现的内容。
    Evidence(GoalEvidence),
    /// 命令验收检查：目标完成后必须通过的命令（如 `cargo test`）。
    Command(String),
}

impl AcceptanceCheck {
    /// 定义时可验证性：检查声明本身是否合法（错误路径清晰）。
    pub fn validate(&self) -> Result<(), GoalSpecError> {
        match self {
            AcceptanceCheck::Evidence(evidence) => {
                require_non_empty("acceptance_plan.checks[].path", &evidence.path)?;
                if let Some(min_lines) = evidence.min_lines {
                    if min_lines == 0 {
                        return Err(GoalSpecError::new(
                            "acceptance_plan.checks[].min_lines",
                            "min_lines must be greater than zero when set",
                        ));
                    }
                }
                if let Some(description) = evidence.description.as_deref() {
                    require_non_empty("acceptance_plan.checks[].description", description)?;
                }
                Ok(())
            }
            AcceptanceCheck::Command(command) => {
                require_non_empty("acceptance_plan.checks[].command", command)
            }
        }
    }

    /// 评估器类型：`evidence`（文件证据）或 `command`（命令验收）。
    pub fn evaluator(&self) -> &'static str {
        match self {
            AcceptanceCheck::Evidence(_) => "evidence",
            AcceptanceCheck::Command(_) => "command",
        }
    }

    /// 人类可读描述（show / verify 展示用）。
    pub fn description(&self) -> String {
        match self {
            AcceptanceCheck::Evidence(evidence) => evidence
                .description
                .clone()
                .unwrap_or_else(|| evidence.path.clone()),
            AcceptanceCheck::Command(command) => command.clone(),
        }
    }
}

/// verifier-first 验收检查契约：可验证（定义时）、可评估（运行时证据判定）。
///
/// 接口先行：goal 生命周期只依赖本 trait，不依赖具体检查实现。
/// `evaluate_contract` 的实现位于 goal_run（证据/命令评估器），
/// 由纯函数 `evaluate_acceptance_check` 承担。
pub trait AcceptanceCheckContract {
    fn validate_contract(&self) -> Result<(), GoalSpecError>;
    fn evaluator(&self) -> &'static str;
    fn description(&self) -> String;
    fn evaluate_contract(&self, root: &Path, check_index: usize) -> AcceptanceVerdict;
}

/// 类型化验收计划：goal 定义时声明的全部验收检查。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GoalAcceptancePlan {
    #[serde(default)]
    pub checks: Vec<AcceptanceCheck>,
}

impl GoalAcceptancePlan {
    pub fn new(checks: Vec<AcceptanceCheck>) -> Self {
        Self { checks }
    }

    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.checks.len()
    }

    /// 校验全部检查声明（逐条错误路径：空 path、min_lines=0、空 command 等）。
    pub fn validate(&self) -> Result<(), GoalSpecError> {
        for check in &self.checks {
            check.validate()?;
        }
        Ok(())
    }
}

/// 单条验收检查的运行时判定结果（证据在模型自述之外）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceVerdict {
    /// acceptance_plan.checks 中的下标。
    pub check_index: usize,
    /// 评估器类型：evidence / command。
    pub evaluator: String,
    pub description: String,
    pub passed: bool,
    /// passed=false 时说明失败原因；passed=true 时为 "ok"。
    pub reason: String,
    /// 命令检查的退出码（非命令检查为 None；超时/启动失败也为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// 收敛控制策略（Penguin goal-file 语义，最小接入）。
///
/// 核心规则：
/// - 每个 checkpoint 可携带规范化 `blocker_key`（同一失败原因的去重键）。
/// - 尾部连续相同 blocker_key（或完全相同的 validation_notes）达到
///   `max_repeated_blockers` 次 → 判定 blocked，禁止再以同策略重试。
///   blocked 审计：同一阻塞条件**连续 3 轮**才允许 blocked（防过早放弃），
///   默认值 `DEFAULT_MAX_REPEATED_BLOCKERS = 3`。
/// - `max_repeated_blockers = 0` 表示禁用重复卡点判定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalConvergencePolicy {
    #[serde(default = "default_max_repeated_blockers")]
    pub max_repeated_blockers: usize,
    #[serde(default = "default_require_progress_between_checkpoints")]
    pub require_progress_between_checkpoints: bool,
}

/// blocked 审计轮数：同一阻塞条件连续 3 轮才允许 blocked（防过早放弃）。
pub const DEFAULT_MAX_REPEATED_BLOCKERS: usize = 3;

fn default_max_repeated_blockers() -> usize {
    DEFAULT_MAX_REPEATED_BLOCKERS
}

fn default_require_progress_between_checkpoints() -> bool {
    true
}

impl Default for GoalConvergencePolicy {
    fn default() -> Self {
        Self {
            max_repeated_blockers: default_max_repeated_blockers(),
            require_progress_between_checkpoints: default_require_progress_between_checkpoints(),
        }
    }
}

/// goal 控制通道中的模型可写状态（Penguin GOAL.yaml 语义，升级点 1）。
///
/// - `Active`：系统持有（运行中默认），模型不可写。
/// - `Complete|Blocked`：模型唯一可写的两个值。
///   解析失败/非法值一律归一化为 `Blocked`（fail-closed：控制通道坏了就停，不空转）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum GoalStatus {
    #[default]
    Active,
    Complete,
    Blocked,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Complete => "complete",
            GoalStatus::Blocked => "blocked",
        }
    }
}

/// GOAL.yaml 收敛控制通道（Penguin goal-file 语义，升级点 1）。
///
/// - `objective` 由系统写：文件中出现的 objective 仅作为审计信息保留，
///   系统读取时永远以 `GoalSpec.objective`（系统持有值）为准，模型写入无效。
/// - `status` 模型唯一可写：只允许 `complete|blocked` 两个值。
/// - 解析失败（格式错误、status 缺失或非法值）归一化为 `Blocked`
///   （fail-closed：控制通道坏了就停，不空转）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalControlFile {
    /// 文件中出现的 objective（审计用；系统以自身持有值为准）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    /// 归一化后的 status（模型可写值；解析失败时为 Blocked）。
    pub status: GoalStatus,
}

impl GoalControlFile {
    /// 解析 GOAL.yaml 文本。status 只接受 `complete|blocked`（大小写不敏感）；
    /// 缺失、非法值或格式损坏一律归一化为 `Blocked`。
    pub fn parse(raw: &str) -> Self {
        let mut objective: Option<String> = None;
        let mut status_value: Option<String> = None;
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("objective:") {
                let value = rest.trim();
                if !value.is_empty() {
                    objective = Some(value.to_string());
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("status:") {
                status_value = Some(rest.trim().to_string());
                continue;
            }
        }
        let status = match status_value
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
        {
            Some(value) if value == "complete" => GoalStatus::Complete,
            Some(value) if value == "blocked" => GoalStatus::Blocked,
            // 缺失、非法值、损坏 → fail-closed：归一化为 blocked。
            _ => GoalStatus::Blocked,
        };
        Self { objective, status }
    }

    /// 渲染 GOAL.yaml：objective 由系统写（调用方传入系统持有值），
    /// status 反映当前值。
    pub fn render(objective: &str, status: GoalStatus) -> String {
        format!(
            "# GOAL control file\n\
# objective is system-written; model may only write status: complete|blocked\n\
objective: {objective}\n\
status: {}\n",
            status.as_str()
        )
    }
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
            status: GoalStatus::Active,
            acceptance_checks: vec![
                "cargo fmt --all".to_string(),
                "git diff --check".to_string(),
                "timeout 240s cargo test -q".to_string(),
            ],
            acceptance_evidence: Vec::new(),
            acceptance_plan: GoalAcceptancePlan::default(),
            budget: GoalBudget {
                max_minutes: Some(60),
                // 工具轮次不设默认上限：goal step 由 tool_loop.max_rounds
                // 兜底（上限 256），不给工具轮次长任务无法干活。
                max_tool_rounds: None,
                max_subtasks: Some(4),
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
            convergence_policy: GoalConvergencePolicy::default(),
        }
    }

    /// 设置验收证据（builder 风格）。
    pub fn with_evidence(mut self, evidence: Vec<GoalEvidence>) -> Self {
        self.acceptance_evidence = evidence;
        self
    }

    /// 设置类型化验收计划（verifier-first，builder 风格）。
    pub fn with_acceptance_plan(mut self, plan: GoalAcceptancePlan) -> Self {
        self.acceptance_plan = plan;
        self
    }

    /// GOAL.yaml 控制通道内容（系统写 objective；status 反映当前值）。
    /// 模型只能通过控制通道写 status（complete|blocked），objective 由系统写。
    pub fn control_file_contents(&self) -> String {
        GoalControlFile::render(&self.objective, self.status)
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
        for (index, evidence) in self.acceptance_evidence.iter().enumerate() {
            require_non_empty(
                &format!("acceptance_evidence[{index}].path"),
                &evidence.path,
            )?;
            if let Some(min_lines) = evidence.min_lines {
                if min_lines == 0 {
                    return Err(GoalSpecError::new(
                        &format!("acceptance_evidence[{index}].min_lines"),
                        "min_lines must be greater than zero when set",
                    ));
                }
            }
            if let Some(description) = evidence.description.as_deref() {
                require_non_empty(
                    &format!("acceptance_evidence[{index}].description"),
                    description,
                )?;
            }
        }
        self.acceptance_plan.validate()?;
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
        if self.convergence_policy.max_repeated_blockers != 0
            && self.convergence_policy.max_repeated_blockers < 2
        {
            return Err(GoalSpecError::new(
                "convergence_policy.max_repeated_blockers",
                "max_repeated_blockers must be 0 (disabled) or at least 2",
            ));
        }
        Ok(())
    }

    pub fn render_context_block(&self) -> Result<String, GoalSpecError> {
        self.validate()?;
        let acceptance_plan_block = if self.acceptance_plan.is_empty() {
            String::new()
        } else {
            format!(
                "acceptance_plan:\n{}\n",
                self.acceptance_plan
                    .checks
                    .iter()
                    .map(|check| { format!("- [{}] {}", check.evaluator(), check.description()) })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        Ok(format!(
            "GOAL_SPEC\n\
goal_id: {}\n\
objective: {}\n\
acceptance_checks:\n{}\n\
{}\
allowed_slots: {}\n\
budget: max_minutes={} max_tool_rounds={} max_subtasks={}\n\
checkpoint_policy: progress_log={} handoff={} commit={}\n\
final_report_policy: validation={} next_steps={}\n\
convergence_policy: max_repeated_blockers={} require_progress={}",
            self.goal_id,
            self.objective,
            render_list(&self.acceptance_checks),
            acceptance_plan_block,
            self.allowed_slots.join(","),
            render_optional(self.budget.max_minutes),
            render_optional(self.budget.max_tool_rounds),
            render_optional(self.budget.max_subtasks),
            self.checkpoint_policy.update_progress_log,
            self.checkpoint_policy.update_handoff,
            self.checkpoint_policy.commit_checkpoint,
            self.final_report_policy.include_validation,
            self.final_report_policy.include_next_steps,
            self.convergence_policy.max_repeated_blockers,
            self.convergence_policy.require_progress_between_checkpoints,
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

fn estimate_goal_tokens(content: &str) -> u32 {
    content.chars().count().div_ceil(4).min(u32::MAX as usize) as u32
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
