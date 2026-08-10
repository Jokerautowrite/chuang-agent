//! `governance::rules_markdown` 模块。公开接口：struct MarkdownRuleSet, RuleCheck；fn load, from_content, path, check。

use std::fs;
use std::path::{Path, PathBuf};

use super::ProposedAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownRuleSet {
    path: PathBuf,
    content: String,
    rule_count: usize,
    fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleCheck {
    pub rule_count: usize,
    pub fingerprint: String,
    pub summary: String,
}

impl MarkdownRuleSet {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("rules_read_failed path={}: {e}", path.display()))?;
        Self::from_content(path, content)
    }

    pub fn from_content(path: impl Into<PathBuf>, content: String) -> Result<Self, String> {
        if content.trim().is_empty() {
            return Err("rules_empty: rules markdown must not be empty".to_string());
        }

        let rule_count = count_rules(&content);
        if rule_count == 0 {
            return Err(
                "rules_invalid: expected at least one numbered or bulleted rule".to_string(),
            );
        }

        Ok(Self {
            path: path.into(),
            fingerprint: fingerprint(&content),
            content,
            rule_count,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn check(&self, action: &ProposedAction) -> RuleCheck {
        RuleCheck {
            rule_count: self.rule_count,
            fingerprint: self.fingerprint.clone(),
            summary: format!(
                "rules={} fingerprint={} action={} target={}",
                self.rule_count, self.fingerprint, action.action_id, action.target
            ),
        }
    }
}

fn count_rules(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                    && trimmed.contains(". ")
        })
        .count()
}

fn fingerprint(content: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
