//! Prompt / norm layering: thick on disk, thin in context.
//!
//! Includes distilled CC harness discipline plus dad's operating theorems:
//! Occam (dev subtract) / Murphy (accept add) / Coase (delegate) / grill-clarify / no optional commentary.

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
const SKILL_OCCAM: &str = include_str!("../assets/norm/skills/occam-develop.md");
const SKILL_MURPHY: &str = include_str!("../assets/norm/skills/murphy-accept.md");
const SKILL_FIRST: &str = include_str!("../assets/norm/skills/first-principles.md");
const SKILL_ADV: &str = include_str!("../assets/norm/skills/adversarial-review.md");
const SKILL_GRILL: &str = include_str!("../assets/norm/skills/grill-clarify.md");
const SKILL_COASE: &str = include_str!("../assets/norm/skills/coase-delegate.md");
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

pub fn doctrine_card_segment() -> ContextSegment {
    segment(DOCTRINE_CARD_ID, DOCTRINE_CARD, 253, "doctrine_card")
}

pub fn skill_index_segment() -> ContextSegment {
    segment(SKILL_INDEX_ID, SKILL_INDEX, 252, "skill_index")
}

pub fn dispatch_worker_brief() -> &'static str {
    DISPATCH_WORKER_BRIEF.trim()
}

pub fn wrap_task_for_worker(task: &str) -> String {
    format!("{}\n\n---\n任务：\n{}", dispatch_worker_brief(), task.trim())
}

struct SkillSpec {
    id: &'static str,
    body: &'static str,
    matcher: fn(&str) -> bool,
}

/// Order = priority when multiple match (cap 2).
const SKILLS: &[SkillSpec] = &[
    SkillSpec {
        id: "norm-skill-grill-clarify",
        body: SKILL_GRILL,
        matcher: matches_grill,
    },
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
        id: "norm-skill-first-principles",
        body: SKILL_FIRST,
        matcher: matches_first_principles,
    },
    SkillSpec {
        id: "norm-skill-coding-dispatch",
        body: SKILL_CODING,
        matcher: matches_coding,
    },
    SkillSpec {
        id: "norm-skill-occam-develop",
        body: SKILL_OCCAM,
        matcher: matches_occam,
    },
    SkillSpec {
        id: "norm-skill-surgical-diff",
        body: SKILL_SURGICAL,
        matcher: matches_surgical,
    },
    SkillSpec {
        id: "norm-skill-murphy-accept",
        body: SKILL_MURPHY,
        matcher: matches_murphy,
    },
    SkillSpec {
        id: "norm-skill-verify-before-claim",
        body: SKILL_VERIFY,
        matcher: matches_verify,
    },
    SkillSpec {
        id: "norm-skill-adversarial-review",
        body: SKILL_ADV,
        matcher: matches_adversarial,
    },
    SkillSpec {
        id: "norm-skill-coase-delegate",
        body: SKILL_COASE,
        matcher: matches_coase,
    },
    SkillSpec {
        id: "norm-skill-think-before-act",
        body: SKILL_THINK,
        matcher: matches_think,
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

fn matches_grill(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "不清楚",
        "你先问我",
        "帮我理清",
        "需求模糊",
        "grill",
        "追问",
        "到底要什么",
        "确认需求",
    ];
    KEYS.iter().any(|k| text.contains(k))
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

fn matches_first_principles(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "第一性",
        "从原理",
        "first principle",
        "根上",
        "本质原因",
        "机制是什么",
    ];
    KEYS.iter().any(|k| text.contains(k))
        || (text.contains("架构") && (text.contains("设计") || text.contains("方案")))
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
        "加个按钮",
        "做一个",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_occam(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "奥卡姆",
        "剃刀",
        "别加戏",
        "不要多余",
        "最小实现",
        "只要",
        "occam",
        "别过度",
        "别炫技",
    ];
    KEYS.iter().any(|k| text.contains(k)) || matches_coding(text)
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
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_murphy(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "墨菲",
        "验收",
        "最坏",
        "边界测试",
        "失败路径",
        "murphy",
        "回归",
        "全过",
        "全部通过",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_verify(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "做完了吗",
        "完成了吗",
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

fn matches_adversarial(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "对抗审查",
        "对抗式",
        "找茬",
        "挑刺",
        "审查",
        "code review",
        "adversarial",
        "多agent审",
        "多代理审",
    ];
    KEYS.iter().any(|k| text.contains(k))
}

fn matches_coase(text: &str) -> bool {
    const KEYS: &[&str] = &[
        "科斯",
        "外包吗",
        "要不要派",
        "自己干还是",
        "子代理吗",
        "coase",
        "派不派",
    ];
    KEYS.iter().any(|k| text.contains(k))
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

pub fn norm_context_segments(user_input: &str) -> Vec<ContextSegment> {
    let mut segments = vec![doctrine_card_segment(), skill_index_segment()];
    segments.extend(on_demand_skill_segments(user_input));
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctrine_has_occam_murphy_coase_no_commentary() {
        let card = doctrine_card_segment();
        assert!(card.content.contains("剃刀") || card.content.contains("奥卡姆"));
        assert!(card.content.contains("墨菲"));
        assert!(card.content.contains("科斯"));
        assert!(card.content.contains("optional commentary") || card.content.contains("旁白"));
    }

    #[test]
    fn coding_loads_occam_or_coding() {
        let segs = on_demand_skill_segments("帮我加个按钮");
        assert!(!segs.is_empty());
        assert!(segs.len() <= 2);
        assert!(segs.iter().any(|s| {
            s.id == "norm-skill-coding-dispatch" || s.id == "norm-skill-occam-develop"
        }));
    }

    #[test]
    fn accept_loads_murphy() {
        let segs = on_demand_skill_segments("帮我验收一下是不是全部通过");
        assert!(segs.iter().any(|s| {
            s.id == "norm-skill-murphy-accept" || s.id == "norm-skill-verify-before-claim"
        }));
    }

    #[test]
    fn wrap_task_forbids_optional_commentary() {
        let w = wrap_task_for_worker("x");
        assert!(w.contains("optional commentary") || w.contains("奥卡姆") || w.contains("墨菲"));
    }

    #[test]
    fn casual_no_skill() {
        assert!(on_demand_skill_segments("今天天气怎么样").is_empty());
    }
}
