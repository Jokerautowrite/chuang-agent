//! EmotionBrain：情感外脑桥接（EmotionSlot ↔ GBrain）。
//!
//! 原则：
//! - 情感核心（emotion_slot）不依赖外脑；外脑只做「更懂主人」的可选增强。
//! - GBrain 是脱敏共享脑图（只读口径），私人情绪记忆走创自己的 MemoryStore，
//!   不写共享 wiki。这里只查询主人相关的历史偏好/上下文，摘要点 + slug 进 prompt。
//!
//! 只读调用本机 CLI：`agent-hub-brain-query semantic <query> <limit>`（JSON 输出）。

use std::env;
use std::process::Command;
use std::str;

use crate::emotion_slot::EmotionStateSnapshot;

/// GBrain 查询命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainHit {
    pub title: String,
    pub slug: String,
    pub snippet: String,
}

/// 情感外脑查询配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmotionBrainConfig {
    /// agent-hub-brain-query 可执行文件路径。
    pub query_bin: String,
    /// 默认检索条数。
    pub default_limit: usize,
}

impl Default for EmotionBrainConfig {
    fn default() -> Self {
        // 默认走环境变量覆盖，避免硬编码本机绝对路径；
        // 未设置时回退到 PATH 查找（同名命令）。
        let query_bin = env::var("CHUANG_BRAIN_QUERY_BIN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "agent-hub-brain-query".to_string());
        Self {
            query_bin,
            default_limit: 3,
        }
    }
}

/// 解析 `agent-hub-brain-query semantic` 的 JSON 输出。
pub fn parse_brain_query_json(raw: &str) -> Vec<BrainHit> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(results) = value.get("results").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    results
        .iter()
        .filter_map(|item| {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let slug = item.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let text = item.get("chunk_text").and_then(|v| v.as_str()).unwrap_or("");
            if title.is_empty() && slug.is_empty() {
                return None;
            }
            // 摘要点：取前 3 行非空文本，去重，限长。
            let snippet = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .take(3)
                .collect::<Vec<_>>()
                .join(" | ");
            Some(BrainHit {
                title: title.to_string(),
                slug: slug.to_string(),
                snippet: snippet.chars().take(220).collect(),
            })
        })
        .collect()
}

/// 查询 GBrain（只读）。失败返回空列表（外脑不可用不应阻断情感主流程）。
pub fn brain_query_semantic(
    config: &EmotionBrainConfig,
    query: &str,
    limit: usize,
) -> Vec<BrainHit> {
    let output = Command::new(&config.query_bin)
        .arg("semantic")
        .arg(query)
        .arg(limit.to_string())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(raw) = str::from_utf8(&output.stdout) else {
        return Vec::new();
    };
    parse_brain_query_json(raw)
}

/// 把外脑命中摘要追加到情感快照的 prompt_context（摘要点 + slug，不贴原文）。
pub fn augment_prompt_context(
    snapshot: &EmotionStateSnapshot,
    hits: &[BrainHit],
) -> String {
    if hits.is_empty() {
        return snapshot.prompt_context.clone();
    }
    let mut context = snapshot.prompt_context.clone();
    context.push_str("\n\n[外脑记忆] 与主人相关的已知上下文：");
    for hit in hits {
        context.push_str(&format!(
            "\n- {}（{}）: {}",
            hit.title, hit.slug, hit.snippet
        ));
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brain_query_json_results() {
        let raw = r#"{
            "ok": true,
            "results": [
                {
                    "title": "主人偏好",
                    "slug": "user/preferences",
                    "chunk_text": "主人喜欢简洁直接的回应。\n不喜欢客套。\n记录于 8-6。"
                },
                {
                    "title": "情绪记录",
                    "slug": "user/mood-history",
                    "chunk_text": "8-6 晚上情绪低落。"
                }
            ]
        }"#;
        let hits = parse_brain_query_json(raw);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "主人偏好");
        assert_eq!(hits[0].slug, "user/preferences");
        assert!(hits[0].snippet.contains("简洁直接"));
    }

    #[test]
    fn parse_ignores_malformed_json() {
        assert_eq!(parse_brain_query_json("not json"), vec![]);
        assert_eq!(parse_brain_query_json(r#"{"ok":true}"#), vec![]);
    }

    #[test]
    #[ignore = "live GBrain integration - run manually"]
    fn live_brain_query_parses_real_cli_output() {
        let hits = brain_query_semantic(&EmotionBrainConfig::default(), "主人 偏好 情绪", 3);
        assert!(!hits.is_empty(), "GBrain should return hits");
        for hit in hits.iter().take(3) {
            assert!(!hit.slug.is_empty());
        }
    }

    #[test]
    fn augment_appends_summary_only_when_hits_exist() {
        let snapshot = EmotionStateSnapshot {
            axes: Default::default(),
            prompt_context: "当前情绪状态：心情平稳".to_string(),
            style_guidance: String::new(),
            last_tick_at: None,
        };
        let plain = augment_prompt_context(&snapshot, &[]);
        assert_eq!(plain, snapshot.prompt_context);
        let hits = vec![BrainHit {
            title: "主人偏好".to_string(),
            slug: "user/preferences".to_string(),
            snippet: "喜欢简洁".to_string(),
        }];
        let augmented = augment_prompt_context(&snapshot, &hits);
        assert!(augmented.contains("外脑记忆"));
        assert!(augmented.contains("user/preferences"));
        assert!(augmented.contains("喜欢简洁"));
    }
}
