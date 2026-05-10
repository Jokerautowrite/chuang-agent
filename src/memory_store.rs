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
