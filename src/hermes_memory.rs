use std::path::PathBuf;

use crate::memory_admission::{MemoryEntryView, DEFAULT_MEMORY_WRITE_MAX_CHARS};

mod file;

pub use file::FileDualFileMemoryStore;

pub const DEFAULT_USER_MEMORY_MAX_CHARS: usize = 1375;
pub const DEFAULT_HOT_MEMORY_MAX_CHARS: usize = DEFAULT_MEMORY_WRITE_MAX_CHARS;
pub const DEFAULT_USER_MEMORY_FILE: &str = "USER.md";
pub const DEFAULT_HOT_MEMORY_FILE: &str = "MEMORY.md";
pub const DEFAULT_EXPERIENCES_MEMORY_FILE: &str = "experiences.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DualFileMemoryConfig {
    pub root: PathBuf,
    pub user_file: String,
    pub memory_file: String,
    pub experiences_file: String,
    pub user_max_chars: usize,
    pub memory_max_chars: usize,
}

impl DualFileMemoryConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            user_file: DEFAULT_USER_MEMORY_FILE.to_string(),
            memory_file: DEFAULT_HOT_MEMORY_FILE.to_string(),
            experiences_file: DEFAULT_EXPERIENCES_MEMORY_FILE.to_string(),
            user_max_chars: DEFAULT_USER_MEMORY_MAX_CHARS,
            memory_max_chars: DEFAULT_HOT_MEMORY_MAX_CHARS,
        }
    }

    pub fn user_path(&self) -> PathBuf {
        self.root.join(&self.user_file)
    }

    pub fn memory_path(&self) -> PathBuf {
        self.root.join(&self.memory_file)
    }

    pub fn experiences_path(&self) -> PathBuf {
        self.root.join(&self.experiences_file)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DualFileMemorySnapshot {
    pub user: String,
    pub memory: String,
    pub experiences: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotMemoryEntry {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DualFileMemoryError {
    StorageUnavailable {
        path: PathBuf,
    },
    DuplicateEntry {
        id: String,
    },
    HardLimitExceeded {
        scope: DualFileMemoryScope,
        limit_chars: usize,
        attempted_chars: usize,
        existing_entries: Vec<MemoryEntryView>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualFileMemoryScope {
    User,
    Memory,
    Experiences,
}

pub trait DualFileMemoryStore {
    fn read_user(&self) -> Result<String, DualFileMemoryError>;
    fn read_memory(&self) -> Result<String, DualFileMemoryError>;
    fn read_experiences(&self) -> Result<String, DualFileMemoryError>;
    fn snapshot(&self) -> Result<DualFileMemorySnapshot, DualFileMemoryError>;
    fn write_user(&mut self, content: &str) -> Result<(), DualFileMemoryError>;
    fn write_memory(&mut self, content: &str) -> Result<(), DualFileMemoryError>;
    fn append_memory(&mut self, entry: HotMemoryEntry) -> Result<(), DualFileMemoryError>;
    fn append_experience(&mut self, entry: HotMemoryEntry) -> Result<(), DualFileMemoryError>;
}
