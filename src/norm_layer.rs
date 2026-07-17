//! Prompt / norm layering: thick on disk, thin in context (Grok-style assembly).
//!
//! Categories: always-on card · on-demand skills · dispatch-only worker brief · disk-only docs.

use crate::context_engine::{ContextSegment, SegmentSource};

pub const DOCTRINE_CARD_ID: &str = "norm-doctrine-card";
pub const SKILL_INDEX_ID: &str = "norm-skill-index";

const DOCTRINE_CARD: &str = include_str!("../assets/norm/doctrine-card.txt");
const SKILL_INDEX: &str = include_str!("../assets/norm/skill-index.txt");
const DISPATCH_WORKER_BRIEF: &str = include_str!("../assets/norm/dispatch-worker-brief.txt");
const SKILL_CODING_DISPATCH: &str = include_str!("../assets/norm/skills/coding-dispatch.md");
const SKILL_VERIFY: &str = include_str!("../assets/norm/skills/verify-before-claim.md");
const SKILL_READONLY: &str = include_str!("../assets/norm/skills/readonly-triage.md");

fn stamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-18T00:00:00Z")
        .expect("static timestamp")
        .with_timezone(&chrono::Utc)
}

fn segment(id: &str, content: &str, priority: u8, kind: &str) -> ContextSegment {
    let content = content.trim().to_string();
    let tokens = content.chars().count().min(u32::MAX as usize) as u32;
    let now = stamp();
    ContextSegment {
        id: id.to_string(),
        source: SegmentSource::System,
        tokens: Some(tokens),
        content,
        priority,
        created_at: now,
        last_accessed: now,
        metadata: std::collections::HashMap::from([("kind".to_string(), kind.to_string())]),
    }
}

/// Always-on thin doctrine card (category A).
pub fn doctrine_card_segment() -> ContextSegment {
    segment(DOCTRINE_CARD_ID, DOCTRINE_CARD, 253, "doctrine_card")
}

/// Always-on skill index only — not full skill bodies (category A index).
pub fn skill_index_segment() -> ContextSegment {
    segment(SKILL_INDEX_ID, SKILL_INDEX, 252, "skill_index")
}

/// Dispatch-only brief prepended to worker tasks (category C). Never for main persona alone.
pub fn dispatch_worker_brief() -> &'static str {
    DISPATCH_WORKER_BRIEF.trim()
}

/// Wrap a user/model task with the worker brief for subagent dispatch.
pub fn wrap_task_for_worker(task: &str) -> String {
    format!("{}\n\n---\n任务：\n{}", dispatch_worker_brief(), task.trim())
}

/// On-demand skills selected from user text (category B). Cap count to protect budget.
pub fn on_demand_skill_segments(user_input: &str) -> Vec<ContextSegment> {
    let text = user_input.to_ascii_lowercase();
    let mut out = Vec::new();

    if matches_coding(&text) {
        out.push(segment(
            "norm-skill-coding-dispatch",
            SKILL_CODING_DISPATCH,
            200,
            "skill_ondemand",
        ));
    }
    if matches_verify(&text) {
        out.push(segment(
            "norm-skill-verify-before-claim",
            SKILL_VERIFY,
            200,
            "skill_ondemand",
        ));
    }
    if matches_readonly_triage(&text) {
        out.push(segment(
            "norm-skill-readonly-triage",
            SKILL_READONLY,
            200,
            "skill_ondemand",
        ));
    }

    // Hard cap: at most 2 on-demand skills per turn.
    out.truncate(2);
    out
}

fn matches_coding(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "写代码",
        "改代码",
        "修bug",
        "修 bug",
        "重构",
        "实现",
        "cargo test",
        "cargo build",
        "fix",
        "implement",
        "refactor",
        "pr ",
        "补丁",
        "测试失败",
        "编译",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_verify(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "做完了吗",
        "完成了吗",
        "验收",
        "验证",
        "确认修好",
        "是否修好",
        "测试通过",
        "verify",
        "done?",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_readonly_triage(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "排查",
        "定位",
        "为什么",
        "怎么回事",
        "只读",
        "分析原因",
        "诊断",
        "triage",
        "root cause",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

/// Segments to inject every turn: doctrine + index + on-demand hits.
pub fn norm_context_segments(user_input: &str) -> Vec<ContextSegment> {
    let mut segments = vec![doctrine_card_segment(), skill_index_segment()];
    segments.extend(on_demand_skill_segments(user_input));
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctrine_card_is_short() {
        let card = doctrine_card_segment();
        assert!(card.content.chars().count() < 800);
        assert!(card.content.contains("调度台"));
        assert_eq!(card.priority, 253);
    }

    #[test]
    fn coding_input_loads_coding_skill() {
        let segs = on_demand_skill_segments("帮我修 bug 并跑 cargo test");
        assert!(segs.iter().any(|s| s.id == "norm-skill-coding-dispatch"));
        assert!(segs.len() <= 2);
    }

    #[test]
    fn casual_input_loads_no_ondemand_skill() {
        let segs = on_demand_skill_segments("今天天气怎么样");
        assert!(segs.is_empty());
    }

    #[test]
    fn wrap_task_includes_brief_and_task() {
        let wrapped = wrap_task_for_worker("读 Cargo.toml 返回包名");
        assert!(wrapped.contains("工人简报"));
        assert!(wrapped.contains("读 Cargo.toml"));
        assert!(wrapped.contains("禁止"));
    }
}
