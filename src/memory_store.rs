use std::collections::BTreeMap;

mod in_memory;

pub use in_memory::InMemoryMemoryStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub metadata: BTreeMap<String, String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryQuery {
    pub text: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub record: MemoryRecord,
    pub score: u32,
}

/// 文本匹配打分（供各 MemoryStore::search 实现复用）。
///
/// - 整句精确包含：最高分（权重 ×10），保证确定性召回。
/// - 否则按检索 token 命中：ASCII 词（≥2 字符）+ 中文 2~4 字滑窗；
///   任一 token 命中即召回，得分 = 命中 token 字符数。
/// - 返回 None 表示无命中。
pub fn text_match_score(query_text: &str, content: &str) -> Option<u32> {
    let q = query_text.trim();
    if q.is_empty() {
        return None;
    }
    if content.contains(q) {
        return Some(q.chars().count() as u32);
    }
    let q_lower = q.to_lowercase();
    let content_lower = content.to_lowercase();
    if content_lower.contains(&q_lower) {
        return Some(q_lower.chars().count() as u32);
    }

    let mut score = 0u32;
    for token in tokenize_query(q) {
        if content_lower.contains(&token.to_lowercase()) {
            score += token.chars().count() as u32;
        }
    }
    if score > 0 {
        Some(score)
    } else {
        None
    }
}

/// 把查询拆成检索 token：ASCII 字母数字词（≥2 字符）+ 中文 2~4 字滑窗。
/// 结果去重，保持稳定顺序。
fn tokenize_query(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();

    // ASCII 词
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        if word.chars().count() >= 2 && word.chars().all(|c| c as u32 <= 127) {
            push_unique(&mut tokens, word.to_string());
        }
    }

    // 中文滑窗 2~4 字（含 CJK 的片段才收）
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    for win in [2usize, 3, 4] {
        if n < win {
            continue;
        }
        for i in 0..=n - win {
            let chunk: String = chars[i..i + win].iter().collect();
            if chunk.chars().any(|c| c as u32 > 127) {
                push_unique(&mut tokens, chunk);
            }
        }
    }

    tokens
}

fn push_unique(tokens: &mut Vec<String>, token: String) {
    if !tokens.iter().any(|existing| existing == &token) {
        tokens.push(token);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLayer {
    InternalIdentity,
    HistoryArchive,
    LimLongTerm,
    ExternalKnowledge,
    MaintenanceRuntime,
}

impl MemoryLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InternalIdentity => "internal_identity",
            Self::HistoryArchive => "history_archive",
            Self::LimLongTerm => "lim_long_term",
            Self::ExternalKnowledge => "external_knowledge",
            Self::MaintenanceRuntime => "maintenance_runtime",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLayerBoundary {
    pub layer: MemoryLayer,
    pub archive_read_only: bool,
    pub maintenance_writeback_allowed: bool,
    pub decay_review_only: bool,
    pub writeback_target: &'static str,
}

impl MemoryLayerBoundary {
    pub fn for_record(record: &MemoryRecord) -> Self {
        let layer = classify_memory_layer(record);
        match layer {
            MemoryLayer::HistoryArchive => Self {
                layer,
                archive_read_only: true,
                maintenance_writeback_allowed: false,
                decay_review_only: false,
                writeback_target: "none",
            },
            MemoryLayer::LimLongTerm => Self {
                layer,
                archive_read_only: false,
                maintenance_writeback_allowed: true,
                decay_review_only: false,
                writeback_target: "experiences",
            },
            MemoryLayer::MaintenanceRuntime => Self {
                layer,
                archive_read_only: true,
                maintenance_writeback_allowed: false,
                decay_review_only: true,
                writeback_target: "none",
            },
            MemoryLayer::ExternalKnowledge => Self {
                layer,
                archive_read_only: true,
                maintenance_writeback_allowed: false,
                decay_review_only: false,
                writeback_target: "none",
            },
            MemoryLayer::InternalIdentity => Self {
                layer,
                archive_read_only: false,
                maintenance_writeback_allowed: false,
                decay_review_only: true,
                writeback_target: "manual_review_only",
            },
        }
    }
}

pub fn classify_memory_layer(record: &MemoryRecord) -> MemoryLayer {
    if let Some(layer) = record.metadata.get("memory_layer") {
        return match layer.as_str() {
            "history_archive" | "session_archive" | "archive" => MemoryLayer::HistoryArchive,
            "lim_long_term" | "experience" | "experiences" => MemoryLayer::LimLongTerm,
            "external_knowledge" | "knowledge" => MemoryLayer::ExternalKnowledge,
            "maintenance_runtime" | "maintenance" => MemoryLayer::MaintenanceRuntime,
            _ => MemoryLayer::InternalIdentity,
        };
    }

    match record.metadata.get("kind").map(String::as_str) {
        Some("turn_summary" | "session_summary") => MemoryLayer::HistoryArchive,
        Some("lim_candidate" | "experience") => MemoryLayer::LimLongTerm,
        Some("knowledge_hit" | "external_knowledge") => MemoryLayer::ExternalKnowledge,
        Some("maintenance_report" | "decay_review") => MemoryLayer::MaintenanceRuntime,
        _ => MemoryLayer::InternalIdentity,
    }
}

pub trait MemoryStore {
    fn put(&mut self, record: MemoryRecord) -> Result<(), MemoryStoreError>;
    fn get(&self, id: &str) -> Result<Option<MemoryRecord>, MemoryStoreError>;
    fn search(&self, query: &MemoryQuery) -> Result<Vec<SearchHit>, MemoryStoreError>;
    fn delete(&mut self, id: &str) -> Result<(), MemoryStoreError>;
    fn expire(&mut self, now: &str) -> Result<usize, MemoryStoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryStoreError {
    DuplicateId { id: String },
    InvalidQuery(&'static str),
    StorageUnavailable,
}
