use std::fs;
use std::path::{Path, PathBuf};

use crate::memory_admission::{
    preview_chars, MemoryEntryView, TextMemoryAdmission, TextMemoryAdmissionDecision,
    DEFAULT_MEMORY_WRITE_MAX_CHARS,
};

pub const DEFAULT_USER_MEMORY_MAX_CHARS: usize = 1375;
pub const DEFAULT_HOT_MEMORY_MAX_CHARS: usize = DEFAULT_MEMORY_WRITE_MAX_CHARS;
pub const DEFAULT_USER_MEMORY_FILE: &str = "USER.md";
pub const DEFAULT_HOT_MEMORY_FILE: &str = "MEMORY.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DualFileMemoryConfig {
    pub root: PathBuf,
    pub user_file: String,
    pub memory_file: String,
    pub user_max_chars: usize,
    pub memory_max_chars: usize,
}

impl DualFileMemoryConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            user_file: DEFAULT_USER_MEMORY_FILE.to_string(),
            memory_file: DEFAULT_HOT_MEMORY_FILE.to_string(),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DualFileMemorySnapshot {
    pub user: String,
    pub memory: String,
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
}

pub trait DualFileMemoryStore {
    fn read_user(&self) -> Result<String, DualFileMemoryError>;
    fn read_memory(&self) -> Result<String, DualFileMemoryError>;
    fn snapshot(&self) -> Result<DualFileMemorySnapshot, DualFileMemoryError>;
    fn write_user(&mut self, content: &str) -> Result<(), DualFileMemoryError>;
    fn append_memory(&mut self, entry: HotMemoryEntry) -> Result<(), DualFileMemoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDualFileMemoryStore {
    config: DualFileMemoryConfig,
}

impl FileDualFileMemoryStore {
    pub fn open(config: DualFileMemoryConfig) -> Result<Self, DualFileMemoryError> {
        fs::create_dir_all(&config.root).map_err(|_| DualFileMemoryError::StorageUnavailable {
            path: config.root.clone(),
        })?;
        ensure_file(&config.user_path())?;
        ensure_file(&config.memory_path())?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &DualFileMemoryConfig {
        &self.config
    }
}

impl DualFileMemoryStore for FileDualFileMemoryStore {
    fn read_user(&self) -> Result<String, DualFileMemoryError> {
        read_to_string(&self.config.user_path())
    }

    fn read_memory(&self) -> Result<String, DualFileMemoryError> {
        read_to_string(&self.config.memory_path())
    }

    fn snapshot(&self) -> Result<DualFileMemorySnapshot, DualFileMemoryError> {
        Ok(DualFileMemorySnapshot {
            user: self.read_user()?,
            memory: self.read_memory()?,
        })
    }

    fn write_user(&mut self, content: &str) -> Result<(), DualFileMemoryError> {
        let existing_user = self.read_user()?;
        match TextMemoryAdmission::new(self.config.user_max_chars).evaluate(
            content,
            vec![MemoryEntryView {
                id: self.config.user_file.clone(),
                content_preview: preview_chars(&existing_user, 80),
                chars: existing_user.chars().count(),
            }],
        ) {
            TextMemoryAdmissionDecision::Accepted => {
                atomic_write(&self.config.user_path(), content)
            }
            TextMemoryAdmissionDecision::Rejected {
                limit_chars,
                attempted_chars,
                existing_entries,
            } => Err(DualFileMemoryError::HardLimitExceeded {
                scope: DualFileMemoryScope::User,
                limit_chars,
                attempted_chars,
                existing_entries,
            }),
        }
    }

    fn append_memory(&mut self, entry: HotMemoryEntry) -> Result<(), DualFileMemoryError> {
        let current = self.read_memory()?;
        let existing_entries = parse_memory_entry_views(&current);
        if existing_entries.iter().any(|view| view.id == entry.id) {
            return Err(DualFileMemoryError::DuplicateEntry { id: entry.id });
        }

        let next = append_entry_text(&current, &entry);
        match TextMemoryAdmission::new(self.config.memory_max_chars)
            .evaluate(&next, existing_entries)
        {
            TextMemoryAdmissionDecision::Accepted => {
                atomic_write(&self.config.memory_path(), &next)
            }
            TextMemoryAdmissionDecision::Rejected {
                limit_chars,
                attempted_chars,
                existing_entries,
            } => Err(DualFileMemoryError::HardLimitExceeded {
                scope: DualFileMemoryScope::Memory,
                limit_chars,
                attempted_chars,
                existing_entries,
            }),
        }
    }
}

fn ensure_file(path: &Path) -> Result<(), DualFileMemoryError> {
    if path.exists() {
        return Ok(());
    }
    atomic_write(path, "")
}

fn read_to_string(path: &Path) -> Result<String, DualFileMemoryError> {
    fs::read_to_string(path).map_err(|_| DualFileMemoryError::StorageUnavailable {
        path: path.to_path_buf(),
    })
}

fn atomic_write(path: &Path, content: &str) -> Result<(), DualFileMemoryError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| DualFileMemoryError::StorageUnavailable {
        path: parent.to_path_buf(),
    })?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content).map_err(|_| DualFileMemoryError::StorageUnavailable {
        path: tmp_path.clone(),
    })?;
    fs::rename(&tmp_path, path).map_err(|_| DualFileMemoryError::StorageUnavailable {
        path: path.to_path_buf(),
    })
}

fn append_entry_text(current: &str, entry: &HotMemoryEntry) -> String {
    let mut next = current.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str("## ");
    next.push_str(&entry.id);
    next.push('\n');
    next.push_str(entry.content.trim());
    next.push('\n');
    next
}

fn parse_memory_entry_views(content: &str) -> Vec<MemoryEntryView> {
    let mut entries = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(id) = line.strip_prefix("## ") {
            push_entry(&mut entries, current_id.take(), &current_body);
            current_id = Some(id.trim().to_string());
            current_body.clear();
        } else {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }
    push_entry(&mut entries, current_id, &current_body);
    entries
}

fn push_entry(entries: &mut Vec<MemoryEntryView>, id: Option<String>, body: &str) {
    if let Some(id) = id {
        let body = body.trim();
        entries.push(MemoryEntryView {
            id,
            content_preview: preview_chars(body, 80),
            chars: body.chars().count(),
        });
    }
}
