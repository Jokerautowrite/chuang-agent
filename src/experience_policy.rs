//! Deterministic admission policy for cross-session experience memory.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExperienceWritePolicy {
    Disabled,
    #[default]
    Deterministic,
    Always,
}

impl ExperienceWritePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Deterministic => "deterministic",
            Self::Always => "always",
        }
    }

    pub fn evaluate(self, candidate: &ExperienceCandidate<'_>) -> ExperienceAdmissionDecision {
        match self {
            Self::Disabled => rejected(ExperienceAdmissionReason::PolicyDisabled),
            Self::Always => accepted(ExperienceAdmissionReason::ExplicitRequest),
            Self::Deterministic => evaluate_deterministic(candidate),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExperienceCandidate<'a> {
    pub user_input: &'a str,
    pub summary: &'a str,
    pub lesson: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperienceAdmissionDecision {
    Accepted { reason: ExperienceAdmissionReason },
    Rejected { reason: ExperienceAdmissionReason },
}

impl ExperienceAdmissionDecision {
    pub fn should_write(self) -> bool {
        matches!(self, Self::Accepted { .. })
    }

    pub fn reason(self) -> ExperienceAdmissionReason {
        match self {
            Self::Accepted { reason } | Self::Rejected { reason } => reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperienceAdmissionReason {
    ExplicitRequest,
    DurableDecision,
    DurableConstraint,
    DurablePreference,
    ReusableWorkflow,
    PolicyDisabled,
    EmptyOrTrivial,
    CodeOrImplementationDetail,
    GitHistory,
    OneOffDebugging,
    ProjectInstructionDuplicate,
    TemporaryDetail,
    NoDurableSignal,
}

impl ExperienceAdmissionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitRequest => "explicit_request",
            Self::DurableDecision => "durable_decision",
            Self::DurableConstraint => "durable_constraint",
            Self::DurablePreference => "durable_preference",
            Self::ReusableWorkflow => "reusable_workflow",
            Self::PolicyDisabled => "policy_disabled",
            Self::EmptyOrTrivial => "empty_or_trivial",
            Self::CodeOrImplementationDetail => "code_or_implementation_detail",
            Self::GitHistory => "git_history",
            Self::OneOffDebugging => "one_off_debugging",
            Self::ProjectInstructionDuplicate => "project_instruction_duplicate",
            Self::TemporaryDetail => "temporary_detail",
            Self::NoDurableSignal => "no_durable_signal",
        }
    }
}

fn evaluate_deterministic(candidate: &ExperienceCandidate<'_>) -> ExperienceAdmissionDecision {
    let text = format!(
        "{}\n{}\n{}",
        candidate.user_input, candidate.summary, candidate.lesson
    )
    .to_lowercase();
    let trimmed = text.trim();
    if trimmed.chars().count() < 8 || ["ok", "好的", "完成", "done"].contains(&trimmed) {
        return rejected(ExperienceAdmissionReason::EmptyOrTrivial);
    }
    // 纯问候/寒暄（无实质内容）不沉淀经验：比如「哈喽，在不在？」「在吗」「你好呀」。
    // 这类对话对跨会话没有可复用价值，且会白白占用 experiences 容量导致后续写入超限。
    if is_pure_greeting(trimmed) {
        return rejected(ExperienceAdmissionReason::EmptyOrTrivial);
    }
    if contains_any(
        trimmed,
        &[
            "claude.md",
            "agents.md",
            "rules.md",
            "项目说明文件",
            "项目规则文件",
        ],
    ) {
        return rejected(ExperienceAdmissionReason::ProjectInstructionDuplicate);
    }
    if contains_any(
        trimmed,
        &[
            "git commit",
            "git status",
            "git log",
            "commit hash",
            "提交历史",
            "分支历史",
        ],
    ) {
        return rejected(ExperienceAdmissionReason::GitHistory);
    }
    if contains_any(
        trimmed,
        &[
            "debug",
            "调试",
            "堆栈",
            "stack trace",
            "临时修复",
            "这次报错",
            "复现步骤",
        ],
    ) {
        return rejected(ExperienceAdmissionReason::OneOffDebugging);
    }
    if contains_any(
        trimmed,
        &[
            "```",
            "fn ",
            "struct ",
            "impl ",
            "const ",
            "src/",
            ".rs:",
            "代码片段",
        ],
    ) {
        return rejected(ExperienceAdmissionReason::CodeOrImplementationDetail);
    }
    if contains_any(
        trimmed,
        &[
            "临时",
            "暂时",
            "本轮",
            "今天",
            "当前进度",
            "pid ",
            "端口占用",
            "一次性",
            "temporary",
            "for now",
        ],
    ) {
        return rejected(ExperienceAdmissionReason::TemporaryDetail);
    }
    let durable = if contains_any(
        trimmed,
        &[
            "用户偏好",
            "我的偏好",
            "我喜欢",
            "我不喜欢",
            "prefer",
            "preference",
        ],
    ) {
        Some(ExperienceAdmissionReason::DurablePreference)
    } else if contains_any(
        trimmed,
        &[
            "长期约束",
            "以后不要",
            "以后必须",
            "始终",
            "永远不要",
            "不得",
            "禁止",
            "硬约束",
            "constraint",
            "must not",
            "always",
        ],
    ) {
        Some(ExperienceAdmissionReason::DurableConstraint)
    } else if contains_any(
        trimmed,
        &[
            "长期决定",
            "架构决策",
            "决定采用",
            "统一使用",
            "后续统一",
            "decision",
            "decided to",
            "standardize on",
        ],
    ) {
        Some(ExperienceAdmissionReason::DurableDecision)
    } else if contains_any(
        trimmed,
        &[
            "可复用流程",
            "标准流程",
            "固定流程",
            "工作流",
            "操作顺序",
            "以后遇到",
            "复用这个方法",
            "workflow",
            "runbook",
            "procedure",
        ],
    ) {
        Some(ExperienceAdmissionReason::ReusableWorkflow)
    } else {
        None
    };
    if let Some(reason) = durable {
        return accepted(reason);
    }
    rejected(ExperienceAdmissionReason::NoDurableSignal)
}

fn is_pure_greeting(text: &str) -> bool {
    const GREETING_WORDS: &[&str] = &[
        "在不在",
        "在吗",
        "哈喽",
        "hello",
        "你好",
        "hi ",
        "hey",
        "早上好",
        "下午好",
        "晚上好",
        "辛苦了",
        "谢谢",
        "好的收到",
        "收到",
    ];
    let compact = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '？' && *c != '?' && *c != '！' && *c != '!' && *c != '，' && *c != ',')
        .collect::<String>();
    // 去掉问候词后剩余内容极少，视为纯寒暄。
    let mut remainder = compact.clone();
    for word in GREETING_WORDS {
        remainder = remainder.replace(word, "");
    }
    remainder.trim().chars().count() <= 6
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}
fn accepted(reason: ExperienceAdmissionReason) -> ExperienceAdmissionDecision {
    ExperienceAdmissionDecision::Accepted { reason }
}
fn rejected(reason: ExperienceAdmissionReason) -> ExperienceAdmissionDecision {
    ExperienceAdmissionDecision::Rejected { reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(text: &str) -> ExperienceCandidate<'_> {
        ExperienceCandidate {
            user_input: text,
            summary: "",
            lesson: "",
        }
    }

    #[test]
    fn keeps_durable_memory() {
        for (text, reason) in [
            (
                "用户偏好：飞书回复保持简洁。",
                ExperienceAdmissionReason::DurablePreference,
            ),
            (
                "长期约束：以后不要自动删除文件。",
                ExperienceAdmissionReason::DurableConstraint,
            ),
            (
                "架构决策：统一使用 SQLite 保存会话。",
                ExperienceAdmissionReason::DurableDecision,
            ),
            (
                "可复用流程：部署前先跑测试，再检查服务状态。",
                ExperienceAdmissionReason::ReusableWorkflow,
            ),
        ] {
            assert_eq!(
                ExperienceWritePolicy::Deterministic.evaluate(&candidate(text)),
                ExperienceAdmissionDecision::Accepted { reason }
            );
        }
    }

    #[test]
    fn rejects_noise() {
        for (text, reason) in [
            (
                "```rust\nfn main() {}\n```",
                ExperienceAdmissionReason::CodeOrImplementationDetail,
            ),
            (
                "git log 显示上一条 commit hash 是 abc。",
                ExperienceAdmissionReason::GitHistory,
            ),
            (
                "这次报错的调试复现步骤是重启一次。",
                ExperienceAdmissionReason::OneOffDebugging,
            ),
            (
                "暂时使用 127.0.0.1:9000，今天测试完再说。",
                ExperienceAdmissionReason::TemporaryDetail,
            ),
            (
                "把这条内容再写进 CLAUDE.md。",
                ExperienceAdmissionReason::ProjectInstructionDuplicate,
            ),
        ] {
            assert_eq!(
                ExperienceWritePolicy::Deterministic.evaluate(&candidate(text)),
                ExperienceAdmissionDecision::Rejected { reason }
            );
        }
    }

    #[test]
    fn explicit_write_bypasses_filter() {
        assert!(ExperienceWritePolicy::Always
            .evaluate(&candidate("git status"))
            .should_write());
    }
}
