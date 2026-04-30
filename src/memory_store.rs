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
