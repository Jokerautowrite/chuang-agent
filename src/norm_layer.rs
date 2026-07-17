//! Prompt / norm layering: thick on disk, thin in context (Grok-style assembly).
//!
//! Claude Code materials (Piebald fragments, learn-claude-code patterns) are distilled into
//! short Chinese assets under `assets/norm/` — not pasted as Anthropic originals.
//!
//! Categories: always-on card · on-demand skills · dispatch-only worker brief · disk-only docs.

use crate::context_engine::{ContextSegment, SegmentSource};

pub const DOCTRINE_CARD_ID: &str = "norm-doctrine-card";
pub const SKILL_INDEX_ID: &str = "norm-skill-index";

const DOCTRINE_CARD: &str = include_str!("../assets/norm/doctrine-card.txt");
const SKILL_INDEX: &str = include_str!("../assets/norm/skill-index.txt");
const DISPATCH_WORKER_BRIEF: &str = include_str!("../assets/norm/dispatch-worker-brief.txt");

const SKILL_EXPLORE: &str = include_str!("../assets/norm/skills/explore.md");
const SKILL_PLAN: &str = include_str!("../assets/norm/skills/plan.md");
const SKILL_CODING: &str = include_str!("../assets/norm/skills/coding-dispatch.md");
const SKILL_SURGICAL: &str = include_str!("../assets/norm/skills/surgical-diff.md");
const SKILL_THINK: &str = include_str!("../assets/norm/skills/think-before-act.md");
const SKILL_VERIFY: &str = include_str!("../assets/norm/skills/verify-before-claim.md");
const SKILL_COMPACT: &str = include_str!("../assets/norm/skills/compact-handoff.md");
const SKILL_TRIAGE: &str = include_str!("../assets/norm/skills/readonly-triage.md");

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

/// Dispatch-only brief prepended to worker tasks (category C).
pub fn dispatch_worker_brief() -> &'static str {
    DISPATCH_WORKER_BRIEF.trim()
}

/// Wrap a user/model task with the worker brief for subagent dispatch.
pub fn wrap_task_for_worker(task: &str) -> String {
    format!("{}\n\n---\n任务：\n{}", dispatch_worker_brief(), task.trim())
}

struct SkillSpec {
    id: &'static str,
    body: &'static str,
    matcher: fn(&str) -> bool,
}

const SKILLS: &[SkillSpec] = &[
    SkillSpec {
        id: "norm-skill-explore",
        body: SKILL_EXPLORE,
        matcher: matches_explore,
    },
    SkillSpec {
        id: "norm-skill-plan",
        body: SKILL_PLAN,
        matcher: matches_plan,
    },
    SkillSpec {
        id: "norm-skill-coding-dispatch",
        body: SKILL_CODING,
        matcher: matches_coding,
    },
    SkillSpec {
        id: "norm-skill-surgical-diff",
        body: SKILL_SURGICAL,
        matcher: matches_surgical,
    },
    SkillSpec {
        id: "norm-skill-think-before-act",
        body: SKILL_THINK,
        matcher: matches_think,
    },
    SkillSpec {
        id: "norm-skill-verify-before-claim",
        body: SKILL_VERIFY,
        matcher: matches_verify,
    },
    SkillSpec {
        id: "norm-skill-compact-handoff",
        body: SKILL_COMPACT,
        matcher: matches_compact,
    },
    SkillSpec {
        id: "norm-skill-readonly-triage",
        body: SKILL_TRIAGE,
        matcher: matches_readonly_triage,
    },
];

/// On-demand skills selected from user text (category B). Cap count to protect budget.
pub fn on_demand_skill_segments(user_input: &str) -> Vec<ContextSegment> {
    let text = user_input.to_ascii_lowercase();
    let mut out = Vec::new();
    for skill in SKILLS {
        if (skill.matcher)(&text) {
            out.push(segment(skill.id, skill.body, 200, "skill_ondemand"));
        }
        if out.len() >= 2 {
            break;
        }
    }
    out
}

fn matches_explore(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "在哪",
        "哪里",
        "定位",
        "搜索代码",
        "谁引用",
        "谁调用",
        "定义在",
        "find where",
        "where is",
        "grep",
        "explore",
        "摸清",
        "扫一遍",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_plan(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "方案",
        "计划",
        "怎么改",
        "如何实现",
        "实施计划",
        "设计一下",
        "架构",
        "plan",
        "roadmap",
        "分步",
    ];
    KEYS.iter().any(|k| text.contains(k))
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
        "补丁",
        "测试失败",
        "编译",
        "加功能",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_surgical(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "最小改动",
        "别重构",
        "不要重构",
        "surgical",
        "只改",
        "精确改",
        "小改",
    ];
    KEYS.iter().any(|k| text.contains(k)) || matches_coding(text)
}

fn matches_think(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "大改",
        "想清楚",
        "有没有更简",
        "权衡",
        "取舍",
        "先讨论",
        "tradeoff",
        "架构决策",
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
        "修好了吗",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_compact(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "交接",
        "总结会话",
        "上下文满",
        "compact",
        "handoff",
        "续作摘要",
        "压缩上下文",
        "换会话",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_readonly_triage(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "排查",
        "定位原因",
        "为什么",
        "怎么回事",
        "只读",
        "分析原因",
        "诊断",
        "triage",
        "root cause",
        "挂了",
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
        assert!(card.content.chars().count() < 900);
        assert!(card.content.contains("调度台"));
        assert!(card.content.contains("如实"));
        assert_eq!(card.priority, 253);
    }

    #[test]
    fn coding_input_loads_coding_or_surgical() {
        let segs = on_demand_skill_segments("帮我修 bug 并跑 cargo test");
        assert!(!segs.is_empty());
        assert!(segs.len() <= 2);
        assert!(segs.iter().any(|s| {
            s.id == "norm-skill-coding-dispatch" || s.id == "norm-skill-surgical-diff"
        }));
    }

    #[test]
    fn explore_input_loads_explore() {
        let segs = on_demand_skill_segments("这个符号在哪定义的");
        assert!(segs.iter().any(|s| s.id == "norm-skill-explore"));
    }

    #[test]
    fn plan_input_loads_plan() {
        let segs = on_demand_skill_segments("先给一个实施计划再动手");
        assert!(segs.iter().any(|s| s.id == "norm-skill-plan"));
    }

    #[test]
    fn casual_input_loads_no_ondemand_skill() {
        let segs = on_demand_skill_segments("今天天气怎么样");
        assert!(segs.is_empty());
    }

    #[test]
    fn wrap_task_includes_worker_fork_style_brief() {
        let wrapped = wrap_task_for_worker("读 Cargo.toml 返回包名");
        assert!(wrapped.contains("工人简报") || wrapped.contains("worker"));
        assert!(wrapped.contains("读 Cargo.toml"));
        assert!(wrapped.contains("禁止") || wrapped.contains("不要反问"));
    }
}
